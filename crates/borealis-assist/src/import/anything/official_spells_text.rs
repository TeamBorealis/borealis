//! Cantrips an element's own text gives ("you learn the X cantrip"): found in the text of each
//! element and granted like a table's spells (see the parent module). Split from the table
//! resolution in `official_spells.rs` with no change in behaviour.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::super::super::types::{ImportPlan, Role};
use super::super::text::{candidates, key, named_cantrips, strip_join};
use super::{
    placeholder_note, rules_mut, str_at, target_field, title, Known, Made, NOT_GRANTED,
    SMALL_WORDS, UNRESOLVED_OFFICIAL,
};

/// Element types whose own text can give a cantrip (traits, features, feats, items, races).
const GIVING_TYPES: [&str; 12] = [
    "Race",
    "Sub Race",
    "Racial Trait",
    "Class Feature",
    "Archetype Feature",
    "Feat",
    "Feat Feature",
    "Background",
    "Background Feature",
    "Magic Item",
    "Item",
    "Weapon",
];
/// Most words of a spell name read from a sentence.
const MAX_NAME_WORDS: usize = 5;
/// Words that make a "the ... cantrip" phrase no spell name ("the chosen cantrip").
const NOT_NAME_WORDS: [&str; 26] = [
    "cantrip",
    "cantrips",
    "following",
    "chosen",
    "same",
    "selected",
    "listed",
    "above",
    "below",
    "new",
    "other",
    "one",
    "two",
    "three",
    "that",
    "this",
    "these",
    "those",
    "your",
    "its",
    "their",
    "any",
    "each",
    "bonus",
    "extra",
    "additional",
];
/// Most grant links followed from an element to its family (a cycle never loops).
const MAX_FAMILY_DEPTH: usize = 4;

/// A name printed in the "the ... cantrip" phrase of `text`, as a spell: marks, a parenthetical
/// note and a glued footnote marker left out.
fn printed_name(piece: &str) -> String {
    let all = candidates(piece);
    let base = all
        .get(1)
        .filter(|_| piece.contains(" ("))
        .unwrap_or(&all[0]);
    candidates(base).last().cloned().unwrap_or_default()
}

/// `name` reads as a spell's name: one to [`MAX_NAME_WORDS`] words of letters (an apostrophe
/// or hyphen inside), none of them a word of [`NOT_NAME_WORDS`].
fn name_like(name: &str) -> bool {
    let words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    !words.is_empty()
        && words.len() <= MAX_NAME_WORDS
        && words.iter().all(|w| {
            w.chars()
                .all(|c| c.is_alphabetic() || c == '\'' || c == '’' || c == '-')
                && !NOT_NAME_WORDS.contains(&w.as_str())
        })
}

/// `name` is printed as a name in the element's text: emphasised (`<em>`, `<i>`), or with every
/// word but a small one capitalised (as the list printed it).
fn printed_as_name(name: &str, html: &str) -> bool {
    let low = html.to_lowercase();
    let n = name.to_lowercase();
    if ["</em>", "</i>"]
        .iter()
        .any(|c| low.contains(&format!(">{n}{c}")))
    {
        return true;
    }
    name.split_whitespace().enumerate().all(|(i, w)| {
        w.chars().next().is_some_and(char::is_uppercase)
            || (i > 0 && SMALL_WORDS.contains(&w.to_lowercase().as_str()))
    })
}

/// The pieces of a printed list ("a, b, and c"): split on commas, semicolons and joins.
fn list_pieces(list: &str, known: &Known) -> Vec<String> {
    let mut out = Vec::new();
    for piece in list.split([',', ';']).map(strip_join) {
        let piece = piece.strip_prefix("the ").unwrap_or(piece).trim();
        if piece.is_empty() {
            continue;
        }
        if known.find(&printed_name(piece)).is_some() {
            out.push(piece.to_string());
            continue;
        }
        out.extend(
            piece
                .split(" and ")
                .flat_map(|x| x.split(" or "))
                .flat_map(|x| x.split(" & "))
                .map(|x| x.trim())
                .map(|x| x.strip_prefix("the ").unwrap_or(x).to_string())
                .filter(|x| !x.is_empty()),
        );
    }
    out
}

/// A cantrip `doc`'s text gives: its printed name, and whether that name is certain to be a
/// spell (known, or printed as a name, or listed next to a known spell).
pub(super) struct Named {
    pub(super) name: String,
    pub(super) certain: bool,
}

/// The cantrips `doc`'s own text says it gives ("you learn the a and b cantrips").
pub(super) fn named_in(doc: &Value, known: &Known) -> Vec<Named> {
    let html = str_at(doc, "description");
    let text = super::super::super::fidelity::plain(html);
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Named> = Vec::new();
    for list in named_cantrips(&lines) {
        let names: Vec<String> = list_pieces(&list, known)
            .iter()
            .map(|p| printed_name(p))
            .filter(|n| name_like(n))
            .collect();
        let any_known = names.iter().any(|n| known.find(n).is_some());
        for name in names {
            if out.iter().any(|o| key(&o.name) == key(&name)) {
                continue;
            }
            let certain = known.find(&name).is_some() || any_known || printed_as_name(&name, html);
            out.push(Named { name, certain });
        }
    }
    out
}

