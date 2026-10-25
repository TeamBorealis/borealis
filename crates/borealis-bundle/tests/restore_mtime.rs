//! SY7a: an Android full-backup restore sets each file's mtime to its original seconds read as
//! milliseconds (January 1970). The startup repair multiplies those back, so newest-edit-wins (SY7,
//! edit time = file mtime) keeps working for restored characters and packs.

use borealis_bundle::{mtime_ms, repair_restored_mtimes};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 2026-10-25T10:00:00Z, the "now" of these tests.
const NOW_MS: u64 = 1_792_922_400_000;
/// 2026-10-20T08:30:15Z: the real edit time of a restored file, in seconds.
const EDITED_S: u64 = 1_792_485_015;

fn write(path: &Path, mtime_ms: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"{}").unwrap();
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_millis(mtime_ms))
        .unwrap();
}

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(NOW_MS)
}

#[test]
fn restored_files_under_characters_and_packs_get_their_edit_time_back() {
    let tmp = tempfile::tempdir().unwrap();
    let chars = tmp.path().join("characters");
    let library = tmp.path().join("library");
    let restored_char = chars.join("c_one.json");
    let restored_pack = library.join("pack-a/content/deep/elements.json");
    let fresh = chars.join("c_two.json");
    let ancient = library.join("pack-a/pack.json");
    let tiny = chars.join("c_three.json");
    // Restored: seconds read as milliseconds.
    write(&restored_char, EDITED_S);
    write(&restored_pack, EDITED_S + 60);
    // A normal recent edit stays.
    write(&fresh, NOW_MS - 3_600_000);
    // 1990-01-01T00:00:00Z: before 2000, but x 1000 is far in the future, so not a restore artefact.
    write(&ancient, 631_152_000_000);
    // 1970 with x 1000 still before 2020: left alone.
    write(&tiny, 1_000_000);

    let repaired = repair_restored_mtimes(&[chars.clone(), library.clone()], now());

    assert_eq!(repaired.repaired, 2);
    assert_eq!(repaired.failed, 0);
    assert_eq!(mtime_ms(&restored_char), EDITED_S * 1000);
    assert_eq!(mtime_ms(&restored_pack), (EDITED_S + 60) * 1000);
    assert_eq!(mtime_ms(&fresh), NOW_MS - 3_600_000);
    assert_eq!(mtime_ms(&ancient), 631_152_000_000);
    assert_eq!(mtime_ms(&tiny), 1_000_000);
    // Deterministic: a second run finds nothing left to repair.
    let again = repair_restored_mtimes(&[chars, library], now());
    assert_eq!(again.repaired, 0);
}

#[test]
fn files_outside_the_given_folders_are_not_touched_and_missing_folders_are_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("settings.json");
    write(&outside, EDITED_S);
    let report = repair_restored_mtimes(
        &[tmp.path().join("characters"), tmp.path().join("library")],
        now(),
    );
    assert_eq!((report.repaired, report.failed), (0, 0));
    assert_eq!(mtime_ms(&outside), EDITED_S);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let chars = tmp.path().join("characters");
    fs::create_dir_all(&chars).unwrap();
    let elsewhere = tmp.path().join("elsewhere/c_one.json");
    write(&elsewhere, EDITED_S);
    std::os::unix::fs::symlink(&elsewhere, chars.join("c_link.json")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("elsewhere"), chars.join("dir_link")).unwrap();
    let report = repair_restored_mtimes(&[chars], now());
    assert_eq!(report.repaired, 0);
    assert_eq!(mtime_ms(&elsewhere), EDITED_S);
}
