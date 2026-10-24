//! Newline-delimited JSON-RPC 2.0 transport for MCP over stdio, dual-era.
//!
//! Primary protocol: MCP **2026-07-28** (stateless; per-request `_meta`, mandatory
//! `server/discover`, `resultType` on every result, cacheable `tools/list`).
//! Spec: <https://modelcontextprotocol.io/specification/2026-07-28/> with
//! - versioning: `.../basic/versioning` (`UnsupportedProtocolVersionError` -32022, dual-era rules)
//! - stdio binding and backward compatibility: `.../basic/transports/stdio`
//! - `_meta`, error-code allocation, `resultType`: `.../basic/index`
//! - discovery: `.../server/discover`; tools: `.../server/tools`
//!
//! Legacy path (`initialize` handshake, `2025-11-25` and earlier): selected per process when the
//! first request is `initialize`; requests without modern `_meta` are then served in the legacy
//! shapes (no `resultType`, `ping` allowed). A request that carries modern `_meta` is always served
//! statelessly, regardless of the connection era.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::tools::{tool_result, Tools};
use crate::ToolExtension;

/// What a server answers with: the read-only tools plus an optional extension (write mode).
struct Host<'a> {
    tools: &'a Tools,
    extension: Option<&'a dyn ToolExtension>,
}

impl Host<'_> {
    fn instructions(&self) -> String {
        match self.extension.and_then(ToolExtension::instructions) {
            Some(extra) => format!("{INSTRUCTIONS} {extra}"),
            None => INSTRUCTIONS.to_string(),
        }
    }

    fn list(&self, with_output_schema: bool) -> Value {
        let mut list = self.tools.list(with_output_schema);
        let Some(ext) = self.extension else {
            return list;
        };
        let mut extra = ext.list();
        if !with_output_schema {
            for t in &mut extra {
                if let Some(obj) = t.as_object_mut() {
                    obj.remove("outputSchema");
                    obj.remove("annotations");
                }
            }
        }
        if let Some(items) = list.as_array_mut() {
            items.extend(extra);
        }
        list
    }

    fn call(&self, name: &str, args: &Value, structured: bool) -> Option<Value> {
        if let Some(r) = self.tools.call(name, args, structured) {
            return Some(r);
        }
        let outcome = self.extension?.call(name, args)?;
        Some(tool_result(outcome, structured))
    }
}

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// `UnsupportedProtocolVersionError` (spec 2026-07-28, basic/index error codes).
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// The modern protocol revision.
pub const MODERN_VERSION: &str = "2026-07-28";
/// Handshake-based revisions answered after `initialize`, newest first.
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
/// Content is fixed for the process lifetime, so lists may be cached for an hour.
const TTL_MS: u64 = 3_600_000;
const INSTRUCTIONS: &str = "Read-only Borealis content tools: search, inspect, validate, class sweep, rules terms. Use list_core_ids and search_content before inventing ids.";

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code": code, "message": message});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": error})
}

fn server_info() -> Value {
    json!({"name": "borealis", "version": env!("CARGO_PKG_VERSION")})
}

fn with_modern_fields(mut result: Value) -> Value {
    result["resultType"] = json!("complete");
    result["_meta"] = json!({ META_SERVER_INFO: server_info() });
    result
}

/// The era of the connection, fixed by an `initialize` request (legacy only).
#[derive(Default)]
struct Connection {
    /// Negotiated legacy protocol version after `initialize`.
    legacy: Option<String>,
}

enum Era {
    Modern,
    Legacy(String),
}

fn unsupported(id: Value, requested: &Value) -> Value {
    error(
        id,
        UNSUPPORTED_PROTOCOL_VERSION,
        "Unsupported protocol version",
        Some(json!({"supported": [MODERN_VERSION], "requested": requested})),
    )
}

