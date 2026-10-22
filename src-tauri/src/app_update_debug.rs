//! Test hook for the "new version available" notice (B-41): a debug build answers the update check
//! from a release-feed fixture in the app's data folder instead of GitHub, so the notice can be
//! tested on any version and offline. Release builds never read the file.
//!
//! The fixture has the shape of GitHub's latest-release answer (`tag_name`, `html_url`, `body`,
//! `draft`, `prerelease`) and goes through the same [`crate::app_update::evaluate`] rules. On a
//! phone, where the UI normally asks GitHub itself, the backend then answers with the result.

use crate::app_update::{self, AppUpdateStatus};
use crate::content_index::Fetcher;
use std::path::Path;
use std::time::SystemTime;

/// The fixture's file name in the app's data folder.
pub const FEED_FILE: &str = "debug-update-feed.json";
/// A release answer is small; anything bigger is not a fixture.
const MAX_FEED_BYTES: u64 = 512 * 1024;

/// The fixture's bytes, only when `is_debug_build` and the file exists and is small enough.
#[must_use]
pub fn feed(data_dir: &Path, is_debug_build: bool) -> Option<Vec<u8>> {
    if !is_debug_build {
        return None;
    }
    let path = data_dir.join(FEED_FILE);
    let size = std::fs::metadata(&path).ok()?.len();
    if size > MAX_FEED_BYTES {
        tracing::warn!("app update: debug release-feed fixture too large, ignored");
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    tracing::info!("app update: answering from the debug release-feed fixture");
    Some(bytes)
}

/// Serves the fixture for every request.
pub struct FixtureFetcher(pub Vec<u8>);

impl Fetcher for FixtureFetcher {
    fn get(&self, _url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        if self.0.len() > max_bytes {
            return Err("answer too large".into());
        }
        Ok(self.0.clone())
    }
}

/// The check this build makes from the fixture, or `None` (release build, or no fixture).
#[must_use]
pub fn check_from_fixture(data_dir: &Path, now: SystemTime) -> Option<AppUpdateStatus> {
    feed(data_dir, cfg!(debug_assertions))
        .map(|bytes| app_update::check_now(&FixtureFetcher(bytes), now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_update::AppUpdateState;

    const FIXTURE: &str = r#"{"tag_name":"v99.0.0","html_url":"https://github.com/alon-mini/borealis/releases/tag/v99.0.0","body":"Fixture notes","draft":false,"prerelease":false}"#;

    fn dir_with_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(FEED_FILE), FIXTURE).expect("write fixture");
        dir
    }

    #[test]
    fn a_release_build_ignores_the_fixture() {
        let dir = dir_with_fixture();
        assert!(feed(dir.path(), false).is_none());
    }

    #[test]
    fn a_debug_build_reads_the_fixture() {
        let dir = dir_with_fixture();
        assert_eq!(feed(dir.path(), true).as_deref(), Some(FIXTURE.as_bytes()));
    }

    #[test]
    fn no_fixture_means_no_override() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(feed(dir.path(), true).is_none());
        assert!(check_from_fixture(dir.path(), SystemTime::UNIX_EPOCH).is_none());
    }

    #[test]
    fn an_oversized_fixture_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let big = vec![b' '; usize::try_from(MAX_FEED_BYTES).expect("fits") + 1];
        std::fs::write(dir.path().join(FEED_FILE), big).expect("write");
        assert!(feed(dir.path(), true).is_none());
    }

    #[test]
    fn the_fixture_reports_a_newer_version_through_the_normal_rules() {
        let dir = dir_with_fixture();
        let status =
            check_from_fixture(dir.path(), SystemTime::UNIX_EPOCH).expect("tests are debug builds");
        assert_eq!(status.state, AppUpdateState::Done);
        assert_eq!(status.latest.as_deref(), Some("99.0.0"));
        assert!(status.newer);
        assert_eq!(status.notes.as_deref(), Some("Fixture notes"));
    }
}
