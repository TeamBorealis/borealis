//! Homebrew creator drafts (spec 10.1, HC7): the working copy of one element (with its children), one JSON file per
//! draft in `<data>/homebrew-drafts/`, so a draft survives a restart or a crash. Drafts stay on the device:
//! `homebrew-drafts` is on `borealis_bundle::is_excluded`, so sync and bundles never carry them.
//!
//! The file holds the **full** form (`FormState` with its memo: the element id, the stored bytes of every field and
//! kept part), so a draft writes back exactly what it read; the UI only ever sees `to_ui` of it.
//!
//! A draft file is written by the first edit (`homebrew_update_draft`). Opening an element only to look at it keeps
//! its draft in memory and writes nothing, so "Unsaved changes" means an edit was made.

use crate::error::{ApiError, ApiResult};
use crate::homebrew_files::write_atomic;
use borealis_authoring::form::{CopyExtras, FormState, ProvisionalClass};
use borealis_authoring::pack::CopiedFrom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Folder (inside the data folder) holding the draft files.
pub const HB_DRAFT_DIR: &str = "homebrew-drafts";
/// The `format` of a draft file.
pub const HB_DRAFT_FORMAT: &str = "borealis-hb-draft/1";
const MAX_ID: usize = 64;

/// One draft as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftFile {
    /// [`HB_DRAFT_FORMAT`].
    pub format: String,
    pub draft_id: String,
    pub pack_id: String,
    /// The saved element it edits; `None` for a new element until its first save.
    #[serde(default)]
    pub element_id: Option<String>,
    /// The creator form.
    pub kind: String,
    /// The full form (memo included).
    pub form: FormState,
    /// Ids of the saved elements the draft started from (the element, then its children); empty for a new element.
    #[serde(default)]
    pub base_ids: Vec<String>,
    /// `borealis_authoring::pack::digest` of those elements when the draft started.
    #[serde(default)]
    pub base_digest: String,
    /// RFC 3339 time of the last edit.
    pub updated_at: String,
    /// A copy: where it came from (written to `creator.json` on save, spec 11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copied_from: Option<CopiedFrom>,
    /// A copy: the elements saved with it, where each copied element came from, the choices offered by copied
    /// options (spec 11).
    #[serde(default, skip_serializing_if = "CopyExtras::is_empty")]
    pub copy: CopyExtras,
    /// A new class: the provisional ids it was offered under (to a subclass's class picker) before its first save; a
    /// subclass that picked one follows the class to its saved id (P3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provisional: Vec<ProvisionalClass>,
    /// When the draft was made (nanoseconds since 1970; 0 for a draft made before this was kept): new drafts of one
    /// pack are given their provisional ids in this order, so a newer draft never takes an older one's (INW8I).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created: u64,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if passes a reference
fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Now, as [`DraftFile::created`].
#[must_use]
pub fn created_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

#[derive(Debug, Clone)]
struct Held {
    draft: DraftFile,
    /// The draft has a file (it was edited).
    persisted: bool,
}

/// The drafts of this device: a memory cache over the files.
#[derive(Debug, Default)]
pub struct CreatorDrafts {
    held: Mutex<HashMap<String, Held>>,
}

/// Draft ids become file names, so only a safe alphabet is accepted.
///
/// # Errors
/// `invalid_argument` for anything else.
pub fn check_draft_id(id: &str) -> ApiResult<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid("That is not a valid draft id."))
    }
}

/// A new draft id (`hb-` + 32 hex digits).
#[must_use]
pub fn new_draft_id() -> String {
    format!("hb-{}", uuid::Uuid::new_v4().simple())
}

fn dir_of(data: &Path) -> PathBuf {
    data.join(HB_DRAFT_DIR)
}

fn file_of(data: &Path, id: &str) -> PathBuf {
    dir_of(data).join(format!("{id}.json"))
}

fn gone() -> ApiError {
    ApiError::not_found("That draft was closed.")
}

/// Read one draft file. A file that can't be read is set aside as `<id>.json.bad` (kept, never deleted) and the
/// draft counts as gone.
fn read_file(data: &Path, id: &str) -> Option<DraftFile> {
    let path = file_of(data, id);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<DraftFile>(&text) {
        Ok(d) if d.format == HB_DRAFT_FORMAT && d.draft_id == id => Some(d),
        Ok(_) => {
            set_aside(&path, "another format or a different draft id");
            None
        }
        Err(e) => {
            set_aside(&path, &e.to_string());
            None
        }
    }
}

