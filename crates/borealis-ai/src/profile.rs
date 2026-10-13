//! Borealis-owned, read-only AI profile.
//!
//! The profile is a directory laid down from app resources. It is passed to the CLIs per
//! invocation only (`--settings`, `--plugin-dir`, `--mcp-config`, `-c`), so the user's own CLI
//! config dir, `CODEX_HOME` and login are never touched. Every file is SHA-256-hashed in a
//! manifest and verified before each run.
//!
//! Layout under `<target>/`: `profile/claude/settings.json`, `profile/claude/plugin/**`,
//! `profile/codex/overrides.toml`, and `manifest.json`. The MCP server command is not stored in the
//! profile because the bundled binary path varies per install; it is passed to
//! [`profile_args`] instead.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Backend;

/// Bumped when the profile layout or semantics change.
pub const PROFILE_VERSION: u32 = 1;
/// Name of the MCP server inside the CLIs (tools appear as `mcp__borealis__*`).
pub const MCP_SERVER_NAME: &str = "borealis";
const MANIFEST: &str = "manifest.json";
const PROFILE_SUBDIR: &str = "profile";
/// The only `overrides.toml` keys passed to Codex (as `-c key=value`).
const CODEX_OVERRIDE_PREFIX: &str = "mcp_servers.borealis.";

mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled_profile.rs"));
}

/// Relative path -> SHA-256 of every profile file compiled into this binary (from
/// `profile-assets/`). Installed files are verified against this, so editing both the profile and
/// its `manifest.json` is still detected.
pub fn bundled_hashes() -> BTreeMap<String, String> {
    bundled::BUNDLED_PROFILE
        .iter()
        .map(|(rel, bytes)| ((*rel).to_string(), hex(&Sha256::digest(bytes))))
        .collect()
}

fn bundled_file(rel: &str) -> Option<&'static [u8]> {
    bundled::BUNDLED_PROFILE
        .iter()
        .find(|(r, _)| *r == rel)
        .map(|(_, b)| *b)
}

/// Default assets shipped in this crate (dev builds and tests; the app bundles its own copy).
pub fn default_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("profile-assets")
}

/// How to launch the Borealis MCP server (stdio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCommand {
    /// Executable, normally the bundled `borealis` binary.
    pub program: PathBuf,
    /// Arguments, normally `["--mcp"]`.
    pub args: Vec<String>,
}

/// Profile settings for a session. `enabled: false` turns the profile off.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Install root (the `target_dir` given to [`install_profile`]).
    pub dir: PathBuf,
    /// MCP server launch command.
    pub mcp: McpCommand,
    /// On by default; set false to run without the profile.
    pub enabled: bool,
}

impl ProfileConfig {
    /// An enabled profile.
    pub fn new(dir: impl Into<PathBuf>, mcp: McpCommand) -> Self {
        Self {
            dir: dir.into(),
            mcp,
            enabled: true,
        }
    }
}

/// Result of [`verify_profile`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStatus {
    /// All files match the manifest and nothing extra exists.
    Ok,
    /// Relative paths that are missing, modified or unexpected (`manifest.json` if unreadable).
    Tampered(Vec<String>),
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    #[serde(rename = "profileVersion")]
    profile_version: u32,
    files: BTreeMap<String, String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(p: &Path) -> io::Result<String> {
    Ok(hex(&Sha256::digest(fs::read(p)?)))
}

/// Relative (`/`-separated) path -> SHA-256 for every regular file under `root`.
fn hash_tree(root: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let ty = entry.file_type()?;
            if ty.is_dir() {
                stack.push(path);
            } else if ty.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(io::Error::other)?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(rel, hash_file(&path)?);
            } else {
                // symlinks and specials are never part of a profile
                out.insert(format!("{}?", path.display()), String::new());
            }
        }
    }
    Ok(out)
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

fn set_mode(path: &Path, is_dir: bool, writable: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = match (is_dir, writable) {
            (true, true) => 0o755,
            (true, false) => 0o555,
            (false, true) => 0o644,
            (false, false) => 0o444,
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let mut perm = fs::metadata(path)?.permissions();
        perm.set_readonly(!writable);
        let _ = is_dir;
        fs::set_permissions(path, perm)
    }
}

