//! Drive the real `borealis-mcp` binary over stdin/stdout pipes.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

pub const MODERN: &str = "2026-07-28";

pub fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

pub struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    pub fn start() -> Self {
        Self::start_with_library(None)
    }

    /// Start with the bundled content plus, when given, a user library directory.
    pub fn start_with_library(library: Option<&std::path::Path>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_borealis-mcp"));
        command.arg("--content").arg(content_root());
        if let Some(dir) = library {
            command.arg("--library").arg(dir);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn borealis-mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    pub fn send_raw(&mut self, line: &str) -> Value {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
        self.read()
    }

    pub fn notify(&mut self, value: &Value) {
        writeln!(self.stdin, "{value}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad response {line:?}: {e}"))
    }

    /// Send a request with explicit params; returns the whole JSON-RPC response.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let response = self.send_raw(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
        assert_eq!(
            response["id"],
            json!(id),
            "response id mismatch: {response}"
        );
        response
    }

    /// A modern (2026-07-28) request with the required `_meta`.
    pub fn modern(&mut self, method: &str, mut params: Value) -> Value {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": MODERN,
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        });
        self.request(method, params)
    }

    /// Modern `tools/call`; returns the `result` object.
    pub fn call(&mut self, name: &str, arguments: Value) -> Value {
        let r = self.modern("tools/call", json!({"name": name, "arguments": arguments}));
        assert!(r.get("error").is_none(), "{name}: {r}");
        r["result"].clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
