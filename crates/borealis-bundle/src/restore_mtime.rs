//! SY7a: repair edit times after an Android full-backup restore.
//!
//! Sync takes a file's modification time as its edit time (SY7). Android's full-backup restore sets
//! each restored file's mtime to the original seconds read as milliseconds, which lands in January
//! 1970 and would make every restored character or pack lose newest-edit-wins. The app calls
//! [`repair_restored_mtimes`] at startup on Android; the rule itself is [`restored_mtime_fix`].

use crate::inventory::mtime_ms;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 2000-01-01T00:00:00Z in Unix milliseconds: only mtimes before this are candidates.
pub const RESTORE_CANDIDATE_BEFORE_MS: u64 = 946_684_800_000;
/// 2020-01-01T00:00:00Z in Unix milliseconds: the earliest repaired edit time.
pub const RESTORE_REPAIRED_FROM_MS: u64 = 1_577_836_800_000;
/// How far past "now" a repaired edit time may land (clock skew between devices).
pub const RESTORE_FUTURE_SLACK_MS: u64 = 86_400_000;

/// The repaired mtime (Unix ms) for a file whose `mtime_ms` looks like seconds read as
/// milliseconds, or `None` to leave it as it is.
#[must_use]
pub fn restored_mtime_fix(mtime_ms: u64, now_ms: u64) -> Option<u64> {
    if mtime_ms >= RESTORE_CANDIDATE_BEFORE_MS {
        return None;
    }
    let fixed = mtime_ms.checked_mul(1000)?;
    let latest = now_ms.saturating_add(RESTORE_FUTURE_SLACK_MS);
    (RESTORE_REPAIRED_FROM_MS..=latest)
        .contains(&fixed)
        .then_some(fixed)
}

/// What one repair pass did (counts only: file and character names are never reported).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreRepair {
    /// Files whose mtime was set back to the real edit time.
    pub repaired: usize,
    /// Files that needed a repair but could not be stamped.
    pub failed: usize,
}

/// Apply [`restored_mtime_fix`] to every regular file under `dirs` (recursively, symlinks are not
/// followed). Missing folders are empty.
#[must_use]
pub fn repair_restored_mtimes(dirs: &[PathBuf], now: SystemTime) -> RestoreRepair {
    let now_ms = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let mut report = RestoreRepair::default();
    for dir in dirs {
        repair_dir(dir, now_ms, &mut report);
    }
    report
}

fn repair_dir(dir: &Path, now_ms: u64, report: &mut RestoreRepair) {
    // A missing or unreadable folder has nothing to repair.
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            repair_dir(&path, now_ms, report);
        } else if meta.is_file() {
            if let Some(fixed) = restored_mtime_fix(mtime_ms(&path), now_ms) {
                match stamp(&path, fixed) {
                    Ok(()) => report.repaired += 1,
                    Err(_) => report.failed += 1,
                }
            }
        }
    }
}

fn stamp(path: &Path, when_ms: u64) -> std::io::Result<()> {
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(UNIX_EPOCH + Duration::from_millis(when_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = RESTORE_FUTURE_SLACK_MS;
    /// 2026-10-25T10:00:00Z.
    const NOW: u64 = 1_792_922_400_000;

    #[test]
    fn seconds_read_as_milliseconds_are_multiplied_back() {
        assert_eq!(
            restored_mtime_fix(1_792_485_015, NOW),
            Some(1_792_485_015_000)
        );
    }

    #[test]
    fn the_repaired_time_may_start_exactly_at_2020() {
        let from_s = RESTORE_REPAIRED_FROM_MS / 1000;
        assert_eq!(
            restored_mtime_fix(from_s, NOW),
            Some(RESTORE_REPAIRED_FROM_MS)
        );
        assert_eq!(restored_mtime_fix(from_s - 1, NOW), None);
    }

    #[test]
    fn the_repaired_time_may_reach_now_plus_one_day_but_not_beyond() {
        let edge_s = (NOW + DAY) / 1000;
        assert_eq!(edge_s * 1000, NOW + DAY, "the test needs an exact edge");
        assert_eq!(restored_mtime_fix(edge_s, NOW), Some(NOW + DAY));
        assert_eq!(restored_mtime_fix(edge_s + 1, NOW), None);
        assert_eq!(restored_mtime_fix(edge_s, NOW - 1), None);
    }

    #[test]
    fn only_mtimes_before_2000_are_candidates() {
        // With a far "now" the x 1000 range check passes, so only the before-2000 rule decides.
        let far_now = u64::MAX / 2;
        assert_eq!(
            restored_mtime_fix(RESTORE_CANDIDATE_BEFORE_MS - 1, far_now),
            Some((RESTORE_CANDIDATE_BEFORE_MS - 1) * 1000)
        );
        assert_eq!(
            restored_mtime_fix(RESTORE_CANDIDATE_BEFORE_MS, far_now),
            None
        );
        assert_eq!(restored_mtime_fix(NOW, NOW), None);
    }

    #[test]
    fn the_epoch_and_real_old_dates_stay() {
        assert_eq!(restored_mtime_fix(0, NOW), None);
        // 1990-01-01: a genuine old file; x 1000 is centuries ahead.
        assert_eq!(restored_mtime_fix(631_152_000_000, NOW), None);
    }

    #[test]
    fn a_huge_now_cannot_overflow() {
        assert_eq!(
            restored_mtime_fix(1_792_485_015, u64::MAX),
            Some(1_792_485_015_000)
        );
    }
}
