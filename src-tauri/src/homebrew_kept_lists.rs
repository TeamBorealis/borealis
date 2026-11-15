//! Unsaved SPELL LISTS checks kept on the device (P3 INW5c, spec 10.1, HC7): leaving the workspace keeps the pending
//! checks of a pack, and like a draft they survive a restart. One small JSON file per pack in
//! `<data>/homebrew-drafts/spell-lists/` (inside the drafts folder, so sync and bundles never carry it, and the draft
//! listing, which reads only `*.json` files directly in that folder, never takes it for a draft).

use crate::error::{ApiError, ApiResult};
use crate::homebrew_drafts::HB_DRAFT_DIR;
use crate::homebrew_dto::HomebrewKeptChecks;
use crate::homebrew_files::write_atomic;
use crate::state::AppState;
use std::path::PathBuf;

/// Folder (inside the drafts folder) of the kept checks.
const KEPT_DIR: &str = "spell-lists";
/// Longest pack id a file name is made from.
const MAX_PACK_ID: usize = 64;

/// Pack ids become file names, so only a safe alphabet is accepted.
fn check_pack_id(id: &str) -> ApiResult<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_PACK_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid("That is not a valid pack id."))
    }
}

impl AppState {
    fn hb_kept_file(&self, pack_id: &str) -> ApiResult<PathBuf> {
        check_pack_id(pack_id)?;
        Ok(self
            .paths
            .data
            .join(HB_DRAFT_DIR)
            .join(KEPT_DIR)
            .join(format!("{pack_id}.json")))
    }

    /// Keep a pack's unsaved SPELL LISTS checks (none, or no list: forget them).
    ///
    /// # Errors
    /// `invalid_argument` for a pack id that can't be a file name; `io_error` when the file can't be written or
    /// deleted.
    pub fn homebrew_keep_spell_list_checks(
        &self,
        pack_id: &str,
        kept: &HomebrewKeptChecks,
    ) -> ApiResult<()> {
        let file = self.hb_kept_file(pack_id)?;
        if kept.list.trim().is_empty() || kept.pending.is_empty() {
            return match std::fs::remove_file(&file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(ApiError::io("forget the unsaved spell list changes", e)),
            };
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| ApiError::io("keep the unsaved spell list changes", e))?;
        }
        let text = serde_json::to_string(kept)?;
        write_atomic(&file, &text)
            .map_err(|e| ApiError::io("keep the unsaved spell list changes", e))
    }

    /// The unsaved SPELL LISTS checks kept for a pack, if any. A file that can't be read counts as none (it is
    /// logged and left as it is).
    ///
    /// # Errors
    /// `invalid_argument` for a pack id that can't be a file name.
    pub fn homebrew_kept_spell_list_checks(
        &self,
        pack_id: &str,
    ) -> ApiResult<Option<HomebrewKeptChecks>> {
        let file = self.hb_kept_file(pack_id)?;
        let Ok(text) = std::fs::read_to_string(&file) else {
            return Ok(None);
        };
        match serde_json::from_str::<HomebrewKeptChecks>(&text) {
            Ok(k) => Ok(Some(k)),
            Err(e) => {
                tracing::warn!(
                    "kept spell list checks {} can't be read: {e}",
                    file.display()
                );
                Ok(None)
            }
        }
    }
}
