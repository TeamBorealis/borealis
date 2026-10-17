//! Repairs the self-check makes itself before it checks, when the right answer is printed and
//! unambiguous (wave 13b). Each is recorded in the element's `normalised` notes ("Fixed
//! automatically" in the review), and each is idempotent (the checks run more than once):
//!
//! * choice options written as names point at their elements (`selfcheck_choices`);
//! * an item's attunement: a restriction stored as the setter's value becomes "required" with
//!   that restriction ([`super::super::mechanics::attunement_by`]), and an item whose printed
//!   header says "(requires attunement ...)" but has no attunement gets it;
//! * a spell the text says can be cast, kept as a bare use pool: when the text names exactly
//!   one spell the library or the import has, the element grants no spell, and its only rule
//!   pool is a fixed number of uses regained on a rest, the pool becomes that spell's free casts.

use borealis_engine::universe::Universe;
use serde_json::{json, Value};

use super::super::fidelity::plain;
use super::{DraftElement, ElementStatus, ImportDraft, Role};

/// Element types of an item (the attunement repair).
const ITEM_TYPES: [&str; 4] = ["Magic Item", "Item", "Weapon", "Armor"];
/// Non-blank lines at the start of an item's passage that hold its printed header.
const HEADER_LINES: usize = 3;
/// Words of an item's header that say it needs attunement.
const REQUIRES: &str = "requires attunement";

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn note(e: &mut DraftElement, n: String) {
    if !e.normalised.contains(&n) {
        e.normalised.push(n);
    }
}

