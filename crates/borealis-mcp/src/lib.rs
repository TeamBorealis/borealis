//! Borealis MCP server: read-only content tools over stdio for the user's local
//! `claude` / `codex` during AI tasks.
//!
//! Hand-rolled JSON-RPC 2.0 (`initialize`, `ping`, `tools/list`, `tools/call`), newline-delimited,
//! instead of the `rmcp` SDK: the surface is tiny, it keeps the dependency tree and the
//! read-only guarantee auditable. No writes, no network; the only filesystem access is reading the
//! configured content directories at startup. Logs go to stderr only.

pub mod config;
pub mod rpc;
pub mod tools;

use std::io::{BufRead, Write};

pub use config::McpConfig;
pub use tools::Tools;

/// Load the content and serve MCP on the process stdin/stdout until stdin closes.
///
/// # Errors
/// A message when the content cannot be loaded or the transport fails.
pub fn run_stdio(config: &McpConfig) -> Result<(), String> {
    let tools = Tools::load(config)?;
    eprintln!("borealis-mcp: ready ({} elements)", tools.element_count());
    serve(&tools, std::io::stdin().lock(), std::io::stdout().lock())
}

/// Serve MCP over arbitrary streams (used by tests).
///
/// # Errors
/// A message on transport I/O errors.
pub fn serve(tools: &Tools, reader: impl BufRead, writer: impl Write) -> Result<(), String> {
    rpc::serve(tools, reader, writer).map_err(|e| format!("transport error: {e}"))
}
