//! The vault's Obsidian settings in `.obsidian/app.json`, read only: Borealis never writes anything under `.obsidian/`.
//! It uses where new notes go (`newFileLocation`, `newFileFolderPath`), where attachments go (`attachmentFolderPath`)
//! and how the links it inserts are written (`useMarkdownLinks`, `newLinkFormat`). A missing or unreadable file means
//! Obsidian's defaults.

use super::paths;
use super::resolve::{self, FileSet};
use super::store::VaultStore;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// The settings file, relative to the vault folder.
pub const APP_JSON: &str = ".obsidian/app.json";
/// Largest settings file read.
pub(crate) const MAX_APP_JSON: u64 = 1024 * 1024;
/// What a Markdown link destination percent-encodes (Obsidian encodes spaces; the rest keeps the link valid).
const LINK_ENCODE: &AsciiSet = &CONTROLS.add(b' ').add(b'(').add(b')').add(b'<').add(b'>');

/// Where new notes go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NewFileLocation {
    /// The vault folder (Obsidian's default).
    #[default]
    Root,
    /// The folder of the note that is open.
    Current,
    /// `newFileFolderPath`.
    Folder,
}

/// How inserted links are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkFormat {
    /// The name alone when it is unique, else the full path (Obsidian's default).
    #[default]
    Shortest,
    /// The path relative to the note.
    Relative,
    /// The full path from the vault folder.
    Absolute,
}

/// The `.obsidian/app.json` options Borealis honours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultConfig {
    pub new_file_location: NewFileLocation,
    /// For [`NewFileLocation::Folder`]: a vault-relative folder (`""` = the vault folder).
    pub new_file_folder_path: String,
    /// As in Obsidian: `/` (vault folder), `./` (the note's folder), `./sub` (a sub-folder of it) or a folder path.
    pub attachment_folder_path: String,
    /// Insert `[text](path.md)` links instead of `[[wikilinks]]`.
    pub use_markdown_links: bool,
    pub new_link_format: LinkFormat,
}

impl Default for VaultConfig {
    /// Obsidian's defaults: new notes and attachments in the vault folder, shortest wikilinks.
    fn default() -> Self {
        Self {
            new_file_location: NewFileLocation::Root,
            new_file_folder_path: String::new(),
            attachment_folder_path: "/".to_string(),
            use_markdown_links: false,
            new_link_format: LinkFormat::Shortest,
        }
    }
}

impl VaultConfig {
    /// Parse the settings JSON; unknown or wrong-typed values fall back to Obsidian's defaults.
    #[must_use]
    pub fn from_json(text: &str) -> Self {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let text_of = |key: &str| map.get(key).and_then(Value::as_str).unwrap_or_default();
        Self {
            new_file_location: match text_of("newFileLocation") {
                "current" => NewFileLocation::Current,
                "folder" => NewFileLocation::Folder,
                _ => NewFileLocation::Root,
            },
            new_file_folder_path: clean_folder(text_of("newFileFolderPath")),
            attachment_folder_path: match text_of("attachmentFolderPath") {
                "" => "/".to_string(),
                other => other.to_string(),
            },
            use_markdown_links: map
                .get("useMarkdownLinks")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            new_link_format: match text_of("newLinkFormat") {
                "relative" => LinkFormat::Relative,
                "absolute" => LinkFormat::Absolute,
                _ => LinkFormat::Shortest,
            },
        }
    }

    /// The settings of the vault in `store` (defaults when `app.json` is missing, too large or not JSON).
    #[must_use]
    pub fn of(store: &dyn VaultStore) -> Self {
        store
            .app_json()
            .map(|t| Self::from_json(&t))
            .unwrap_or_default()
    }

    /// Read `<root>/.obsidian/app.json` (defaults when it is missing, too large or not JSON).
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let path = root.join(APP_JSON);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_APP_JSON);
        if !small {
            return Self::default();
        }
        std::fs::read_to_string(&path)
            .map(|t| Self::from_json(&t))
            .unwrap_or_default()
    }

    /// The folder a new note goes in, for a note created while `from` is open.
    #[must_use]
    pub fn new_note_folder(&self, from: Option<&str>) -> String {
        match self.new_file_location {
            NewFileLocation::Root => String::new(),
            NewFileLocation::Current => from.map(paths::parent).unwrap_or_default().to_string(),
            NewFileLocation::Folder => self.new_file_folder_path.clone(),
        }
    }

    /// The folder a new attachment goes in, for the note `from`.
    #[must_use]
    pub fn attachment_folder(&self, from: &str) -> String {
        let note_dir = paths::parent(from);
        let setting = self.attachment_folder_path.trim();
        let folder = match setting {
            "/" | "" => String::new(),
            "./" | "." => note_dir.to_string(),
            s => match s.strip_prefix("./") {
                Some(sub) => paths::join(note_dir, &clean_folder(sub)),
                None => clean_folder(s),
            },
        };
        if folder.is_empty() || paths::parts(&folder).is_ok() {
            folder
        } else {
            String::new()
        }
    }

    /// The link to insert in the note `from` for the file `to` (`files` holds every vault file, `to` included).
    #[must_use]
    pub fn link_text(&self, files: &FileSet, from: &str, to: &str, embed: bool) -> String {
        let bang = if embed { "!" } else { "" };
        let path = match self.new_link_format {
            LinkFormat::Absolute => to.to_string(),
            LinkFormat::Relative => resolve::relative(paths::parent(from), to),
            LinkFormat::Shortest => files.shortest(to),
        };
        let is_note = resolve::strip_md(to) != to;
        if self.use_markdown_links {
            let with_ext = if is_note && resolve::strip_md(&path) == path {
                format!("{path}.md")
            } else {
                path
            };
            let name = resolve::strip_md(paths::file_name(to));
            let label = if embed { "" } else { name };
            let dest = utf8_percent_encode(&with_ext, LINK_ENCODE);
            format!("{bang}[{label}]({dest})")
        } else {
            format!("{bang}[[{}]]", resolve::strip_md(&path))
        }
    }
}

/// A folder setting as a vault-relative folder: no leading or trailing `/`.
fn clean_folder(folder: &str) -> String {
    folder.trim().trim_matches('/').to_string()
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
