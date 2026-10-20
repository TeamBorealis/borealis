//! Shared support of the acceptance harness (`acceptance.rs`). `prereq` is also included by the
//! engine sweeps (`fivetools_corpus`, `aurora_corpus_sweep`) with `#[path]`.
#![allow(dead_code)]

pub mod app;
pub mod count;
pub mod files;
pub mod plan;
pub mod prereq;
pub mod stage2;

/// A progress line on stderr (`PROGRESS\t...`), past libtest's capture: setup labels, phases,
/// counts and seconds only, never content text. A long opt-in run (acceptance-comm) shows where
/// it is.
pub fn progress(what: &str) {
    use std::io::Write as _;
    let _ = std::io::stderr()
        .lock()
        .write_all(format!("PROGRESS\t{what}\n").as_bytes());
}
