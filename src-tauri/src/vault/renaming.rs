//! Renaming a vault file, with the link updates the user confirmed ([`super::rename`] has the edit rules).

use super::dto::{RenameDone, RenameFile, RenamePlan};
use super::paths::{self, Vault};
use super::tree::{self, EntryKind, Listing};
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
    fn rename_target(
        vault: &Vault,
        path: &str,
        new_name: &str,
    ) -> ApiResult<(paths::Located, String)> {
        let found = vault.existing_file(path)?;
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
        let dir = found
            .path
            .parent()
            .ok_or_else(|| ApiError::invalid("That file is not in your vault."))?;
        let exact_taken = std::fs::read_dir(dir)
            .map_err(|e| ApiError::io("read the folder", e))?
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_str() == Some(name.as_str()));
        let taken =
            exact_taken || (!same_but_case && std::fs::symlink_metadata(dir.join(&name)).is_ok());
        if taken {
            return Err(ApiError::invalid("A file with that name is already there."));
        }
        Ok((found, to))
    }

    /// The link edits renaming `from` to `to` needs, per note.
    fn rename_edits(vault: &Vault, listing: &Listing, from: &str, to: &str) -> Vec<LinkUpdate> {
        let before = file_set(listing);
        let after = rename::renamed(&before, from, to);
        listing
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Note && e.size <= index::MAX_NOTE_BYTES)
            .filter_map(|e| {
                let real = vault.existing_file(&e.path).ok()?.real;
                let bytes = std::fs::read(real).ok()?;
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
        let vault = self.vault()?;
        let (_, to) = Self::rename_target(&vault, path, new_name)?;
        let listing = tree::list(&vault);
        let mut files: Vec<RenameFile> = Self::rename_edits(&vault, &listing, path, &to)
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
        let vault = self.vault()?;
        let (found, to) = Self::rename_target(&vault, path, new_name)?;
        let listing = tree::list(&vault);
        let edits = if update_links {
            Self::rename_edits(&vault, &listing, path, &to)
        } else {
            Vec::new()
        };
        let new_path = found.path.with_file_name(paths::file_name(&to));
        let case_only = to.to_lowercase() == path.to_lowercase();
        write::rename_no_clobber(&found.path, &new_path, case_only).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ApiError::invalid("A file with that name is already there.")
            } else {
                ApiError::io("rename the file", e)
            }
        })?;
        let mut done = RenameDone {
            to: to.clone(),
            updated: Vec::new(),
            skipped: Vec::new(),
        };
        for u in edits {
            let written = vault.existing_file(&u.note_after).ok().map(|f| {
                let bom = u.bytes.starts_with(text::BOM);
                let next = text::encode_raw(&rename::apply(&u.text, &u.edits), bom);
                write::guarded_write(&f.real, &text::hash(&u.bytes), &next)
            });
            match written {
                Some(Ok(write::Outcome::Saved { .. })) => done.updated.push(u.note_after),
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