fn set_aside(path: &Path, why: &str) {
    tracing::warn!(
        "homebrew draft {} can't be read ({why}); kept as .bad",
        path.display()
    );
    let mut bad = path.as_os_str().to_owned();
    bad.push(".bad");
    let _ = std::fs::rename(path, PathBuf::from(bad));
}

impl CreatorDrafts {
    /// Hold a draft in memory only (an element opened to look at, a new element before its first edit).
    ///
    /// # Errors
    /// `internal` when the lock is poisoned.
    pub fn hold(&self, draft: DraftFile) -> ApiResult<()> {
        check_draft_id(&draft.draft_id)?;
        self.held.lock()?.insert(
            draft.draft_id.clone(),
            Held {
                draft,
                persisted: false,
            },
        );
        Ok(())
    }

    /// The draft, from memory or else from its file.
    ///
    /// # Errors
    /// `invalid_argument` for a bad id, `not_found` when there is no such draft (or its file can't be read).
    pub fn get(&self, data: &Path, id: &str) -> ApiResult<DraftFile> {
        check_draft_id(id)?;
        if let Some(h) = self.held.lock()?.get(id) {
            return Ok(h.draft.clone());
        }
        let draft = read_file(data, id).ok_or_else(gone)?;
        self.held.lock()?.insert(
            id.to_string(),
            Held {
                draft: draft.clone(),
                persisted: true,
            },
        );
        Ok(draft)
    }

