//! "Import anything" drafts: a memory cache over one JSON file per draft in the data folder, so a
//! draft survives an app restart.

use crate::error::{ApiError, ApiResult};
use crate::state::AppPaths;
use borealis_assist::import::anything::review::ReviewOverrides;
use borealis_assist::ImportDraft;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

/// Folder (inside the data folder) holding the draft files.
pub const DRAFT_DIR: &str = "import-drafts";

/// One draft and where its run stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredDraft {
    /// Last stage reached (`detect` ... `ready`, or `error`).
    pub stage: String,
    /// The draft once detection and planning produced one.
    pub draft: Option<ImportDraft>,
    /// Why the run stopped, when it did.
    pub error: Option<String>,
    /// The user's review decisions (accepted elements, text-only overrides).
    #[serde(default, skip_serializing_if = "is_default")]
    pub review: ReviewOverrides,
}

fn is_default(r: &ReviewOverrides) -> bool {
    *r == ReviewOverrides::default()
}

impl StoredDraft {
    /// A run that has just started.
    #[must_use]
    pub fn started() -> Self {
        Self {
            stage: "detect".into(),
            draft: None,
            error: None,
            review: ReviewOverrides::default(),
        }
    }
}

/// Drafts by id.
#[derive(Debug, Default)]
pub struct DraftStore {
    cache: Mutex<HashMap<String, StoredDraft>>,
    running: Mutex<HashSet<String>>,
}

/// Draft ids become file names, so only a safe alphabet is accepted.
///
/// # Errors
/// `invalid_argument` for anything else.
pub fn check_draft_id(id: &str) -> ApiResult<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid("That is not a valid import id."))
    }
}

fn file_of(paths: &AppPaths, id: &str) -> PathBuf {
    paths.data.join(DRAFT_DIR).join(format!("{id}.json"))
}

impl DraftStore {
    /// The draft, from memory or else from disk.
    ///
    /// # Errors
    /// `invalid_argument` for a bad id, `not_found` when no such draft exists.
    pub fn get(&self, paths: &AppPaths, id: &str) -> ApiResult<StoredDraft> {
        check_draft_id(id)?;
        if let Some(d) = self.cache.lock()?.get(id) {
            return Ok(d.clone());
        }
        let text = std::fs::read_to_string(file_of(paths, id))
            .map_err(|_| ApiError::not_found("That import no longer exists."))?;
        let mut stored: StoredDraft = serde_json::from_str(&text)?;
        if stored.draft.is_none() && stored.error.is_none() {
            stored.stage = "error".into();
            stored.error = Some("This import was interrupted. Start it again.".into());
        }
        self.cache.lock()?.insert(id.to_string(), stored.clone());
        Ok(stored)
    }

    /// Store (and persist) a draft.
    ///
    /// # Errors
    /// `io_error` when the file cannot be written.
    pub fn put(&self, paths: &AppPaths, id: &str, stored: StoredDraft) -> ApiResult<()> {
        check_draft_id(id)?;
        let dir = paths.data.join(DRAFT_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("save the import draft", e))?;
        let text = serde_json::to_string(&stored)?;
        let tmp = dir.join(format!("{id}.tmp"));
        std::fs::write(&tmp, text)
            .and_then(|()| borealis_character::format::replace_file(&tmp, &file_of(paths, id)))
            .map_err(|e| ApiError::io("save the import draft", e))?;
        self.cache.lock()?.insert(id.to_string(), stored);
        Ok(())
    }

    /// Mark a draft as being worked on.
    ///
    /// # Errors
    /// `requirement_failed` when it already is.
    pub fn claim(&self, id: &str) -> ApiResult<()> {
        if self.running.lock()?.insert(id.to_string()) {
            Ok(())
        } else {
            Err(ApiError::requirement(
                "The AI is still working on this import.",
            ))
        }
    }

    /// Mark a draft as idle again.
    pub fn release(&self, id: &str) {
        if let Ok(mut r) = self.running.lock() {
            r.remove(id);
        }
    }

    /// Whether a run is in progress.
    #[must_use]
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().is_ok_and(|r| r.contains(id))
    }

    /// Forget a draft and delete its file.
    ///
    /// # Errors
    /// `invalid_argument` for a bad id.
    pub fn remove(&self, paths: &AppPaths, id: &str) -> ApiResult<()> {
        check_draft_id(id)?;
        self.cache.lock()?.remove(id);
        let _ = std::fs::remove_file(file_of(paths, id));
        Ok(())
    }
}
