//! `explain_rules_term`.

use borealis_ai::retrieval::Filters;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, parse_ruleset, plain_text, Tools};

const TEXT_CHARS: usize = 1_500;
const MAX_MATCHES: usize = 3;
const GLOSSARY_TYPES: &[&str] = &["Rules Glossary Entry", "Condition"];
const FUZZY_FETCH: usize = 30;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    term: String,
    ruleset: Option<String>,
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let a: Args = parse_args(args)?;
    let term = a.term.trim().to_lowercase();
    if term.is_empty() {
        return Err("term must not be empty".into());
    }
    let ruleset = a.ruleset.as_deref().map(parse_ruleset).transpose()?;
    let mut matches: Vec<Value> = tools
        .store
        .iter()
        .map(|s| &s.element)
        .filter(|e| GLOSSARY_TYPES.contains(&e.ty.as_str()) && !e.id.starts_with("ID_INTERNAL"))
        .filter(|e| e.name.trim().to_lowercase() == term)
        .filter(|e| match (ruleset, e.ruleset) {
            (Some(want), Some(have)) => have.compatible_with(want),
            _ => true,
        })
        .take(MAX_MATCHES)
        .map(|e| {
            json!({"id": e.id, "name": e.name, "type": e.ty, "ruleset": e.ruleset,
                   "text": plain_text(&e.description, TEXT_CHARS)})
        })
        .collect();
    let mut exact = true;
    if matches.is_empty() {
        exact = false;
        let filters = Filters {
            ruleset,
            ..Filters::default()
        };
        matches = tools
            .index()
            .retrieve(&a.term, FUZZY_FETCH, &filters)
            .into_iter()
            .filter(|h| GLOSSARY_TYPES.contains(&h.ty.as_str()))
            .take(MAX_MATCHES)
            .filter_map(|h| {
                let e = tools.store.element(&h.id)?;
                Some(
                    json!({"id": e.id, "name": e.name, "type": e.ty, "ruleset": e.ruleset,
                            "text": plain_text(&e.description, TEXT_CHARS)}),
                )
            })
            .collect();
    }
    if matches.is_empty() {
        return Err(format!(
            "no glossary entry or condition found for {:?}",
            a.term
        ));
    }
    Ok(json!({"exact": exact, "matches": matches}))
}
