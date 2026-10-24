//! `eval/baseline.json`: the adopted configuration and its scores (written in phase C after judging).
use super::data::eval_dir;
use super::metrics::VariantRun;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A run may drop at most this far below the baseline overall (spec section 6).
pub const OVERALL_TOLERANCE: f64 = 0.01;
/// Per-category tolerance (the adoption rule's category loss bound).
pub const CATEGORY_TOLERANCE: f64 = 0.02;

#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    /// Variant name understood by `VariantId::from_str` (the adopted config).
    pub variant: String,
    pub ndcg10: f64,
    pub categories: BTreeMap<String, f64>,
}

impl Baseline {
    pub fn from_run(run: &VariantRun) -> Self {
        Self {
            variant: run.name.clone(),
            ndcg10: run.ndcg10(None),
            categories: run
                .categories()
                .into_iter()
                .map(|c| {
                    let v = run.ndcg10(Some(&c));
                    (c, v)
                })
                .collect(),
        }
    }

    /// `None` when `baseline.json` does not exist yet.
    pub fn load() -> Option<Self> {
        let path = eval_dir().join("baseline.json");
        let text = std::fs::read_to_string(&path).ok()?;
        Some(serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    pub fn write(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(eval_dir().join("baseline.json"), text + "\n")
    }

    /// Human-readable regressions of `run` against this baseline (empty = fine).
    pub fn regressions(&self, run: &VariantRun) -> Vec<String> {
        let mut out = Vec::new();
        let overall = run.ndcg10(None);
        if overall < self.ndcg10 - OVERALL_TOLERANCE {
            out.push(format!(
                "overall nDCG@10 {overall:.4} is more than {OVERALL_TOLERANCE} below baseline {:.4}",
                self.ndcg10
            ));
        }
        for (category, base) in &self.categories {
            let now = run.ndcg10(Some(category));
            if now < base - CATEGORY_TOLERANCE {
                out.push(format!(
                    "{category}: nDCG@10 {now:.4} is more than {CATEGORY_TOLERANCE} below baseline {base:.4}"
                ));
            }
        }
        out
    }
}
