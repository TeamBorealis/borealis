//! Backend-owned file picking.
//!
//! The webview never hands the backend a filesystem path. A native dialog opened by the backend
//! records what the user chose under a random, single-purpose, short-lived token; commands that
//! read or write files accept only such a token and resolve it here. A compromised webview can
//! therefore only reach files the user explicitly picked for that purpose.

use crate::error::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a pick stays usable.
pub const PICK_TTL: Duration = Duration::from_secs(10 * 60);
/// Most live picks kept; the oldest is dropped beyond this.
pub const MAX_PICKS: usize = 32;

/// What a picked path may be used for. A token only resolves for the purpose it was minted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PickPurpose {
    /// A file to save a character copy to.
    SaveCharacter,
    /// A text, image or PDF file for "Import anything".
    ImportFile,
    /// A local 5etools `data/` folder.
    #[serde(rename = "5etools-folder")]
    FivetoolsFolder,
    /// A folder of the reference app content to import as one set ("Import my content folder").
    #[serde(rename = "content-folder")]
    ContentFolder,
    /// The AI CLI executable override.
    AiOverride,
    /// The documents directory (where characters are saved), Settings > DATA FILES.
    DocumentsDir,
    /// The secondary content directory, Settings > DATA FILES.
    SecondaryContentDir,
    /// Character files for File > Load Character (read once with `take_picked_files`).
    CharacterFile,
    /// A file dropped on the window from the OS. Minted only by the native drag-drop handler
    /// (`picked_files::grant_drop`), never by `pick_path`.
    DroppedFile,
    /// A `.borealis` bundle to preview and apply (File > Open bundle, a drop on the window, or the
    /// OS "open with"). It is opened by path in the backend, never read into the webview.
    BundleFile,
}

/// `pick_path` result: the token plus what to show the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedPathDto {
    /// Opaque token to pass to the command that uses the pick.
    pub token: String,
    /// File or folder name (no directories).
    pub name: String,
    /// Full path, for display only.
    pub path: String,
}

struct Entry {
    path: PathBuf,
    purpose: PickPurpose,
    at: Instant,
    seq: u64,
}

/// `{token -> (path, purpose)}` for paths the user chose in a backend-opened dialog.
pub struct PickedPaths {
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
    seq: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for PickedPaths {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let live = self.entries.lock().map_or(0, |m| m.len());
        f.debug_struct("PickedPaths").field("live", &live).finish()
    }
}

impl Default for PickedPaths {
    fn default() -> Self {
        Self::with_ttl(PICK_TTL)
    }
}

/// The last path component as text (empty when there is none).
#[must_use]
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn rejected() -> ApiError {
    ApiError::invalid("That file choice has expired or is not valid here. Choose it again.")
}

impl PickedPaths {
    /// A store whose picks expire after `ttl`.
    #[must_use]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Record a path the user chose in a native dialog. Only backend code that opened the dialog
    /// (or tests) may call this; it is never reachable from IPC with a caller-supplied path.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn grant(&self, path: PathBuf, purpose: PickPurpose) -> ApiResult<PickedPathDto> {
        let token = format!("pick_{}", uuid::Uuid::new_v4().simple());
        let dto = PickedPathDto {
            token: token.clone(),
            name: file_name(&path),
            path: path.to_string_lossy().into_owned(),
        };
        let mut map = self.entries.lock()?;
        let ttl = self.ttl;
        map.retain(|_, e| e.at.elapsed() < ttl);
        while map.len() >= MAX_PICKS {
            let Some(oldest) = map
                .iter()
                .min_by_key(|(_, e)| e.seq)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            map.remove(&oldest);
        }
        map.insert(
            token,
            Entry {
                path,
                purpose,
                at: Instant::now(),
                seq: self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            },
        );
        Ok(dto)
    }

    /// The path behind `token`, if it is live and was minted for `purpose`. The pick stays usable
    /// (a folder is scanned and then imported with the same token).
    ///
    /// # Errors
    /// `invalid_argument` for an unknown, expired or wrong-purpose token.
    pub fn resolve(&self, token: &str, purpose: PickPurpose) -> ApiResult<PathBuf> {
        let mut map = self.entries.lock()?;
        let ttl = self.ttl;
        map.retain(|_, e| e.at.elapsed() < ttl);
        match map.get(token.trim()) {
            Some(e) if e.purpose == purpose => Ok(e.path.clone()),
            _ => Err(rejected()),
        }
    }

    /// Like [`Self::resolve`] but consumes the token.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown, expired or wrong-purpose token.
    pub fn take(&self, token: &str, purpose: PickPurpose) -> ApiResult<PathBuf> {
        let path = self.resolve(token, purpose)?;
        self.entries.lock()?.remove(token.trim());
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_for_the_minted_purpose() {
        let picks = PickedPaths::default();
        let t = picks
            .grant(PathBuf::from("/x/data"), PickPurpose::FivetoolsFolder)
            .unwrap();
        assert_eq!(t.name, "data");
        assert!(picks.resolve(&t.token, PickPurpose::ImportFile).is_err());
        assert!(picks.resolve(&t.token, PickPurpose::SaveCharacter).is_err());
        assert_eq!(
            picks
                .resolve(&t.token, PickPurpose::FivetoolsFolder)
                .unwrap(),
            PathBuf::from("/x/data")
        );
        assert!(picks
            .resolve("pick_unknown", PickPurpose::FivetoolsFolder)
            .is_err());
        assert!(picks
            .resolve("/etc/passwd", PickPurpose::FivetoolsFolder)
            .is_err());
    }

    #[test]
    fn take_consumes_and_expired_picks_are_refused() {
        let picks = PickedPaths::default();
        let t = picks
            .grant(PathBuf::from("/x/claude"), PickPurpose::AiOverride)
            .unwrap();
        assert!(picks.take(&t.token, PickPurpose::AiOverride).is_ok());
        assert!(picks.take(&t.token, PickPurpose::AiOverride).is_err());
        let stale = PickedPaths::with_ttl(Duration::ZERO);
        let t = stale
            .grant(PathBuf::from("/x/a.txt"), PickPurpose::ImportFile)
            .unwrap();
        assert!(stale.resolve(&t.token, PickPurpose::ImportFile).is_err());
    }

    #[test]
    fn purposes_use_the_ui_names() {
        let names: Vec<String> = [
            PickPurpose::SaveCharacter,
            PickPurpose::ImportFile,
            PickPurpose::FivetoolsFolder,
            PickPurpose::AiOverride,
            PickPurpose::CharacterFile,
            PickPurpose::DroppedFile,
            PickPurpose::BundleFile,
        ]
        .iter()
        .map(|p| {
            serde_json::to_value(p)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
        assert_eq!(
            names,
            [
                "save-character",
                "import-file",
                "5etools-folder",
                "ai-override",
                "character-file",
                "dropped-file",
                "bundle-file"
            ]
        );
    }

    #[test]
    fn the_map_is_bounded() {
        let picks = PickedPaths::default();
        let first = picks
            .grant(PathBuf::from("/x/0"), PickPurpose::ImportFile)
            .unwrap();
        for i in 1..=MAX_PICKS {
            picks
                .grant(PathBuf::from(format!("/x/{i}")), PickPurpose::ImportFile)
                .unwrap();
        }
        assert!(picks.entries.lock().unwrap().len() <= MAX_PICKS);
        assert!(picks
            .resolve(&first.token, PickPurpose::ImportFile)
            .is_err());
    }
}
