//! `search_rules`, `search_lore` and the `search_content` alias (Rules + Lore).
//!
//! With the app's index available the tools query it read-only (Tantivy, adopted keyword config).
//! Without it (the app never ran, or the index is stale) they answer from the in-memory content
//! index, as before this index existed. Every hit carries `cite`, the key `get_passage` and
//! `[[cite]]` citations use.

use borealis_ai::retrieval::Filters;
use borealis_content::Ruleset;
use borealis_search::corpus::RULES_ELEMENT_TYPES;
use borealis_search::{
    base_key, Corpus, Edition, EditionScope, Hit, SearchConfig, SearchEngine, SearchRequest,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, parse_ruleset, Tools};

const DEFAULT_K: usize = 8;
const MAX_K: usize = 20;
pub const SNIPPET_CHARS: usize = 240;
/// Over-fetch factor when a type filter is applied after ranking (in-memory path).
const OVERFETCH: usize = 8;

/// Which corpora a tool searches.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Rules,
    Lore,
    Both,
}

impl Scope {
    fn corpora(self) -> &'static [Corpus] {
        match self {
            Self::Rules => &[Corpus::Rules],
            Self::Lore => &[Corpus::Lore],
            Self::Both => &[Corpus::Rules, Corpus::Lore],
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    query: String,
    #[serde(rename = "type")]
    ty: Option<String>,
    ruleset: Option<String>,
    k: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesArgs {
    query: String,
    ruleset: Option<String>,
    k: Option<usize>,
}

pub fn content(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    search(tools, Scope::Both, a)
}

pub fn lore(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    search(tools, Scope::Lore, a)
}

pub fn rules(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: RulesArgs = parse_args(args)?;
    let a = Args {
        query: a.query,
        ty: None,
        ruleset: a.ruleset,
        k: a.k,
    };
    search(tools, Scope::Rules, a)
}

fn search(tools: &Tools, scope: Scope, a: Args) -> Result<Value, String> {
    if a.query.trim().is_empty() {
        return Err("query must not be empty".into());
    }
    let k = a.k.unwrap_or(DEFAULT_K).clamp(1, MAX_K);
    let ruleset = a.ruleset.as_deref().map(parse_ruleset).transpose()?;
    let hits = tools
        .search_engine()
        .and_then(|engine| indexed(engine, tools.allowed_packs(), scope, &a, ruleset, k))
        .unwrap_or_else(|| in_memory(tools, scope, &a, ruleset, k));
    Ok(json!({"count": hits.len(), "hits": hits}))
}

fn edition_scope(ruleset: Option<Ruleset>) -> EditionScope {
    match ruleset {
        Some(Ruleset::Dnd2014) => EditionScope::Only(Edition::Y2014),
        Some(Ruleset::Dnd2024) => EditionScope::Only(Edition::Y2024),
        Some(Ruleset::Any) | None => EditionScope::Any,
    }
}

/// `None` when the index fails (stale, being rebuilt): the caller falls back to memory.
fn indexed(
    engine: &SearchEngine,
    packs: Option<&[String]>,
    scope: Scope,
    a: &Args,
    ruleset: Option<Ruleset>,
    k: usize,
) -> Option<Vec<Value>> {
    let request = SearchRequest {
        edition: edition_scope(ruleset),
        types: a.ty.iter().cloned().collect(),
        limit: k,
        packs: packs.map(<[String]>::to_vec),
        ..SearchRequest::new(a.query.clone())
    };
    let config = SearchConfig::default();
    let mut per_corpus: Vec<std::vec::IntoIter<Hit>> = Vec::new();
    for corpus in scope.corpora() {
        // A campaign-tagged chunk never belongs to Rules or Lore; searching without a campaign
        // filter must not surface one either.
        let hits = engine.search(*corpus, &request, &config).ok()?;
        per_corpus.push(hits.into_iter());
    }
    Some(interleave(per_corpus, k))
}

/// Hits of several corpora by rank (scores are not comparable across corpora), the same document
/// once, at most `k`.
fn interleave(mut lists: Vec<std::vec::IntoIter<Hit>>, k: usize) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    loop {
        let mut progressed = false;
        for list in &mut lists {
            let Some(hit) = list.next() else { continue };
            progressed = true;
            if seen.insert(base_key(&hit.doc_key).to_string()) {
                out.push(index_hit(&hit));
                if out.len() == k {
                    return out;
                }
            }
        }
        if !progressed {
            return out;
        }
    }
}

fn index_hit(h: &Hit) -> Value {
    let mut v = json!({
        "cite": h.doc_key, "name": h.title, "type": h.ty, "path": h.path,
        "ruleset": (h.ruleset != "none").then_some(h.ruleset.as_str()),
        "source": h.source,
        "snippet": h.snippet.chars().take(SNIPPET_CHARS).collect::<String>()
    });
    if !h.doc_key.starts_with("rules:") {
        v["id"] = json!(base_key(&h.doc_key));
    }
    v
}

fn is_rules_type(ty: &str) -> bool {
    RULES_ELEMENT_TYPES
        .iter()
        .any(|t| t.eq_ignore_ascii_case(ty))
}

fn in_memory(
    tools: &Tools,
    scope: Scope,
    a: &Args,
    ruleset: Option<Ruleset>,
    k: usize,
) -> Vec<Value> {
    let filters = Filters {
        ruleset,
        ..Filters::default()
    };
    let want = a.ty.as_ref().map(|t| t.to_lowercase());
    let narrowed = want.is_some() || scope != Scope::Both;
    let fetch = if narrowed { k * OVERFETCH } else { k };
    tools
        .index()
        .retrieve(&a.query, fetch, &filters)
        .into_iter()
        .filter(|h| want.as_ref().is_none_or(|w| h.ty.to_lowercase() == *w))
        .filter(|h| match scope {
            Scope::Both => true,
            Scope::Rules => is_rules_type(&h.ty),
            Scope::Lore => !is_rules_type(&h.ty),
        })
        .take(k)
        .map(|h| {
            json!({
                "cite": h.id, "id": h.id, "name": h.name, "type": h.ty, "ruleset": h.ruleset,
                "source": h.source,
                "snippet": h.snippet.chars().take(SNIPPET_CHARS).collect::<String>()
            })
        })
        .collect()
}
