//! Scope enforcement with a real sessions source (two campaigns): a run reads only the campaign it
//! was granted, whatever the model asks and whatever the source returns.

use std::path::PathBuf;

use borealis_mcp::config::SessionScope;
use borealis_mcp::tools::{SessionChunk, SessionSource};
use borealis_mcp::{McpConfig, Tools};
use serde_json::{json, Value};

fn chunk(campaign: &str, entry: &str, text: &str) -> SessionChunk {
    SessionChunk {
        cite: format!("session:{entry}#1"),
        title: format!("Entry {entry}"),
        path: format!("{campaign} > {entry}"),
        text: text.to_string(),
        campaign: campaign.to_string(),
    }
}

/// Two campaigns. `leaky` makes `search` ignore the campaign it was asked for, like a buggy
/// source would; the tool must still not hand the other campaign's chunks to the model.
struct TwoCampaigns {
    chunks: Vec<SessionChunk>,
    leaky: bool,
}

impl TwoCampaigns {
    fn new(leaky: bool) -> Self {
        Self {
            chunks: vec![
                chunk("camp-a", "a1", "The dragon sleeps under Mount Karn."),
                chunk("camp-b", "b1", "The dragon is the duke in disguise."),
            ],
            leaky,
        }
    }
}

impl SessionSource for TwoCampaigns {
    fn search(&self, campaign: &str, query: &str, k: usize) -> Vec<SessionChunk> {
        self.chunks
            .iter()
            .filter(|c| self.leaky || c.campaign == campaign)
            .filter(|c| c.text.to_lowercase().contains(&query.to_lowercase()))
            .take(k)
            .cloned()
            .collect()
    }

    fn passage(&self, cite: &str) -> Option<SessionChunk> {
        self.chunks.iter().find(|c| c.cite == cite).cloned()
    }
}

fn tools(scope: SessionScope, leaky: bool) -> Tools {
    let config = McpConfig {
        content_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content")],
        enabled_packs: Some(["core".to_string()].into()),
        session_scope: scope,
        ..McpConfig::default()
    };
    Tools::load(&config)
        .unwrap()
        .with_sessions(Box::new(TwoCampaigns::new(leaky)))
}

fn campaign(id: &str) -> SessionScope {
    SessionScope::Campaign(id.to_string())
}

fn call(t: &Tools, name: &str, args: &Value) -> Option<Value> {
    t.call(name, args, true)
}

fn listed(t: &Tools) -> Vec<String> {
    t.list(true)
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap().to_string())
        .collect()
}

fn cites(result: &Value) -> Vec<String> {
    result["structuredContent"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cite"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn search_sessions_without_scope_is_absent_and_rejected() {
    let t = tools(SessionScope::None, false);
    assert!(!listed(&t).iter().any(|n| n == "search_sessions"));
    assert!(call(&t, "search_sessions", &json!({"query": "dragon"})).is_none());
    // Not even a campaign argument can switch it on.
    assert!(call(
        &t,
        "search_sessions",
        &json!({"query": "dragon", "campaign_id": "camp-a"})
    )
    .is_none());
}

#[test]
fn get_passage_refuses_session_cites_without_scope() {
    let t = tools(SessionScope::None, false);
    for cite in ["session:a1#1", "session:b1#1", "session:missing#1"] {
        let r = call(&t, "get_passage", &json!({"cite": cite})).unwrap();
        assert_eq!(r["isError"], true, "{cite}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(!text.contains("dragon"), "no session text leaks: {text}");
    }
}

#[test]
fn a_scoped_run_lists_the_tool_and_finds_only_its_campaign() {
    let t = tools(campaign("camp-a"), false);
    assert!(listed(&t).iter().any(|n| n == "search_sessions"));
    let r = call(&t, "search_sessions", &json!({"query": "dragon"})).unwrap();
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(cites(&r), ["session:a1#1"]);
    assert!(!r.to_string().contains("duke"));

    let other = tools(campaign("camp-b"), false);
    let r = call(&other, "search_sessions", &json!({"query": "dragon"})).unwrap();
    assert_eq!(cites(&r), ["session:b1#1"]);
}

#[test]
fn a_source_that_over_returns_still_cannot_leak_another_campaign() {
    let t = tools(campaign("camp-a"), true);
    let r = call(&t, "search_sessions", &json!({"query": "dragon", "k": 20})).unwrap();
    assert_eq!(cites(&r), ["session:a1#1"], "camp-b filtered by the tool");
    assert!(!r.to_string().contains("duke"));
}

#[test]
fn a_scoped_run_cannot_read_another_campaigns_passage() {
    let t = tools(campaign("camp-a"), false);
    let own = call(&t, "get_passage", &json!({"cite": "session:a1#1"})).unwrap();
    assert_eq!(own["isError"], false, "{own}");
    assert!(own["structuredContent"]["text"]
        .as_str()
        .unwrap()
        .contains("Mount Karn"));

    let other = call(&t, "get_passage", &json!({"cite": "session:b1#1"})).unwrap();
    let missing = call(&t, "get_passage", &json!({"cite": "session:zzz#1"})).unwrap();
    assert_eq!(other["isError"], true);
    assert!(!other.to_string().contains("duke"));
    // Another campaign's cite looks exactly like one that does not exist.
    let strip =
        |v: &Value, cite: &str| v["content"][0]["text"].as_str().unwrap().replace(cite, "X");
    assert_eq!(
        strip(&other, "session:b1#1"),
        strip(&missing, "session:zzz#1")
    );
}

#[test]
fn the_scope_cannot_be_widened_by_arguments_or_cite_tricks() {
    let t = tools(campaign("camp-a"), true);
    for args in [
        json!({"query": "dragon", "campaign": "camp-b"}),
        json!({"query": "dragon", "campaign_id": "camp-b"}),
        json!({"query": "dragon", "session_scope": "camp-b"}),
    ] {
        let r = call(&t, "search_sessions", &args).unwrap();
        assert_eq!(r["isError"], true, "{args}");
    }
    for cite in [
        "session:b1#1 ",
        "SESSION:b1#1",
        "session:b1",
        "session:b1#1#1",
        "../session:b1#1",
        "session:a1#1/../b1#1",
    ] {
        let r = call(&t, "get_passage", &json!({"cite": cite})).unwrap();
        assert!(!r.to_string().contains("duke"), "cite {cite:?} leaked: {r}");
    }
}

#[test]
fn session_text_is_unreachable_through_the_rules_and_lore_tools() {
    let t = tools(campaign("camp-a"), false);
    for name in ["search_rules", "search_lore", "search_content"] {
        let r = call(&t, name, &json!({"query": "dragon"})).unwrap();
        assert!(!r.to_string().contains("Mount Karn"), "{name}");
    }
}