/// Apply read-only (or writable again) to a whole tree. Directories are handled so that
/// children are processed while the parent is still writable when unlocking.
fn set_tree_writable(root: &Path, writable: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(root)?;
    if meta.is_dir() {
        if writable {
            set_mode(root, true, true)?;
        }
        for entry in fs::read_dir(root)? {
            set_tree_writable(&entry?.path(), writable)?;
        }
        if !writable {
            set_mode(root, true, false)?;
        }
    } else if meta.is_file() {
        set_mode(root, false, writable)?;
    }
    Ok(())
}

fn remove_locked(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        if path.is_dir() {
            set_tree_writable(path, true)?;
            fs::remove_dir_all(path)?;
        } else {
            set_mode(path, false, true)?;
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Copy `resources_dir` (containing `claude/` and `codex/`) to `<target_dir>/profile`, make it
/// read-only and write `<target_dir>/manifest.json`. Any previous install is replaced. Safe to
/// call on every launch and after updates.
pub fn install_profile(resources_dir: &Path, target_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(target_dir)?;
    let profile = target_dir.join(PROFILE_SUBDIR);
    let manifest = target_dir.join(MANIFEST);
    remove_locked(&profile)?;
    remove_locked(&manifest)?;
    copy_tree(resources_dir, &profile)?;
    let files = hash_tree(&profile)?;
    let body = serde_json::to_vec_pretty(&Manifest {
        profile_version: PROFILE_VERSION,
        files,
    })
    .map_err(io::Error::other)?;
    fs::write(&manifest, body)?;
    set_tree_writable(&profile, false)?;
    set_mode(&manifest, false, false)
}

/// Check the installed profile against its manifest.
pub fn verify_profile(target_dir: &Path) -> ProfileStatus {
    let manifest: Manifest = match fs::read(target_dir.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(m) => m,
        None => return ProfileStatus::Tampered(vec![MANIFEST.to_string()]),
    };
    let Ok(actual) = hash_tree(&target_dir.join(PROFILE_SUBDIR)) else {
        return ProfileStatus::Tampered(vec![PROFILE_SUBDIR.to_string()]);
    };
    let mut bad: Vec<String> = Vec::new();
    for (path, hash) in &manifest.files {
        if actual.get(path) != Some(hash) {
            bad.push(path.clone());
        }
    }
    bad.extend(
        actual
            .keys()
            .filter(|p| !manifest.files.contains_key(*p))
            .cloned(),
    );
    // The manifest lives next to the profile and is writable by the same user, so it only
    // proves self-consistency. The hashes compiled into this binary are the reference.
    let bundled = bundled_hashes();
    for (path, hash) in &bundled {
        if actual.get(path) != Some(hash) && !bad.contains(path) {
            bad.push(format!("{path} (differs from the bundled profile)"));
        }
    }
    let extra: Vec<String> = actual
        .keys()
        .filter(|p| !bundled.contains_key(*p) && !bad.contains(*p))
        .map(|p| format!("{p} (not in the bundled profile)"))
        .collect();
    bad.extend(extra);
    if manifest.profile_version != PROFILE_VERSION {
        bad.push(format!(
            "{MANIFEST} (profile version {})",
            manifest.profile_version
        ));
    }
    if bad.is_empty() {
        ProfileStatus::Ok
    } else {
        bad.sort();
        ProfileStatus::Tampered(bad)
    }
}

/// Verify and, if tampered, reinstall from `resources_dir`. Returns the status found *before*
/// repair (`Ok` means nothing was changed).
pub fn repair_profile(resources_dir: &Path, target_dir: &Path) -> io::Result<ProfileStatus> {
    let before = verify_profile(target_dir);
    if before != ProfileStatus::Ok {
        install_profile(resources_dir, target_dir)?;
    }
    Ok(before)
}

fn os(s: impl Into<OsString>) -> OsString {
    s.into()
}

/// Extra CLI args that apply the profile to one run of `backend`.
///
/// Claude: inline `--mcp-config` (the existing `--strict-mcp-config` makes it the only MCP
/// source), `--settings` (allow `mcp__borealis__*`, deny shell/file/web tools, no hooks),
/// `--plugin-dir` (skills) and `--permission-mode dontAsk` so anything not allowed is refused
/// instead of prompting. Codex: `-c mcp_servers.borealis.*` overrides plus the lines of
/// `codex/overrides.toml`.
pub fn profile_args(backend: Backend, profile_dir: &Path, mcp: &McpCommand) -> Vec<OsString> {
    let root = profile_dir.join(PROFILE_SUBDIR);
    match backend {
        Backend::Claude => {
            let claude = root.join("claude");
            let mcp_json = serde_json::json!({
                "mcpServers": { MCP_SERVER_NAME: {
                    "command": mcp.program.to_string_lossy(),
                    "args": mcp.args,
                }}
            });
            vec![
                os("--mcp-config"),
                os(mcp_json.to_string()),
                os("--settings"),
                claude.join("settings.json").into_os_string(),
                os("--plugin-dir"),
                claude.join("plugin").into_os_string(),
                os("--permission-mode"),
                os("dontAsk"),
            ]
        }
        Backend::Codex => {
            let mut out = codex_mcp_args(mcp);
            let overrides =
                fs::read_to_string(root.join("codex").join("overrides.toml")).unwrap_or_default();
            out.extend(codex_override_args(&overrides));
            out
        }
    }
}

/// The `-c mcp_servers.borealis.{command,args}` pairs for Codex.
fn codex_mcp_args(mcp: &McpCommand) -> Vec<OsString> {
    let key = format!("mcp_servers.{MCP_SERVER_NAME}");
    let program = serde_json::Value::from(mcp.program.to_string_lossy().into_owned());
    let args = serde_json::Value::from(mcp.args.clone());
    vec![
        os("-c"),
        os(format!("{key}.command={program}")),
        os("-c"),
        os(format!("{key}.args={args}")),
    ]
}

/// `-c key=value` pairs from `overrides.toml` text. Only `mcp_servers.borealis.*` keys are
/// accepted; anything else (such as `sandbox_mode` or `approval_policy`) is dropped with a
/// warning, so a tampered file cannot widen the sandbox.
pub fn codex_override_args(text: &str) -> Vec<OsString> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            tracing::warn!("ignoring malformed line in the Codex profile overrides");
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        let key_ok = k.strip_prefix(CODEX_OVERRIDE_PREFIX).is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        });
        if !key_ok || v.is_empty() {
            tracing::warn!(
                key = k,
                "ignoring a Codex profile override outside mcp_servers.borealis"
            );
            continue;
        }
        out.extend([os("-c"), os(format!("{k}={v}"))]);
    }
    out
}

