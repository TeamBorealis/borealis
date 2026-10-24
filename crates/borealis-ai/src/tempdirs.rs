//! Cleanup of session temp dirs left behind by a hard kill.
//!
//! Sessions create `borealis-ai-work-*` and `borealis-ai-support-*` (0700) under the system temp
//! dir and remove them on drop. If the app is killed, they leak, and the support dir may still
//! hold attachment images. Call [`sweep_stale_temp_dirs`] once at startup.

use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Prefixes of the dirs a session creates.
pub const TEMP_DIR_PREFIXES: [&str; 2] = ["borealis-ai-work-", "borealis-ai-support-"];

/// Dirs younger than this may belong to a running session and are kept.
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Remove stale session dirs under [`std::env::temp_dir`]. Returns how many were removed.
/// Errors on single entries are logged and skipped.
pub fn sweep_stale_temp_dirs() -> usize {
    match sweep_stale_in(&std::env::temp_dir(), STALE_AFTER, SystemTime::now()) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %crate::redact_paths(&e.to_string()), "temp dir sweep failed");
            0
        }
    }
}

/// [`sweep_stale_temp_dirs`] for `root`, removing matching dirs last modified more than
/// `max_age` before `now`. Only real directories (never symlinks) owned by the current user are
/// touched.
///
/// # Errors
/// When `root` cannot be listed.
pub fn sweep_stale_in(root: &Path, max_age: Duration, now: SystemTime) -> io::Result<usize> {
    let owner = current_owner(root)?;
    let mut removed = 0;
    for entry in fs::read_dir(root)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !TEMP_DIR_PREFIXES.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let path = entry.path();
        // symlink_metadata: a symlink is reported as a symlink, never as its target.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_dir() || !owned_by(&meta, owner) {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok());
        if age.is_none_or(|a| a < max_age) {
            continue;
        }
        // remove_dir_all does not follow symlinks inside the tree.
        match fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(e) => tracing::debug!(error = %e, "could not remove a stale ai temp dir"),
        }
    }
    if removed > 0 {
        tracing::info!(removed, "removed stale ai temp dirs");
    }
    Ok(removed)
}

/// The uid that owns files this process creates in `root` (no libc needed).
#[cfg(unix)]
fn current_owner(root: &Path) -> io::Result<Option<u32>> {
    use std::os::unix::fs::MetadataExt;
    let probe = tempfile::Builder::new()
        .prefix("borealis-ai-probe-")
        .tempfile_in(root)?;
    Ok(Some(probe.as_file().metadata()?.uid()))
}

#[cfg(not(unix))]
fn current_owner(_root: &Path) -> io::Result<Option<u32>> {
    // The Windows temp dir is per user already.
    Ok(None)
}

#[cfg(unix)]
fn owned_by(meta: &fs::Metadata, owner: Option<u32>) -> bool {
    use std::os::unix::fs::MetadataExt;
    owner == Some(meta.uid())
}

#[cfg(not(unix))]
fn owned_by(_meta: &fs::Metadata, _owner: Option<u32>) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATER: Duration = Duration::from_secs(2 * 60 * 60);

    #[test]
    fn stale_session_dirs_are_removed_and_others_kept() {
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join("borealis-ai-work-abc");
        let support = root.path().join("borealis-ai-support-def");
        let other = root.path().join("unrelated-dir");
        for d in [&work, &support, &other] {
            fs::create_dir(d).unwrap();
            fs::write(d.join("f"), "x").unwrap();
        }
        // Fresh: nothing removed.
        assert_eq!(
            sweep_stale_in(root.path(), STALE_AFTER, SystemTime::now()).unwrap(),
            0
        );
        // Two hours later: both session dirs go, the unrelated one stays.
        let n = sweep_stale_in(root.path(), STALE_AFTER, SystemTime::now() + LATER).unwrap();
        assert_eq!(n, 2);
        assert!(!work.exists() && !support.exists() && other.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        let root = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(target.path().join("keep"), "x").unwrap();
        let link = root.path().join("borealis-ai-work-link");
        std::os::unix::fs::symlink(target.path(), &link).unwrap();
        let n = sweep_stale_in(root.path(), STALE_AFTER, SystemTime::now() + LATER).unwrap();
        assert_eq!(n, 0);
        assert!(target.path().join("keep").exists());
        assert!(
            fs::symlink_metadata(&link).is_ok(),
            "the link itself is left alone"
        );
    }

    #[test]
    fn regular_files_with_the_prefix_are_ignored() {
        let root = tempfile::tempdir().unwrap();
        let f = root.path().join("borealis-ai-support-file");
        fs::write(&f, "x").unwrap();
        sweep_stale_in(root.path(), STALE_AFTER, SystemTime::now() + LATER).unwrap();
        assert!(f.exists());
    }
}
