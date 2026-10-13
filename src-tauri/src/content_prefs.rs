//! Persistent content preferences: disabled packs and the user's update indexes.

use crate::error::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// A user-added update index (no defaults are ever shipped).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IndexRecord {
    /// HTTPS location of the index.
    pub url: String,
    /// Name from the last fetched index (the URL until then).
    pub name: String,
    /// RFC 3339 time of the last successful check.
    pub last_checked: Option<String>,
    /// Files listed by the last fetched index.
    pub file_count: usize,
    /// Listed files that differ from what is installed.
    pub updates_available: usize,
    /// The index asks for a newer app version.
    pub requires_newer_app: bool,
    /// Base64 ed25519 public key pinned on the first signed fetch (trust on first use).
    pub public_key: Option<String>,
    /// The last fetched index carried a valid signature from the pinned key.
    pub verified: bool,
}

/// Everything the Content screen persists besides the packs themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContentPrefs {
    /// Pack ids that are installed but switched off.
    pub disabled_packs: BTreeSet<String>,
    /// Update indexes added by the user.
    pub indexes: Vec<IndexRecord>,
}

impl ContentPrefs {
    /// Read the file; a missing or damaged file means defaults.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Write the file atomically.
    ///
    /// # Errors
    /// `io_error` if the file cannot be written.
    pub fn save(&self, path: &Path) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| ApiError::io("save the content settings", e))?;
        borealis_character::format::replace_file(&tmp, path)
            .map_err(|e| ApiError::io("save the content settings", e))
    }
}
