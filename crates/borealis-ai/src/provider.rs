//! Locating the `claude` / `codex` executables.
//!
//! GUI apps (especially on macOS) often start with a minimal `PATH`, so besides `PATH` we also
//! probe the usual install locations. The user can always override with an explicit path.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::Backend;

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Where a detected executable came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    /// Explicit user override.
    Override,
    /// Found on `PATH`.
    Path,
    /// Found in a well-known install directory.
    KnownLocation,
}

/// A usable CLI on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Which CLI.
    pub backend: Backend,
    /// Absolute path to the executable.
    pub path: PathBuf,
    /// First line of `--version`, if the probe succeeded.
    pub version: Option<String>,
    /// How it was found.
    pub source: DetectionSource,
}

/// Detection inputs; everything is injectable so tests never depend on the host machine.
#[derive(Debug, Clone, Default)]
pub struct DetectConfig {
    /// User-chosen executable per backend (wins over everything).
    pub overrides: HashMap<Backend, PathBuf>,
    /// Extra directories to search after `PATH` and the built-in locations.
    pub extra_dirs: Vec<PathBuf>,
    /// `PATH` value to use instead of the process environment's.
    pub path_env: Option<OsString>,
    /// Home directory to use instead of `$HOME`.
    pub home: Option<PathBuf>,
    /// Skip well-known locations (tests).
    pub skip_known_locations: bool,
    /// Skip running `--version` (tests, or fast first paint).
    pub skip_version_probe: bool,
}

impl DetectConfig {
    fn home_dir(&self) -> Option<PathBuf> {
        self.home
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        let raw = self
            .path_env
            .clone()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        std::env::split_paths(&raw)
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    }

    fn known_dirs(&self) -> Vec<PathBuf> {
        if self.skip_known_locations {
            return Vec::new();
        }
        let mut dirs = Vec::new();
        if let Some(h) = self.home_dir() {
            for rel in [
                ".claude/local",
                ".local/bin",
                ".npm-global/bin",
                ".volta/bin",
                ".bun/bin",
                ".cargo/bin",
            ] {
                dirs.push(h.join(rel));
            }
            if cfg!(windows) {
                // npm's global prefix and Scoop's shims (the native Claude installer uses .local/bin).
                dirs.push(h.join("AppData").join("Roaming").join("npm"));
                dirs.push(h.join("scoop").join("shims"));
            }
        }
        if !cfg!(windows) {
            for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
                dirs.push(PathBuf::from(d));
            }
        }
        dirs
    }
}

/// Candidate paths for `backend` in priority order, with the source of each.
/// Only paths that exist and look executable are returned.
pub fn candidates(backend: Backend, cfg: &DetectConfig) -> Vec<(PathBuf, DetectionSource)> {
    let mut out: Vec<(PathBuf, DetectionSource)> = Vec::new();
    let mut push = |p: PathBuf, src: DetectionSource| {
        if is_executable(&p) && !out.iter().any(|(q, _)| q == &p) {
            out.push((p, src));
        }
    };
    if let Some(p) = cfg.overrides.get(&backend) {
        push(p.clone(), DetectionSource::Override);
    }
    for d in cfg.path_dirs() {
        for name in exe_names(backend) {
            push(d.join(name), DetectionSource::Path);
        }
    }
    for d in cfg
        .known_dirs()
        .into_iter()
        .chain(cfg.extra_dirs.iter().cloned())
    {
        for name in exe_names(backend) {
            push(d.join(name), DetectionSource::KnownLocation);
        }
    }
    out
}

fn exe_names(backend: Backend) -> Vec<String> {
    crate::spawn::host_launcher_names(backend.program())
}

