//! Renaming a vault file, with the link updates the user confirmed ([`super::rename`] has the edit rules).

use super::dto::{RenameDone, RenameFile, RenamePlan};
use super::paths;
use super::store::VaultStore;
use super::tree::{EntryKind, Listing};
use super::{file_set, index, lock, rename, text, write};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// The link updates one note needs for a rename.
struct LinkUpdate {
    /// The note's path before the rename.
    note: String,
    /// Its path after (different only for the renamed note itself).
    note_after: String,
    /// The bytes the edits were computed from (the write is checked against them).
    bytes: Vec<u8>,
    text: String,
    edits: Vec<rename::Edit>,
}

impl AppState {
    /// Where renaming `path` to `new_name` (a name without the extension, which is kept) puts it.
    fn rename_target(store: &dyn VaultStore, path: &str, new_name: &str) -> ApiResult<String> {
        store.file(path)?;
        let old_name = paths::file_name(path);
        let ext = paths::extension(old_name);
        // A typed extension (`Harbor.md`) is the one the file keeps.
        let stem = ext
            .as_ref()
            .and_then(|e| {
                let cut = new_name.len().checked_sub(e.len() + 1)?;
                let tail = new_name.get(cut..)?;
                tail.eq_ignore_ascii_case(&format!(".{e}"))
                    .then(|| &new_name[..cut])
            })
            .unwrap_or(new_name);
        paths::check_new_name(stem)?;
        let name = match &ext {
            Some(_) => format!(
                "{stem}.{}",
                old_name.rsplit_once('.').map_or("", |(_, e)| e)
            ),
            None => stem.to_string(),
        };
        let to = paths::join(paths::parent(path), &name);
        if name == old_name {
            return Err(ApiError::invalid("That is already its name."));
        }
        let same_but_case = name.to_lowercase() == old_name.to_lowercase();
        if store.rename_taken(path, &name, same_but_case)? {
            return Err(ApiError::invalid("A file with that name is already there."));
        }
        Ok(to)
    }

    /// The link edits renaming `from` to `to` needs, per note.
    fn rename_edits(
        store: &dyn VaultStore,
        listing: &Listing,
        from: &str,
        to: &str,
    ) -> Vec<LinkUpdate> {
        let before = file_set(listing);
        let after = rename::renamed(&before, from, to);
        listing
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Note && e.size <= index::MAX_NOTE_BYTES)
            .filter_map(|e| {
                let bytes = store.read(&e.path, index::MAX_NOTE_BYTES).ok()??.bytes;
                let decoded = text::decode(&bytes);
                if !decoded.utf8 {
                    return None;
                }
                let note_after = if e.path == from {
                    to.to_string()
                } else {
                    e.path.clone()
                };
                let edits = rename::edits_for(
                    &before,
                    &after,
                    &e.path,
                    &note_after,
                    &decoded.text,
                    from,
                    to,
                );
                (!edits.is_empty()).then(|| LinkUpdate {
                    note: e.path.clone(),
                    note_after,
                    bytes,
                    text: decoded.text,
                    edits,
                })
            })
            .collect()
    }

    /// What renaming `path` to `new_name` would change: the new path and every link update, line by line.
    ///
    /// # Errors
    /// `invalid_argument` for a bad or taken name or a path outside the vault.
    pub fn vault_rename_preview(&self, path: &str, new_name: &str) -> ApiResult<RenamePlan> {
        let store = self.vault()?;
        let to = Self::rename_target(store.as_ref(), path, new_name)?;
        let listing = store.list();
        let mut files: Vec<RenameFile> = Self::rename_edits(store.as_ref(), &listing, path, &to)
            .into_iter()
            .map(|u| RenameFile {
                changes: rename::line_changes(&u.text, &u.edits),
                path: u.note,
            })
            .collect();
        files.sort_by_key(|f| f.path.to_lowercase());
        Ok(RenamePlan {
            from: path.to_string(),
            to,
            files,
        })
    }

    /// Rename `path` to `new_name` (same folder, same extension) and, when `update_links`, rewrite the links to it.
    /// A note that changed after it was read is skipped, never overwritten.
    ///
    /// # Errors
    /// `invalid_argument` for a bad or taken name; `io_error` when the rename fails.
    pub fn vault_rename(
        &self,
        path: &str,
        new_name: &str,
        update_links: bool,
    ) -> ApiResult<RenameDone> {
        let store = self.vault()?;
        let to = Self::rename_target(store.as_ref(), path, new_name)?;
        let listing = store.list();
        let edits = if update_links {
            Self::rename_edits(store.as_ref(), &listing, path, &to)
        } else {
            Vec::new()
        };
        let case_only = to.to_lowercase() == path.to_lowercase();
        store.rename(path, paths::file_name(&to), case_only)?;
        let mut done = RenameDone {
            to: to.clone(),
            updated: Vec::new(),
            skipped: Vec::new(),
        };
        for u in edits {
            let bom = u.bytes.starts_with(text::BOM);
            let next = text::encode_raw(&rename::apply(&u.text, &u.edits), bom);
            let written = store.guarded_write(&u.note_after, &text::hash(&u.bytes), &next);
            match written {
                Ok(write::Outcome::Saved { .. }) => done.updated.push(u.note_after),
                _ => {
                    tracing::warn!(
                        "vault: links in a note were not updated (it changed or cannot be written)"
                    );
                    done.skipped.push(u.note_after);
                }
            }
        }
        let mut index = lock(&self.vault.index)?;
        index.invalidate(path);
        done.updated.iter().for_each(|p| index.invalidate(p));
        Ok(done)
    }
}
