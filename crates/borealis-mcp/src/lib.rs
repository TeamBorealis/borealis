//! Borealis MCP server: read-only content tools over stdio for the user's local
//! `claude` / `codex` during AI tasks.
//!
//! Hand-rolled JSON-RPC 2.0 (`initialize`, `ping`, `tools/list`, `tools/call`), newline-delimited,
//! instead of the `rmcp` SDK: the surface is tiny, it keeps the dependency tree and the
//! read-only guarantee auditable. No writes, no network; the only filesystem access is reading the
//! configured content directories at startup. Logs go to stderr only. A host may add its own
//! tools through [`ToolExtension`] (the desktop app's explicit `--allow-write` mode); without
//! one the server is exactly the read-only tool set.

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
    rpc::serve(tools, None, reader, writer).map_err(|e| format!("transport error: {e}"))
}

/// Extra tools a host registers next to the read-only ones (the desktop app's opt-in write
/// mode). The transport, protocol versions and result shapes stay this crate's.
pub trait ToolExtension {
    /// `tools/list` entries (name, title, description, `inputSchema`, `outputSchema`,
    /// `annotations`), appended after the read-only tools in this order.
    fn list(&self) -> Vec<serde_json::Value>;
    /// Run a tool: `None` when the name is not one of this extension's tools, otherwise the
    /// structured result or a message for the model.
    fn call(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Option<Result<serde_json::Value, String>>;
    /// A sentence appended to the server instructions.
    fn instructions(&self) -> Option<&str> {
        None
    }
}

/// Serve MCP with an extension's tools added to the read-only ones.
///
/// # Errors
/// A message on transport I/O errors.
pub fn serve_extended(
    tools: &Tools,
    extension: &dyn ToolExtension,
    reader: impl BufRead,
    writer: impl Write,
) -> Result<(), String> {
    rpc::serve(tools, Some(extension), reader, writer).map_err(|e| format!("transport error: {e}"))
}
