//! Android: the vault in a Storage Access Framework document tree (B-221, decision U17b).
//!
//! Android apps cannot open another app's files by path, so the vault folder is picked with
//! `ACTION_OPEN_DOCUMENT_TREE` and kept with a persisted URI permission. The setting `vaultDir` then holds the tree's
//! `content://` URI, and every file is reached through a [`DocumentTree`]: the Kotlin `VaultPlugin` in the app, or a
//! fake in tests. [`TreeStore`] puts the notes view's rules on top of it, the same ones the local folder store keeps:
//! - every path the webview sends is checked ([`paths::parts`]); hidden names are never listed, read or written, so
//!   nothing under `.obsidian/` is touched except reading `app.json`;
//! - a note is written only while it still hashes to what the editor loaded;
//! - nothing is ever replaced by a create or a rename.
//!
//! What the Storage Access Framework cannot do, and what is done instead (also in the B-221 handoff):
//! - No atomic replace: a document cannot be renamed over another. The plugin first writes the new bytes to a hidden
//!   temporary document next to the note, then rewrites the note in place (`"wt"`), reads it back, and only then
//!   deletes the temporary copy. A crash in between leaves the full new text in that hidden copy, never half a note.
//! - The hash check and the write are two calls, so the window in which another app's change can slip in is longer
//!   than on a local disk (tens of milliseconds instead of a few).
//! - No trash: deleting asks "Delete for good?", as on desktop when the trash is refused.
//! - No change notifications: the notes view reads the vault again when it opens and when the app returns to the
//!   foreground.

use super::config::MAX_APP_JSON;
use super::dto::DeleteStatus;
use super::paths;
use super::store::{taken, FileBytes, Trash, VaultStore};
use super::text;
use super::tree::{self, EntryKind, Listing, VaultEntry};
use super::write::Outcome;
use crate::error::{ApiError, ApiResult, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The scheme of an Android document tree.
const CONTENT_SCHEME: &str = "content://";
/// The part of a tree URI before the tree's document id.
const TREE_SEGMENT: &str = "/tree/";
/// The volume id of the phone's own storage in an `externalstorage` document id.
const PRIMARY_VOLUME: &str = "primary";
/// How the phone's own storage is shown.
const PRIMARY_LABEL: &str = "Internal storage";
/// Largest note checked before a write (larger notes do not open, so they are never saved).
const MAX_CHECKED_BYTES: u64 = super::index::MAX_NOTE_BYTES;

/// One entry of a document tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    /// Tree-relative, `/`-separated (`Places/Harbor.md`).
    pub path: String,
    pub dir: bool,
    pub size: u64,
    /// Milliseconds since 1970 (0 when unknown).
    pub mtime_ms: u64,
}

impl DocEntry {
    /// The entry's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        paths::file_name(&self.path)
    }
}

/// Why a document tree call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// Nothing is at that path.
    NotFound,
    /// Something is already at that path (creates and renames never replace).
    Exists,
    /// A file is where a folder of the path should be.
    NotAFolder,
    /// Anything else (no permission, provider error), with the platform's message for the log.
    Failed(String),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Exists => f.write_str("already exists"),
            Self::NotAFolder => f.write_str("not a folder"),
            Self::Failed(m) => f.write_str(m),
        }
    }
}

