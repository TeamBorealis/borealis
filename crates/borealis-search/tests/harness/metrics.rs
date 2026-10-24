//! nDCG@10, MRR@10, Recall@50, judged@10 and run aggregation.
use super::data::{EvalQuery, Qrels};
use super::Ranker;
use std::collections::HashMap;
use std::time::Instant;

pub const NDCG_CUTOFF: usize = 10;
pub const RECALL_CUTOFF: usize = 50;
/// A document with at least this grade counts as relevant for MRR and Recall.
pub const RELEVANT_GRADE: u8 = 2;
/// Latency is the best of this many repetitions (warm cache).
const LATENCY_REPS: usize = 3;

#[derive(Debug, Clone)]
pub struct QueryRun {
    pub id: String,
    pub category: String,
    pub ndcg10: f64,
    pub rr10: f64,
    pub recall50: f64,
    pub judged10: f64,
    pub latency_ms: f64,
    pub top: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VariantRun {
    pub name: String,
    pub queries: Vec<QueryRun>,
}

fn gain(grade: u8) -> f64 {
    f64::from(2u32.pow(u32::from(grade)) - 1)
}

fn discount(rank0: usize) -> f64 {
    1.0 / ((rank0 + 2) as f64).log2()
}

/// Exponential-gain nDCG. Unjudged documents count as grade 0.
pub fn ndcg(ranked: &[String], judged: &HashMap<String, u8>, cutoff: usize) -> f64 {
    let dcg: f64 = ranked
        .iter()
        .take(cutoff)
        .enumerate()
        .map(|(i, k)| gain(judged.get(k).copied().unwrap_or(0)) * discount(i))
        .sum();
    let mut ideal: Vec<u8> = judged.values().copied().collect();
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(cutoff)
        .enumerate()
        .map(|(i, g)| gain(*g) * discount(i))
        .sum();
    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

pub fn reciprocal_rank(ranked: &[String], judged: &HashMap<String, u8>, cutoff: usize) -> f64 {
    ranked
        .iter()
        .take(cutoff)
        .position(|k| judged.get(k).copied().unwrap_or(0) >= RELEVANT_GRADE)
        .map_or(0.0, |i| 1.0 / (i as f64 + 1.0))
}

pub fn recall(ranked: &[String], judged: &HashMap<String, u8>, cutoff: usize) -> f64 {
    let total = judged.values().filter(|g| **g >= RELEVANT_GRADE).count();
    if total == 0 {
        return 0.0;
    }
    let found = ranked
        .iter()
        .take(cutoff)
        .filter(|k| judged.get(*k).copied().unwrap_or(0) >= RELEVANT_GRADE)
        .count();
    found as f64 / total as f64
}

/// Share of the top `cutoff` results that carry any judgement (including grade 0).
pub fn judged_share(ranked: &[String], judged: &HashMap<String, u8>, cutoff: usize) -> f64 {
    let n = ranked.len().min(cutoff);
    if n == 0 {
        return 1.0;
    }
    ranked
        .iter()
        .take(cutoff)
        .filter(|k| judged.contains_key(*k))
        .count() as f64
        / n as f64
}

/// Run every query through `ranker` (top 50) and score it against `qrels`.
pub fn evaluate(ranker: &dyn Ranker, queries: &[EvalQuery], qrels: &Qrels) -> VariantRun {
    let empty = HashMap::new();
    let runs = queries
        .iter()
        .map(|q| {
            let judged = qrels.get(&q.id).unwrap_or(&empty);
            let mut best = f64::MAX;
            let mut top = Vec::new();
            for _ in 0..LATENCY_REPS {
                let started = Instant::now();
                top = ranker.rank(q, RECALL_CUTOFF);
                best = best.min(started.elapsed().as_secs_f64() * 1000.0);
            }
            QueryRun {
                id: q.id.clone(),
                category: q.category.clone(),
                ndcg10: ndcg(&top, judged, NDCG_CUTOFF),
                rr10: reciprocal_rank(&top, judged, NDCG_CUTOFF),
                recall50: recall(&top, judged, RECALL_CUTOFF),
                judged10: judged_share(&top, judged, NDCG_CUTOFF),
                latency_ms: best,
                top,
            }
        })
        .collect();
    VariantRun {
        name: ranker.name(),
        queries: runs,
    }
}

pub fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

pub fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let rank = (p * (values.len() - 1) as f64).round() as usize;
    values[rank]
}

impl VariantRun {
    /// Categories in order of first appearance.
    pub fn categories(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for q in &self.queries {
            if !out.contains(&q.category) {
                out.push(q.category.clone());
            }
        }
        out
    }

    /// Queries of one category (`None` = all).
    pub fn select<'a>(&'a self, category: Option<&'a str>) -> impl Iterator<Item = &'a QueryRun> {
        self.queries
            .iter()
            .filter(move |q| category.map_or(true, |c| q.category == c))
    }

    pub fn mean_of(&self, category: Option<&str>, f: impl Fn(&QueryRun) -> f64) -> f64 {
        mean(self.select(category).map(f))
    }

    pub fn ndcg10(&self, category: Option<&str>) -> f64 {
        self.mean_of(category, |q| q.ndcg10)
    }

    pub fn latency(&self, category: Option<&str>, p: f64) -> f64 {
        percentile(self.select(category).map(|q| q.latency_ms).collect(), p)
    }

    /// Per-query nDCG@10 in query order (paired tests).
    pub fn ndcg_vector(&self) -> Vec<f64> {
        self.queries.iter().map(|q| q.ndcg10).collect()
    }
}