/// Decide the era of one request, or the error response to send.
fn era_of(conn: &Connection, id: &Value, params: &Value) -> Result<Era, Value> {
    let meta = params.get("_meta").and_then(Value::as_object);
    if let Some(version) = meta.and_then(|m| m.get(META_VERSION)) {
        if version.as_str() != Some(MODERN_VERSION) {
            return Err(unsupported(id.clone(), version));
        }
        let caps = meta.and_then(|m| m.get(META_CAPABILITIES));
        if !caps.is_some_and(Value::is_object) {
            return Err(error(
                id.clone(),
                INVALID_PARAMS,
                "missing required _meta[\"io.modelcontextprotocol/clientCapabilities\"]",
                None,
            ));
        }
        return Ok(Era::Modern);
    }
    match &conn.legacy {
        Some(version) => Ok(Era::Legacy(version.clone())),
        None => Err(error(
            id.clone(),
            INVALID_PARAMS,
            "missing required _meta[\"io.modelcontextprotocol/protocolVersion\"] (send server/discover or initialize first)",
            None,
        )),
    }
}

fn initialize(host: &Host, conn: &mut Connection, id: Value, params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| LEGACY_VERSIONS.contains(v))
        .unwrap_or(LEGACY_VERSIONS[0]);
    conn.legacy = Some(version.to_string());
    json!({"jsonrpc": "2.0", "id": id, "result": {
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": server_info(),
        "instructions": host.instructions(),
    }})
}

fn discover(host: &Host, id: &Value) -> Value {
    let result = with_modern_fields(json!({
        "supportedVersions": [MODERN_VERSION],
        "capabilities": {"tools": {}, "extensions": {}},
        "instructions": host.instructions(),
        "ttlMs": TTL_MS,
        "cacheScope": "private",
    }));
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn handle_tools(host: &Host, era: &Era, id: Value, method: &str, params: &Value) -> Value {
    let legacy_version = match era {
        Era::Legacy(v) => Some(v.as_str()),
        Era::Modern => None,
    };
    // `outputSchema` and `structuredContent` exist from 2025-06-18 on.
    let structured = legacy_version != Some("2024-11-05");
    let result = if method == "tools/list" {
        let mut r = json!({"tools": host.list(structured)});
        if legacy_version.is_none() {
            r["ttlMs"] = json!(TTL_MS);
            r["cacheScope"] = json!("private");
        }
        r
    } else {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error(
                id,
                INVALID_PARAMS,
                "tools/call requires a string `name`",
                None,
            );
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return error(id, INVALID_PARAMS, "`arguments` must be an object", None);
        }
        match host.call(name, &args, structured) {
            Some(r) => r,
            None => return error(id, INVALID_PARAMS, &format!("Unknown tool: {name}"), None),
        }
    };
    let result = if legacy_version.is_none() {
        with_modern_fields(result)
    } else {
        result
    };
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Handle one parsed message. `None` means no response (a notification).
fn handle(host: &Host, conn: &mut Connection, msg: &Value) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(error(
            Value::Null,
            INVALID_REQUEST,
            "request must be a JSON object",
            None,
        ));
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        // A response or a malformed request; this server never sends requests.
        return obj
            .get("id")
            .map(|id| error(id.clone(), INVALID_REQUEST, "missing method", None));
    };
    let id = match obj.get("id") {
        None => return None, // notification (notifications/initialized, notifications/cancelled, ...)
        Some(id) if id.is_string() || id.is_i64() || id.is_u64() => id.clone(),
        Some(_) => {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "id must be a string or integer",
                None,
            ))
        }
    };
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    if method == "initialize" {
        return Some(initialize(host, conn, id, &params));
    }
    // Methods removed in 2026-07-28 (ping, logging/setLevel, ...) and unknown ones fall through.
    let known = matches!(method, "server/discover" | "tools/list" | "tools/call")
        || (method == "ping" && conn.legacy.is_some());
    if !known {
        let message = format!("method not found: {method}");
        return Some(error(id, METHOD_NOT_FOUND, &message, None));
    }
    let era = match era_of(conn, &id, &params) {
        Ok(era) => era,
        Err(response) => return Some(response),
    };
    Some(match (method, &era) {
        ("server/discover", Era::Modern) => discover(host, &id),
        // `ping` is a legacy-only method; a modern request for it is method-not-found.
        ("ping", Era::Legacy(_)) => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
        ("tools/list" | "tools/call", era) => handle_tools(host, era, id, method, &params),
        _ => error(
            id,
            METHOD_NOT_FOUND,
            &format!("method not found: {method}"),
            None,
        ),
    })
}