fn is_executable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Find the best CLI for one backend. A user override that exists but is not runnable
/// is skipped so detection falls through to the next candidate.
pub async fn detect_one(backend: Backend, cfg: &DetectConfig) -> Option<ProviderInfo> {
    for (path, source) in candidates(backend, cfg) {
        let version = if cfg.skip_version_probe {
            None
        } else {
            probe_version(&path).await
        };
        if !cfg.skip_version_probe && version.is_none() {
            continue; // exists but does not run `--version`: not a usable CLI
        }
        return Some(ProviderInfo {
            backend,
            path,
            version,
            source,
        });
    }
    None
}

/// Detect every supported backend, returning those that are installed.
pub async fn detect_all(cfg: &DetectConfig) -> Vec<ProviderInfo> {
    let mut out = Vec::new();
    for b in Backend::ALL {
        if let Some(p) = detect_one(b, cfg).await {
            out.push(p);
        }
    }
    out
}

/// Run `<path> --version` with a short timeout and return its first non-empty line.
pub async fn probe_version(path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    crate::spawn::hide_console(&mut cmd);
    let out = tokio::time::timeout(VERSION_PROBE_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn cfg_for(dir: &Path) -> DetectConfig {
        DetectConfig {
            path_env: Some(dir.as_os_str().to_os_string()),
            home: Some(dir.join("nohome")),
            skip_known_locations: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn finds_on_path_and_reads_version() {
        let tmp = tempfile::tempdir().unwrap();
        script(tmp.path(), "claude", "echo '9.9.9 (Claude Code)'");
        let found = detect_one(Backend::Claude, &cfg_for(tmp.path()))
            .await
            .unwrap();
        assert_eq!(found.version.as_deref(), Some("9.9.9 (Claude Code)"));
        assert_eq!(found.source, DetectionSource::Path);
        assert!(detect_one(Backend::Codex, &cfg_for(tmp.path()))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn override_wins_and_broken_override_falls_through() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        script(tmp.path(), "codex", "echo 'codex-cli 1.0.0'");
        let custom = script(other.path(), "my-codex", "echo 'custom 2.0'");
        let mut cfg = cfg_for(tmp.path());
        cfg.overrides.insert(Backend::Codex, custom.clone());
        let found = detect_one(Backend::Codex, &cfg).await.unwrap();
        assert_eq!(found.path, custom);
        assert_eq!(found.source, DetectionSource::Override);

        let broken = script(other.path(), "broken", "exit 3");
        cfg.overrides.insert(Backend::Codex, broken);
        let found = detect_one(Backend::Codex, &cfg).await.unwrap();
        assert_eq!(
            found.source,
            DetectionSource::Path,
            "broken override must be skipped"
        );
    }

    #[tokio::test]
    async fn known_locations_are_searched_under_home() {
        let home = tempfile::tempdir().unwrap();
        let local = home.path().join(".claude/local");
        std::fs::create_dir_all(&local).unwrap();
        script(&local, "claude", "echo 1.2.3");
        let cfg = DetectConfig {
            path_env: Some(OsString::from("/nonexistent")),
            home: Some(home.path().to_path_buf()),
            ..Default::default()
        };
        let found = detect_one(Backend::Claude, &cfg).await.unwrap();
        assert_eq!(found.source, DetectionSource::KnownLocation);
        assert!(found.path.starts_with(&local));
    }

    #[tokio::test]
    async fn non_executable_files_and_dirs_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("claude"), "#!/bin/sh\n").unwrap(); // mode 0644
        std::fs::create_dir(tmp.path().join("codex")).unwrap();
        let mut cfg = cfg_for(tmp.path());
        cfg.skip_version_probe = true;
        assert!(detect_all(&cfg).await.is_empty());
    }

    #[tokio::test]
    async fn detect_all_reports_both_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        script(tmp.path(), "claude", "echo c");
        script(tmp.path(), "codex", "echo x");
        let mut cfg = cfg_for(tmp.path());
        cfg.extra_dirs.push(tmp.path().to_path_buf());
        assert_eq!(candidates(Backend::Claude, &cfg).len(), 1);
        assert_eq!(detect_all(&cfg).await.len(), 2);
    }
}