/// Verify the installed profile and build the args for one run, without reading profile files
/// again after verification (no check-then-use gap for the files Borealis passes by value).
///
/// The verified content equals the bundled bytes, so those bytes are what is used: Claude's
/// settings and MCP config are written to `scratch` (the session's private 0700 support dir)
/// and passed as paths, which also keeps the MCP command path out of `ps` output; Codex's
/// overrides are taken from the bundled `overrides.toml` and allowlisted. Only Claude's
/// `--plugin-dir` (skill markdown) is still read from the installed tree by the CLI.
///
/// # Errors
/// `Err(files)` when verification fails (see [`verify_profile`]) or `scratch` is not writable.
pub fn prepare_profile_args(
    backend: Backend,
    profile_dir: &Path,
    mcp: &McpCommand,
    scratch: &Path,
) -> Result<Vec<OsString>, Vec<String>> {
    if let ProfileStatus::Tampered(files) = verify_profile(profile_dir) {
        return Err(files);
    }
    let bundled = |rel: &str| bundled_file(rel).ok_or_else(|| vec![format!("{rel} (not bundled)")]);
    let write = |name: &str, bytes: &[u8]| -> Result<OsString, Vec<String>> {
        let p = scratch.join(name);
        fs::write(&p, bytes).map_err(|e| vec![format!("{name}: {e}")])?;
        Ok(p.into_os_string())
    };
    match backend {
        Backend::Claude => {
            let mcp_json = serde_json::json!({
                "mcpServers": { MCP_SERVER_NAME: {
                    "command": mcp.program.to_string_lossy(),
                    "args": mcp.args,
                }}
            });
            let plugin = profile_dir
                .join(PROFILE_SUBDIR)
                .join("claude")
                .join("plugin");
            Ok(vec![
                os("--mcp-config"),
                write("mcp.json", mcp_json.to_string().as_bytes())?,
                os("--settings"),
                write("settings.json", bundled("claude/settings.json")?)?,
                os("--plugin-dir"),
                plugin.into_os_string(),
                os("--permission-mode"),
                os("dontAsk"),
            ])
        }
        Backend::Codex => {
            let overrides = String::from_utf8_lossy(bundled("codex/overrides.toml")?).into_owned();
            let mut out = codex_mcp_args(mcp);
            out.extend(codex_override_args(&overrides));
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests;
