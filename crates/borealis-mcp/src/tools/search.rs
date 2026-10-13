//! `search_content`.

use borealis_ai::retrieval::Filters;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, parse_ruleset, Tools};

const DEFAULT_K: usize = 8;
const MAX_K: usize = 20;
const SNIPPET_CHARS: usize = 240;
/// Over-fetch factor when a type filter is applied after ranking.
const OVERFETCH: usize = 8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    query: String,
    #[serde(rename = "type")]
    ty: Option<String>,
    ruleset: Option<String>,
    k: Option<usize>,
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    if a.query.trim().is_empty() {
        return Err("query must not be empty".into());
    }
    let k = a.k.unwrap_or(DEFAULT_K).clamp(1, MAX_K);
    let filters = Filters {
        ruleset: a.ruleset.as_deref().map(parse_ruleset).transpose()?,
        ..Filters::default()
    };
    let fetch = if a.ty.is_some() { k * OVERFETCH } else { k };
    let want = a.ty.as_ref().map(|t| t.to_lowercase());
    let hits: Vec<Value> = tools
        .index()
        .retrieve(&a.query, fetch, &filters)
        .into_iter()
        .filter(|h| want.as_ref().map_or(true, |w| h.ty.to_lowercase() == *w))
        .take(k)
        .map(|h| {
            json!({
                "id": h.id, "name": h.name, "type": h.ty, "ruleset": h.ruleset,
                "source": h.source, "snippet": h.snippet.chars().take(SNIPPET_CHARS).collect::<String>()
            })
        })
        .collect();
    Ok(json!({"count": hits.len(), "hits": hits}))
}