/// Longest accepted input line; longer lines are discarded and answered with an error.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
const INTERNAL_ERROR: i64 = -32603;

/// One input line, or a marker that it exceeded the cap (its bytes were dropped).
#[derive(Debug, PartialEq, Eq)]
enum Line {
    Text(Vec<u8>),
    TooLong,
}

/// Read one `\n`-terminated line holding at most `max` bytes in memory. An overlong line is
/// consumed to its end without being buffered. `None` at EOF.
fn read_line_bounded(reader: &mut impl BufRead, max: usize) -> std::io::Result<Option<Line>> {
    let mut buf = Vec::new();
    let mut overflow = false;
    let mut seen_any = false;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(match (seen_any, overflow) {
                (false, _) => None,
                (true, true) => Some(Line::TooLong),
                (true, false) => Some(Line::Text(buf)),
            });
        }
        seen_any = true;
        let (take, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(pos) => (pos, true),
            None => (chunk.len(), false),
        };
        if !overflow {
            if buf.len() + take > max {
                overflow = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        reader.consume(take + usize::from(done));
        if done {
            return Ok(Some(if overflow {
                Line::TooLong
            } else {
                Line::Text(buf)
            }));
        }
    }
}

/// Run `work`; a panic becomes a -32603 response instead of ending the server.
fn guarded(id: Value, work: impl FnOnce() -> Option<Value>) -> Option<Value> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
        Some(error(
            id,
            INTERNAL_ERROR,
            "internal error while handling the request",
            None,
        ))
    })
}

fn request_id(msg: &Value) -> Value {
    match msg.get("id") {
        Some(id) if id.is_string() || id.is_i64() || id.is_u64() => id.clone(),
        _ => Value::Null,
    }
}

/// Serve requests from `reader` until EOF, writing one response line per request.
///
/// # Errors
/// I/O errors on the transport.
pub fn serve(
    tools: &Tools,
    extension: Option<&dyn ToolExtension>,
    mut reader: impl BufRead,
    mut writer: impl Write,
) -> std::io::Result<()> {
    let host = Host { tools, extension };
    let mut conn = Connection::default();
    while let Some(line) = read_line_bounded(&mut reader, MAX_LINE_BYTES)? {
        let bytes = match line {
            Line::TooLong => {
                let message = format!("request line exceeds {MAX_LINE_BYTES} bytes");
                writeln!(
                    writer,
                    "{}",
                    error(Value::Null, INVALID_REQUEST, &message, None)
                )?;
                writer.flush()?;
                continue;
            }
            Line::Text(bytes) => bytes,
        };
        if bytes.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let response = match serde_json::from_slice::<Value>(&bytes) {
            Ok(msg) => guarded(request_id(&msg), || handle(&host, &mut conn, &msg)),
            Err(e) => Some(error(
                Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
                None,
            )),
        };
        if let Some(response) = response {
            writeln!(writer, "{response}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn bounded_reader_drops_an_overlong_line_and_keeps_the_next() {
        let mut input = Cursor::new(b"aaaaaaaaaa\nok\n".to_vec());
        assert_eq!(
            read_line_bounded(&mut input, 4).unwrap(),
            Some(Line::TooLong)
        );
        assert_eq!(
            read_line_bounded(&mut input, 4).unwrap(),
            Some(Line::Text(b"ok".to_vec()))
        );
        assert_eq!(read_line_bounded(&mut input, 4).unwrap(), None);
    }

    #[test]
    fn bounded_reader_handles_overlong_final_line_without_newline() {
        let mut input = Cursor::new(b"xxxxxxxx".to_vec());
        assert_eq!(
            read_line_bounded(&mut input, 4).unwrap(),
            Some(Line::TooLong)
        );
        assert_eq!(read_line_bounded(&mut input, 4).unwrap(), None);
    }

    #[test]
    fn a_panicking_handler_becomes_an_internal_error() {
        let out = guarded(json!(7), || panic!("boom")).expect("a response");
        assert_eq!(out["id"], json!(7));
        assert_eq!(out["error"]["code"], INTERNAL_ERROR);
        assert_eq!(guarded(json!(1), || None), None);
    }
}
