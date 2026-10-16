//! What an element's rules point at and keep: a spell it grants (or offers in a choice) that
//! neither the import nor the rules library has (the sheet never shows it), and a pool its
//! passage says is regained on a rest that has no recharge (it never refills). Both read the
//! draft as the pack will hold it (the derived fixes applied, so a class-table counter is
//! there).

use std::collections::HashSet;

use borealis_content::ContentStore;
use serde_json::Value;

use super::{failed, norm, passed, quoted_list, readable_id, span};
use super::{DraftElement, ElementCheck, ImportDraft, Role, GRANT_UNRESOLVED, RECHARGE_MISSING};

/// Start of the note the derived fixes leave on an element that refers to an official spell
/// neither the import nor the library has (granted as a name-only placeholder, or dropped);
/// the same words as `feature_spells::official::UNRESOLVED_OFFICIAL`, read here as text so the
/// check needs no part of that step. The note quotes the spell's name.
pub(crate) const GRANT_NOTE_PREFIX: &str = "unresolved official reference: ";
/// Rule fields holding one spell id.
const SPELL_FIELDS: [&str; 2] = ["spell", "spell_id"];
/// Rule fields holding a list of spell ids.
const SPELL_LIST_FIELDS: [&str; 1] = ["spells"];
/// Words of a sentence that refills a pool on a rest.
const REGAIN_WORDS: [&str; 4] = ["regain", "recover", "regains", "recovers"];
/// The rests a pool can refill on.
const RESTS: [&str; 2] = ["short rest", "long rest"];
/// Words of a sentence that limits an element's own use to once per rest ("you can't use it
/// again until you finish a long rest").
const UNTIL_REST: &str = "until you finish a";
/// Fewest letters of a pool name looked for in a sentence.
const MIN_POOL_NAME: usize = 3;
/// Longest quote in a message.
const QUOTE_CHARS: usize = 120;

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The spell ids an element's rules grant or offer.
fn spell_targets(el: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for r in el
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for f in SPELL_FIELDS {
            out.push(str_at(r, f).to_string());
        }
        for f in SPELL_LIST_FIELDS {
            out.extend(
                r.get(f)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string),
            );
        }
        if str_at(r, "kind") == "select" && str_at(r, "type") == "Spell" {
            out.extend(
                str_at(r, "supports")
                    .split('|')
                    .map(|s| s.trim().to_string()),
            );
        }
    }
    out.retain(|id| id.starts_with("ID_"));
    out.dedup();
    out
}

/// The name a note quotes (`... "Ice Knife" is ...` -> "Ice Knife"), else the whole note.
fn quoted(note: &str) -> String {
    note.split('"')
        .nth(1)
        .filter(|q| !q.is_empty())
        .unwrap_or(note)
        .trim()
        .to_string()
}

/// What the grant check reads besides the draft: the element as the pack will hold it (the
/// derived doc), the ids the derived fixes add (placeholders), and the derive notes on it.
pub(super) struct GrantView<'a> {
    pub(super) doc: &'a Value,
    pub(super) extra: &'a HashSet<String>,
    pub(super) notes: Vec<&'a str>,
}

