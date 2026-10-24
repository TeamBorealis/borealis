//! Validates the evaluation query set (`eval/queries.jsonl`, `eval/qrels.jsonl`, spec section 6).
mod common;

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalQuery {
    id: String,
    text: String,
    category: String,
    #[serde(default)]
    ruleset: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Qrel {
    query_id: String,
    doc_key: String,
    grade: u8,
}

/// Minimum number of queries per category (spec section 6; the sessions category waits for the recorder).
const MINIMUMS: &[(&str, usize)] = &[
    ("exact", 40),
    ("typo", 30),
    ("prefix", 25),
    ("paraphrase", 30),
    ("rules question", 35),
    ("abbreviation/synonym", 15),
    ("edition-filtered", 10),
];
const MIN_TOTAL: usize = 185;
const MAX_GRADE: u8 = 3;

fn eval_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("eval")
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(name: &str) -> Vec<T> {
    let path = eval_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("{name} line {}: {e}", i + 1))
        })
        .collect()
}

fn corpus_keys() -> HashSet<String> {
    let store = common::srd_store();
    let (rules, lore) = common::srd_inputs(&store);
    rules
        .into_iter()
        .chain(lore)
        .flat_map(|p| p.chunks)
        .map(|c| c.doc_key)
        .collect()
}

#[test]
fn queries_are_well_formed_with_unique_ids() {
    let queries: Vec<EvalQuery> = read_jsonl("queries.jsonl");
    let mut seen = HashSet::new();
    for q in &queries {
        assert!(!q.id.trim().is_empty(), "empty id");
        assert!(seen.insert(q.id.as_str()), "duplicate id {}", q.id);
        assert!(!q.text.trim().is_empty(), "{}: empty text", q.id);
        if let Some(r) = &q.ruleset {
            assert!(
                matches!(r.as_str(), "2014" | "2024"),
                "{}: ruleset {r}",
                q.id
            );
        }
    }
}

#[test]
fn category_counts_meet_the_spec_minimums() {
    let queries: Vec<EvalQuery> = read_jsonl("queries.jsonl");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for q in &queries {
        *counts.entry(q.category.as_str()).or_default() += 1;
    }
    for (category, minimum) in MINIMUMS {
        let n = counts.get(category).copied().unwrap_or(0);
        assert!(n >= *minimum, "{category}: {n} < {minimum}");
    }
    for category in counts.keys() {
        assert!(
            MINIMUMS.iter().any(|(c, _)| c == category),
            "unknown category {category}"
        );
    }
    assert!(queries.len() >= MIN_TOTAL, "{} queries", queries.len());
}

#[test]
fn edition_filtered_queries_carry_a_ruleset() {
    let queries: Vec<EvalQuery> = read_jsonl("queries.jsonl");
    for q in queries.iter().filter(|q| q.category == "edition-filtered") {
        assert!(
            q.ruleset.is_some(),
            "{}: edition-filtered without ruleset",
            q.id
        );
    }
}

#[test]
fn qrels_reference_known_queries_and_real_documents() {
    let queries: Vec<EvalQuery> = read_jsonl("queries.jsonl");
    let ids: HashSet<&str> = queries.iter().map(|q| q.id.as_str()).collect();
    let qrels: Vec<Qrel> = read_jsonl("qrels.jsonl");
    let keys = corpus_keys();
    let mut pairs = HashSet::new();
    for r in &qrels {
        assert!(
            ids.contains(r.query_id.as_str()),
            "unknown query {}",
            r.query_id
        );
        assert!(
            keys.contains(&r.doc_key),
            "{}: unknown doc_key {}",
            r.query_id,
            r.doc_key
        );
        assert!(r.grade <= MAX_GRADE, "{}: grade {}", r.query_id, r.grade);
        assert!(
            pairs.insert((r.query_id.as_str(), r.doc_key.as_str())),
            "{}: duplicate judgement for {}",
            r.query_id,
            r.doc_key
        );
    }
}

#[test]
fn every_query_has_a_target_or_directly_relevant_document() {
    let queries: Vec<EvalQuery> = read_jsonl("queries.jsonl");
    let qrels: Vec<Qrel> = read_jsonl("qrels.jsonl");
    let mut best: HashMap<&str, u8> = HashMap::new();
    for r in &qrels {
        let e = best.entry(r.query_id.as_str()).or_default();
        *e = (*e).max(r.grade);
    }
    for q in &queries {
        assert!(
            best.get(q.id.as_str()).copied().unwrap_or(0) >= 2,
            "{}: no grade 2/3 document",
            q.id
        );
    }
}
