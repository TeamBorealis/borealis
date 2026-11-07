//! Wire types of the notes view (`ui/src/api/types/vault.ts`).

use super::config::VaultConfig;
use super::rename::LineChange;
use super::text::LineEnding;
use super::tree::VaultEntry;
use serde::Serialize;

/// `vault_tree`: the vault and its folder tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTree {
    /// The chosen vault folder; `None` = no vault.
    pub dir: Option<String>,
    /// The folder's name (the vault's name in Obsidian).
    pub name: Option<String>,
    /// The chosen folder cannot be read right now (nothing listed; the setting is kept).
    pub unavailable: bool,
    /// The folder has a `.obsidian` settings folder.
    pub obsidian: bool,
    pub config: VaultConfig,
    pub entries: Vec<VaultEntry>,
    /// The vault holds more than is listed.
    pub truncated: bool,
}

/// `vault_read`: a note or a text file as it is on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultNote {
    pub path: String,
    /// The text after any byte order mark, line endings as in the file.
    pub text: String,
    /// SHA-256 of the file's bytes: pass it back to `vault_save`.
    pub hash: String,
    pub mtime_ms: u64,
    pub line_ending: LineEnding,
    pub bom: bool,
    /// The file can be shown but not saved (not UTF-8, too large, or a canvas).
    pub read_only: Option<String>,
}

/// What `vault_save` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveStatus {
    /// Written (or already identical).
    Saved,
    /// The file changed on disk since it was loaded: nothing was written; `disk` holds what is there.
    Conflict,
    /// The file is gone from disk: nothing was written.
    Missing,
}

/// `vault_save` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSave {
    pub status: SaveStatus,
    /// After `saved`: the new content hash.
    pub hash: Option<String>,
    pub mtime_ms: Option<u64>,
    /// After `conflict`: the file as it is now.
    pub disk: Option<VaultNote>,
}

/// One note's link updates in a rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFile {
    /// The note's path (before the rename, for the renamed note itself).
    pub path: String,
    pub changes: Vec<LineChange>,
}

/// `vault_rename_preview`: what renaming would change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub from: String,
    pub to: String,
    /// Notes whose links would be updated, in path order.
    pub files: Vec<RenameFile>,
}

/// `vault_rename` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDone {
    /// The file's new path.
    pub to: String,
    /// Notes whose links were updated.
    pub updated: Vec<String>,
    /// Notes whose links could not be updated (changed meanwhile, not UTF-8 or not writable).
    pub skipped: Vec<String>,
}

/// What `vault_delete` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeleteStatus {
    /// Moved to the OS trash.
    Trashed,
    /// Deleted for good (only when asked: `permanent`).
    Deleted,
    /// This system has no trash for that file: nothing was deleted; ask before deleting for good.
    TrashUnavailable,
}

/// `vault_attach` result: the copied file and the link to insert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultAttachment {
    pub path: String,
    pub link: String,
}
