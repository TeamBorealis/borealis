//! Hybrid keyword + vector ranking (spec sec. 7), behind the `embed` feature. The fusion itself is
//! [`borealis_embed::fusion`]; this module maps it onto [`Hit`] rows:
//! * exact-name hits (title equals the query under [`normalize_name`]) stay pinned first;
//! * a hit only the vector side found is read back by its `doc_key` and gets the snippet of
//!   [`borealis_embed::vector_only_snippet`] (no highlights);
//! * `score` becomes the RRF score.

use borealis_embed::fusion::{rrf, vector_only_snippet};

use crate::analyzer::normalize_name;
use crate::engine::{Corpus, SearchEngine};
use crate::error::Result;
use crate::index::{Hit, Passage};

/// True when `hit` is named exactly `query`.
#[must_use]
pub fn is_exact_name(hit: &Hit, query: &str) -> bool {
    let q = normalize_name(query);
    !q.is_empty() && normalize_name(&hit.title) == q
}

/// Fuse `keyword` hits (ranked, best first) with `vector` hits (`(doc_key, score)` from the vector store, best
/// first). `passage` reads a vector-only key back (None drops it); `keep` filters those passages the same way
/// the keyword request was filtered (edition, type, campaign). At most `limit` hits.
///
/// # Errors
/// Errors of `passage`.
pub fn fuse_hits(
    query: &str,
    keyword: Vec<Hit>,
    vector: &[(&str, f32)],
    limit: usize,
    mut passage: impl FnMut(&str) -> Result<Option<Passage>>,
    keep: impl Fn(&Passage) -> bool,
) -> Result<Vec<Hit>> {
    let keyword_keys: Vec<&str> = keyword.iter().map(|h| h.doc_key.as_str()).collect();
    let vector_keys: Vec<&str> = vector.iter().map(|(k, _)| *k).collect();
    let exact: std::collections::HashSet<&str> = keyword
        .iter()
        .filter(|h| is_exact_name(h, query))
        .map(|h| h.doc_key.as_str())
        .collect();
    let fused = rrf(&keyword_keys, &vector_keys, |k| exact.contains(k));
    let mut by_key: std::collections::HashMap<&str, &Hit> =
        keyword.iter().map(|h| (h.doc_key.as_str(), h)).collect();
    let mut out = Vec::with_capacity(limit.min(fused.len()));
    for f in fused {
        if out.len() >= limit {
            break;
        }
        if let Some(hit) = by_key.remove(f.doc_key.as_str()) {
            out.push(Hit {
                score: f.score,
                ..hit.clone()
            });
            continue;
        }
        let Some(p) = passage(&f.doc_key)? else {
            continue;
        };
        if !keep(&p) {
            continue;
        }
        out.push(Hit {
            snippet: vector_only_snippet(&p.body, query),
            highlights: Vec::new(),
            score: f.score,
            doc_key: p.doc_key,
            title: p.title,
            path: p.path,
            ty: p.ty,
            ruleset: p.ruleset,
            pack: p.pack,
            source: p.source,
        });
    }
    Ok(out)
}

/// [`fuse_hits`] reading vector-only passages from `engine`'s `corpus`.
///
/// # Errors
/// Index errors.
pub fn fuse_with_engine(
    engine: &SearchEngine,
    corpus: Corpus,
    query: &str,
    keyword: Vec<Hit>,
    vector: &[(&str, f32)],
    limit: usize,
    keep: impl Fn(&Passage) -> bool,
) -> Result<Vec<Hit>> {
    fuse_hits(
        query,
        keyword,
        vector,
        limit,
        |key| engine.passage(corpus, key),
        keep,
    )
}
