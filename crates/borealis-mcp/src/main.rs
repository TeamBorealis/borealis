//! `borealis-mcp --content <dir>... [--packs a,b,c]` : stdio MCP server.

use std::collections::BTreeSet;
use std::path::PathBuf;

use borealis_mcp::{run_stdio, McpConfig};

fn parse_args() -> Result<McpConfig, String> {
    let mut config = McpConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--content" | "--library" => {
                let dir = args
                    .next()
                    .ok_or_else(|| format!("{arg} needs a directory"))?;
                config.content_dirs.push(PathBuf::from(dir));
            }
            "--packs" => {
                let list = args.next().ok_or("--packs needs a comma-separated list")?;
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
        return Err("usage: borealis-mcp --content <dir> [--library <dir>] [--packs a,b]".into());
    }
    Ok(config)
}

fn main() {
    let result = parse_args().and_then(|config| run_stdio(&config));
    if let Err(e) = result {
        eprintln!("borealis-mcp: {e}");
        std::process::exit(1);
    }
}
