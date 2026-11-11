//! Undo of the creator's pack-wide changes (spec 10.2 "Deleting an element is undone from a toast", 12.5 "One
//! undoable step"): a delete, a multi-select edit, a move to another pack, a duplicate. Each change keeps, in memory,
//! the content and `creator.json` of every pack it wrote as they were before; Undo writes them back as one change,
//! provided no pack changed since (its version is still the one the change wrote). Undone deletes come back with their
//! ids (un-retired, `creator.json` entries and their CS1 marks included); elements an undone change made (duplicates,
//! moved copies) are removed and their ids retired. The pack's `.trash` copies of deletes stay on disk as before.

use crate::content_dto::ContentPack;
use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::homebrew_files::PackWrite;
use crate::homebrew_packs::{CommitError, CreatorPack};
use crate::state::AppState;
use borealis_authoring::pack::{bump_patch, CreatorFile};
use borealis_content::ContentFile;
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::SystemTime;

/// How many changes can still be undone (the toast offers the last one; a few more survive quick work).
const KEPT: usize = 20;

/// One pack as a change found it.
#[derive(Debug, Clone)]
pub(crate) struct PackBefore {
    pub(crate) pack_id: String,
    /// The version the change wrote: Undo refuses when the pack changed since.
    pub(crate) version_after: String,
    pub(crate) content: ContentFile,
    pub(crate) creator: CreatorFile,
}

impl PackBefore {
    /// `pack` as it was, and the version `next` writes.
    pub(crate) fn of(pack: &CreatorPack, next: &PackWrite) -> Self {
        Self {
            pack_id: pack.id().to_string(),
            version_after: next.manifest.version.clone(),
            content: pack.disk.content.clone(),
            creator: pack.creator.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct Record {
    id: String,
    packs: Vec<PackBefore>,
}

/// The changes that can be undone, newest last.
#[derive(Debug, Default)]
pub struct CreatorUndo {
    records: Mutex<VecDeque<Record>>,
    seq: Mutex<u64>,
}

impl CreatorUndo {
    /// Keep a change; its id for the toast's Undo.
    pub(crate) fn record(&self, packs: Vec<PackBefore>) -> ApiResult<String> {
        let n = {
            let mut seq = self
                .seq
                .lock()
                .map_err(|_| ApiError::internal("undo lock"))?;
            *seq += 1;
            *seq
        };
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let id = format!("undo-{ms}-{n}");
        let mut records = self
            .records
            .lock()
            .map_err(|_| ApiError::internal("undo lock"))?;
        records.push_back(Record {
            id: id.clone(),
            packs,
        });
        while records.len() > KEPT {
            records.pop_front();
        }
        Ok(id)
    }

    fn take(&self, id: &str) -> ApiResult<Record> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| ApiError::internal("undo lock"))?;
        let at = records
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| ApiError::not_found("That change can't be undone any more."))?;
        records
            .remove(at)
            .ok_or_else(|| ApiError::internal("undo record"))
    }
}

/// The ids of a content file's elements.
fn ids(content: &ContentFile) -> HashSet<&str> {
    content.elements.iter().map(|e| e.id.as_str()).collect()
}

impl AppState {
    /// Undo a change (spec 10.2, 12.5): every pack it wrote goes back to its content and `creator.json` from before,
    /// as one change (version patch +1, fast reload). Elements the change made are removed and their ids retired.
    ///
    /// # Errors
    /// `not_found` for an unknown (or already undone) change; `requirement_failed` when a pack changed since (nothing
    /// is undone then); as `hb_pack` for a pack that is gone.
    pub fn homebrew_undo(&self, undo_id: &str, sink: &dyn EventSink) -> ApiResult<ContentPack> {
        let _guard = self.content_lock()?;
        let record = self.hb_undo.take(undo_id)?;
        let mut packs = Vec::new();
        for before in &record.packs {
            let pack = self.hb_pack(&before.pack_id)?;
            if pack.disk.manifest.version != before.version_after {
                return Err(ApiError::requirement(format!(
                    "\"{}\" changed since, so nothing was undone.",
                    pack.disk.manifest.name
                )));
            }
            packs.push(pack);
        }
        let restored: HashSet<&str> = record.packs.iter().flat_map(|b| ids(&b.content)).collect();
        let writes: Vec<PackWrite> = record
            .packs
            .iter()
            .zip(&packs)
            .map(|(before, pack)| {
                let made: Vec<&str> = ids(&pack.disk.content)
                    .into_iter()
                    .filter(|id| !restored.contains(id))
                    .collect();
                let mut manifest = pack.disk.manifest.clone();
                manifest.version = bump_patch(&manifest.version);
                PackWrite {
                    manifest,
                    content: before.content.clone(),
                    creator: before.creator.with_retired(made),
                    trashed: Vec::new(),
                }
            })
            .collect();
        let pairs: Vec<(&CreatorPack, &PackWrite)> = packs.iter().zip(&writes).collect();
        match self.hb_commit_all(&pairs, &[], sink) {
            Ok(()) => {}
            Err(CommitError::Api(e)) => return Err(e),
            Err(CommitError::Problems(_)) => {
                return Err(ApiError::internal("an undo failed a check"))
            }
        }
        let first = record
            .packs
            .first()
            .ok_or_else(|| ApiError::internal("an empty undo"))?;
        self.hb_pack_dto(&first.pack_id)
    }
}
