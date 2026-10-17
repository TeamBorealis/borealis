//! User rule: Aurora (XML, `.index`, packs) and 5etools imports are deterministic and never
//! involve AI. These guards fail the build when that stops being true at the crate or file level.
//!
//! * The deterministic importer crates must not depend on `borealis-ai` or `borealis-assist`,
//!   directly or through another workspace crate. Read with `cargo metadata --no-deps --offline`
//!   (no network, no lockfile resolution). Only workspace crates can depend on these two path
//!   crates, so the workspace graph is the whole transitive answer.
//! * The app's content import files (`content_*.rs`, `commands_content*.rs` and the helpers they
//!   read files with) must not reference the AI crates or the app's AI modules. The AI importer's
//!   own intake (`import_sources.rs`, `import_service.rs`) is AI by design and is not covered.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Crates that convert Aurora and 5etools content.
const DETERMINISTIC_CRATES: [&str; 3] = [
    "borealis-migrate",
    "borealis-import-5etools",
    "borealis-content",
];
/// Crates that call (or orchestrate calls to) a model.
const AI_CRATES: [&str; 2] = ["borealis-ai", "borealis-assist"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn metadata() -> Value {
    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let out = Command::new(cargo)
        .current_dir(workspace_root())
        .args([
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1",
        ])
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata JSON")
}

/// Workspace crate -> the workspace crates it depends on as normal or build dependencies
/// (dev-dependencies never reach a shipped binary).
fn workspace_graph(meta: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let packages = meta["packages"].as_array().expect("packages");
    let members: BTreeSet<String> = packages
        .iter()
        .filter_map(|p| p["name"].as_str().map(str::to_string))
        .collect();
    packages
        .iter()
        .map(|p| {
            let name = p["name"].as_str().unwrap_or_default().to_string();
            let deps = p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].is_null() || d["kind"] == "build")
                .filter_map(|d| d["name"].as_str())
                .filter(|n| members.contains(*n))
                .map(str::to_string)
                .collect();
            (name, deps)
        })
        .collect()
}

/// Every workspace crate `root` reaches, with the path that first reached it.
fn reachable(graph: &BTreeMap<String, BTreeSet<String>>, root: &str) -> BTreeMap<String, String> {
    let mut seen = BTreeMap::new();
    let mut stack = vec![(root.to_string(), root.to_string())];
    while let Some((name, via)) = stack.pop() {
        for dep in graph.get(&name).into_iter().flatten() {
            if !seen.contains_key(dep) {
                let path = format!("{via} -> {dep}");
                seen.insert(dep.clone(), path.clone());
                stack.push((dep.clone(), path));
            }
        }
    }
    seen
}

#[test]
fn deterministic_importers_never_depend_on_ai_crates() {
    let graph = workspace_graph(&metadata());
    for ai in AI_CRATES {
        assert!(
            graph.contains_key(ai),
            "{ai} is no longer a workspace crate; update this guard"
        );
    }
    for root in DETERMINISTIC_CRATES {
        assert!(graph.contains_key(root), "{root} is not a workspace crate");
        let reach = reachable(&graph, root);
        for ai in AI_CRATES {
            assert!(
                !reach.contains_key(ai),
                "{root} must stay AI-free but depends on {ai}: {}",
                reach[ai]
            );
        }
    }
}

#[test]
fn the_dependency_walk_does_see_the_app_reaching_the_ai_crates() {
    // Proves the walker is not vacuous: the app crate links the AI importer (needed for it).
    let graph = workspace_graph(&metadata());
    let reach = reachable(&graph, "borealis");
    assert!(reach.contains_key("borealis-assist"), "{reach:?}");
    assert!(reach.contains_key("borealis-ai"), "{reach:?}");
}

/// Tokens that would put a model on a content import path.
const AI_REFERENCES: [&str; 7] = [
    "borealis_ai",
    "borealis_assist",
    "crate::ai_",
    "crate::commands_ai",
    "crate::import_service",
    "crate::import_sources",
    "crate::import_review",
];

/// Helpers the content commands read files with.
const CONTENT_HELPERS: [&str; 2] = ["zip_read.rs", "secondary_content.rs"];

fn content_import_files() -> Vec<PathBuf> {
    let src = workspace_root().join("src-tauri/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("src-tauri/src")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.ends_with(".rs")
                && (name.starts_with("content_")
                    || name.starts_with("commands_content")
                    || CONTENT_HELPERS.contains(&name))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn content_import_sources_never_reference_ai() {
    let files = content_import_files();
    let names: BTreeSet<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    // The paths the user rule is about must stay covered if files move.
    for must in [
        "commands_content.rs",
        "content_import.rs",
        "content_preview.rs",
        "content_index.rs",
        "content_index_aurora.rs",
        "content_packs.rs",
    ] {
        assert!(
            names.contains(must),
            "{must} is no longer covered: {names:?}"
        );
    }
    let mut hits = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("read source");
        for (n, line) in text.lines().enumerate() {
            if let Some(tok) = AI_REFERENCES.iter().find(|t| line.contains(**t)) {
                hits.push(format!(
                    "{}:{}: {tok}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    n + 1
                ));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "content import code must not reach AI:\n{}",
        hits.join("\n")
    );
}
