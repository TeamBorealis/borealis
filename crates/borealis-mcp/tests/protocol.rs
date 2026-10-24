//! Protocol behaviour: modern 2026-07-28 path, legacy handshake path and malformed input.

mod common;

use common::{Session, MODERN};
use serde_json::json;

const TOOLS: [&str; 6] = [
    "search_content",
    "get_element",
    "list_core_ids",
    "validate_element",
    "class_sweep",
    "explain_rules_term",
];

#[test]
fn modern_discover_reports_versions_capabilities_and_identity() {
    let mut s = Session::start();
    let r = s.modern("server/discover", json!({}));
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["supportedVersions"], json!([MODERN]));
    assert!(res["capabilities"]["tools"].is_object());
    assert!(res["capabilities"]["extensions"].is_object());
    assert_eq!(
        res["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "borealis"
    );
    assert_eq!(res["cacheScope"], "private");
    assert!(res["ttlMs"].is_u64());
}

#[test]
fn modern_tools_list_has_all_tools_with_schemas_and_cache_fields() {
    let mut s = Session::start();
    let r = s.modern("tools/list", json!({}));
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["cacheScope"], "private");
    assert!(res["ttlMs"].is_u64());
    let names: Vec<&str> = res["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, TOOLS, "deterministic order");
    for tool in res["tools"].as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert_eq!(tool["outputSchema"]["type"], "object", "{tool}");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
    }
}

#[test]
fn modern_call_returns_result_type_and_structured_content() {
    let mut s = Session::start();
    let r = s.modern(
        "tools/call",
        json!({"name": "list_core_ids", "arguments": {"category": "sizes"}}),
    );
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["isError"], false);
    assert!(res["structuredContent"]["count"].as_u64().unwrap() > 0);
    assert_eq!(res["content"][0]["type"], "text");
}

#[test]
fn unsupported_protocol_version_is_32022_with_supported_list() {
    let mut s = Session::start();
    let r = s.request(
        "tools/list",
        json!({"_meta": {
            "io.modelcontextprotocol/protocolVersion": "1900-01-01",
            "io.modelcontextprotocol/clientCapabilities": {}
        }}),
    );
    assert_eq!(r["error"]["code"], -32022);
    assert_eq!(r["error"]["data"]["supported"], json!([MODERN]));
    assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
}

#[test]
fn removed_methods_are_method_not_found() {
    let mut s = Session::start();
    for method in ["ping", "logging/setLevel", "resources/list"] {
        let r = s.modern(method, json!({}));
        assert_eq!(r["error"]["code"], -32601, "{method}: {r}");
    }
}

#[test]
fn missing_meta_or_capabilities_is_invalid_params() {
    let mut s = Session::start();
    let r = s.request("tools/list", json!({}));
    assert_eq!(r["error"]["code"], -32602);
    let r = s.request(
        "tools/list",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}),
    );
    assert_eq!(r["error"]["code"], -32602);
}

#[test]
fn unknown_tool_and_bad_arguments() {
    let mut s = Session::start();
    let r = s.modern("tools/call", json!({"name": "nope", "arguments": {}}));
    assert_eq!(r["error"]["code"], -32602);
    // Tool execution errors are results with isError, not protocol errors.
    let r = s.modern(
        "tools/call",
        json!({"name": "get_element", "arguments": {"id": "ID_NOPE"}}),
    );
    assert_eq!(r["result"]["isError"], true);
    let r = s.modern(
        "tools/call",
        json!({"name": "search_content", "arguments": {"query": "x", "bogus": 1}}),
    );
    assert_eq!(r["result"]["isError"], true);
}

#[test]
fn malformed_requests_return_jsonrpc_errors() {
    let mut s = Session::start();
    let r = s.send_raw("{not json");
    assert_eq!(r["error"]["code"], -32700);
    assert!(r["id"].is_null());
    let r = s.send_raw("[1,2,3]");
    assert_eq!(r["error"]["code"], -32600);
    let r = s.send_raw(r#"{"jsonrpc":"2.0","id":9,"method":"frobnicate"}"#);
    assert_eq!(r["error"]["code"], -32601);
    assert_eq!(r["id"], 9);
    let r = s.send_raw(r#"{"jsonrpc":"2.0","id":null,"method":"tools/list"}"#);
    assert_eq!(r["error"]["code"], -32600);
    // The server keeps serving after bad input.
    assert!(s.modern("server/discover", json!({}))["result"].is_object());
}

#[test]
fn legacy_handshake_initialize_list_call() {
    let mut s = Session::start();
    let init = s.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "claude-code", "version": "2"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert_eq!(init["result"]["serverInfo"]["name"], "borealis");
    s.notify(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let list = s.request("tools/list", json!({}));
    assert_eq!(
        list["result"]["tools"].as_array().unwrap().len(),
        TOOLS.len()
    );
    assert!(list["result"].get("resultType").is_none(), "legacy shape");

    let call = s.request(
        "tools/call",
        json!({"name": "list_core_ids", "arguments": {"category": "sizes"}}),
    );
    assert_eq!(call["result"]["isError"], false);
    assert!(call["result"].get("resultType").is_none());
    assert!(call["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("sizes"));

    assert_eq!(s.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn legacy_initialize_with_unknown_version_negotiates_newest_legacy() {
    let mut s = Session::start();
    let init = s.request(
        "initialize",
        json!({"protocolVersion": "2099-01-01", "capabilities": {}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn oversized_line_is_rejected_and_the_server_keeps_serving() {
    let mut s = Session::start();
    let huge = format!("{{\"pad\":\"{}\"}}", "a".repeat(5 * 1024 * 1024));
    let r = s.send_raw(&huge);
    assert_eq!(r["error"]["code"], -32600, "{r}");
    assert!(r["id"].is_null());
    let ok = s.modern("server/discover", json!({}));
    assert_eq!(ok["result"]["resultType"], "complete");
}
