//! Hybrid keyword + vector ranking (spec sec. 7), behind the `embed` feature. The fusion itself is
//! [`borealis_embed::fusion`]; this module maps it onto [`Hit`] rows:
//! * exact-name hits (title equals the query under [`normalize_name`]) stay pinned first;
//! * a hit only the vector side found is read back by its `doc_key` and gets the snippet of
//!   [`borealis_embed::vector_only_snippet`] (no highlights);
//! * `score` becomes the RRF score (0 for keyword hits below the fusion depth, which follow in keyword order).
//!
//! Fusion works per **document**, like the collapsed keyword search: chunk keys (`X#1`, `X#2`) count as their
//! [`base_key`], each side keeps its best chunk, and a vector-only document is read back by its best chunk.

use borealis_embed::fusion::{rrf, vector_only_snippet};

use crate::analyzer::normalize_name;
use crate::engine::{Corpus, SearchEngine};
use crate::error::Result;
use crate::index::{base_key, Hit, Passage};
use std::collections::{HashMap, HashSet};

/// True when `hit` is named exactly `query`.
#[must_use]
pub fn is_exact_name(hit: &Hit, query: &str) -> bool {
    let q = normalize_name(query);
    !q.is_empty() && normalize_name(&hit.title) == q
}

/// Fuse `keyword` hits (ranked, best first) with `vector` hits (`(doc_key, score)` from the vector store, best
/// first). `passage` reads a vector-only key back (None drops it); `keep` filters those passages the same way
/// the keyword request was filtered (edition, type, campaign). Keyword hits below the fusion depth follow the
/// fused ones in keyword order (a long keyword list loses nothing). At most `limit` hits.
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
    let keyword_docs: Vec<&str> = keyword.iter().map(|h| base_key(&h.doc_key)).collect();
    // Best chunk per document on the vector side (the list is best first).
    let mut best_chunk: HashMap<&str, &str> = HashMap::new();
    let mut vector_docs: Vec<&str> = Vec::new();
    for (chunk, _) in vector {
        let doc = base_key(chunk);
        if !best_chunk.contains_key(doc) {
            best_chunk.insert(doc, chunk);
            vector_docs.push(doc);
        }
    }
    let exact: HashSet<&str> = keyword
        .iter()
        .filter(|h| is_exact_name(h, query))
        .map(|h| base_key(&h.doc_key))
        .collect();
    let fused = rrf(&keyword_docs, &vector_docs, |k| exact.contains(k));
    let mut by_doc: HashMap<&str, &Hit> = HashMap::new();
    for h in &keyword {
        by_doc.entry(base_key(&h.doc_key)).or_insert(h);
    }
    let mut out = Vec::with_capacity(limit.min(keyword.len() + vector_docs.len()));
    for f in fused {
        if out.len() >= limit {
            break;
        }
        if let Some(hit) = by_doc.remove(f.doc_key.as_str()) {
            out.push(Hit {
                score: f.score,
                ..hit.clone()
            });
            continue;
        }
        if f.keyword_rank.is_some() {
            continue; // a second keyword chunk of a document already placed
        }
        let chunk = best_chunk
            .get(f.doc_key.as_str())
            .copied()
            .unwrap_or(&f.doc_key);
        let Some(p) = passage(chunk)? else {
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
    for hit in &keyword {
        if out.len() >= limit {
            break;
        }
        if by_doc.remove(base_key(&hit.doc_key)).is_some() {
            // Below the fusion depth: no RRF score, the order is the keyword order.
            out.push(Hit {
                score: 0.0,
                ..hit.clone()
            });
        }
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
