//! Local BM25 retrieval over [`ContentStore`] elements.
//!
//! Pure Rust, no network, no embedding model. The index is built lazily on first use
//! ([`LazyIndex`]) and cached for the lifetime of the store `Arc`; build a new [`LazyIndex`]
//! when the loaded packs change.
//!
//! Indexed fields: name (weight 3), element type and source (weight 1), and the plain-text
//! description (HTML stripped, weight 1). Elements without a description are skipped because
//! there is nothing to cite.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use borealis_content::{ContentStore, Ruleset, SourceRestriction};
use serde::{Deserialize, Serialize};

const K1: f32 = 1.2;
const B: f32 = 0.75;
const NAME_WEIGHT: f32 = 3.0;
/// Score multiplier when every token of the element name appears in the query.
const NAME_MATCH_BOOST: f32 = 1.5;
/// Extra multiplier when the unstemmed name words all appear verbatim in the query.
const EXACT_NAME_BOOST: f32 = 1.25;
/// Multiplier when every token of the element type appears in the query ("... condition").
const TYPE_MATCH_BOOST: f32 = 1.2;
/// Longest snippet returned in a [`Hit`], in characters.
pub const SNIPPET_CHARS: usize = 280;
/// Minimum stem length kept after suffix stripping.
const MIN_STEM: usize = 3;

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how",
    "i", "if", "in", "is", "it", "my", "of", "on", "or", "that", "the", "to", "what", "when",
    "which", "with", "you", "your",
];

/// Restricts which elements may be returned.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    /// Only elements usable by this ruleset (elements tagged `any` or untagged always pass).
    pub ruleset: Option<Ruleset>,
    /// Only elements loaded from these pack ids; `None` means every pack.
    pub enabled_packs: Option<BTreeSet<String>>,
    /// Elements whose source is unchecked are excluded.
    pub restriction: SourceRestriction,
}

/// One retrieval result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// Element id (the citation key).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Element type, e.g. `Condition`.
    #[serde(rename = "type")]
    pub ty: String,
    /// Ruleset tag of the element, when it has one.
    pub ruleset: Option<Ruleset>,
    /// Source name, e.g. `SRD 5.2.1`.
    pub source: String,
    /// Short plain-text excerpt around the best match.
    pub snippet: String,
    /// BM25 score (higher is better).
    pub score: f32,
}

struct Doc {
    id: String,
    name: String,
    ty: String,
    source: String,
    ruleset: Option<Ruleset>,
    pack: Option<String>,
    text: String,
    name_tokens: HashSet<String>,
    raw_name_words: HashSet<String>,
    type_tokens: HashSet<String>,
    len: f32,
}

/// A built BM25 index.
pub struct RetrievalIndex {
    docs: Vec<Doc>,
    postings: HashMap<String, Vec<(u32, f32)>>,
    by_id: HashMap<String, u32>,
    avg_len: f32,
}

impl RetrievalIndex {
    /// Index every element of `store` that has a non-empty description.
    #[must_use]
    pub fn build(store: &ContentStore) -> Self {
        let mut docs: Vec<Doc> = Vec::new();
        let mut postings: HashMap<String, Vec<(u32, f32)>> = HashMap::new();
        let mut by_id = HashMap::new();
        for stored in store.iter() {
            let e = &stored.element;
            let text = html_to_text(&e.description);
            if text.is_empty() || e.name.trim().is_empty() {
                continue;
            }
            let mut tf: HashMap<String, f32> = HashMap::new();
            let name_tokens: Vec<String> = tokenize(&e.name);
            for t in &name_tokens {
                *tf.entry(t.clone()).or_default() += NAME_WEIGHT;
            }
            for t in tokenize(&e.ty).into_iter().chain(tokenize(&e.source)) {
                *tf.entry(t).or_default() += 1.0;
            }
            for t in tokenize(&text) {
                *tf.entry(t).or_default() += 1.0;
            }
            let doc_idx = u32::try_from(docs.len()).unwrap_or(u32::MAX);
            let len: f32 = tf.values().sum();
            for (term, w) in tf {
                postings.entry(term).or_default().push((doc_idx, w));
            }
            by_id.insert(e.id.clone(), doc_idx);
            docs.push(Doc {
                id: e.id.clone(),
                name: e.name.clone(),
                ty: e.ty.clone(),
                source: e.source.clone(),
                ruleset: e.ruleset,
                pack: stored.file.split('/').next().map(str::to_string),
                text,
                raw_name_words: words(&e.name).into_iter().collect(),
                type_tokens: tokenize(&e.ty).into_iter().collect(),
                name_tokens: name_tokens.into_iter().collect(),
                len,
            });
        }
        let avg_len = if docs.is_empty() {
            1.0
        } else {
            docs.iter().map(|d| d.len).sum::<f32>() / docs.len() as f32
        };
        Self {
            docs,
            postings,
            by_id,
            avg_len,
        }
    }

    /// Number of indexed elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// True when nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Plain-text description of an indexed element.
    #[must_use]
    pub fn text(&self, id: &str) -> Option<&str> {
        self.by_id
            .get(id)
            .map(|&i| self.docs[i as usize].text.as_str())
    }

    fn passes(doc: &Doc, filters: &Filters) -> bool {
        if let (Some(profile), Some(tag)) = (filters.ruleset, doc.ruleset) {
            if !tag.compatible_with(profile) {
                return false;
            }
        }
        if let Some(packs) = &filters.enabled_packs {
            if !doc.pack.as_ref().is_some_and(|p| packs.contains(p)) {
                return false;
            }
        }
        !filters.restriction.is_restricted(&doc.source)
    }

