//! Settings > DATA FILES: the documents directory, folder validation, Restore Defaults and
//! "clear caches". Folders always arrive as backend-picked tokens, never as raw paths.

use crate::content_packs::EventSink;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::PickPurpose;
use crate::settings::AppSettings;
use crate::settings_dto::{ClearCachesResult, DataDirs};
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Sub-folder of the documents directory that holds character files.
pub const CHARACTERS_SUBDIR: &str = "characters";
/// Scratch folders younger than this may belong to a running import and are kept.
const SCRATCH_STALE_AFTER: Duration = Duration::from_secs(10 * 60);
/// Prefix of the import/download scratch folders in the data folder.
const SCRATCH_PREFIX: &str = ".tmp-";

/// Folders (and everything below them) that are never accepted as user data folders.
#[cfg(unix)]
const SYSTEM_PREFIXES: &[&str] = &[
    "/bin",
    "/sbin",
    "/usr",
    "/etc",
    "/dev",
    "/proc",
    "/sys",
    "/boot",
    "/lib",
    "/lib64",
    "/opt",
    "/root",
    "/snap",
    "/System",
    "/Library",
    "/Applications",
    "/private/etc",
    "/private/var/db",
    "/var/lib",
    "/var/log",
    "/var/db",
];
/// Folders refused themselves (their sub-folders are fine).
#[cfg(unix)]
const SYSTEM_EXACT: &[&str] = &[
    "/Users", "/home", "/private", "/var", "/Volumes", "/mnt", "/media",
];
#[cfg(not(unix))]
const SYSTEM_PREFIXES: &[&str] = &[];
#[cfg(not(unix))]
const SYSTEM_EXACT: &[&str] = &[];
/// Windows system folders, read from the environment.
const SYSTEM_ENV_DIRS: [&str; 5] = [
    "SystemRoot",
    "windir",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramData",
];

fn canonical(p: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(p).ok()
}

fn is_system_dir(dir: &Path) -> bool {
    if dir.parent().is_none() || SYSTEM_EXACT.iter().any(|s| dir == Path::new(s)) {
        return true;
    }
    let env_dirs = SYSTEM_ENV_DIRS
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from);
    SYSTEM_PREFIXES
        .iter()
        .map(PathBuf::from)
        .chain(env_dirs)
        .any(|root| dir.starts_with(&root) || canonical(&root).is_some_and(|c| dir.starts_with(c)))
}

