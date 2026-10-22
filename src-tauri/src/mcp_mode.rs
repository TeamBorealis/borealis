//! `borealis --mcp`: run the read-only Borealis MCP server on stdio instead of the GUI.
//!
//! Arguments: `--mcp [--content-dir <dir>]... [--packs a,b] [--allow-write --library <dir>]`.
//! Without `--content-dir` the bundled content next to the executable is used (installed bundle
//! layouts), so the command also works when launched by a CLI from the app bundle path.
//! `--allow-write --library <dir>` (desktop only, AI15) adds the character tools of
//! [`crate::drive`], scoped to that library folder; without it the server is read-only.

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
}

/// The per-user data folder of the app, exactly where the desktop app keeps it (Tauri's
/// `app_data_dir`): `~/Library/Application Support/<id>` on macOS, `%APPDATA%\<id>` on Windows,
/// `$XDG_DATA_HOME/<id>` (default `~/.local/share/<id>`) on Linux. Pure: `env` answers variable
/// lookups, so each platform is testable on any host.
pub(crate) fn user_data_dir(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.trim().is_empty());
    let base = match platform {
        Platform::MacOs => PathBuf::from(var("HOME")?).join("Library/Application Support"),
        Platform::Windows => var("APPDATA").map(PathBuf::from).or_else(|| {
            var("USERPROFILE").map(|u| PathBuf::from(u).join("AppData").join("Roaming"))
        })?,
        Platform::Linux => match var("XDG_DATA_HOME").map(PathBuf::from) {
            Some(x) if x.is_absolute() => x,
            _ => PathBuf::from(var("HOME")?).join(".local/share"),
        },
    };
    Some(base.join(APP_IDENTIFIER))
}

/// The user content library (imported packs) of this machine, when it exists.
fn user_library_dir() -> Option<PathBuf> {
    let env = |name: &str| std::env::var(name).ok();
    let dir = user_data_dir(Platform::current(), &env)?.join("library");
    dir.is_dir().then_some(dir)
}

/// Parse MCP-mode arguments.
///
/// # Errors
/// A message for unknown or incomplete arguments.
pub fn parse_args(args: &[String], exe: &Path) -> Result<McpConfig, String> {
    parse_args_with(args, exe, user_library_dir())
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

    fn tail(path: &Path, n: usize) -> Vec<String> {
        let parts: Vec<String> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        parts[parts.len() - n..].to_vec()
    }

    #[test]
    fn macos_uses_application_support_under_home() {
        let dir = user_data_dir(Platform::MacOs, &lookup(&[("HOME", "/Users/mira")])).unwrap();
        assert_eq!(
            tail(&dir, 3),
            ["Library", "Application Support", "app.borealis"]
        );
        assert!(dir.starts_with("/Users/mira"));
    }

    #[test]
    fn windows_uses_roaming_appdata_then_the_profile() {
        let appdata = user_data_dir(
            Platform::Windows,
            &lookup(&[("APPDATA", "C:/Users/mira/AppData/Roaming")]),
        )
        .unwrap();
        assert!(appdata.starts_with("C:/Users/mira/AppData/Roaming"));
        assert_eq!(tail(&appdata, 1), ["app.borealis"]);
        let profile = user_data_dir(
            Platform::Windows,
            &lookup(&[("USERPROFILE", "C:/Users/mira")]),
        )
        .unwrap();
        assert_eq!(tail(&profile, 3), ["AppData", "Roaming", "app.borealis"]);
        assert!(user_data_dir(Platform::Windows, &lookup(&[])).is_none());
    }

    #[test]
    fn linux_honours_an_absolute_xdg_data_home_else_local_share() {
        let xdg = user_data_dir(
            Platform::Linux,
            &lookup(&[("XDG_DATA_HOME", "/data/xdg"), ("HOME", "/home/mira")]),
        )
        .unwrap();
        assert_eq!(xdg, PathBuf::from("/data/xdg/app.borealis"));
        let relative = user_data_dir(
            Platform::Linux,
            &lookup(&[("XDG_DATA_HOME", "rel"), ("HOME", "/home/mira")]),
        )
        .unwrap();
        assert_eq!(
            relative,
            PathBuf::from("/home/mira/.local/share/app.borealis")
        );
        let blank = user_data_dir(
            Platform::Linux,
            &lookup(&[("XDG_DATA_HOME", " "), ("HOME", "/home/mira")]),
        )
        .unwrap();
        assert_eq!(blank, PathBuf::from("/home/mira/.local/share/app.borealis"));
        assert!(user_data_dir(Platform::Linux, &lookup(&[])).is_none());
    }
}
