//! Unit tests of the evaluation harness itself: metrics, the randomisation test, pooling, variants.
mod common;
mod harness;

use harness::data::{load_qrels, load_queries, Qrels};
use harness::metrics::{judged_share, ndcg, recall, reciprocal_rank, VariantRun};
use harness::pool::{pool_lines, pool_pairs};
use harness::stats::{paired_randomisation, PERMUTATIONS, SEED};
use harness::variants::{without, Fixture, VariantId};
use std::collections::HashMap;

fn judged(pairs: &[(&str, u8)]) -> HashMap<String, u8> {
    pairs.iter().map(|(k, g)| ((*k).to_string(), *g)).collect()
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn ndcg_is_one_for_the_ideal_order_and_lower_otherwise() {
    let j = judged(&[("a", 3), ("b", 2), ("c", 1)]);
    assert!((ndcg(&keys(&["a", "b", "c"]), &j, 10) - 1.0).abs() < 1e-12);
    assert!(ndcg(&keys(&["c", "b", "a"]), &j, 10) < 1.0);
    assert_eq!(ndcg(&keys(&["x", "y"]), &j, 10), 0.0);
}

#[test]
fn unjudged_documents_count_as_grade_zero() {
    let j = judged(&[("a", 3)]);
    let with_noise = ndcg(&keys(&["u1", "a"]), &j, 10);
    let clean = ndcg(&keys(&["a"]), &j, 10);
    assert!(with_noise < clean);
}

#[test]
fn mrr_recall_and_coverage() {
    let j = judged(&[("a", 3), ("b", 2), ("c", 1), ("z", 0)]);
    let ranked = keys(&["z", "b", "q", "a"]);
    assert!((reciprocal_rank(&ranked, &j, 10) - 0.5).abs() < 1e-12);
    assert!((recall(&ranked, &j, 50) - 1.0).abs() < 1e-12);
    assert!((recall(&keys(&["a"]), &j, 50) - 0.5).abs() < 1e-12);
    assert!((judged_share(&ranked, &j, 10) - 0.75).abs() < 1e-12);
}

#[test]
fn randomisation_test_is_deterministic_and_separates_signal_from_noise() {
    let a: Vec<f64> = (0..100).map(|i| f64::from(i % 7) / 10.0).collect();
    let better: Vec<f64> = a.iter().map(|x| x + 0.05).collect();
    let p1 = paired_randomisation(&a, &better, PERMUTATIONS, SEED);
    let p2 = paired_randomisation(&a, &better, PERMUTATIONS, SEED);
    assert_eq!(p1, p2);
    assert!(p1 < 0.01, "consistent gain should be significant, p={p1}");

    let alternating: Vec<f64> = a
        .iter()
        .enumerate()
        .map(|(i, x)| if i % 2 == 0 { x + 0.1 } else { x - 0.1 })
        .collect();
    let p = paired_randomisation(&a, &alternating, PERMUTATIONS, SEED);
    assert!(p > 0.5, "zero net change is not significant, p={p}");
    assert_eq!(
        paired_randomisation(&a, &a, PERMUTATIONS, SEED),
        1.0,
        "identical runs are never significant"
    );
}

#[test]
fn variant_names_round_trip_and_ablations_flip_exactly_one_tweak() {
    for id in VariantId::runnable() {
        assert_eq!(id.name().parse::<VariantId>().unwrap(), id);
    }
    assert!("V9".parse::<VariantId>().is_err());
    assert!("V2-t8".parse::<VariantId>().is_err());
    let all = harness::variants::without(0);
    assert_eq!(all, borealis_search::Tweaks::ALL);
    for k in 1..=7 {
        assert_ne!(without(k), all, "ablation {k} must change one tweak");
    }
    assert!(!without(3).prefix && without(3).typo);
    assert_eq!(
        VariantId::V1.tweaks(),
        Some(borealis_search::Tweaks::none())
    );
    assert!(VariantId::V0.tweaks().is_none());
}

fn run_of(name: &str, tops: &[(&str, &[&str])]) -> VariantRun {
    VariantRun {
        name: name.into(),
        queries: tops
            .iter()
            .map(|(id, top)| harness::metrics::QueryRun {
                id: (*id).into(),
                category: "exact".into(),
                ndcg10: 0.0,
                rr10: 0.0,
                recall50: 0.0,
                judged10: 0.0,
                latency_ms: 0.0,
                top: keys(top),
            })
            .collect(),
    }
}

#[test]
fn pool_excludes_judged_pairs_is_deterministic_and_carries_no_variant_info() {
    let mut qrels: Qrels = HashMap::new();
    qrels.insert("q1".into(), judged(&[("a", 3)]));
    let runs = [
        run_of("V1", &[("q1", &["a", "b"]), ("q2", &["c"])]),
        run_of("V2", &[("q1", &["b", "d"]), ("q2", &["c", "e"])]),
    ];
    let pairs = pool_pairs(&runs, &qrels);
    assert_eq!(pairs.len(), 4, "b,d for q1 and c,e for q2; a is judged");
    assert!(!pairs.iter().any(|(q, k)| q == "q1" && k == "a"));
    assert_eq!(pairs, pool_pairs(&runs, &qrels));

    let queries = load_queries();
    let qid = queries[0].id.clone();
    let docs: HashMap<String, (String, String)> =
        [("k".to_string(), ("Title".to_string(), "word ".repeat(500)))].into();
    let (lines, skipped) = pool_lines(
        &[(qid, "k".into()), ("nope".into(), "k".into())],
        &queries,
        &docs,
    );
    assert_eq!((lines.len(), skipped), (1, 1));
    let row: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    let mut fields: Vec<&str> = row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["doc_key", "query_id", "query_text", "text_excerpt", "title"]
    );
    assert!(row["text_excerpt"].as_str().unwrap().chars().count() <= harness::pool::EXCERPT_CHARS);
}

#[test]
fn fixture_ranks_fireball_first_for_v1_and_v2_and_v2b() {
    let fixture = Fixture::build();
    let q = load_queries()
        .into_iter()
        .find(|q| q.text == "Fireball")
        .unwrap();
    for id in [VariantId::V1, VariantId::V2, VariantId::V2b] {
        let ranker = fixture.ranker(id).unwrap();
        let top = ranker.rank(&q, 5);
        assert!(
            top.first().is_some_and(|k| k.contains("FIREBALL")),
            "{}: {top:?}",
            id.name()
        );
    }
    assert!(fixture.ranker(VariantId::V3).is_err() && fixture.ranker(VariantId::V4).is_err());
    let qrels = load_qrels();
    let missing = qrels
        .values()
        .flat_map(HashMap::keys)
        .filter(|k| !fixture.docs.contains_key(*k))
        .count();
    assert_eq!(missing, 0, "every judged document exists in the corpus");
}
