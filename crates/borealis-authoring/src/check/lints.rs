//! Creator lints of the live pipeline (spec 8.1 step 5): what loads but is likely a mistake. A name another element
//! of the same type in the pack has, a description left empty, escape leftovers in it (spec 7.3), a resource cost
//! naming a pool nothing defines. (Choices and spell lists that match nothing need the engine's view of the content;
//! they are engine checks, `deep.rs`.)

use super::messages::{type_noun, words, Finding};
use crate::form::{form_path_for_location, kind, FormContext, FormState};
use borealis_content::rich;
use borealis_content::{ContentStore, Element, Rule};
use std::collections::HashSet;

/// A finding at a form path, on an element.
pub type Lint = (String, Option<String>, Finding);

/// An element of the pack the draft belongs to (for duplicate names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackElement {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its type.
    pub ty: String,
}

fn fold(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The text of a description (tags dropped).
fn has_text(html: &str) -> bool {
    let mut inside = false;
    html.chars().any(|c| match c {
        '<' => {
            inside = true;
            false
        }
        '>' => {
            inside = false;
            false
        }
        c => !inside && !c.is_whitespace(),
    })
}

/// The lints of the draft's elements; `pairs` are each form of the draft (with its path prefix) and the element it
/// writes.
pub fn lints(
    pairs: &[(String, &FormState, &Element)],
    pack: &[PackElement],
    store: &ContentStore,
    cx: &FormContext<'_>,
) -> Vec<Lint> {
    let own: HashSet<&str> = pairs.iter().map(|(_, _, e)| e.id.as_str()).collect();
    let mut pools: Option<HashSet<String>> = None;
    let mut out = Vec::new();
    for (prefix, form, e) in pairs {
        let id = Some(e.id.clone());
        let fields = kind(&form.kind)
            .map(|k| k.fields.as_slice())
            .unwrap_or_default();
        let has = |key: &str| fields.iter().any(|f| f.key == key);
        let name = fold(&e.name);
        if has("name") && !name.is_empty() {
            let twin = pack
                .iter()
                .any(|p| !own.contains(p.id.as_str()) && p.ty == e.ty && fold(&p.name) == name);
            if twin {
                out.push((
                    format!("{prefix}fields.name"),
                    id.clone(),
                    Finding::DuplicateName {
                        noun: type_noun(&e.ty),
                        name: e.name.trim().to_string(),
                    },
                ));
            }
        }
        if has("description") {
            let at = format!("{prefix}fields.description");
            if !has_text(&e.description) {
                out.push((at, id.clone(), Finding::EmptyDescription));
            } else if !rich::lint(&rich::parse(&e.description)).is_empty() {
                out.push((at, id.clone(), Finding::Escape));
            }
        }
        if let Some(noun) = choice_noun(&e.ty) {
            let draft: Vec<&Element> = pairs.iter().map(|(_, _, d)| *d).collect();
            if has("parent") && !e.supports.is_empty() && !offered(e, store, &draft) {
                out.push((
                    format!("{prefix}fields.parent"),
                    id.clone(),
                    Finding::NotOffered { noun: noun.into() },
                ));
            }
        }
        for (i, rule) in e.rules.iter().enumerate() {
            let Some(pool) = cost_pool(rule) else {
                continue;
            };
            let known = pools.get_or_insert_with(|| defined_pools(store, pairs));
            if !known.contains(pool) {
                let at = form_path_for_location(form, e, &format!("rules[{i}]"), cx)
                    .map(|p| format!("{prefix}{p}.fields.resource"))
                    .unwrap_or_else(|| prefix.trim_end_matches('.').to_string());
                out.push((at, id.clone(), Finding::UnknownPool { pool: words(pool) }));
            }
        }
    }
    out
}

/// What a race's choice of this type is called ("subrace"), for the types a race offers by tag.
fn choice_noun(ty: &str) -> Option<&'static str> {
    match ty {
        "Sub Race" => Some("subrace"),
        "Race Variant" => Some("variant"),
        _ => None,
    }
}

/// A choice of `e`'s type somewhere in the loaded content or the draft names one of its tags or its id.
fn offered(e: &Element, store: &ContentStore, draft: &[&Element]) -> bool {
    let loaded = store.iter().map(|s| &s.element);
    loaded
        .chain(draft.iter().copied())
        .flat_map(Element::all_rules)
        .any(|r| match r {
            Rule::Select(s) if s.ty == e.ty => s.parse_supports().ok().flatten().is_some_and(|x| {
                x.tags()
                    .iter()
                    .any(|t| *t == e.id || e.supports.iter().any(|own| own == t))
            }),
            _ => false,
        })
}

/// The pool a `resource_cost` rule spends.
fn cost_pool(rule: &Rule) -> Option<&str> {
    match rule {
        Rule::Custom(c) if c.kind == "resource_cost" => {
            c.fields.get("resource").and_then(serde_json::Value::as_str)
        }
        _ => None,
    }
}

/// The ids of every `resource` pool of the loaded content and the draft.
fn defined_pools(
    store: &ContentStore,
    pairs: &[(String, &FormState, &Element)],
) -> HashSet<String> {
    let loaded = store.iter().map(|s| &s.element);
    let draft = pairs.iter().map(|(_, _, e)| *e);
    loaded
        .chain(draft)
        .flat_map(Element::all_rules)
        .filter_map(|r| match r {
            Rule::Custom(c) if c.kind == "resource" => c
                .fields
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            _ => None,
        })
        .collect()
}
