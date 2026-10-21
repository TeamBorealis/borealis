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

/// Parse MCP-mode arguments.
///
/// # Errors
/// A message for unknown or incomplete arguments.
pub fn parse_args(args: &[String], exe: &Path) -> Result<McpConfig, String> {
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
