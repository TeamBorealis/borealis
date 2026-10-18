//! Golden character suite (acceptance 3e): build, sheet extraction, label normaliser, compare.
#![allow(dead_code)]

pub mod build;
pub mod compare;
pub mod norm;
pub mod weapons;

use serde_json::Value;
use std::path::PathBuf;

pub fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

pub fn read_json(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Golden ids, sorted (one per `specs/*.json`).
pub fn ids() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(goldens_dir().join("specs"))
        .expect("specs dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
        .collect();
    out.sort();
    out
}
