//! The vault's folder tree: every visible folder and file, Markdown notes and everything else (images, canvases,
//! attachments). Hidden entries (`.obsidian`, `.trash`, dot files, OS files) are left out. A symbolic link to a file
//! inside the vault is listed; links that lead out of it, and linked folders (which could loop), are not.
//!
//! On iOS and iPadOS a file that iCloud Drive has not downloaded yet is a hidden placeholder, `.<name>.icloud`. Those
//! are listed under their own name and marked `cloud`, so the notes view can say the file is still on its way.

use super::paths::{self, Vault};
use serde::Serialize;
use std::path::{Path, PathBuf};
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
    /// iOS: iCloud Drive has not downloaded this file yet (only a placeholder is there).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cloud: bool,
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

/// The suffix iCloud Drive gives a placeholder.
const ICLOUD_SUFFIX: &str = ".icloud";

/// The name of the file an iCloud Drive placeholder stands for (`.Session 1.md.icloud` -> `Session 1.md`).
#[must_use]
pub fn icloud_placeholder(name: &str) -> Option<&str> {
    let inner = name.strip_prefix('.')?.strip_suffix(ICLOUD_SUFFIX)?;
    (!inner.is_empty() && !paths::is_hidden_name(inner)).then_some(inner)
}

/// The placeholder name iCloud Drive uses for `name`.
#[must_use]
pub fn icloud_placeholder_name(name: &str) -> String {
    format!(".{name}{ICLOUD_SUFFIX}")
}

/// One visible entry of a folder.
struct Seen {
    /// The name shown (a placeholder's file name, not the placeholder's).
    name: String,
    /// Where it is now (the placeholder itself for a file not downloaded).
    path: PathBuf,
    /// The entry's metadata, or that of the file a link points at.
    meta: std::fs::Metadata,
    cloud: bool,
}

/// The visible entries of one folder; with `placeholders`, iCloud placeholders too (as the files they stand for).
fn visible(vault: &Vault, dir: &Path, placeholders: bool) -> Vec<Seen> {
    let Ok(read) = std::fs::read_dir(dir) else {
        tracing::warn!("vault: a folder could not be read");
        return Vec::new();
    };
    let named: Vec<(String, PathBuf)> = read
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .collect();
    let present: std::collections::HashSet<&str> = named.iter().map(|(n, _)| n.as_str()).collect();
    let mut out: Vec<Seen> = named
        .iter()
        .filter_map(|(name, path)| {
            if paths::is_hidden_name(name) {
                // A placeholder of a file that is not also there already.
                let real = icloud_placeholder(name).filter(|_| placeholders)?;
                if present.contains(real) {
                    return None;
                }
                let meta = std::fs::symlink_metadata(path).ok()?;
                return meta.is_file().then(|| Seen {
                    name: real.to_string(),
                    path: path.clone(),
                    meta,
                    cloud: true,
                });
            }
            let link = std::fs::symlink_metadata(path).ok()?;
            if !link.file_type().is_symlink() {
                return Some(Seen {
                    name: name.clone(),
                    path: path.clone(),
                    meta: link,
                    cloud: false,
                });
            }
            // A link: only to a visible file inside the vault.
            let real = std::fs::canonicalize(path).ok()?;
            let meta = std::fs::metadata(&real).ok()?;
            (vault.reachable(&real) && meta.is_file()).then(|| Seen {
                name: name.clone(),
                path: path.clone(),
                meta,
                cloud: false,
            })
        })
        .collect();
    out.sort_by_cached_key(|s| (!s.meta.is_dir(), s.name.to_lowercase(), s.name.clone()));
    out
}

fn walk(vault: &Vault, dir: &Path, depth: usize, placeholders: bool, out: &mut Listing) {
    for seen in visible(vault, dir, placeholders) {
        if out.entries.len() >= MAX_ENTRIES {
            out.truncated = true;
            return;
        }
        let shown = if seen.cloud {
            seen.path.with_file_name(&seen.name)
        } else {
            seen.path.clone()
        };
        let Some(rel) = vault.relative(&shown) else {
            continue;
        };
        let is_dir = seen.meta.is_dir();
        out.entries.push(VaultEntry {
            path: rel,
            kind: if is_dir {
                EntryKind::Folder
            } else {
                kind_of(&seen.name)
            },
            name: seen.name,
            // A placeholder's own size is not the file's.
            size: if is_dir || seen.cloud {
                0
            } else {
                seen.meta.len()
            },
            mtime_ms: mtime_ms(&seen.meta),
            cloud: seen.cloud,
        });
        if is_dir {
            if depth + 1 < MAX_DEPTH {
                walk(vault, &seen.path, depth + 1, placeholders, out);
            } else {
                out.truncated = true;
            }
        }
    }
}

/// List the vault.
#[must_use]
pub fn list(vault: &Vault) -> Listing {
    list_with(vault, false)
}

/// List the vault; with `placeholders`, files iCloud Drive has not downloaded are listed too (marked `cloud`).
#[must_use]
pub fn list_with(vault: &Vault, placeholders: bool) -> Listing {
    let mut out = Listing::default();
    walk(vault, vault.root(), 0, placeholders, &mut out);
    out
}
