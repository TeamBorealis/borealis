//! Apply a receive [`Plan`] to this device (A1 gap, built for B3; A2's file import uses it too).
//!
//! Two phases, so a hostile or broken bundle never leaves a partial result:
//! 1. **Stage**: every item the plan writes is read (SHA-256 verified by the reader), its key is
//!    re-checked, and it is validated by the app's [`ItemValidator`] (character parser with
//!    `drop_unsafe_portrait`, pack content validator) into a scratch area. Any failure refuses the
//!    whole bundle and deletes the scratch area; nothing on the device changed.
//! 2. **Commit**: per item, the local copy (if any) moves to `<data>/sync/trash/` (30 days), then
//!    the staged copy is renamed into place and stamped with the incoming edit time.
//!
//! Peer-supplied names never become paths: keys must be plain ids ([`is_plain_key`]) and pack
//! files keep only the relative paths the reader already proved safe.

use crate::error::{BundleError, Result};
use crate::exclude::is_safe_rel_path;
use crate::manifest::ItemKind;
use crate::plan::{Action, Plan};
use crate::reader::Bundle;
use crate::TRASH_RETENTION_DAYS;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Longest key accepted on receive.
pub const MAX_KEY_LEN: usize = 64;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// Where a receive writes.
#[derive(Debug, Clone)]
pub struct ApplyTargets {
    /// `<documents>/characters`.
    pub characters_dir: PathBuf,
    /// `<data>/library`.
    pub library_dir: PathBuf,
    /// `<data>/sync/trash`.
    pub trash_dir: PathBuf,
    /// Scratch for staged packs, on the same filesystem as `library_dir` (`<data>/sync/staging`).
    pub staging_dir: PathBuf,
}

/// The app's validation of incoming items.
pub trait ItemValidator {
    /// Parse and validate a character; return the bytes to write (sanitised if needed).
    ///
    /// # Errors
    /// A user-facing reason; the whole bundle is then refused.
    fn character(&self, key: &str, bytes: &[u8]) -> std::result::Result<Vec<u8>, String>;

    /// Validate a pack staged at `dir` (all its files are there).
    ///
    /// # Errors
    /// As [`ItemValidator::character`].
    fn pack(&self, key: &str, dir: &Path) -> std::result::Result<(), String>;
}

/// What a receive did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Items written that were not on this device.
    pub added: Vec<(ItemKind, String)>,
    /// Items that replaced an older local copy (the old copy is in the trash).
    pub replaced: Vec<(ItemKind, String)>,
    /// Items kept because the local copy is newer or tied (incoming dropped).
    pub kept_local: Vec<(ItemKind, String)>,
    /// Items already identical.
    pub identical: Vec<(ItemKind, String)>,
}

/// A key that is safe as a file or folder name on every platform: ASCII letters, digits, `_`
/// and `-`, not starting with `-`, at most [`MAX_KEY_LEN`].
#[must_use]
pub fn is_plain_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && !key.starts_with('-')
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn rejected(kind: ItemKind, key: &str, why: impl std::fmt::Display) -> BundleError {
    let what = match kind {
        ItemKind::Character => "character",
        ItemKind::Pack => "pack",
    };
    BundleError::Rejected(format!("{what} `{key}`: {why}"))
}

enum Staged {
    Character { key: String, bytes: Vec<u8> },
    Pack { key: String, dir: PathBuf },
}

/// Apply `plan` (made from `bundle`'s manifest) to `targets`. Returns what happened.
///
/// # Errors
/// [`BundleError::Rejected`] when an item fails validation (nothing written), I/O errors during
/// the commit (items committed before the error stay; none is half-written).
pub fn apply_plan<R: Read + Seek>(
    bundle: &mut Bundle<R>,
    plan: &Plan,
    targets: &ApplyTargets,
    validator: &dyn ItemValidator,
) -> Result<ApplyReport> {
    fs::create_dir_all(&targets.staging_dir)
        .map_err(|e| BundleError::io_at(&targets.staging_dir, e))?;
    let scratch = tempfile::Builder::new()
        .prefix("recv-")
        .tempdir_in(&targets.staging_dir)?;
    let mut staged = Vec::new();
    let mut report = ApplyReport::default();
    for entry in &plan.entries {
        let id = (entry.kind, entry.key.clone());
        match entry.action {
            Action::Identical => report.identical.push(id),
            Action::KeepLocalNewer | Action::KeepLocalTie => report.kept_local.push(id),
            Action::New | Action::ReplaceWithIncoming => {
                staged.push((
                    stage(bundle, entry.kind, &entry.key, scratch.path(), validator)?,
                    entry.action,
                    entry.incoming_edited_at_ms,
                ));
            }
        }
    }
    for (item, action, edited_ms) in staged {
        let id = commit(item, targets, edited_ms)?;
        match action {
            Action::ReplaceWithIncoming => report.replaced.push(id),
            _ => report.added.push(id),
        }
    }
    Ok(report)
}

