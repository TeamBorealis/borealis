//! `borealis --mcp`: run the read-only Borealis MCP server on stdio instead of the GUI.
//!
//! Arguments: `--mcp [--content-dir <dir>]... [--packs a,b] [--search-dir <dir>]
//! [--session-scope <campaign>] [--allow-write --library <dir>]`. `--search-dir` defaults to the
//! app's `<app-data>/search/v1` when it exists (opened read-only); `--session-scope` grants one
//! ask one campaign's notes (default: none).
//! Without `--content-dir` the bundled content next to the executable is used (installed bundle
//! layouts), so the command also works when launched by a CLI from the app bundle path.
//! `--allow-write --library <dir>` (desktop only, AI15) adds the character tools of
//! [`crate::drive`], scoped to that library folder; without it the server is read-only.

use borealis_mcp::config::SessionScope;
use borealis_mcp::McpConfig;
use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// True when the arguments (without argv[0]) ask for MCP mode.
#[must_use]
pub fn requested(args: &[String]) -> bool {
    args.iter().any(|a| a == "--mcp")
}

/// Bundled content next to the executable (macOS `Resources`, Windows/Linux next to the binary).
pub(crate) fn default_content_dirs(exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    [
        dir.join("../Resources/content"),
        dir.join("content"),
        dir.join("../lib/Borealis/content"),
        dir.join("../lib/borealis/content"),
    ]
    .into_iter()
    .filter(|d| d.join("core").join("pack.json").is_file())
    .take(1)
    .collect()
}

/// The app identifier: the per-user data folder is named after it on every platform.
const APP_IDENTIFIER: &str = "app.borealis";

/// Operating systems with their own per-user data folder convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this binary runs on (other Unixes follow the Linux convention).
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// The path separator of this platform's paths.
    const fn separator(self) -> char {
        match self {
            Self::Windows => '\\',
            Self::MacOs | Self::Linux => '/',
        }
    }

    /// Whether `path` is absolute by this platform's rules, not the host's: a drive (`C:\`,
    /// `C:/`) or UNC prefix on Windows, a leading `/` elsewhere.
    fn is_absolute(self, path: &str) -> bool {
        match self {
            Self::Windows => {
                let bytes = path.as_bytes();
                path.starts_with(r"\\")
                    || path.starts_with("//")
                    || (bytes.len() > 2
                        && bytes[0].is_ascii_alphabetic()
                        && bytes[1] == b':'
                        && matches!(bytes[2], b'\\' | b'/'))
            }
            Self::MacOs | Self::Linux => path.starts_with('/'),
        }
    }

    /// `base` followed by `parts`, joined with this platform's separator, not the host's.
    fn join(self, base: &str, parts: &[&str]) -> String {
        let sep = self.separator();
        let mut out = base.trim_end_matches(['/', sep]).to_string();
        for part in parts {
            out.push(sep);
            out.push_str(part);
        }
        out
    }
}

/// The per-user data folder of the app, exactly where the desktop app keeps it (Tauri's
/// `app_data_dir`): `~/Library/Application Support/<id>` on macOS, `%APPDATA%\<id>` on Windows,
/// `$XDG_DATA_HOME/<id>` (default `~/.local/share/<id>`) on Linux. Pure: `env` answers variable
/// lookups and the result is that platform's path text, built with that platform's separator and
/// absolute-path rules rather than the host's, so each platform is testable on any host.
pub(crate) fn user_data_dir(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let var = |name: &str| env(name).filter(|v| !v.trim().is_empty());
    let dir = match platform {
        Platform::MacOs => platform.join(
            &var("HOME")?,
            &["Library", "Application Support", APP_IDENTIFIER],
        ),
        Platform::Windows => match var("APPDATA") {
            Some(appdata) => platform.join(&appdata, &[APP_IDENTIFIER]),
            None => platform.join(
                &var("USERPROFILE")?,
                &["AppData", "Roaming", APP_IDENTIFIER],
            ),
        },
        Platform::Linux => match var("XDG_DATA_HOME") {
            Some(xdg) if platform.is_absolute(&xdg) => platform.join(&xdg, &[APP_IDENTIFIER]),
            _ => platform.join(&var("HOME")?, &[".local", "share", APP_IDENTIFIER]),
        },
    };
    Some(dir)
}

