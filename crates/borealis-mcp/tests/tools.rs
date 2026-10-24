//! Tool behaviour through the real binary (modern protocol).

mod common;

use borealis_ai::task::{ContentValidator, JsonSchemaValidator};
use common::{content_root, Session};
use serde_json::{json, Value};

/// The structured result of a successful call, checked against the tool's `outputSchema`.
fn structured(s: &mut Session, name: &str, args: Value) -> Value {
    let result = s.call(name, args);
    assert_eq!(result["isError"], false, "{name}: {result}");
    let list = s.modern("tools/list", json!({}));
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap()
        .clone();
    let validator = JsonSchemaValidator::from_schema(&tool["outputSchema"]).unwrap();
    let out = result["structuredContent"].clone();
    let issues = validator.validate(&out);
    assert!(
        issues.is_empty(),
        "{name} violates its outputSchema: {issues:?}"
    );
    out
}

#[test]
fn search_grappled_2024_finds_the_condition() {
    let mut s = Session::start();
    let out = structured(
        &mut s,
        "search_content",
        json!({"query": "grappled", "ruleset": "2024", "k": 5}),
    );
    let ids: Vec<&str> = out["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"ID_2024_CONDITION_GRAPPLED"), "{ids:?}");
    assert!(out["hits"].as_array().unwrap().len() <= 5);
    let typed = structured(
        &mut s,
        "search_content",
        json!({"query": "grappled", "type": "Condition"}),
    );
    assert!(typed["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["type"] == "Condition"));
}

#[test]
fn get_element_returns_plain_text_description() {
    let mut s = Session::start();
    let out = structured(
        &mut s,
        "get_element",
        json!({"id": "ID_2024_CONDITION_GRAPPLED"}),
    );
    assert_eq!(out["name"], "Grappled");
    let text = out["description"].as_str().unwrap();
    assert!(!text.contains('<'), "html stripped: {text}");
    assert!(out.get("sheet").is_none());
}

#[test]
fn list_core_ids_categories() {
    let mut s = Session::start();
    let skills = structured(&mut s, "list_core_ids", json!({"category": "skills"}));
    assert_eq!(skills["count"], 18);
    let conditions = structured(
        &mut s,
        "list_core_ids",
        json!({"category": "conditions", "ruleset": "2024"}),
    );
    assert!(conditions["ids"]
        .to_string()
        .contains("ID_2024_CONDITION_GRAPPLED"));
    let bad = s.call("list_core_ids", json!({"category": "nope"}));
    assert_eq!(bad["isError"], true);
}

#[test]
fn validate_reports_dangling_reference() {
    let mut s = Session::start();
    let broken = json!({
        "id": "ID_HB_FEAT_BROKEN", "name": "Broken", "type": "Feat", "source": "Homebrew",
        "rules": [{"kind": "grant", "type": "Feat", "id": "ID_DOES_NOT_EXIST"}]
    });
    let out = structured(&mut s, "validate_element", json!({"json": broken}));
    assert_eq!(out["ok"], false);
    assert!(out["report"]["warnings"].as_u64().unwrap() >= 1, "{out}");
    assert!(out.to_string().contains("dangling_reference"), "{out}");

    let good =
        json!({"id": "ID_HB_FEAT_FINE", "name": "Fine", "type": "Feat", "source": "Homebrew"});
    let out = structured(&mut s, "validate_element", json!({"elements": [good]}));
    assert_eq!(out["ok"], true, "{out}");

    let with_ruleset = json!({"id": "ID_HB_FEAT_ED", "name": "Ed", "type": "Feat", "source": "Homebrew", "ruleset": "dnd2024"});
    let out = structured(
        &mut s,
        "validate_element",
        json!({"json": with_ruleset.clone(), "ruleset": "2024"}),
    );
    assert_eq!(out["ok"], true, "{out}");
    let out = structured(
        &mut s,
        "validate_element",
        json!({"elements": [with_ruleset], "ruleset": "dnd2024"}),
    );
    assert_eq!(out["ok"], true, "{out}");

    let schema_bad = structured(
        &mut s,
        "validate_element",
        json!({"json": {"elements": [{"id": "X"}]}}),
    );
    assert_eq!(schema_bad["stage"], "schema");
    assert_eq!(schema_bad["ok"], false);
}

#[test]
fn class_sweep_srd_fighter_passes_levels_1_to_20() {
    let root = content_root().join("srd-5.2.1/classes");
    let load = |f: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(root.join(f)).unwrap()).unwrap()
    };
    let mut elements = load("fighter.json")["elements"].as_array().unwrap().clone();
    elements.extend(
        load("fighter-champion.json")["elements"]
            .as_array()
            .unwrap()
            .clone(),
    );
    let bundle =
        json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": elements});

    let mut s = Session::start();
    let out = structured(
        &mut s,
        "class_sweep",
        json!({"class_json_bundle": bundle, "edition": "2024", "subclass": "Champion"}),
    );
    assert_eq!(out["class"], "ID_2024_CLASS_FIGHTER");
    assert_eq!(out["levels_reached"], 20, "{out}");
    assert_eq!(out["passed"], true, "{out}");
    let levels = out["levels"].as_array().unwrap();
    assert_eq!(levels.len(), 20);
    assert_eq!(levels[19]["prof"], 6);
    assert!(levels[19]["hp"].as_i64().unwrap() > levels[0]["hp"].as_i64().unwrap());

    let no_class = s.call(
        "class_sweep",
        json!({"class_json_bundle": {"elements": []}, "edition": "2024"}),
    );
    assert_eq!(no_class["isError"], true);
}

#[test]
fn explain_rules_term_exact_and_fuzzy() {
    let mut s = Session::start();
    let out = structured(
        &mut s,
        "explain_rules_term",
        json!({"term": "Grappled", "ruleset": "2024"}),
    );
    assert_eq!(out["exact"], true);
    assert!(out["matches"][0]["text"].as_str().unwrap().len() > 20);
    let miss = s.call("explain_rules_term", json!({"term": "zzzqqq"}));
    assert_eq!(miss["isError"], true);
}

/// Live smoke test against the user's installed `claude` CLI. Run with
/// `cargo test -p borealis-mcp -- --ignored --nocapture`.
#[test]
#[ignore = "needs the claude CLI, a login and network"]
fn live_claude_can_search_via_mcp() {
    let bin = env!("CARGO_BIN_EXE_borealis-mcp");
    let config = json!({"mcpServers": {"borealis": {
        "command": bin, "args": ["--content", content_root().to_string_lossy()]
    }}});
    let dir = std::env::temp_dir().join("borealis-mcp-live");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp.json");
    std::fs::write(&path, config.to_string()).unwrap();
    let out = std::process::Command::new("claude")
        .args(["-p", "Use the borealis search_content tool to search for \"grappled\" (ruleset 2024) and reply with the first result id only.",
               "--mcp-config"])
        .arg(&path)
        .args(["--strict-mcp-config", "--allowedTools", "mcp__borealis__*", "--output-format", "json"])
        .output()
        .expect("run claude");
    let text = String::from_utf8_lossy(&out.stdout);
    eprintln!("{text}");
    assert!(
        text.contains("ID_2024_CONDITION_GRAPPLED"),
        "claude did not surface the id: {text}"
    );
}

#[test]
fn oversized_bundle_is_a_tool_error_not_a_hang() {
    let mut s = common::Session::start();
    let elements: Vec<_> = (0..2_500)
        .map(|i| json!({"id": format!("ID_X_{i}"), "type": "Spell", "name": "x"}))
        .collect();
    let r = s.call("validate_element", json!({"elements": elements}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("too large"));
    let blob = "x".repeat(1_100_000);
    let r = s.call(
        "class_sweep",
        json!({"class_json_bundle": blob, "edition": "2024"}),
    );
    assert_eq!(r["isError"], true, "{r}");
}
