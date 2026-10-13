//! Wire types for Settings > DATA FILES: data folders, secondary content, caches and the
//! startup content update status.

use serde::Serialize;

/// Where Borealis keeps the user's files (display only: paths never travel the other way).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirs {
    /// The documents directory in effect (the chosen one, or the default).
    pub documents_dir: String,
    /// The app default documents directory.
    pub default_documents_dir: String,
    /// No custom documents directory is in effect.
    pub is_default: bool,
    /// Folder the character files are read from and saved to (`<documentsDir>/characters`).
    pub characters_dir: String,
    /// The linked secondary content directory, if any.
    pub secondary_content_dir: Option<String>,
}

/// A secondary-directory file that was not loaded, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedContentFile {
    /// Path relative to the secondary directory.
    pub file: String,
    pub reason: String,
}

/// Outcome of loading the secondary directory into user packs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryContentReport {
    /// Pack ids installed or refreshed.
    pub installed: Vec<String>,
    /// Pack ids removed (source file gone, or the directory was unlinked).
    pub removed: Vec<String>,
    /// Files that were refused.
    pub skipped: Vec<SkippedContentFile>,
}

/// `set_secondary_content_dir` / `unlink_secondary_content_dir` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryContentResult {
    pub dirs: DataDirs,
    pub report: SecondaryContentReport,
}

/// `clear_caches` result ("Clear News Articles Cache" in the reference app).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearCachesResult {
    /// Stale import/download scratch folders removed from the data folder.
    pub temp_folders_removed: usize,
    /// Stale AI session folders removed from the system temp folder.
    pub ai_temp_folders_removed: usize,
    /// Update indexes whose cached check results were reset.
    pub indexes_reset: usize,
}

/// State of the automatic content update that runs at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentUpdateState {
    /// Not scheduled (mobile, or not started yet).
    Idle,
    /// "Automatic Updates" is off.
    Disabled,
    /// Scheduled; waiting for the startup delay.
    Pending,
    /// Checking the user's indexes.
    Running,
    /// Finished (see `updated` / `failed`).
    Done,
    /// The check could not run.
    Failed,
}

/// `content_update_status` result; also mirrored on `borealis://content-progress`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateStatus {
    pub state: ContentUpdateState,
    /// Status-bar text (the reference app wording).
    pub message: String,
    /// Files installed by the last run.
    pub updated: usize,
    /// Index or file URLs that were rejected or unreachable.
    pub failed: Vec<String>,
    /// RFC 3339 time the last run finished.
    pub checked_at: Option<String>,
}

impl Default for ContentUpdateStatus {
    fn default() -> Self {
        Self {
            state: ContentUpdateState::Idle,
            message: String::new(),
            updated: 0,
            failed: Vec::new(),
            checked_at: None,
        }
    }
}
