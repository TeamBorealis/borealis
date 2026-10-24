//! Hybrid ranking (spec sec. 7): reciprocal rank fusion of the keyword top 100 and the vector top 100,
//! `score = sum 1 / (60 + rank)` with 1-based ranks (Cormack, Clarke and Buettcher, SIGIR 2009), exact-name hits
//! pinned first, and the snippet rule for hits only the vector side found.
//!
//! Pure functions over `doc_key`s: `borealis-search` (which depends on this crate through its `embed` feature)
//! feeds its keyword hits and the [`crate::VectorStore`] hits in and maps the keys back to its own hit rows.

use std::collections::{HashMap, HashSet};

/// The RRF constant `k`.
pub const RRF_K: f32 = 60.0;
/// How many hits of each list take part.
pub const FUSION_DEPTH: usize = 100;
/// Fallback snippet length for vector-only hits.
pub const SNIPPET_FALLBACK_CHARS: usize = 200;
/// Longest best-overlap sentence used as a snippet (the keyword snippet length of `borealis-search`).
pub const SNIPPET_MAX_CHARS: usize = 220;
/// Query words shorter than this are ignored by the snippet rule (articles, "of", ...).
const MIN_TERM_CHARS: usize = 3;
/// A query word of at least this length also matches longer forms ("grapple" -> "grappled").
const PREFIX_TERM_CHARS: usize = 4;

/// One fused result.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedHit {
    pub doc_key: String,
    /// RRF score (pinned hits keep theirs but sort first regardless).
    pub score: f32,
    /// 1-based rank in the keyword list, if it was there.
    pub keyword_rank: Option<usize>,
    /// 1-based rank in the vector list, if it was there.
    pub vector_rank: Option<usize>,
    /// An exact-name hit, kept at the top.
    pub pinned: bool,
}

impl FusedHit {
    /// Found only by the vector side: needs [`vector_only_snippet`].
    #[must_use]
    pub fn vector_only(&self) -> bool {
        self.keyword_rank.is_none()
    }
}

fn rrf_term(rank: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // ranks <= 100
    let r = rank as f32;
    1.0 / (RRF_K + r)
}

/// Fuse ranked keyword and vector keys (best first; only the first [`FUSION_DEPTH`] of each count, duplicates
/// keep their first rank). Keyword hits for which `is_exact_name` holds come first, in keyword order; the rest
/// sort by RRF score, ties by keyword rank, then vector rank, then key.
#[must_use]
pub fn rrf<K: AsRef<str>>(
    keyword: &[K],
    vector: &[K],
    is_exact_name: impl Fn(&str) -> bool,
) -> Vec<FusedHit> {
    let mut hits: Vec<FusedHit> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut add = |key: &str, rank: usize, keyword_side: bool| {
        let i = *at.entry(key.to_string()).or_insert_with(|| {
            hits.push(FusedHit {
                doc_key: key.to_string(),
                score: 0.0,
                keyword_rank: None,
                vector_rank: None,
                pinned: false,
            });
            hits.len() - 1
        });
        let hit = &mut hits[i];
        let slot = if keyword_side {
            &mut hit.keyword_rank
        } else {
            &mut hit.vector_rank
        };
        if slot.is_none() {
            *slot = Some(rank);
            hit.score += rrf_term(rank);
        }
    };
    for (side, list) in [(true, keyword), (false, vector)] {
        let mut seen = HashSet::new();
        let unique = list.iter().map(AsRef::as_ref).filter(|k| seen.insert(*k));
        for (i, key) in unique.take(FUSION_DEPTH).enumerate() {
            add(key, i + 1, side);
        }
    }
    for hit in &mut hits {
        hit.pinned = hit.keyword_rank.is_some() && is_exact_name(&hit.doc_key);
    }
    hits.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| {
                if a.pinned {
                    a.keyword_rank.cmp(&b.keyword_rank)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| rank_order(a.keyword_rank, b.keyword_rank))
            .then_with(|| rank_order(a.vector_rank, b.vector_rank))
            .then_with(|| a.doc_key.cmp(&b.doc_key))
    });
    hits
}

