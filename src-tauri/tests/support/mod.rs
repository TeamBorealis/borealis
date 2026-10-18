//! Shared support of the acceptance harness (`acceptance.rs`). `prereq` is also included by the
//! engine sweeps (`fivetools_corpus`, `aurora_corpus_sweep`) with `#[path]`.
#![allow(dead_code)]

pub mod app;
pub mod count;
pub mod files;
pub mod plan;
pub mod prereq;
pub mod stage2;