/// The document tree operations, on tree-relative `/` paths of the tree `tree` (its `content://` URI). The paths are
/// checked by [`TreeStore`] before they get here; implementations may assume plain names.
pub trait DocumentTree: Send + Sync + std::fmt::Debug {
    /// The tree is still granted and can be read.
    ///
    /// # Errors
    /// When it cannot.
    fn check(&self, tree: &str) -> Result<(), TreeError>;
    /// Every entry of the tree in any order. Hidden folders (a name starting with `.`) are listed but not entered.
    ///
    /// # Errors
    /// When the tree cannot be read.
    fn list(&self, tree: &str) -> Result<Vec<DocEntry>, TreeError>;
    /// The entries directly in the folder `dir` (`""` = the tree itself).
    ///
    /// # Errors
    /// `NotFound` when there is no such folder.
    fn children(&self, tree: &str, dir: &str) -> Result<Vec<DocEntry>, TreeError>;
    /// The entry at `rel`, if any.
    ///
    /// # Errors
    /// When the tree cannot be read.
    fn stat(&self, tree: &str, rel: &str) -> Result<Option<DocEntry>, TreeError>;
    /// The bytes of the file `rel`; `None` when it holds more than `max`.
    ///
    /// # Errors
    /// `NotFound` when it is gone.
    fn read(&self, tree: &str, rel: &str, max: u64) -> Result<Option<Vec<u8>>, TreeError>;
    /// Replace the content of the existing file `rel` (see the module docs for how).
    ///
    /// # Errors
    /// `NotFound` when it is gone.
    fn write(&self, tree: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError>;
    /// Create the new file `rel` with `bytes`, and any missing folder on the way.
    ///
    /// # Errors
    /// `Exists` when something is there; `NotAFolder` when a file is in the way of a folder.
    fn create(&self, tree: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError>;
    /// Rename the entry `rel` to `name` in its folder.
    ///
    /// # Errors
    /// `Exists` when the name is taken; `NotFound` when `rel` is gone.
    fn rename(&self, tree: &str, rel: &str, name: &str) -> Result<(), TreeError>;
    /// Delete the file `rel` for good.
    ///
    /// # Errors
    /// `NotFound` when it is gone.
    fn delete(&self, tree: &str, rel: &str) -> Result<(), TreeError>;
    /// Give the persisted permission back (the vault was cleared or another one chosen). Never fails.
    fn release(&self, tree: &str);
}

/// The vault setting `dir` names an Android document tree.
#[must_use]
pub fn is_tree_uri(dir: &str) -> bool {
    dir.starts_with(CONTENT_SCHEME) && dir.contains(TREE_SEGMENT)
}

/// The tree's document id, decoded (`primary:Documents/My Vault`).
fn tree_document_id(uri: &str) -> Option<String> {
    let (_, rest) = uri.split_once(TREE_SEGMENT)?;
    let id = rest.split('/').next().filter(|id| !id.is_empty())?;
    percent_encoding::percent_decode_str(id)
        .decode_utf8()
        .ok()
        .map(std::borrow::Cow::into_owned)
}

/// Where the tree is, for people: `Internal storage/Documents/My Vault`, or the provider's own id.
#[must_use]
pub fn display_path(uri: &str) -> Option<String> {
    let id = tree_document_id(uri)?;
    Some(match id.split_once(':') {
        Some((PRIMARY_VOLUME, "")) => PRIMARY_LABEL.to_string(),
        Some((PRIMARY_VOLUME, path)) => format!("{PRIMARY_LABEL}/{path}"),
        Some((volume, path)) if !path.is_empty() => format!("{volume}/{path}"),
        _ => id,
    })
}

/// The tree folder's own name (`My Vault`).
#[must_use]
pub fn display_name(uri: &str) -> Option<String> {
    let path = display_path(uri)?;
    path.rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// A path that crosses a hidden name (`.obsidian/app.json`, `.trash/x.md`, `.dotfile`).
fn is_hidden_path(rel: &str) -> bool {
    rel.split('/').any(paths::is_hidden_name)
}

/// The tree's visible entries as the notes view lists them: depth-first, in each folder its folders first, then its
/// files, each in case-insensitive name order; cut at [`tree::MAX_ENTRIES`] and [`tree::MAX_DEPTH`].
#[must_use]
pub fn listing_of(entries: Vec<DocEntry>) -> Listing {
    let mut by_folder: BTreeMap<String, Vec<DocEntry>> = BTreeMap::new();
    for e in entries
        .into_iter()
        .filter(|e| !e.path.is_empty() && !is_hidden_path(&e.path) && paths::parts(&e.path).is_ok())
    {
        by_folder
            .entry(paths::parent(&e.path).to_string())
            .or_default()
            .push(e);
    }
    for list in by_folder.values_mut() {
        list.sort_by_cached_key(|e| (!e.dir, e.name().to_lowercase(), e.name().to_string()));
    }
    let mut out = Listing::default();
    walk(&by_folder, "", 0, &mut out);
    out
}

fn walk(by_folder: &BTreeMap<String, Vec<DocEntry>>, dir: &str, depth: usize, out: &mut Listing) {
    let Some(list) = by_folder.get(dir) else {
        return;
    };
    for e in list {
        if out.entries.len() >= tree::MAX_ENTRIES {
            out.truncated = true;
            return;
        }
        out.entries.push(VaultEntry {
            path: e.path.clone(),
            name: e.name().to_string(),
            kind: if e.dir {
                EntryKind::Folder
            } else {
                tree::kind_of(e.name())
            },
            size: if e.dir { 0 } else { e.size },
            mtime_ms: e.mtime_ms,
            cloud: false,
        });
        if e.dir {
            if depth + 1 < tree::MAX_DEPTH {
                walk(by_folder, &e.path, depth + 1, out);
            } else {
                out.truncated = true;
            }
        }
    }
}

/// A vault in an Android document tree.
#[derive(Debug, Clone)]
pub struct TreeStore {
    tree: Arc<dyn DocumentTree>,
    uri: String,
}

fn gone() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "That file is no longer in your vault.")
}

fn failed(what: &str, e: &TreeError) -> ApiError {
    match e {
        TreeError::NotFound => gone(),
        TreeError::Exists => taken(),
        TreeError::NotAFolder => ApiError::invalid("A file is in the way of that folder."),
        TreeError::Failed(m) => ApiError::io(what, m),
    }
}

impl TreeStore {
    /// The vault in the tree `uri`, when it is still granted and readable.
    ///
    /// # Errors
    /// The platform's reason when it is not.
    pub fn open(tree: Arc<dyn DocumentTree>, uri: &str) -> Result<Self, TreeError> {
        tree.check(uri)?;
        Ok(Self {
            tree,
            uri: uri.to_string(),
        })
    }

