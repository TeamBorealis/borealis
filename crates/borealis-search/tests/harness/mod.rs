//! Evaluation harness shared by `examples/eval.rs` and the regression tests (spec section 6, WP S1.3).
//!
//! Include with `mod common; mod harness;` at the crate root (it uses `crate::common`).
#![allow(dead_code)]

pub mod baseline;
pub mod data;
pub mod metrics;
pub mod pool;
pub mod report;
pub mod stats;
pub mod variants;

use data::EvalQuery;

/// One ranking system under evaluation.
pub trait Ranker {
    /// Variant label (`V2`, `V2-t3`, ...).
    fn name(&self) -> String;
    /// Canonical (chunk-suffix-free, de-duplicated) document keys, best first, at most `k`.
    fn rank(&self, query: &EvalQuery, k: usize) -> Vec<String>;
}
