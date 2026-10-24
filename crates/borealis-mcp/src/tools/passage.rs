//! `get_passage {cite}`: the full text (at most 4,000 characters) behind a search hit.

use borealis_content::Ruleset;
use borealis_search::{base_key, Corpus, Passage};
use serde::Deserialize;
use serde_json::{json, Value};

use super::sessions::{self, SESSION_PREFIX};
use super::{parse_args, plain_text, Tools};

/// Longest passage returned, in characters.
pub const MAX_PASSAGE_CHARS: usize = 4_000;
/// Longest accepted cite.
const MAX_CITE_CHARS: usize = 300;
const RULES_PREFIX: &str = "rules:";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    cite: String,
}

/// The message for a cite that resolves to nothing (also used for another campaign's session cite).
pub fn not_found(cite: &str) -> String {
    format!("no passage for cite {cite:?}; use a `cite` value from a search result")
}

/// The tool result for one passage, text cut to [`MAX_PASSAGE_CHARS`].
pub fn result(
    cite: &str,
    title: &str,
    path: &str,
    ty: &str,
    ruleset: Option<&str>,
    source: &str,
    text: &str,
) -> Value {
    let truncated = text.chars().count() > MAX_PASSAGE_CHARS;
    let text: String = text.chars().take(MAX_PASSAGE_CHARS).collect();
    json!({
        "cite": cite, "title": title, "path": path, "type": ty, "ruleset": ruleset,
        "source": source, "text": text, "truncated": truncated
    })
}

fn from_index(cite: &str, p: &Passage) -> Value {
    let ruleset = (p.ruleset != "none").then_some(p.ruleset.as_str());
    result(cite, &p.title, &p.path, &p.ty, ruleset, &p.source, &p.body)
}

/// Exact key first, then the first chunk of a split document (`cite#1`).
fn lookup(tools: &Tools, corpus: Corpus, cite: &str) -> Option<Passage> {
    let engine = tools.search_engine()?;
    let exact = engine.passage(corpus, cite).ok().flatten();
    let found = exact.or_else(|| {
        if base_key(cite) == cite {
            engine.passage(corpus, &format!("{cite}#1")).ok().flatten()
        } else {
            None
        }
    });
    // Rules and Lore never hold campaign text; refuse a chunk that claims a campaign anyway.
    found.filter(|p| p.campaign.is_none() && tools.pack_allowed(&p.pack))
}

/// An element straight from the loaded content (no index, or the index lacks it).
fn from_store(tools: &Tools, cite: &str) -> Option<Value> {
    let element = tools.store.element(base_key(cite))?;
    let text = plain_text(&element.description, MAX_PASSAGE_CHARS);
    let ruleset = element.ruleset.and_then(|r| match r {
        Ruleset::Dnd2014 => Some("2014"),
        Ruleset::Dnd2024 => Some("2024"),
        Ruleset::Any => None,
    });
    Some(result(
        cite,
        &element.name,
        &element.ty,
        &element.ty,
        ruleset,
        &element.source,
        &text,
    ))
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    let cite = a.cite.trim();
    if cite.is_empty() || cite.chars().count() > MAX_CITE_CHARS {
        return Err("cite must be a search hit's `cite` value".into());
    }
    if cite.starts_with(SESSION_PREFIX) {
        return sessions::passage(tools, cite);
    }
    if cite.starts_with(RULES_PREFIX) {
        return lookup(tools, Corpus::Rules, cite)
            .map(|p| from_index(cite, &p))
            .ok_or_else(|| {
                if tools.search_engine().is_none() {
                    "rules text is not available: the Borealis app has not built its search index yet"
                        .to_string()
                } else {
                    not_found(cite)
                }
            });
    }
    if let Some(p) = lookup(tools, Corpus::Lore, cite) {
        return Ok(from_index(cite, &p));
    }
    if let Some(p) = lookup(tools, Corpus::Rules, cite) {
        return Ok(from_index(cite, &p));
    }
    from_store(tools, cite).ok_or_else(|| not_found(cite))
}