/// Present ranks first, smaller first.
fn rank_order(a: Option<usize>, b: Option<usize>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Snippet for a vector-only hit: the sentence of `text` sharing the most distinct query words (earliest on
/// ties, at most [`SNIPPET_MAX_CHARS`]), or the first [`SNIPPET_FALLBACK_CHARS`] characters when no sentence
/// shares a word.
#[must_use]
pub fn vector_only_snippet(text: &str, query: &str) -> String {
    let terms: Vec<String> = {
        let mut seen = HashSet::new();
        words(query)
            .filter(|w| w.chars().count() >= MIN_TERM_CHARS)
            .filter(|w| seen.insert(w.clone()))
            .collect()
    };
    let mut best: Option<(usize, &str)> = None;
    for sentence in sentences(text) {
        let sentence_words: Vec<String> = words(sentence).collect();
        let overlap = terms
            .iter()
            .filter(|t| sentence_words.iter().any(|w| matches_term(w, t)))
            .count();
        let better = match best {
            Some((n, _)) => overlap > n,
            None => overlap > 0,
        };
        if better {
            best = Some((overlap, sentence));
        }
    }
    match best {
        Some((_, sentence)) => first_chars(sentence.trim(), SNIPPET_MAX_CHARS),
        None => first_chars(text.trim(), SNIPPET_FALLBACK_CHARS),
    }
}

fn matches_term(word: &str, term: &str) -> bool {
    word == term || (term.chars().count() >= PREFIX_TERM_CHARS && word.starts_with(term))
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Split after `.`, `!` or `?` followed by whitespace, and at line breaks.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next_is_space = chars.peek().is_some_and(|(_, n)| n.is_whitespace());
        let end = match c {
            '\n' => Some(i),
            '.' | '!' | '?' if next_is_space => Some(i + c.len_utf8()),
            _ => None,
        };
        if let Some(end) = end {
            let s = text[start..end].trim();
            if !s.is_empty() {
                out.push(s);
            }
            start = end;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(hits: &[FusedHit]) -> Vec<&str> {
        hits.iter().map(|h| h.doc_key.as_str()).collect()
    }

    #[test]
    fn scores_are_sum_of_reciprocal_ranks() {
        let fused = rrf(&["a", "b", "c"], &["c", "d", "a"], |_| false);
        let score = |k: &str| fused.iter().find(|h| h.doc_key == k).expect(k).score;
        assert!((score("a") - (1.0 / 61.0 + 1.0 / 63.0)).abs() < 1e-7);
        assert!((score("c") - (1.0 / 63.0 + 1.0 / 61.0)).abs() < 1e-7);
        assert!((score("b") - 1.0 / 62.0).abs() < 1e-7);
        assert!((score("d") - 1.0 / 62.0).abs() < 1e-7);
        // a and c tie on score: keyword rank breaks it. b and d tie: b has a keyword rank.
        assert_eq!(keys(&fused), ["a", "c", "b", "d"]);
        let d = fused.iter().find(|h| h.doc_key == "d").expect("d");
        assert!(d.vector_only());
        assert_eq!((d.keyword_rank, d.vector_rank), (None, Some(2)));
    }

    #[test]
    fn found_by_both_beats_found_by_one() {
        let fused = rrf(&["x", "both"], &["y", "both"], |_| false);
        assert_eq!(fused[0].doc_key, "both");
    }

    #[test]
    fn exact_names_stay_pinned_first_in_keyword_order() {
        // "fireball" is an exact name at keyword rank 3; "delayed-blast" exact at rank 1. The vector side loves
        // "flame-strike", which would otherwise win.
        let keyword = ["delayed-blast", "fire-bolt", "fireball", "flame-strike"];
        let vector = ["flame-strike", "fire-bolt", "burning-hands"];
        let exact = |k: &str| k == "fireball" || k == "delayed-blast";
        let fused = rrf(&keyword, &vector, exact);
        assert_eq!(&keys(&fused)[..2], ["delayed-blast", "fireball"]);
        assert!(fused[0].pinned && fused[1].pinned);
        assert_eq!(&keys(&fused)[2..4], ["fire-bolt", "flame-strike"]);
        assert!(!fused[2].pinned);
        assert_eq!(fused.last().expect("last").doc_key, "burning-hands");
    }

    #[test]
    fn exact_name_only_on_the_vector_side_is_not_pinned() {
        let fused = rrf(&["a"], &["b"], |k| k == "b");
        assert_eq!(keys(&fused), ["a", "b"]);
        assert!(!fused[1].pinned);
    }

    #[test]
    fn only_the_top_100_of_each_list_count() {
        let keyword: Vec<String> = (0..150).map(|i| format!("k{i}")).collect();
        let vector: Vec<String> = vec!["k120".into()];
        let fused = rrf(&keyword, &vector, |_| false);
        assert_eq!(fused.len(), 101); // k0..k99 plus k120 (vector side only)
        let k120 = fused.iter().find(|h| h.doc_key == "k120").expect("k120");
        assert_eq!((k120.keyword_rank, k120.vector_rank), (None, Some(1)));
    }

    #[test]
    fn duplicates_keep_their_first_rank() {
        let fused = rrf(&["a", "a", "b"], &[] as &[&str], |_| false);
        assert_eq!(keys(&fused), ["a", "b"]);
        assert_eq!(fused[1].keyword_rank, Some(2));
    }

    #[test]
    fn snippet_takes_the_best_overlap_sentence() {
        let text = "Fire spreads around corners. A grappled creature's speed becomes 0, and it can't benefit from \
                    bonuses to speed. The condition ends if the grappler is incapacitated.";
        let s = vector_only_snippet(text, "what happens to speed when grappled");
        assert_eq!(
            s,
            "A grappled creature's speed becomes 0, and it can't benefit from bonuses to speed."
        );
    }

    #[test]
    fn snippet_falls_back_to_the_first_200_chars() {
        let text = "x".repeat(500);
        assert_eq!(
            vector_only_snippet(&text, "nothing shared"),
            "x".repeat(200)
        );
        assert_eq!(vector_only_snippet("Short text.", "of an"), "Short text.");
    }

    #[test]
    fn snippet_is_capped_and_char_safe() {
        let long = format!("{} grapple.", "é".repeat(400));
        let s = vector_only_snippet(&long, "grapple");
        assert_eq!(s.chars().count(), SNIPPET_MAX_CHARS);
    }

    #[test]
    fn sentences_split_on_terminators_and_lines() {
        assert_eq!(
            sentences("One. Two! Three?\nFour 1.5 five"),
            ["One.", "Two!", "Three?", "Four 1.5 five"]
        );
    }
}