    /// Top `k` elements for `query`, best first. Ties break by id for determinism.
    #[must_use]
    pub fn retrieve(&self, query: &str, k: usize, filters: &Filters) -> Vec<Hit> {
        let terms: Vec<String> = {
            let mut seen = HashSet::new();
            tokenize(query)
                .into_iter()
                .filter(|t| seen.insert(t.clone()))
                .collect()
        };
        if terms.is_empty() || k == 0 {
            return Vec::new();
        }
        let n = self.docs.len() as f32;
        let mut scores: HashMap<u32, f32> = HashMap::new();
        for term in &terms {
            let Some(list) = self.postings.get(term) else {
                continue;
            };
            let df = list.len() as f32;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for &(doc, w) in list {
                let d = &self.docs[doc as usize];
                if !Self::passes(d, filters) {
                    continue;
                }
                let norm = w * (K1 + 1.0) / (w + K1 * (1.0 - B + B * d.len / self.avg_len));
                *scores.entry(doc).or_default() += idf * norm;
            }
        }
        let query_set: HashSet<&str> = terms.iter().map(String::as_str).collect();
        let query_words: HashSet<String> = words(query).into_iter().collect();
        let mut ranked: Vec<(u32, f32)> = scores
            .into_iter()
            .map(|(doc, s)| {
                let d = &self.docs[doc as usize];
                let name_match = !d.name_tokens.is_empty()
                    && d.name_tokens.iter().all(|t| query_set.contains(t.as_str()));
                let mut s = s;
                if name_match {
                    s *= NAME_MATCH_BOOST;
                    if d.raw_name_words.iter().all(|w| query_words.contains(w)) {
                        s *= EXACT_NAME_BOOST;
                    }
                }
                if !d.type_tokens.is_empty()
                    && d.type_tokens.iter().all(|t| query_set.contains(t.as_str()))
                {
                    s *= TYPE_MATCH_BOOST;
                }
                (doc, s)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| self.docs[a.0 as usize].id.cmp(&self.docs[b.0 as usize].id))
        });
        ranked
            .into_iter()
            .take(k)
            .map(|(doc, score)| {
                let d = &self.docs[doc as usize];
                Hit {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    ty: d.ty.clone(),
                    ruleset: d.ruleset,
                    source: d.source.clone(),
                    snippet: snippet(&d.text, &terms),
                    score,
                }
            })
            .collect()
    }
}

/// A [`RetrievalIndex`] built on first use and cached.
pub struct LazyIndex {
    store: Arc<ContentStore>,
    index: OnceLock<RetrievalIndex>,
}

impl LazyIndex {
    /// Wrap a store; nothing is indexed until the first query.
    #[must_use]
    pub fn new(store: Arc<ContentStore>) -> Self {
        Self {
            store,
            index: OnceLock::new(),
        }
    }

    /// The index, building it if needed.
    pub fn index(&self) -> &RetrievalIndex {
        self.index
            .get_or_init(|| RetrievalIndex::build(&self.store))
    }

    /// Convenience for [`RetrievalIndex::retrieve`].
    pub fn retrieve(&self, query: &str, k: usize, filters: &Filters) -> Vec<Hit> {
        self.index().retrieve(query, k, filters)
    }
}

/// Lowercase alphanumeric words, no stopword removal or stemming.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Lowercase alphanumeric tokens with stopwords removed and a light stem applied.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect()
}

fn stem(word: &str) -> String {
    let mut w = word;
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(rest) = w.strip_suffix(suffix) {
            if suffix == "s" && rest.ends_with('s') {
                break;
            }
            if rest.chars().count() > MIN_STEM {
                w = rest;
            }
            break;
        }
    }
    if w.chars().count() > MIN_STEM + 1 {
        w = w.strip_suffix('e').unwrap_or(w);
    }
    w.to_string()
}

/// Strip HTML tags and decode the common entities; block tags become spaces.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A window of about [`SNIPPET_CHARS`] characters starting near the first query-term match.
fn snippet(text: &str, terms: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= SNIPPET_CHARS {
        return text.to_string();
    }
    let start = first_match_start(&chars, terms).saturating_sub(40);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

fn first_match_start(chars: &[char], terms: &[String]) -> usize {
    let mut idx = 0;
    let mut word_start = 0;
    while idx <= chars.len() {
        let at_end = idx == chars.len() || !chars[idx].is_alphanumeric();
        if at_end {
            if idx > word_start {
                let word: String = chars[word_start..idx].iter().collect();
                if tokenize(&word).iter().any(|t| terms.contains(t)) {
                    return word_start;
                }
            }
            word_start = idx + 1;
        }
        idx += 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_stripped_and_entities_decoded() {
        assert_eq!(
            html_to_text("<p>Speed 0.</p><p><b>Bold</b> &amp; fast&nbsp;now</p>"),
            "Speed 0. Bold & fast now"
        );
    }

    #[test]
    fn stemming_unifies_grapple_forms() {
        let forms = ["grappled", "grapple", "grappling", "grapples"];
        let stems: HashSet<String> = forms.iter().map(|f| stem(f)).collect();
        assert_eq!(stems.len(), 1, "{stems:?}");
    }

    #[test]
    fn stopwords_are_dropped() {
        assert_eq!(tokenize("What is the Grappled condition?").len(), 2);
    }
}