/// The import's element ids `doc` grants (its traits, features, sub-features).
fn granted_ids<'a>(doc: &'a Value, own: &HashSet<String>) -> Vec<&'a str> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| str_at(r, "kind") == "grant" && str_at(r, "type") != "Spell")
        .map(|r| str_at(r, "id"))
        .filter(|id| own.contains(*id))
        .collect()
}

/// The spell rules of `doc` (grants and alternative casts).
fn spell_rules(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| target_field(r).is_some())
}

/// `doc` grants spell `name` (by any of its spell rules).
fn grants(doc: &Value, name: &str, known: &Known, made: &Made) -> bool {
    let k = key(name);
    spell_rules(doc).any(|r| {
        let id = target_field(r).map_or("", |f| str_at(r, f));
        known.key_of(id) == k || made.by_key.get(&k).is_some_and(|p| p.id == id)
    })
}

/// Indices of the elements reached from `i` through grants (`down`), or granting it (up).
fn family(docs: &[Value], i: usize, own: &HashSet<String>, down: bool) -> Vec<usize> {
    let mut seen: Vec<usize> = vec![i];
    let mut frontier = vec![i];
    for _ in 0..MAX_FAMILY_DEPTH {
        let mut next = Vec::new();
        for &f in &frontier {
            let found: Vec<usize> = if down {
                let ids = granted_ids(&docs[f], own);
                (0..docs.len())
                    .filter(|&j| ids.contains(&str_at(&docs[j], "id")))
                    .collect()
            } else {
                let id = str_at(&docs[f], "id");
                (0..docs.len())
                    .filter(|&j| granted_ids(&docs[j], own).contains(&id))
                    .collect()
            };
            next.extend(found.into_iter().filter(|j| !seen.contains(j)));
        }
        next.sort_unstable();
        next.dedup();
        if next.is_empty() {
            break;
        }
        seen.extend(&next);
        frontier = next;
    }
    seen.remove(0);
    seen
}

/// The rule granting cantrip `id`, in the shape of a spell grant its family already has (same
/// kind, list, ability and spellcasting), else as a known cantrip (on the sheet in both
/// editions, whatever the element is).
fn cantrip_rule(id: &str, template: Option<&Value>) -> Value {
    match template {
        Some(t) if str_at(t, "kind") == "grant" => {
            let mut r = json!({"kind": "grant", "id": id, "type": "Spell"});
            if let Some(v) = t.get("spellcasting") {
                r["spellcasting"] = v.clone();
            }
            r
        }
        _ => {
            let mut r = json!({"kind": "grant_spell", "spell": id, "mode": "known_cantrip"});
            for f in ["spell_list", "ability"] {
                if let Some(v) = template.and_then(|t| t.get(f)) {
                    r[f] = v.clone();
                }
            }
            r
        }
    }
}

fn not_granted_note(name: &str) -> String {
    format!(
        "{UNRESOLVED_OFFICIAL}\"{}\" is named as a cantrip its text gives, but {NOT_GRANTED} and the name is not certain to be a spell: not granted, check it",
        title(name)
    )
}

/// Every cantrip an element's own text gives that neither it nor its family (the elements it
/// grants or that grant it) grants: granted when its name is certain, noted otherwise. Returns
/// the notes.
pub(super) fn text_grants(
    plan: &ImportPlan,
    known: &Known,
    made: &mut Made,
    docs: &mut [Value],
) -> Vec<(String, String)> {
    let text_only: HashSet<&str> = plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Text)
        .map(|p| p.id.as_str())
        .collect();
    let mut notes = Vec::new();
    for i in 0..docs.len() {
        let doc = &docs[i];
        let id = str_at(doc, "id").to_string();
        if !GIVING_TYPES.contains(&str_at(doc, "type")) || text_only.contains(id.as_str()) {
            continue;
        }
        let named = named_in(doc, known);
        if named.is_empty() {
            continue;
        }
        let up = family(docs, i, &known.own, false);
        let down = family(docs, i, &known.own, true);
        let mut add: Vec<Value> = Vec::new();
        for n in named {
            let granted = |j: usize| grants(&docs[j], &n.name, known, made);
            let elsewhere = |j: usize| {
                granted(j)
                    || named_in(&docs[j], known)
                        .iter()
                        .any(|m| key(&m.name) == key(&n.name))
            };
            if granted(i) || up.iter().any(|&j| granted(j)) || down.iter().any(|&j| elsewhere(j)) {
                continue;
            }
            if !n.certain {
                notes.push((id.clone(), not_granted_note(&n.name)));
                continue;
            }
            let target = match known.find(&n.name) {
                Some((_, Some(level))) if *level != 0 => continue,
                Some((found, _)) => {
                    notes.push((
                        id.clone(),
                        format!(
                            "rules: its text gives the \"{}\" cantrip and no rule granted it: granted ({found})",
                            title(&n.name)
                        ),
                    ));
                    found.clone()
                }
                None => {
                    let pid = made.get(&n.name, 0, &docs[i], known);
                    notes.push((id.clone(), placeholder_note(&n.name, &pid)));
                    pid
                }
            };
            let template = std::iter::once(i)
                .chain(up.iter().copied())
                .chain(down.iter().copied())
                .find_map(|j| spell_rules(&docs[j]).next());
            add.push(cantrip_rule(&target, template));
        }
        if let Some(rules) = rules_mut(&mut docs[i]) {
            rules.extend(add);
        }
    }
    notes
}
