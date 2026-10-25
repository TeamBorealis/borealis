//! Removing an index folder reliably on every OS.
//!
//! On Windows a file is only gone once its last handle and mapped view are closed: until then it stays
//! "delete pending" inside its folder, and removing the folder fails with `DirectoryNotEmpty` (or the file
//! itself with `PermissionDenied`). Handles are released a moment late when an index was just dropped, or
//! when an antivirus or indexing service is scanning the new files, so a removal is retried briefly before
//! it counts as failed.

use std::io::{self, ErrorKind};
use std::path::Path;
use std::time::Duration;

/// Attempts before a removal counts as failed.
const ATTEMPTS: u32 = 10;
/// Pause after the first failed attempt; it grows linearly (about 2 s in all).
const FIRST_PAUSE: Duration = Duration::from_millis(50);

/// Remove `dir` and everything in it; a folder that does not exist is already removed.
///
/// # Errors
/// The last I/O error when the folder still cannot be removed after the retries.
pub fn remove_index_dir(dir: &Path) -> io::Result<()> {
    remove_with_retry(dir, |d| std::fs::remove_dir_all(d), std::thread::sleep)
}

/// Whether `e` is the kind of failure a file still being released causes (worth another attempt).
fn is_transient(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::DirectoryNotEmpty | ErrorKind::PermissionDenied | ErrorKind::ResourceBusy
    )
}

fn remove_with_retry(
    dir: &Path,
    mut remove: impl FnMut(&Path) -> io::Result<()>,
    mut pause: impl FnMut(Duration),
) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match remove(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) if attempt < ATTEMPTS && is_transient(&e) => {
                pause(FIRST_PAUSE * attempt);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing(kinds: &[ErrorKind]) -> impl FnMut(&Path) -> io::Result<()> + '_ {
        let mut calls = 0;
        move |_| {
            let result = kinds.get(calls).map_or(Ok(()), |k| Err(io::Error::from(*k)));
            calls += 1;
            result
        }
    }

    #[test]
    fn a_folder_still_being_released_is_removed_on_a_later_attempt() {
        let mut pauses = Vec::new();
        let kinds = [ErrorKind::DirectoryNotEmpty, ErrorKind::PermissionDenied];
        remove_with_retry(Path::new("x"), failing(&kinds), |d| pauses.push(d)).unwrap();
        assert_eq!(pauses, [FIRST_PAUSE, FIRST_PAUSE * 2]);
    }

    #[test]
    fn a_folder_that_never_frees_up_fails_after_the_last_attempt() {
        let mut pauses = 0;
        let kinds = [ErrorKind::DirectoryNotEmpty; ATTEMPTS as usize];
        let err = remove_with_retry(Path::new("x"), failing(&kinds), |_| pauses += 1).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DirectoryNotEmpty);
        assert_eq!(pauses, ATTEMPTS - 1);
    }

    #[test]
    fn other_errors_fail_at_once_and_a_missing_folder_is_already_removed() {
        let mut pauses = 0;
        let err = remove_with_retry(
            Path::new("x"),
            failing(&[ErrorKind::InvalidInput]),
            |_| pauses += 1,
        )
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(pauses, 0);
        remove_with_retry(Path::new("x"), failing(&[ErrorKind::NotFound]), |_| {}).unwrap();
    }

    #[test]
    fn a_real_folder_is_removed_and_removing_it_again_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("lore");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("a.store"), b"x").unwrap();
        remove_index_dir(&dir).unwrap();
        assert!(!dir.exists());
        remove_index_dir(&dir).unwrap();
    }
}
