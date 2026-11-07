//! The vault's folder tree: every visible folder and file, Markdown notes and everything else (images, canvases,
//! attachments). Hidden entries (`.obsidian`, `.trash`, dot files, OS files) are left out. A symbolic link to a file
//! inside the vault is listed; links that lead out of it, and linked folders (which could loop), are not.

use super::paths::{self, Vault};
use serde::Serialize;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Most entries listed; a bigger vault is cut off (`truncated`).
pub const MAX_ENTRIES: usize = 20_000;
/// Deepest folder level listed.
pub const MAX_DEPTH: usize = 32;
/// Image types the notes view shows (and embeds).
pub const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif"];

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Folder,
    /// A Markdown note (`.md`).
    Note,
    Image,
    /// An Obsidian canvas (`.canvas`, shown as plain text in v1).
    Canvas,
    /// Anything else (PDFs, audio, other attachments): listed only.
    File,
}

/// The kind of a file named `name`.
#[must_use]
pub fn kind_of(name: &str) -> EntryKind {
    match paths::extension(name).as_deref() {
        Some("md") => EntryKind::Note,
        Some("canvas") => EntryKind::Canvas,
        Some(ext) if IMAGE_EXTENSIONS.contains(&ext) => EntryKind::Image,
        _ => EntryKind::File,
    }
}

/// One entry of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    /// Vault-relative path (`Folder/Note.md`).
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    /// Bytes (0 for folders).
    pub size: u64,
    /// Last change, milliseconds since 1970 (0 when unknown).
    pub mtime_ms: u64,
}

/// The tree listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// Depth-first: in each folder, its folders first, then its files, each in case-insensitive name order.
    pub entries: Vec<VaultEntry>,
    pub truncated: bool,
}

/// Milliseconds since 1970 of a modification time.
#[must_use]
pub fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// The visible entries of one folder: (name, path, metadata of the entry or of the file a link points at).
fn visible(vault: &Vault, dir: &Path) -> Vec<(String, std::path::PathBuf, std::fs::Metadata)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        tracing::warn!("vault: a folder could not be read");
        return Vec::new();
    };
    let mut out: Vec<_> = read
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(name, _)| !paths::is_hidden_name(name))
        .filter_map(|(name, path)| {
            let link = std::fs::symlink_metadata(&path).ok()?;
            if !link.file_type().is_symlink() {
                return Some((name, path, link));
            }
            // A link: only to a visible file inside the vault.
            let real = std::fs::canonicalize(&path).ok()?;
            let meta = std::fs::metadata(&real).ok()?;
            (vault.reachable(&real) && meta.is_file()).then_some((name, path, meta))
        })
        .collect();
    out.sort_by_cached_key(|(name, _, meta)| (!meta.is_dir(), name.to_lowercase(), name.clone()));
    out
}

fn walk(vault: &Vault, dir: &Path, depth: usize, out: &mut Listing) {
    for (name, path, meta) in visible(vault, dir) {
        if out.entries.len() >= MAX_ENTRIES {
            out.truncated = true;
            return;
        }
        let Some(rel) = vault.relative(&path) else {
            continue;
        };
        let is_dir = meta.is_dir();
        out.entries.push(VaultEntry {
            path: rel,
            kind: if is_dir {
                EntryKind::Folder
            } else {
                kind_of(&name)
            },
            name,
            size: if is_dir { 0 } else { meta.len() },
            mtime_ms: mtime_ms(&meta),
        });
        if is_dir {
            if depth + 1 < MAX_DEPTH {
                walk(vault, &path, depth + 1, out);
            } else {
                out.truncated = true;
            }
        }
    }
}

/// List the vault.
#[must_use]
pub fn list(vault: &Vault) -> Listing {
    let mut out = Listing::default();
    walk(vault, vault.root(), 0, &mut out);
    out
}
