//! Pooling: the union of the top 20 of every variant, minus what is already judged, shuffled and variant-blind.
use super::data::{EvalQuery, Qrels};
use super::metrics::VariantRun;
use super::stats::SplitMix64;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

pub const POOL_DEPTH: usize = 20;
/// Characters of document text shown to the judge.
pub const EXCERPT_CHARS: usize = 700;
const POOL_SEED: u64 = 0xB0EA_1150_0001_F00D;

/// `(query_id, doc_key)` pairs to judge, in a deterministic shuffled order.
pub fn pool_pairs(runs: &[VariantRun], qrels: &Qrels) -> Vec<(String, String)> {
    let mut set: BTreeSet<(String, String)> = BTreeSet::new();
    for run in runs {
        for q in &run.queries {
            let judged = qrels.get(&q.id);
            for key in q.top.iter().take(POOL_DEPTH) {
                if !judged.is_some_and(|j| j.contains_key(key)) {
                    set.insert((q.id.clone(), key.clone()));
                }
            }
        }
    }
    let mut pairs: Vec<_> = set.into_iter().collect();
    let mut rng = SplitMix64(POOL_SEED);
    for i in (1..pairs.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        pairs.swap(i, j);
    }
    pairs
}

fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(EXCERPT_CHARS).collect()
}

/// JSON lines `{query_id, query_text, doc_key, title, text_excerpt}`; pairs whose document is unknown are skipped
/// and counted in the second return value.
pub fn pool_lines(
    pairs: &[(String, String)],
    queries: &[EvalQuery],
    docs: &HashMap<String, (String, String)>,
) -> (Vec<String>, usize) {
    let texts: HashMap<&str, &str> = queries
        .iter()
        .map(|q| (q.id.as_str(), q.text.as_str()))
        .collect();
    let mut skipped = 0;
    let mut lines = Vec::new();
    for (qid, key) in pairs {
        let (Some(text), Some((title, body))) = (texts.get(qid.as_str()), docs.get(key)) else {
            skipped += 1;
            continue;
        };
        lines.push(
            json!({
                "query_id": qid,
                "query_text": text,
                "doc_key": key,
                "title": title,
                "text_excerpt": excerpt(body),
            })
            .to_string(),
        );
    }
    (lines, skipped)
}
