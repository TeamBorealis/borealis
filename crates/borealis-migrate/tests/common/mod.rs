//! Helpers shared by the integration tests: locating external data inputs.
//!
//! The Aurora XML fixtures are never part of this repository. Tests find them through
//! `BOREALIS_AURORA_FIXTURES`, or by looking for `fixtures/aurora-xml` in an ancestor of the
//! workspace, and skip with a printed notice when they are absent.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

fn ancestors_with(relative: &str) -> Option<PathBuf> {
    let start = Path::new(env!("CARGO_MANIFEST_DIR"));
    start
        .ancestors()
        .map(|a| a.join(relative))
        .find(|candidate| candidate.is_dir())
}

/// Directory with the Aurora XML fixtures, if available.
pub fn fixtures_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BOREALIS_AURORA_FIXTURES") {
        let p = PathBuf::from(dir);
        return p.is_dir().then_some(p);
    }
    ancestors_with("fixtures/aurora-xml")
}

/// Directory with the spec's XML/JSON sample pairs, if available.
pub fn samples_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("BOREALIS_SPEC_SAMPLES") {
        let p = PathBuf::from(dir);
        return p.is_dir().then_some(p);
    }
    ancestors_with("fixtures/content-samples")
}

/// Print a skip notice and return `None` when the data is missing.
pub fn require(what: &str, dir: Option<PathBuf>, env_var: &str) -> Option<PathBuf> {
    if dir.is_none() {
        eprintln!("SKIPPED: {what} not found; set {env_var} to run this test");
    }
    dir
}

/// Sorted file names in `dir` with this extension.
pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("readable directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    out.sort();
    out
}