/// This machine's per-user data folder of the app.
fn this_user_data_dir() -> Option<PathBuf> {
    let env = |name: &str| std::env::var(name).ok();
    user_data_dir(Platform::current(), &env).map(PathBuf::from)
}

/// The user content library (imported packs) of this machine, when it exists.
fn user_library_dir() -> Option<PathBuf> {
    let dir = this_user_data_dir()?.join("library");
    dir.is_dir().then_some(dir)
}

/// The app's search index folder of this machine (`<app-data>/search/v1`), when the app built one.
fn user_search_dir() -> Option<PathBuf> {
    let dir = this_user_data_dir()?
        .join(crate::search_service::SEARCH_DIR)
        .join(crate::search_service::LAYOUT_DIR);
    dir.is_dir().then_some(dir)
}

/// Parse MCP-mode arguments.
///
/// # Errors
/// A message for unknown or incomplete arguments.
pub fn parse_args(args: &[String], exe: &Path) -> Result<McpConfig, String> {
    let mut config = parse_args_with(args, exe, user_library_dir())?;
    if config.search_dir.is_none() {
        config.search_dir = user_search_dir();
    }
    Ok(config)
}

/// [`parse_args`] with the user content library given (`None` = the user has none).
///
/// # Errors
/// A message for unknown or incomplete arguments.
pub fn parse_args_with(
    args: &[String],
    exe: &Path,
    user_library: Option<PathBuf>,
) -> Result<McpConfig, String> {
    let mut config = McpConfig::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--mcp" => {}
            "--content-dir" => {
                let dir = it.next().ok_or("--content-dir needs a directory")?;
                config.content_dirs.push(PathBuf::from(dir));
            }
            "--packs" => {
                let list = it.next().ok_or("--packs needs a comma-separated list")?;
                let set: BTreeSet<String> = list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
                config.enabled_packs = Some(set);
            }
            "--search-dir" => {
                let dir = it.next().ok_or("--search-dir needs a directory")?;
                config.search_dir = Some(PathBuf::from(dir));
            }
            "--session-scope" => {
                let id = it.next().ok_or("--session-scope needs a campaign id")?;
                if id.trim().is_empty() {
                    return Err("--session-scope needs a non-empty campaign id".into());
                }
                config.session_scope = SessionScope::Campaign(id.clone());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if config.content_dirs.is_empty() {
        config.content_dirs = default_content_dirs(exe);
        // Without an explicit folder the server also serves the user's imported content, found
        // in the platform data folder (bundled packs come first and are never shadowed).
        if !config.content_dirs.is_empty() {
            config.content_dirs.extend(user_library);
        }
    }
    if config.content_dirs.is_empty() {
        return Err("no content directory found; pass --content-dir <dir>".into());
    }
    Ok(config)
}

/// Split the write-mode flags off: the remaining arguments and, with `--allow-write`, the
/// `--library` folder. Each flag needs the other, so write mode is never implied.
///
/// # Errors
/// A message when only one of the two is given or `--library` has no folder.
pub fn split_write_args(args: &[String]) -> Result<(Vec<String>, Option<PathBuf>), String> {
    let mut rest = Vec::new();
    let mut allow_write = false;
    let mut library = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--allow-write" => allow_write = true,
            "--library" => {
                let dir = it.next().ok_or("--library needs a directory")?;
                library = Some(PathBuf::from(dir));
            }
            _ => rest.push(arg.clone()),
        }
    }
    match (allow_write, library) {
        (false, None) => Ok((rest, None)),
        (true, Some(dir)) => Ok((rest, Some(dir))),
        (true, None) => {
            Err("--allow-write needs --library <dir> (the folder it may change)".into())
        }
        (false, Some(_)) => Err("--library is only used with --allow-write".into()),
    }
}

/// Serve over the given streams (tests use in-memory pipes).
///
/// # Errors
/// Argument, content or transport errors as a message.
pub fn serve_with(
    args: &[String],
    exe: &Path,
    reader: impl BufRead,
    writer: impl Write,
) -> Result<(), String> {
    let (rest, library) = split_write_args(args)?;
    let config = parse_args(&rest, exe)?;
    let tools = borealis_mcp::Tools::load(&config)?;
    eprintln!("borealis --mcp: ready ({} elements)", tools.element_count());
    match library {
        None => borealis_mcp::serve(&tools, reader, writer),
        Some(dir) => serve_write(&tools, &config, &dir, reader, writer),
    }
}