/// Run every repair over the live elements of `draft`; `uni` is the rules library's universe
/// (names and editions of the elements a repair may point at).
pub(super) fn apply(uni: &Universe, draft: &mut ImportDraft) {
    super::choices::resolve_names(uni, draft);
    let headers: Vec<Option<String>> = draft
        .elements
        .iter()
        .map(|e| printed_attunement(draft, e))
        .collect();
    let spells: Vec<Option<(String, String)>> = draft
        .elements
        .iter()
        .map(|e| cast_spell(uni, draft, e))
        .collect();
    for ((e, header), spell) in draft.elements.iter_mut().zip(headers).zip(spells) {
        if e.json.is_null() || e.status == ElementStatus::Failed {
            continue;
        }
        if ITEM_TYPES.contains(&str_at(&e.json, "type")) {
            for n in attunement(&mut e.json, header.as_deref()) {
                note(e, n);
            }
        }
        if let Some((name, id)) = spell {
            if let Some(n) = pool_to_free_cast(&mut e.json, &name, &id) {
                note(e, n);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Attunement

/// What the item's printed header says after "requires attunement": `Some("")` for a bare
/// requirement, `Some("by a ...")` for a restriction; `None` when the header does not say it
/// once (no header, or two items' headers).
fn printed_attunement(draft: &ImportDraft, e: &DraftElement) -> Option<String> {
    if !ITEM_TYPES.contains(&e.ty.as_str()) {
        return None;
    }
    let p = draft.plan.elements.iter().find(|p| p.id == e.id)?;
    let text = super::super::source::section_text(&draft.source_text, p)?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next()?;
    let header: Vec<&str> = std::iter::once(first)
        .chain(lines.take_while(|l| !l.trim_start().starts_with('#')))
        .take(HEADER_LINES)
        .collect();
    let joined = header
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = joined.to_lowercase();
    if lower.matches(REQUIRES).count() != 1 {
        return None;
    }
    let at = lower.find(REQUIRES)? + REQUIRES.len();
    let rest = &joined[at..];
    let end = rest.find([')', '\n']).unwrap_or(rest.len());
    let restriction = rest[..end].trim().trim_end_matches('.').trim();
    Some(if restriction.to_lowercase().starts_with("by ") {
        restriction.to_string()
    } else {
        String::new()
    })
}

/// The attunement repair of one item: the generation fix ([`attunement_by`]) and, when the
/// printed header requires attunement, a setter that says so.
///
/// [`attunement_by`]: super::super::mechanics::attunement_by
fn attunement(el: &mut Value, printed: Option<&str>) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(by) = printed {
        let setters = el
            .as_object_mut()
            .map(|o| o.entry("setters").or_insert_with(|| json!([])));
        if let Some(list) = setters.and_then(Value::as_array_mut) {
            let has = list
                .iter()
                .any(|s| str_at(s, "name") == "attunement" && str_at(s, "value") != "false");
            if !has {
                list.retain(|s| str_at(s, "name") != "attunement");
                let mut setter = json!({"name": "attunement", "value": "true"});
                if !by.is_empty() {
                    setter["attributes"] = json!({"addition": by});
                }
                list.push(setter);
                notes.push(format!(
                    "attunement: required{} (its printed header says so)",
                    if by.is_empty() {
                        String::new()
                    } else {
                        format!(" \"{by}\"")
                    }
                ));
            }
        }
    }
    notes.extend(super::super::mechanics::attunement_by(el));
    notes
}

// ---------------------------------------------------------------------------------------------
// A spell kept as a use pool

/// The one spell (name, id) the element's text says can be cast that no rule of it grants,
/// resolved in the import or the library (the import's edition first).
fn cast_spell(uni: &Universe, draft: &ImportDraft, e: &DraftElement) -> Option<(String, String)> {
    if e.json.is_null()
        || matches!(e.role, Role::Text)
        || matches!(e.ty.as_str(), "Spell" | "Monster")
    {
        return None;
    }
    let rules = e.json.get("rules").and_then(Value::as_array)?;
    let grants_spell = rules.iter().any(|r| {
        str_at(r, "kind") == "grant_spell"
            || (str_at(r, "type") == "Spell" && matches!(str_at(r, "kind"), "grant" | "select"))
    });
    if grants_spell {
        return None;
    }
    let text = plain(str_at(&e.json, "description"));
    let names = super::losses::cast_names(&text);
    let [name] = names.as_slice() else {
        return None;
    };
    let import: Vec<(String, String, String)> = draft
        .elements
        .iter()
        .filter(|o| !o.json.is_null() && o.status != ElementStatus::Failed)
        .map(|o| {
            (
                str_at(&o.json, "type").to_string(),
                str_at(&o.json, "name").to_string(),
                o.id.clone(),
            )
        })
        .collect();
    let id = super::choices::by_name(uni, &import, "Spell", name)?;
    Some((name.clone(), id))
}

/// Replace the element's only `resource` rule (a fixed number of uses regained on a rest,
/// referred to by no other rule) with free casts of spell `id`.
fn pool_to_free_cast(el: &mut Value, name: &str, id: &str) -> Option<String> {
    let rules = el.get_mut("rules")?.as_array_mut()?;
    let pools: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| str_at(r, "kind") == "resource")
        .map(|(i, _)| i)
        .collect();
    let [at] = pools.as_slice() else {
        return None;
    };
    let pool = &rules[*at];
    let uses = pool
        .get("max")
        .filter(|m| str_at(m, "type") == "fixed")
        .and_then(|m| m.get("value"))
        .and_then(Value::as_u64)
        .or_else(|| pool.get("max").and_then(Value::as_u64))
        .filter(|n| *n > 0)?;
    let rests: Vec<&str> = match pool.get("recharge")? {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let recharge = if rests.contains(&"short_rest") {
        "short_rest"
    } else if rests.contains(&"long_rest") {
        "long_rest"
    } else {
        return None;
    };
    let pool_id = str_at(pool, "id").to_string();
    let referred = rules
        .iter()
        .enumerate()
        .any(|(i, r)| i != *at && !pool_id.is_empty() && r.to_string().contains(&pool_id));
    if referred {
        return None;
    }
    rules[*at] = json!({"kind": "grant_spell", "spell": id, "mode": "always_prepared",
        "free_cast": {"uses": uses, "recharge": recharge}});
    Some(format!(
        "the use pool \"{pool_id}\" became free casts of {name} ({id}): its text says the spell can be cast"
    ))
}