fn stage<R: Read + Seek>(
    bundle: &mut Bundle<R>,
    kind: ItemKind,
    key: &str,
    scratch: &Path,
    validator: &dyn ItemValidator,
) -> Result<Staged> {
    if !is_plain_key(key) {
        return Err(rejected(kind, key, "not a valid id"));
    }
    let data = bundle.read_item(kind, key)?;
    match kind {
        ItemKind::Character => {
            let [(_, bytes)] = data.files.as_slice() else {
                return Err(rejected(kind, key, "a character is exactly one file"));
            };
            let bytes = validator
                .character(key, bytes)
                .map_err(|why| rejected(kind, key, why))?;
            Ok(Staged::Character {
                key: key.to_string(),
                bytes,
            })
        }
        ItemKind::Pack => {
            let dir = scratch.join(key);
            let prefix = format!("packs/{key}/");
            for (path, bytes) in &data.files {
                let rel = path
                    .strip_prefix(&prefix)
                    .filter(|r| is_safe_rel_path(r, bundle.limits().max_depth))
                    .ok_or_else(|| rejected(kind, key, "unsafe file path"))?;
                let out = dir.join(rel);
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent).map_err(|e| BundleError::io_at(parent, e))?;
                }
                fs::write(&out, bytes).map_err(|e| BundleError::io_at(&out, e))?;
            }
            validator
                .pack(key, &dir)
                .map_err(|why| rejected(kind, key, why))?;
            Ok(Staged::Pack {
                key: key.to_string(),
                dir,
            })
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn stamp(path: &Path, edited_ms: u64) -> Result<()> {
    let when = UNIX_EPOCH + Duration::from_millis(edited_ms);
    let file = fs::File::options()
        .write(true)
        .open(path)
        .map_err(|e| BundleError::io_at(path, e))?;
    file.set_modified(when)
        .map_err(|e| BundleError::io_at(path, e))
}

fn stamp_tree(dir: &Path, edited_ms: u64) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| BundleError::io_at(dir, e))? {
        let path = entry?.path();
        if path.is_dir() {
            stamp_tree(&path, edited_ms)?;
        } else {
            stamp(&path, edited_ms)?;
        }
    }
    Ok(())
}

fn to_trash(path: &Path, trash: &Path, name: &str) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    fs::create_dir_all(trash).map_err(|e| BundleError::io_at(trash, e))?;
    let dest = trash.join(format!("{}-{name}", now_ms()));
    move_path(path, &dest)
}

/// Rename, falling back to copy + delete across filesystems (the documents folder can live on
/// another volume than the data folder).
fn move_path(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_tree(from, to)?;
    if from.is_dir() {
        fs::remove_dir_all(from)
    } else {
        fs::remove_file(from)
    }
    .map_err(|e| BundleError::io_at(from, e))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to).map_err(|e| BundleError::io_at(to, e))?;
        for entry in fs::read_dir(from).map_err(|e| BundleError::io_at(from, e))? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| BundleError::io_at(from, e))
    }
}

fn commit(item: Staged, t: &ApplyTargets, edited_ms: u64) -> Result<(ItemKind, String)> {
    match item {
        Staged::Character { key, bytes } => {
            fs::create_dir_all(&t.characters_dir)
                .map_err(|e| BundleError::io_at(&t.characters_dir, e))?;
            let target = t.characters_dir.join(format!("{key}.json"));
            // Temp file in the target folder: the final rename is atomic on every platform.
            let mut tmp = tempfile::Builder::new()
                .prefix(".sync-")
                .suffix(".tmp")
                .tempfile_in(&t.characters_dir)?;
            std::io::Write::write_all(&mut tmp, &bytes)?;
            tmp.as_file().sync_all()?;
            stamp(tmp.path(), edited_ms)?;
            to_trash(&target, &t.trash_dir, &format!("{key}.json"))?;
            tmp.persist(&target)
                .map_err(|e| BundleError::io_at(&target, e.error))?;
            Ok((ItemKind::Character, key))
        }
        Staged::Pack { key, dir } => {
            stamp_tree(&dir, edited_ms)?;
            fs::create_dir_all(&t.library_dir)
                .map_err(|e| BundleError::io_at(&t.library_dir, e))?;
            let target = t.library_dir.join(&key);
            to_trash(&target, &t.trash_dir, &key)?;
            move_path(&dir, &target)?;
            Ok((ItemKind::Pack, key))
        }
    }
}

/// Delete trash entries older than [`TRASH_RETENTION_DAYS`] (by the time stamp in their name).
/// Returns how many were removed.
///
/// # Errors
/// I/O failure listing the folder (a missing folder is fine).
pub fn sweep_trash(trash_dir: &Path, now_ms: u64) -> Result<usize> {
    let Ok(entries) = fs::read_dir(trash_dir) else {
        return Ok(0);
    };
    let cutoff = now_ms.saturating_sub(TRASH_RETENTION_DAYS * DAY_MS);
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let stamp = name
            .to_str()
            .and_then(|n| n.split_once('-'))
            .and_then(|(ms, _)| ms.parse::<u64>().ok());
        if stamp.is_some_and(|ms| ms < cutoff) {
            let path = entry.path();
            let gone = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if gone.is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_keys_only() {
        for ok in ["c_0123abcd", "homebrew-pack", "Pack_2"] {
            assert!(is_plain_key(ok), "{ok}");
        }
        for bad in ["", "-x", "a b", "a/b", "..", "a.json", "é", &"x".repeat(65)] {
            assert!(!is_plain_key(bad), "{bad}");
        }
    }

    #[test]
    fn sweep_removes_only_old_entries() {
        let dir = tempfile::tempdir().unwrap();
        let now = 100 * DAY_MS;
        let old = now - 31 * DAY_MS;
        let young = now - DAY_MS;
        fs::write(dir.path().join(format!("{old}-c_a.json")), b"x").unwrap();
        fs::create_dir(dir.path().join(format!("{old}-pack"))).unwrap();
        fs::write(dir.path().join(format!("{young}-c_b.json")), b"x").unwrap();
        fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        assert_eq!(sweep_trash(dir.path(), now).unwrap(), 2);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        assert_eq!(sweep_trash(&dir.path().join("missing"), now).unwrap(), 0);
    }
}