/// The app bundle (`X.app` on macOS) or the folder holding the executable.
#[must_use]
pub fn app_bundle_dir() -> Option<PathBuf> {
    let exe = canonical(&std::env::current_exe().ok()?)?;
    exe.ancestors()
        .find(|a| a.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .or_else(|| exe.parent().map(Path::to_path_buf))
}

/// Check a user-chosen folder: an existing directory, not a system folder, not inside any of
/// `forbidden` (app bundle, bundled content, temp, ...), and writable when `writable` is set.
/// Returns the canonical path.
///
/// # Errors
/// `invalid_argument` naming the problem.
pub fn validate_user_dir(dir: &Path, forbidden: &[PathBuf], writable: bool) -> ApiResult<PathBuf> {
    let dir = canonical(dir)
        .filter(|d| d.is_absolute() && d.is_dir())
        .ok_or_else(|| ApiError::invalid("That folder does not exist."))?;
    if is_system_dir(&dir) {
        return Err(ApiError::invalid("A system folder cannot be used."));
    }
    let blocked = forbidden
        .iter()
        .map(|f| canonical(f).unwrap_or_else(|| f.clone()))
        .any(|f| dir.starts_with(&f));
    if blocked {
        return Err(ApiError::invalid(
            "Folders inside the application, its data or the temporary folder cannot be used.",
        ));
    }
    if writable {
        let probe = dir.join(format!(".borealis-write-test-{}", crate::session::new_id()));
        std::fs::write(&probe, b"ok")
            .map_err(|_| ApiError::invalid("Borealis cannot write to that folder."))?;
        let _ = std::fs::remove_file(&probe);
    }
    Ok(dir)
}

fn display(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl AppState {
    /// The documents directory in effect: the chosen one while it still exists, else the default.
    #[must_use]
    pub fn documents_dir(&self) -> PathBuf {
        self.settings()
            .ok()
            .and_then(|s| s.documents_dir)
            .map(PathBuf::from)
            .filter(|d| d.join(CHARACTERS_SUBDIR).is_dir())
            .unwrap_or_else(|| self.paths.data.clone())
    }

    /// Folder character files are listed from and saved to.
    #[must_use]
    pub fn characters_dir(&self) -> PathBuf {
        let docs = self.documents_dir();
        if docs == self.paths.data {
            self.paths.characters()
        } else {
            docs.join(CHARACTERS_SUBDIR)
        }
    }

    /// Current data folders (Settings > DATA FILES).
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn data_dirs(&self) -> ApiResult<DataDirs> {
        let documents = self.documents_dir();
        Ok(DataDirs {
            documents_dir: display(&documents),
            default_documents_dir: display(&self.paths.data),
            is_default: documents == self.paths.data,
            characters_dir: display(&self.characters_dir()),
            secondary_content_dir: self.settings()?.secondary_content_dir,
        })
    }

    /// Folders no documents directory may lie in.
    fn documents_forbidden(&self) -> Vec<PathBuf> {
        let mut out = vec![
            self.paths.content_root.clone(),
            self.paths.library(),
            std::env::temp_dir(),
        ];
        out.extend(app_bundle_dir());
        out
    }

    /// Folders no secondary content directory may lie in.
    pub(crate) fn secondary_forbidden(&self) -> Vec<PathBuf> {
        let mut out = vec![
            self.paths.content_root.clone(),
            self.paths.data.clone(),
            std::env::temp_dir(),
        ];
        out.extend(app_bundle_dir());
        out
    }

    /// Use the folder behind a `documents-dir` pick (`None` = back to the default). Characters are
    /// saved to `<dir>/characters` from now on; existing files are not moved (the reference app does not move
    /// them either) and open characters keep saving where they were opened from.
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an unusable folder; `io_error` if settings cannot be
    /// written.
    pub fn set_documents_dir(&self, token: Option<&str>) -> ApiResult<DataDirs> {
        let chosen = match token {
            Some(t) => {
                let picked = self.picked.take(t, PickPurpose::DocumentsDir)?;
                let dir = validate_user_dir(&picked, &self.documents_forbidden(), true)?;
                std::fs::create_dir_all(dir.join(CHARACTERS_SUBDIR))
                    .map_err(|e| ApiError::io("create the characters folder", e))?;
                Some(display(&dir))
            }
            None => None,
        };
        let mut next = self.settings()?;
        next.documents_dir = chosen;
        next.save(&self.paths.settings_file())?;
        self.set_settings(next)?;
        self.data_dirs()
    }

    /// Reference app "Restore Defaults": reset every setting the reference app dialog owns, save at once and
    /// apply (documents directory back to the default, secondary directory unlinked). AI, default
    /// ruleset and autosave (the Borealis settings window) are kept.
    ///
    /// # Errors
    /// `io_error` if the settings cannot be written; content errors from unlinking.
    pub fn restore_default_settings(&self, sink: &dyn EventSink) -> ApiResult<AppSettings> {
        let current = self.settings()?;
        let next = current.restored_defaults();
        next.save(&self.paths.settings_file())?;
        if next.engine_settings() != current.engine_settings() {
            let hub = self.hub()?.with_settings(&next);
            self.set_hub(hub)?;
        }
        self.set_settings(next.clone())?;
        if current.secondary_content_dir.is_some() {
            self.sync_secondary_content(sink)?;
        }
        Ok(next)
    }

    /// Clear Borealis's caches (the reference app's "Clear News Articles Cache"; Borealis has no news feed):
    /// stale scratch folders in the data folder, stale AI session folders (`ai_sweep`), and the
    /// cached results of index checks. Pinned index keys and installed content are kept.
    ///
    /// # Errors
    /// `internal` if a lock is poisoned; `io_error` if the content settings cannot be written.
    pub fn clear_caches(&self, ai_sweep: fn() -> usize) -> ApiResult<ClearCachesResult> {
        let _guard = self.content_lock()?;
        let temp_folders_removed = remove_stale_scratch(&self.paths.data, SystemTime::now());
        let mut prefs = self.content_prefs();
        let mut indexes_reset = 0;
        for index in prefs
            .indexes
            .iter_mut()
            .filter(|i| i.last_checked.is_some())
        {
            index.last_checked = None;
            index.file_count = 0;
            index.updates_available = 0;
            index.requires_newer_app = false;
            indexes_reset += 1;
        }
        if indexes_reset > 0 {
            prefs.save(&self.paths.prefs_file())?;
        }
        Ok(ClearCachesResult {
            temp_folders_removed,
            ai_temp_folders_removed: ai_sweep(),
            indexes_reset,
        })
    }
}

/// Remove `.tmp-*` scratch folders (real folders only) older than [`SCRATCH_STALE_AFTER`].
fn remove_stale_scratch(data: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(data) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.filter_map(Result::ok) {
        let stale = entry
            .file_name()
            .to_string_lossy()
            .starts_with(SCRATCH_PREFIX)
            && std::fs::symlink_metadata(entry.path()).is_ok_and(|m| {
                m.is_dir()
                    && m.modified()
                        .ok()
                        .and_then(|t| now.duration_since(t).ok())
                        .is_some_and(|age| age >= SCRATCH_STALE_AFTER)
            });
        if stale && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_dirs_are_refused() {
        assert!(is_system_dir(Path::new("/")));
        #[cfg(unix)]
        {
            assert!(is_system_dir(Path::new("/usr/local/share")));
            assert!(is_system_dir(Path::new("/Users")));
            assert!(!is_system_dir(Path::new("/Users/someone/Documents")));
        }
    }

    #[test]
    fn stale_scratch_is_removed_and_fresh_kept() {
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir(data.path().join(".tmp-old")).unwrap();
        std::fs::create_dir(data.path().join("keep")).unwrap();
        let later = SystemTime::now() + SCRATCH_STALE_AFTER + Duration::from_secs(1);
        assert_eq!(remove_stale_scratch(data.path(), SystemTime::now()), 0);
        assert_eq!(remove_stale_scratch(data.path(), later), 1);
        assert!(data.path().join("keep").is_dir());
    }
}
