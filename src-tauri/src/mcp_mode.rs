//! `borealis --mcp`: run the read-only Borealis MCP server on stdio instead of the GUI.
//!
//! Arguments: `--mcp [--content-dir <dir>]... [--packs a,b]`. Without `--content-dir` the bundled
//! content next to the executable is used (installed bundle layouts), so the command also works
//! when launched by a CLI from the app bundle path.

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
fn default_content_dirs(exe: &Path) -> Vec<PathBuf> {
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
    let config = parse_args(args, exe)?;
    let tools = borealis_mcp::Tools::load(&config)?;
    borealis_mcp::serve(&tools, reader, writer)
}

/// Run on the process stdio; returns the exit code. Logs go to stderr only.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let result = parse_args(args, &exe).and_then(|c| borealis_mcp::run_stdio(&c));
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("borealis --mcp: {e}");
            1
        }
    }
}