/// The grant check of `e`: every spell it grants or offers is in the import or the library.
/// A note the derived fixes leave on it ([`GRANT_NOTE_PREFIX`]: an official spell nobody has,
/// granted as a placeholder or dropped) fails it too, naming the spell.
pub(super) fn grant_checks(
    library: &ContentStore,
    draft: &ImportDraft,
    e: &DraftElement,
    view: &GrantView,
) -> Vec<ElementCheck> {
    let targets = spell_targets(view.doc);
    let noted: Vec<String> = view
        .notes
        .iter()
        .copied()
        .chain(e.issues.iter().map(String::as_str))
        .filter_map(|n| n.strip_prefix(GRANT_NOTE_PREFIX))
        .map(quoted)
        .collect();
    if targets.is_empty() && noted.is_empty() {
        return Vec::new();
    }
    let import: HashSet<&str> = draft.elements.iter().map(|x| x.id.as_str()).collect();
    let mut missing: Vec<String> = Vec::new();
    for name in targets
        .iter()
        .filter(|id| {
            !import.contains(id.as_str()) && !view.extra.contains(*id) && !library.contains_id(id)
        })
        .map(|id| match id.split_once("_SPELL_") {
            Some((_, name)) => readable_id(name, ""),
            None => readable_id(id, &draft.id_prefix),
        })
        .chain(noted)
    {
        if !missing.contains(&name) {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        return vec![passed(GRANT_UNRESOLVED, "Every spell it grants exists")];
    }
    vec![failed(
        GRANT_UNRESOLVED,
        format!(
            "It grants {}, which neither this import nor your rules have: the sheet shows {} only as a name or not at all. Install that content or change the grant",
            quoted_list(&missing),
            if missing.len() == 1 { "it" } else { "them" }
        ),
    )]
}

// ---------------------------------------------------------------------------------------------
// Pools without a recharge

/// A pool of the pack with no recharge: its words (name and id), and the element holding it.
#[derive(Debug, Clone)]
pub(super) struct Unrecharged {
    names: Vec<String>,
    owner: String,
}

/// The pools of the pack (derived docs: the draft as the pack will hold it) that never refill.
pub(super) fn unrecharged(docs: &[Value]) -> Vec<Unrecharged> {
    let mut out = Vec::new();
    for d in docs {
        for r in d
            .get("rules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|r| str_at(r, "kind") == "resource")
        {
            let empty = match r.get("recharge") {
                None | Some(Value::Null) => true,
                Some(Value::Array(a)) => a.is_empty(),
                Some(Value::String(s)) => s.trim().is_empty(),
                Some(_) => false,
            };
            if !empty {
                continue;
            }
            let names: Vec<String> = [str_at(r, "name"), str_at(r, "id")]
                .iter()
                .map(|n| norm(n))
                .filter(|n| n.len() >= MIN_POOL_NAME)
                .collect();
            out.push(Unrecharged {
                names,
                owner: str_at(d, "id").to_string(),
            });
        }
    }
    out
}

/// The sentences of `lines` (wrapped lines joined), lowercased and normalised.
fn sentences(lines: &[&str]) -> Vec<String> {
    lines
        .join(" ")
        .split(['.', '!', '?', ';'])
        .map(|s| format!(" {} ", norm(s)))
        .filter(|s| s.trim().len() > 1)
        .collect()
}

/// The recharge check of `e`: a sentence of its own passage that refills a pool on a rest names
/// a pool with no recharge (or, for a pool `e` holds, limits its use until a rest).
pub(super) fn recharge_checks(
    draft: &ImportDraft,
    lines: &[String],
    pools: &[Unrecharged],
    e: &DraftElement,
) -> Vec<ElementCheck> {
    if pools.is_empty() || e.role == Role::Text {
        return Vec::new();
    }
    let Some((s, end)) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(span)
    else {
        return Vec::new();
    };
    let others: Vec<(usize, usize)> = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.id != e.id)
        .filter_map(span)
        .filter(|(o, _)| *o > s && *o < end)
        .collect();
    let own: Vec<&str> = (s..end.min(lines.len()))
        .filter(|i| !others.iter().any(|(o, oe)| i >= o && i < oe))
        .map(|i| lines[i].as_str())
        .collect();
    let holds_pool = pools.iter().any(|p| p.owner == e.id);
    for sentence in sentences(&own) {
        let rest = RESTS.iter().any(|r| sentence.contains(&format!(" {r} ")));
        if !rest {
            continue;
        }
        let regains = REGAIN_WORDS
            .iter()
            .any(|w| sentence.contains(&format!(" {w} ")));
        let named = pools
            .iter()
            .find(|p| p.names.iter().any(|n| sentence.contains(&format!(" {n} "))));
        let own_pool = holds_pool && (regains || sentence.contains(&format!(" {UNTIL_REST} ")));
        if (regains && named.is_some()) || own_pool {
            let what = named
                .or_else(|| pools.iter().find(|p| p.owner == e.id))
                .and_then(|p| p.names.first().cloned())
                .unwrap_or_default();
            return vec![failed(
                RECHARGE_MISSING,
                format!(
                    "Its text says{} is regained on a rest (\"{}\"), but the pool has no recharge: the sheet will never refill it",
                    if what.is_empty() { String::new() } else { format!(" \"{what}\"") },
                    sentence.trim().chars().take(QUOTE_CHARS).collect::<String>()
                ),
            )];
        }
    }
    Vec::new()
}
