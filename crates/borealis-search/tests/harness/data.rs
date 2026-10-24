//! Query and judgement files, key canonicalisation.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct EvalQuery {
    pub id: String,
    pub text: String,
    pub category: String,
    #[serde(default)]
    pub ruleset: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QrelRow {
    query_id: String,
    doc_key: String,
    grade: u8,
}

/// `query_id -> canonical doc_key -> grade`.
pub type Qrels = HashMap<String, HashMap<String, u8>>;

pub fn eval_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("eval")
}

/// Canonical key: the document key without its `#n` chunk suffix.
pub fn canon(key: &str) -> &str {
    borealis_search::index::base_key(key)
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("{} line {}: {e}", path.display(), i + 1))
        })
        .collect()
}

pub fn load_queries() -> Vec<EvalQuery> {
    read_jsonl(&eval_dir().join("queries.jsonl"))
}

/// Best grade per canonical key within one judgement file (`x` and `x#1` may both be labelled).
fn load_file(name: &str) -> Qrels {
    let mut out: Qrels = HashMap::new();
    for row in read_jsonl::<QrelRow>(&eval_dir().join(name)) {
        let grade = out
            .entry(row.query_id)
            .or_default()
            .entry(canon(&row.doc_key).to_string())
            .or_insert(0);
        *grade = (*grade).max(row.grade);
    }
    out
}

/// All judgements: the blind labeller's `qrels.jsonl` plus the pooled judge's `qrels-pool.jsonl`.
/// On a conflict (same query and canonical document) the labeller's grade wins.
pub fn load_qrels() -> Qrels {
    let mut out = load_file("qrels.jsonl");
    for (query, docs) in load_file("qrels-pool.jsonl") {
        let entry = out.entry(query).or_default();
        for (doc, grade) in docs {
            entry.entry(doc).or_insert(grade);
        }
    }
    out
}