    /// Store a draft and write its file (atomic: an interrupted write keeps the previous version).
    ///
    /// # Errors
    /// `io_error` when the file cannot be written (the previous version stays, in memory and on disk).
    pub fn put(&self, data: &Path, draft: DraftFile) -> ApiResult<()> {
        check_draft_id(&draft.draft_id)?;
        let dir = dir_of(data);
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("keep the draft", e))?;
        let text = serde_json::to_string(&draft)?;
        write_atomic(&file_of(data, &draft.draft_id), &text)
            .map_err(|e| ApiError::io("keep the draft", e))?;
        self.held.lock()?.insert(
            draft.draft_id.clone(),
            Held {
                draft,
                persisted: true,
            },
        );
        Ok(())
    }

    /// Forget a draft and delete its file.
    ///
    /// # Errors
    /// `invalid_argument` for a bad id.
    pub fn remove(&self, data: &Path, id: &str) -> ApiResult<()> {
        check_draft_id(id)?;
        self.held.lock()?.remove(id);
        match std::fs::remove_file(file_of(data, id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ApiError::io("delete the draft", e)),
        }
    }

    /// Every draft that has a file (edited drafts), newest first. Unreadable files are set aside.
    ///
    /// # Errors
    /// `internal` when the lock is poisoned.
    pub fn list(&self, data: &Path) -> ApiResult<Vec<DraftFile>> {
        let mut out: HashMap<String, DraftFile> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(dir_of(data)) {
            for path in entries.filter_map(Result::ok).map(|e| e.path()) {
                let Some(id) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_suffix(".json"))
                    .filter(|id| check_draft_id(id).is_ok())
                else {
                    continue;
                };
                if let Some(d) = read_file(data, id) {
                    out.insert(id.to_string(), d);
                }
            }
        }
        for h in self.held.lock()?.values().filter(|h| h.persisted) {
            if file_of(data, &h.draft.draft_id).is_file() {
                out.insert(h.draft.draft_id.clone(), h.draft.clone());
            }
        }
        let mut list: Vec<DraftFile> = out.into_values().collect();
        list.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then(a.draft_id.cmp(&b.draft_id))
        });
        Ok(list)
    }

    /// The edited draft of a saved element, if there is one (the newest).
    ///
    /// # Errors
    /// As [`CreatorDrafts::list`].
    pub fn for_element(
        &self,
        data: &Path,
        pack_id: &str,
        element_id: &str,
    ) -> ApiResult<Option<DraftFile>> {
        Ok(self
            .list(data)?
            .into_iter()
            .find(|d| d.pack_id == pack_id && d.element_id.as_deref() == Some(element_id)))
    }

    /// Drop every draft (held or on disk) of these saved elements (they were deleted).
    ///
    /// # Errors
    /// As [`CreatorDrafts::list`].
    pub fn remove_for_elements(&self, data: &Path, pack_id: &str, ids: &[String]) -> ApiResult<()> {
        let mut doomed: Vec<String> = self
            .list(data)?
            .into_iter()
            .filter(|d| {
                d.pack_id == pack_id && d.element_id.as_ref().is_some_and(|e| ids.contains(e))
            })
            .map(|d| d.draft_id)
            .collect();
        doomed.extend(
            self.held
                .lock()?
                .values()
                .filter(|h| {
                    h.draft.pack_id == pack_id
                        && h.draft.element_id.as_ref().is_some_and(|e| ids.contains(e))
                })
                .map(|h| h.draft.draft_id.clone()),
        );
        for id in doomed {
            self.remove(data, &id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(id: &str, at: &str) -> DraftFile {
        DraftFile {
            format: HB_DRAFT_FORMAT.into(),
            draft_id: id.into(),
            pack_id: "homebrew-gh".into(),
            element_id: Some("ID_GH_LANGUAGE_A".into()),
            kind: "language".into(),
            form: FormState::new("e", "language"),
            base_ids: vec!["ID_GH_LANGUAGE_A".into()],
            base_digest: "x".into(),
            updated_at: at.into(),
            copied_from: None,
            copy: CopyExtras::default(),
            provisional: Vec::new(),
            created: 0,
        }
    }

    #[test]
    fn a_held_draft_is_not_listed_until_it_is_written() {
        let data = tempfile::tempdir().unwrap();
        let store = CreatorDrafts::default();
        store.hold(draft("hb-1", "2026-11-10T10:00:00Z")).unwrap();
        assert!(store.list(data.path()).unwrap().is_empty());
        assert!(!data.path().join(HB_DRAFT_DIR).exists(), "nothing written");
        store
            .put(data.path(), draft("hb-1", "2026-11-10T10:00:01Z"))
            .unwrap();
        assert_eq!(store.list(data.path()).unwrap().len(), 1);
        // a new process (empty memory) reads it back from its file
        let fresh = CreatorDrafts::default();
        let back = fresh.get(data.path(), "hb-1").unwrap();
        assert_eq!(back, draft("hb-1", "2026-11-10T10:00:01Z"));
        assert!(fresh
            .for_element(data.path(), "homebrew-gh", "ID_GH_LANGUAGE_A")
            .unwrap()
            .is_some());
        fresh.remove(data.path(), "hb-1").unwrap();
        assert!(fresh.list(data.path()).unwrap().is_empty());
        assert_eq!(
            fresh.get(data.path(), "hb-1").unwrap_err().code,
            crate::error::ErrorCode::NotFound
        );
    }

    #[test]
    fn a_damaged_draft_file_is_set_aside_not_deleted() {
        let data = tempfile::tempdir().unwrap();
        let store = CreatorDrafts::default();
        store
            .put(data.path(), draft("hb-ok", "2026-11-10T10:00:00Z"))
            .unwrap();
        let dir = data.path().join(HB_DRAFT_DIR);
        std::fs::write(dir.join("hb-bad.json"), "{ half a dra").unwrap();
        std::fs::write(
            dir.join("hb-other.json"),
            serde_json::to_string(&draft("hb-x", "t")).unwrap(),
        )
        .unwrap();
        let fresh = CreatorDrafts::default();
        let listed: Vec<String> = fresh
            .list(data.path())
            .unwrap()
            .into_iter()
            .map(|d| d.draft_id)
            .collect();
        assert_eq!(listed, ["hb-ok"]);
        assert!(dir.join("hb-bad.json.bad").is_file(), "kept for the user");
        assert!(
            dir.join("hb-other.json.bad").is_file(),
            "a file whose id doesn't match its name"
        );
        assert_eq!(
            fresh.get(data.path(), "hb-bad").unwrap_err().code,
            crate::error::ErrorCode::NotFound
        );
    }

    #[test]
    fn draft_ids_are_file_name_safe() {
        assert!(check_draft_id(&new_draft_id()).is_ok());
        for bad in ["", "../x", "a/b", "a.json", &"x".repeat(65)] {
            assert!(check_draft_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn deleting_elements_drops_their_drafts() {
        let data = tempfile::tempdir().unwrap();
        let store = CreatorDrafts::default();
        store.put(data.path(), draft("hb-1", "t1")).unwrap();
        store.hold(draft("hb-2", "t2")).unwrap();
        let mut other = draft("hb-3", "t3");
        other.element_id = Some("ID_GH_LANGUAGE_B".into());
        store.put(data.path(), other).unwrap();
        store
            .remove_for_elements(data.path(), "homebrew-gh", &["ID_GH_LANGUAGE_A".into()])
            .unwrap();
        let left: Vec<String> = store
            .list(data.path())
            .unwrap()
            .into_iter()
            .map(|d| d.draft_id)
            .collect();
        assert_eq!(left, ["hb-3"]);
        assert!(store.get(data.path(), "hb-2").is_err());
    }
}
