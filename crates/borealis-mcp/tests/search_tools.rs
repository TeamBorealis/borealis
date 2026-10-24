//! The search tools through the real binary: indexed path, in-memory fallback, passages, citations,
//! and the read-only guarantee on the app's index.

mod common;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::time::SystemTime;

use borealis_ai::task::{ContentValidator, JsonSchemaValidator};
use common::{build_index, Session};
use serde_json::{json, Value};

fn start(index: Option<&Path>, scope: Option<&str>) -> Session {
    let mut args: Vec<&OsStr> = Vec::new();
    if let Some(dir) = index {
        args.extend([OsStr::new("--search-dir"), dir.as_os_str()]);
    }
    if let Some(campaign) = scope {
        args.extend([OsStr::new("--session-scope"), OsStr::new(campaign)]);
    }
    Session::start_with_args(&args)
}

fn tool_names(s: &mut Session) -> Vec<String> {
    let r = s.modern("tools/list", json!({}));
    r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// A successful call, validated against the tool's `outputSchema`.
fn ok(s: &mut Session, name: &str, args: Value) -> Value {
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
    assert!(issues.is_empty(), "{name} violates its schema: {issues:?}");
    out
}

fn cites(out: &Value) -> Vec<String> {
    out["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cite"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn indexed_search_rules_lore_and_the_content_alias() {
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    let mut s = start(Some(dir.path()), None);

    let rules = ok(
        &mut s,
        "search_rules",
        json!({"query": "grappled", "ruleset": "2024"}),
    );
    assert!(
        cites(&rules).contains(&"ID_2024_CONDITION_GRAPPLED".to_string()),
        "{rules}"
    );
    assert!(rules["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["type"] == "Rules"
            || h["type"] == "Condition"
            || h["type"] == "Rules Glossary Entry"));

    let lore = ok(&mut s, "search_lore", json!({"query": "firball", "k": 3}));
    assert_eq!(lore["hits"][0]["name"], "Fireball", "typo tolerant, rank 1");
    assert_eq!(lore["hits"][0]["type"], "Spell");
    let spells = ok(
        &mut s,
        "search_lore",
        json!({"query": "fireball", "type": "Spell"}),
    );
    assert!(spells["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["type"] == "Spell"));

    let both = ok(&mut s, "search_content", json!({"query": "prone", "k": 10}));
    let keys = cites(&both);
    assert!(
        keys.iter().any(|c| c.starts_with("rules:")),
        "rules hits: {keys:?}"
    );
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "one hit per document");
    assert!(both["hits"].as_array().unwrap().len() <= 10);
}

#[test]
fn get_passage_returns_whole_chunks_capped_at_4000_chars() {
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    let mut s = start(Some(dir.path()), None);

    let rules = ok(
        &mut s,
        "search_rules",
        json!({"query": "opportunity attack reaction"}),
    );
    let rules_cite = cites(&rules)
        .into_iter()
        .find(|c| c.starts_with("rules:"))
        .expect("a rules section");
    let p = ok(&mut s, "get_passage", json!({"cite": rules_cite}));
    assert_eq!(p["cite"], rules_cite.as_str());
    assert!(!p["text"].as_str().unwrap().is_empty());
    assert!(p["text"].as_str().unwrap().chars().count() <= 4_000);
    assert!(!p["text"].as_str().unwrap().contains('<'), "plain text");

    let spell = ok(
        &mut s,
        "search_lore",
        json!({"query": "Fireball", "type": "Spell"}),
    );
    let spell_cite = spell["hits"][0]["cite"].as_str().unwrap().to_string();
    let p = ok(&mut s, "get_passage", json!({"cite": spell_cite}));
    assert_eq!(p["title"], "Fireball");
    assert!(!p["text"].as_str().unwrap().is_empty(), "{p}");

    // A document whose chunks are numbered resolves by its base key to the first chunk.
    let base = spell_cite.split('#').next().unwrap();
    let p = ok(&mut s, "get_passage", json!({"cite": base}));
    assert_eq!(p["title"], "Fireball");

    let unknown = s.call("get_passage", json!({"cite": "rules:nope/none"}));
    assert_eq!(unknown["isError"], true);
    let empty = s.call("get_passage", json!({"cite": "  "}));
    assert_eq!(empty["isError"], true);
    let extra = s.call("get_passage", json!({"cite": base, "campaign": "c1"}));
    assert_eq!(extra["isError"], true, "unknown arguments are refused");
}

#[test]
fn without_an_index_the_tools_fall_back_to_the_in_memory_content() {
    let missing = tempfile::tempdir().unwrap().path().join("never-built");
    for dir in [None, Some(missing.as_path())] {
        let mut s = start(dir, None);
        let rules = ok(
            &mut s,
            "search_rules",
            json!({"query": "grappled", "ruleset": "2024"}),
        );
        assert_eq!(rules["hits"][0]["cite"], "ID_2024_CONDITION_GRAPPLED");
        assert!(rules["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["type"] == "Condition" || h["type"] == "Rules Glossary Entry"));
        let lore = ok(&mut s, "search_lore", json!({"query": "fireball"}));
        assert!(lore["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["type"] != "Condition"));
        assert!(!lore["hits"].as_array().unwrap().is_empty());
        let p = ok(
            &mut s,
            "get_passage",
            json!({"cite": "ID_2024_CONDITION_GRAPPLED"}),
        );
        assert_eq!(p["title"], "Grappled");
        let r = s.call(
            "get_passage",
            json!({"cite": "rules:srd-5.2.1-rules-text/x"}),
        );
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not built"));
    }
    assert!(
        !missing.exists(),
        "the MCP process never creates the index folder"
    );
}

#[test]
fn a_stale_index_is_ignored_not_repaired() {
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    let manifest = dir.path().join("manifest.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let stale = text.replace("\"schema_version\": 1", "\"schema_version\": 99");
    assert_ne!(text, stale);
    std::fs::write(&manifest, &stale).unwrap();
    let mut s = start(Some(dir.path()), None);
    let out = ok(
        &mut s,
        "search_rules",
        json!({"query": "grappled", "ruleset": "2024"}),
    );
    assert_eq!(out["hits"][0]["cite"], "ID_2024_CONDITION_GRAPPLED");
    drop(s);
    assert_eq!(
        std::fs::read_to_string(&manifest).unwrap(),
        stale,
        "left as found"
    );
}

/// File name -> (size, mtime) of everything below `dir`, including dot files.
fn snapshot(dir: &Path) -> BTreeMap<String, (u64, SystemTime)> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, (u64, SystemTime)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let meta = entry.metadata().unwrap();
            let name = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if meta.is_dir() {
                walk(&path, base, out);
            } else {
                out.insert(name, (meta.len(), meta.modified().unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn the_mcp_process_never_writes_to_the_index() {
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    let before = snapshot(dir.path());
    assert!(!before.is_empty());
    {
        let mut s = start(Some(dir.path()), Some("camp-1"));
        for q in ["grappled", "firball", "opportunity attack"] {
            ok(&mut s, "search_rules", json!({"query": q}));
            ok(&mut s, "search_lore", json!({"query": q}));
            ok(&mut s, "search_content", json!({"query": q}));
            ok(&mut s, "search_sessions", json!({"query": q}));
        }
        let hit = ok(&mut s, "search_lore", json!({"query": "fireball"}));
        ok(
            &mut s,
            "get_passage",
            json!({"cite": hit["hits"][0]["cite"]}),
        );
    }
    assert_eq!(
        snapshot(dir.path()),
        before,
        "no file created, resized or touched"
    );
}

#[cfg(unix)]
#[test]
fn the_tools_work_on_a_read_only_filesystem_view_of_the_index() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    let chmod = |mode_dir: u32, mode_file: u32| {
        let mut stack = vec![dir.path().to_path_buf()];
        let mut all = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                if e.metadata().unwrap().is_dir() {
                    stack.push(e.path());
                }
                all.push(e.path());
            }
            all.push(d);
        }
        for p in all {
            let mode = if p.is_dir() { mode_dir } else { mode_file };
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    chmod(0o555, 0o444);
    let result = std::panic::catch_unwind(|| {
        let mut s = start(Some(dir.path()), None);
        let out = ok(&mut s, "search_lore", json!({"query": "fireball"}));
        assert_eq!(out["hits"][0]["name"], "Fireball");
        let cite = out["hits"][0]["cite"].clone();
        ok(&mut s, "get_passage", json!({"cite": cite}));
    });
    chmod(0o755, 0o644);
    result.expect("a read-only index must be enough");
}

#[test]
fn the_mcp_sources_never_use_a_write_path_of_the_index() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.clone()];
    let mut text = String::new();
    while let Some(p) = files.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                files.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let code = std::fs::read_to_string(&path).unwrap();
                // Unit-test modules may build fixtures; only shipped code is scanned.
                text.push_str(code.split("#[cfg(test)]").next().unwrap());
            }
        }
    }
    for forbidden in [
        "SearchEngine::open(",
        "open_or_create",
        ".sync(",
        ".apply(",
        "IndexWriter",
        "writer(",
        "create_dir",
        "fs::write",
        "remove_dir",
        "remove_file",
    ] {
        assert!(!text.contains(forbidden), "borealis-mcp uses {forbidden}");
    }
    assert!(text.contains("open_read_only"));
}

#[test]
fn session_scope_gate_through_the_binary() {
    // No scope: absent from tools/list, rejected if called, session cites unreadable.
    let mut s = start(None, None);
    assert!(!tool_names(&mut s).contains(&"search_sessions".to_string()));
    let r = s.modern(
        "tools/call",
        json!({"name": "search_sessions", "arguments": {"query": "dragon"}}),
    );
    assert_eq!(r["error"]["code"], -32602, "{r}");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown tool"));
    let p = s.call("get_passage", json!({"cite": "session:entry-1#1"}));
    assert_eq!(p["isError"], true);
    drop(s);

    // Scope given: listed and callable; the empty sessions corpus yields nothing.
    let mut s = start(None, Some("camp-1"));
    let names = tool_names(&mut s);
    assert!(names.contains(&"search_sessions".to_string()));
    assert_eq!(names.len(), 10);
    let out = ok(&mut s, "search_sessions", json!({"query": "dragon"}));
    assert_eq!(out["count"], 0);
    // The campaign can only come from the launch arguments.
    let r = s.call(
        "search_sessions",
        json!({"query": "dragon", "campaign_id": "camp-2"}),
    );
    assert_eq!(r["isError"], true, "{r}");
    let p = s.call("get_passage", json!({"cite": "session:entry-1#1"}));
    assert_eq!(p["isError"], true);
}

#[test]
fn an_empty_session_scope_argument_is_refused_at_startup() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_borealis-mcp"))
        .arg("--content")
        .arg(common::content_root())
        .args(["--session-scope", " "])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn packs_not_enabled_for_the_run_never_appear_in_search_or_passages() {
    let dir = tempfile::tempdir().unwrap();
    build_index(dir.path());
    // Index holds SRD 5.1 and 5.2.1; this run enables only SRD 5.1.
    let mut s = Session::start_with_args(&[
        OsStr::new("--search-dir"),
        dir.path().as_os_str(),
        OsStr::new("--packs"),
        OsStr::new("srd-5.1"),
    ]);
    for tool in ["search_rules", "search_lore", "search_content"] {
        for q in ["grappled", "fireball", "opportunity attack", "prone"] {
            let out = ok(&mut s, tool, json!({"query": q, "k": 20}));
            for h in out["hits"].as_array().unwrap() {
                let cite = h["cite"].as_str().unwrap();
                assert!(
                    !cite.contains("2024") && !cite.contains("5.2.1") && h["ruleset"] != "2024",
                    "{tool} {q}: {h}"
                );
                assert_ne!(h["source"], "SRD 5.2.1", "{h}");
            }
        }
    }
    let on = ok(&mut s, "search_lore", json!({"query": "fireball"}));
    assert_eq!(
        on["hits"][0]["name"], "Fireball",
        "enabled pack still found"
    );
    let p = s.call("get_passage", json!({"cite": "ID_2024_CONDITION_GRAPPLED"}));
    assert_eq!(p["isError"], true, "{p}");
    assert!(p["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no passage"));
    let r = s.call(
        "get_passage",
        json!({"cite": "rules:srd-5.2.1-rules-text/combat"}),
    );
    assert_eq!(r["isError"], true, "{r}");
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no passage"));
}
