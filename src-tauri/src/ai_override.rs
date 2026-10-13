//! Validation for the AI CLI executable override.
//!
//! The override is only set from a backend-picked file (`pick_path` purpose `ai-override`) after a
//! native confirmation, and every stored value is re-checked before it is used: a settings file
//! edited behind the app's back cannot point the AI runner at an arbitrary program.

use crate::error::{ApiError, ApiResult};
use std::path::{Path, PathBuf};

/// Executable names an override may have.
pub const ALLOWED_NAMES: &[&str] = &[
    "claude",
    "codex",
    "claude.exe",
    "codex.exe",
    "claude.cmd",
    "codex.cmd",
];

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Folders an override must not live under: the temp dir(s) and the app data dir.
#[must_use]
pub fn forbidden_roots(data_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    if cfg!(unix) {
        roots.extend(["/tmp", "/private/tmp", "/var/tmp"].map(PathBuf::from));
    }
    roots.extend(data_dir.map(Path::to_path_buf));
    let canon: Vec<PathBuf> = roots.iter().map(|r| canonical(r)).collect();
    roots.extend(canon);
    roots
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &std::fs::Metadata) -> bool {
    // The allowed names already require an .exe/.cmd extension or a bare npm shim name.
    true
}

/// Windows cannot start npm's extension-less `#!/bin/sh` shim; only `.exe`/`.cmd` names run there.
fn bare_name_runs(name: &str, windows: bool) -> bool {
    !windows || name.contains('.')
}

fn refuse(why: &str) -> ApiError {
    ApiError::invalid(format!("That program cannot be used for AI: {why}."))
}

/// Check that `path` is an existing, executable `claude`/`codex` binary outside `forbidden`.
///
/// The name is checked on the path as chosen (npm installs `claude` as a symlink to a script),
/// the location on both the chosen and the resolved path.
///
/// # Errors
/// `invalid_argument` naming the failed check.
pub fn validate(path: &Path, forbidden: &[PathBuf]) -> ApiResult<PathBuf> {
    if !path.is_absolute() {
        return Err(refuse("the path must be absolute"));
    }
    let name = crate::picked_paths::file_name(path).to_lowercase();
    if !ALLOWED_NAMES.contains(&name.as_str()) {
        return Err(refuse(
            "only the claude or codex command line programs are allowed",
        ));
    }
    if !bare_name_runs(&name, cfg!(windows)) {
        return Err(refuse(
            "on Windows pick claude.exe, claude.cmd, codex.exe or codex.cmd",
        ));
    }
    let meta = std::fs::metadata(path).map_err(|_| refuse("the file does not exist"))?;
    if !meta.is_file() {
        return Err(refuse("it is not a file"));
    }
    if !is_executable(&meta) {
        return Err(refuse("the file is not executable"));
    }
    let resolved = canonical(path);
    if forbidden
        .iter()
        .any(|root| path.starts_with(root) || resolved.starts_with(root))
    {
        return Err(refuse("it lives in a temporary or app data folder"));
    }
    Ok(path.to_path_buf())
}

/// The stored override if it still validates; otherwise `None` with a warning (never an error, so
/// a stale or tampered setting falls back to `PATH` detection).
#[must_use]
pub fn usable(stored: Option<&str>, data_dir: Option<&Path>) -> Option<PathBuf> {
    let raw = stored.map(str::trim).filter(|p| !p.is_empty())?;
    match validate(Path::new(raw), &forbidden_roots(data_dir)) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!("ignoring the stored AI program override: {}", e.message);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_exe(dir: &Path, name: &str, mode: u32) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
        p
    }

    /// Names `validate` accepts on this platform (Windows refuses the bare npm shim names).
    const CLAUDE: &str = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    const CODEX: &str = if cfg!(windows) { "codex.exe" } else { "codex" };

    fn outside_temp() -> tempfile::TempDir {
        tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    #[test]
    fn rejects_a_shell_by_name() {
        let roots = forbidden_roots(None);
        assert!(validate(Path::new("/bin/sh"), &roots).is_err());
    }

    #[test]
    fn rejects_a_binary_in_the_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_exe(dir.path(), CLAUDE, 0o755);
        let err = validate(&p, &forbidden_roots(None)).unwrap_err();
        assert!(err.message.contains("temporary"), "{}", err.message);
    }

    #[test]
    fn bare_npm_shim_names_are_refused_only_on_windows() {
        assert!(bare_name_runs("claude", false));
        assert!(!bare_name_runs("claude", true));
        assert!(bare_name_runs("codex.cmd", true));
        assert!(bare_name_runs("claude.exe", true));
    }

    #[test]
    fn rejects_the_app_data_dir_and_missing_files() {
        let dir = outside_temp();
        let p = write_exe(dir.path(), CODEX, 0o755);
        assert!(validate(&p, &forbidden_roots(None)).is_ok());
        let err = validate(&p, &forbidden_roots(Some(dir.path()))).unwrap_err();
        assert!(err.message.contains("app data"), "{}", err.message);
        let err = validate(&dir.path().join(CLAUDE), &forbidden_roots(None)).unwrap_err();
        assert!(err.message.contains("does not exist"), "{}", err.message);
        assert!(validate(Path::new("claude"), &forbidden_roots(None)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_executable_file() {
        let dir = outside_temp();
        let p = write_exe(dir.path(), "claude", 0o644);
        let err = validate(&p, &forbidden_roots(None)).unwrap_err();
        assert!(err.message.contains("not executable"), "{}", err.message);
    }

    #[test]
    fn stale_stored_overrides_are_ignored() {
        assert_eq!(usable(Some("/bin/sh"), None), None);
        assert_eq!(usable(Some("  "), None), None);
        assert_eq!(usable(None, None), None);
    }
}
