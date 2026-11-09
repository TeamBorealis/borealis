//! The description pane's Grants list: what choosing an element gives, in level order, named the
//! way the builder asks for it (tester video 2026-11-02: A6, B9).

use crate::dto_ext::ElementGrant;
use borealis_content::{ContentStore, CustomRule, Element, Rule};
use serde_json::Value;

/// The level of a rule with no level gate.
const FIRST_LEVEL: u32 = 1;

fn name_of(store: &ContentStore, id: &str) -> String {
    store
        .element(id)
        .map_or_else(|| id.to_string(), |e| e.name.clone())
}

/// What choosing `el` grants, by level (stable within a level). Only the element's own rules: a
/// class's multiclass node (a bare "Multiclass" grant and proficiencies the class already gives)
/// applies only when the class is taken as a multiclass (A6).
pub(crate) fn grants(store: &ContentStore, el: &Element) -> Vec<ElementGrant> {
    let mut out: Vec<ElementGrant> = Vec::new();
    for rule in &el.rules {
        let entry = match rule {
            Rule::Grant(g) => Some((name_of(store, &g.id), g.level)),
            Rule::Select(s) => {
                let what = if s.name.is_empty() { &s.ty } else { &s.name };
                Some((format!("Choose {}: {what}", s.number.unwrap_or(1)), s.level))
            }
            Rule::Custom(c) => custom_grant(store, c).map(|name| (name, custom_level(c))),
            Rule::Stat(_) | Rule::Unknown(_) => None,
        };
        let Some((name, level)) = entry else { continue };
        let grant = ElementGrant {
            name,
            level: level.unwrap_or(FIRST_LEVEL).max(FIRST_LEVEL),
        };
        if !out.contains(&grant) {
            out.push(grant);
        }
    }
    out.sort_by_key(|g| g.level);
    out
}

fn custom_level(c: &CustomRule) -> Option<u32> {
    c.fields
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|l| u32::try_from(l).ok())
}

/// B9: the custom rules that give the character something to have or to choose, named the way
/// the builder asks for them ("Magic Initiate (Cleric)", "Choose: Ability Scores (Acolyte)").
fn custom_grant(store: &ContentStore, c: &CustomRule) -> Option<String> {
    let text = |key: &str| {
        c.fields
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    match c.kind.as_str() {
        "grant_feat" => {
            let name = name_of(store, text("feat")?);
            let params: Vec<&str> = c
                .fields
                .get("params")
                .and_then(Value::as_object)
                .map(|p| p.values().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            Some(if params.is_empty() {
                name
            } else {
                format!("{name} ({})", params.join(", "))
            })
        }
        "ability_distribution" | "equipment_package" => {
            text("name").map(|n| format!("Choose: {n}"))
        }
        _ => None,
    }
}