#[cfg(desktop)]
fn serve_write(
    tools: &borealis_mcp::Tools,
    config: &McpConfig,
    library: &Path,
    reader: impl BufRead,
    writer: impl Write,
) -> Result<(), String> {
    let content_root = config
        .content_dirs
        .first()
        .cloned()
        .ok_or("no content directory found; pass --content-dir <dir>")?;
    let drive = crate::drive::Drive::open(library, content_root).map_err(|e| e.to_string())?;
    eprintln!(
        "borealis --mcp: write mode, library {}",
        drive.root().display()
    );
    let extension = crate::drive::mcp::DriveTools::new(drive);
    borealis_mcp::serve_extended(tools, &extension, reader, writer)
}

#[cfg(not(desktop))]
fn serve_write(
    _tools: &borealis_mcp::Tools,
    _config: &McpConfig,
    _library: &Path,
    _reader: impl BufRead,
    _writer: impl Write,
) -> Result<(), String> {
    Err("write mode is only available in the desktop app".into())
}

/// Run on the process stdio; returns the exit code. Logs go to stderr only.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let result = serve_with(
        args,
        &exe,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    );
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("borealis --mcp: {e}");
            1
        }
    }
}

#[cfg(test)]
mod data_dir_tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn dir(platform: Platform, pairs: &[(&str, &str)]) -> Option<String> {
        user_data_dir(platform, &lookup(pairs))
    }

    #[test]
    fn macos_uses_application_support_under_home() {
        for home in ["/Users/mira", "/Users/mira/"] {
            assert_eq!(
                dir(Platform::MacOs, &[("HOME", home)]).as_deref(),
                Some("/Users/mira/Library/Application Support/app.borealis")
            );
        }
        assert!(dir(Platform::MacOs, &[]).is_none());
    }

    #[test]
    fn windows_uses_roaming_appdata_then_the_profile() {
        let appdata = ("APPDATA", r"C:\Users\mira\AppData\Roaming");
        assert_eq!(
            dir(Platform::Windows, &[appdata, ("USERPROFILE", r"C:\Users\x")]).as_deref(),
            Some(r"C:\Users\mira\AppData\Roaming\app.borealis")
        );
        assert_eq!(
            dir(Platform::Windows, &[("USERPROFILE", r"C:\Users\mira\")]).as_deref(),
            Some(r"C:\Users\mira\AppData\Roaming\app.borealis")
        );
        assert!(dir(Platform::Windows, &[]).is_none());
    }

    #[test]
    fn linux_honours_an_absolute_xdg_data_home_else_local_share() {
        let home = ("HOME", "/home/mira");
        assert_eq!(
            dir(Platform::Linux, &[("XDG_DATA_HOME", "/data/xdg"), home]).as_deref(),
            Some("/data/xdg/app.borealis")
        );
        for ignored in ["rel", " ", r"C:\data"] {
            assert_eq!(
                dir(Platform::Linux, &[("XDG_DATA_HOME", ignored), home]).as_deref(),
                Some("/home/mira/.local/share/app.borealis"),
                "XDG_DATA_HOME={ignored:?} is not an absolute Linux path"
            );
        }
        assert!(dir(Platform::Linux, &[]).is_none());
    }

    #[test]
    fn absolute_paths_follow_the_target_platform_not_the_host() {
        assert!(Platform::Linux.is_absolute("/data"));
        assert!(!Platform::Linux.is_absolute(r"C:\data"));
        assert!(Platform::Windows.is_absolute(r"C:\data"));
        assert!(Platform::Windows.is_absolute("C:/data"));
        assert!(Platform::Windows.is_absolute(r"\\server\share"));
        assert!(!Platform::Windows.is_absolute("/data"));
        assert!(!Platform::Windows.is_absolute("C:data"));
    }

    #[test]
    fn this_machine_resolves_the_folder_of_its_own_platform() {
        let env = |name: &str| std::env::var(name).ok();
        let text = user_data_dir(Platform::current(), &env).expect("HOME or APPDATA is set");
        assert!(Path::new(&text).is_absolute(), "{text}");
        assert!(text.ends_with(APP_IDENTIFIER), "{text}");
    }
}