    /// The visible file `rel`, checked.
    fn existing_file(&self, rel: &str) -> ApiResult<DocEntry> {
        paths::parts(rel)?;
        match self.tree.stat(&self.uri, rel) {
            Ok(Some(e)) if e.dir => Err(ApiError::invalid("That is a folder, not a file.")),
            Ok(Some(e)) => Ok(e),
            Ok(None) | Err(TreeError::NotFound) => Err(gone()),
            Err(e) => Err(failed("read the file", &e)),
        }
    }
}

impl VaultStore for TreeStore {
    fn identity(&self) -> String {
        self.uri.clone()
    }

    fn name(&self) -> Option<String> {
        display_name(&self.uri)
    }

    fn has_obsidian(&self) -> bool {
        matches!(self.tree.stat(&self.uri, ".obsidian"), Ok(Some(e)) if e.dir)
    }

    fn app_json(&self) -> Option<String> {
        let bytes = self
            .tree
            .read(&self.uri, super::config::APP_JSON, MAX_APP_JSON)
            .ok()??;
        String::from_utf8(bytes).ok()
    }

    fn list(&self) -> Listing {
        match self.tree.list(&self.uri) {
            Ok(entries) => listing_of(entries),
            Err(e) => {
                tracing::warn!("vault: the document tree could not be listed: {e}");
                Listing::default()
            }
        }
    }

    fn exists(&self, rel: &str) -> bool {
        paths::parts(rel).is_ok() && matches!(self.tree.stat(&self.uri, rel), Ok(Some(_)))
    }

    fn file(&self, rel: &str) -> ApiResult<String> {
        Ok(self.existing_file(rel)?.name().to_string())
    }

    fn read(&self, rel: &str, max: u64) -> ApiResult<Option<FileBytes>> {
        let entry = self.existing_file(rel)?;
        let bytes = self
            .tree
            .read(&self.uri, rel, max)
            .map_err(|e| failed("read the file", &e))?;
        Ok(bytes.map(|bytes| FileBytes {
            bytes,
            mtime_ms: entry.mtime_ms,
            real_name: entry.name().to_string(),
        }))
    }

    fn mtime(&self, rel: &str) -> u64 {
        self.existing_file(rel).map_or(0, |e| e.mtime_ms)
    }

    fn guarded_write(&self, rel: &str, base_hash: &str, bytes: &[u8]) -> ApiResult<Outcome> {
        paths::parts(rel)?;
        let current = match self.tree.read(&self.uri, rel, MAX_CHECKED_BYTES) {
            Ok(Some(b)) => b,
            Ok(None) => return Err(ApiError::invalid("That file is too large to save.")),
            Err(TreeError::NotFound) => return Ok(Outcome::Missing),
            Err(e) => return Err(failed("save the note", &e)),
        };
        if text::hash(&current) != base_hash {
            return Ok(Outcome::Changed { disk: current });
        }
        if current == bytes {
            return Ok(Outcome::Saved {
                hash: base_hash.to_string(),
            });
        }
        match self.tree.write(&self.uri, rel, bytes) {
            Ok(()) => Ok(Outcome::Saved {
                hash: text::hash(bytes),
            }),
            Err(TreeError::NotFound) => Ok(Outcome::Missing),
            Err(e) => Err(failed("save the note", &e)),
        }
    }

    fn create_new(&self, rel: &str, bytes: &[u8]) -> ApiResult<()> {
        paths::parts(rel)?;
        self.tree
            .create(&self.uri, rel, bytes)
            .map_err(|e| failed("create the note", &e))
    }

    fn copy_new(&self, src: &Path, rel: &str) -> ApiResult<()> {
        let bytes = std::fs::read(src).map_err(|e| ApiError::io("read the file", e))?;
        paths::parts(rel)?;
        self.tree
            .create(&self.uri, rel, &bytes)
            .map_err(|e| failed("copy the file", &e))
    }

    fn rename_taken(&self, rel: &str, name: &str, case_only: bool) -> ApiResult<bool> {
        self.existing_file(rel)?;
        let siblings = self
            .tree
            .children(&self.uri, paths::parent(rel))
            .map_err(|e| failed("read the folder", &e))?;
        Ok(siblings.iter().any(|e| {
            e.name() == name || (!case_only && e.name().to_lowercase() == name.to_lowercase())
        }))
    }

    fn rename(&self, rel: &str, name: &str, _case_only: bool) -> ApiResult<()> {
        self.existing_file(rel)?;
        paths::check_new_name(name)?;
        self.tree
            .rename(&self.uri, rel, name)
            .map_err(|e| failed("rename the file", &e))
    }

    fn delete(&self, rel: &str, permanent: bool, _trash: Trash<'_>) -> ApiResult<DeleteStatus> {
        self.existing_file(rel)?;
        if !permanent {
            // A document tree has no trash: the user is asked before the file goes for good.
            return Ok(DeleteStatus::TrashUnavailable);
        }
        self.tree
            .delete(&self.uri, rel)
            .map_err(|e| failed("delete the file", &e))?;
        Ok(DeleteStatus::Deleted)
    }
}

#[cfg(test)]
#[path = "tree_store_tests.rs"]
mod tests;
