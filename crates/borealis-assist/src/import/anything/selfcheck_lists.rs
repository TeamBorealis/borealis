//! The list members check: a line of an element's own passage that lists spells (a table row
//! or a comma list naming two or more) is granted whole. When the element grants some of the
//! spells of the line and not the others, the others were dropped. A cantrip the passage says
//! the element teaches ("you learn the X cantrip") is granted too.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{failed, norm, passed, quoted_list, span, Checker};
use super::{DraftElement, ElementCheck, ImportDraft, Role, LIST_MEMBER_MISSING};

/// Fewest spells a line names before it is a list.
const MIN_LISTED: usize = 2;
/// Words before "the <spell> cantrip" that say the element teaches it.
const TEACHES: [&str; 4] = ["learn the ", "know the ", "gain the ", "learn one "];

/// Every id an element carries.
fn ids(v: &Value, out: &mut HashSet<String>) {
    match v {
        // A choice's `supports` joins its ids with "|".
        Value::String(s) => out.extend(
            s.split(['|', ',', ' '])
                .map(str::trim)
                .filter(|t| t.starts_with("ID_"))
                .map(str::to_string),
        ),
        Value::Array(a) => a.iter().for_each(|x| ids(x, out)),
        Value::Object(m) => m.values().for_each(|x| ids(x, out)),
        _ => {}
    }
}

/// The spells the checker knows (the library's and the import's), normalised name -> ids.
fn known_spells(c: &Checker, draft: &ImportDraft) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let import = draft
        .elements
        .iter()
        .filter(|e| e.ty == "Spell")
        .map(|e| (norm(&e.name), e.id.clone()));
    for (n, id) in c.spells.iter().cloned().chain(import) {
        out.entry(n).or_default().push(id);
    }
    out
}

/// `name` is granted: one of its ids is carried, or a carried spell id spells the name (a
/// library other than the one checked against).
fn granted(name: &str, spell_ids: &[String], carried: &HashSet<String>) -> bool {
    let tail = format!("_SPELL_{}", name.to_uppercase().replace(' ', "_"));
    spell_ids.iter().any(|id| carried.contains(id)) || carried.iter().any(|id| id.ends_with(&tail))
}

/// The spells `line` names (a name inside a longer named one is not counted twice).
fn named_in<'a>(line: &str, spells: &'a HashMap<String, Vec<String>>) -> Vec<&'a str> {
    let text = format!(" {} ", norm(line));
    let found: Vec<&str> = spells
        .keys()
        .filter(|n| text.contains(&format!(" {n} ")))
        .map(String::as_str)
        .collect();
    found
        .iter()
        .filter(|n| {
            !found
                .iter()
                .any(|m| m.len() > n.len() && format!(" {m} ").contains(&format!(" {n} ")))
        })
        .copied()
        .collect()
}

/// The cantrips `line` says are taught ("you learn the X cantrip").
fn taught<'a>(line: &str, spells: &'a HashMap<String, Vec<String>>) -> Vec<&'a str> {
    let low = format!(" {} ", norm(line));
    named_in(line, spells)
        .into_iter()
        .filter(|n| {
            TEACHES
                .iter()
                .any(|t| low.contains(&format!(" {t}{n} cantrip ")))
        })
        .collect()
}

/// Most words of a list member named in the list (longer pieces are prose around the list).
const MAX_MEMBER_WORDS: usize = 4;
/// Words joining the last members of a list.
const LIST_JOINS: [&str; 2] = [" or ", " and "];

/// A piece of a list line that names a member: short, no digits, a capitalised or italic name.
fn member_name(piece: &str) -> Option<String> {
    let raw = piece.trim().trim_end_matches(['.', ':']);
    let marked = raw.starts_with(['*', '_']);
    let name = raw.trim_matches(|c: char| c == '*' || c == '_' || c.is_whitespace());
    let words: Vec<&str> = name.split_whitespace().collect();
    let plain_words = name
        .chars()
        .all(|c| c.is_alphabetic() || c.is_whitespace() || matches!(c, '\'' | '\u{2019}' | '-'));
    let capital = name.chars().next().is_some_and(char::is_uppercase);
    ((1..=MAX_MEMBER_WORDS).contains(&words.len()) && plain_words && (capital || marked))
        .then(|| name.to_string())
}

/// The parts of a line that are a list of names: each cell of a table row, else the text after
/// a colon ("Learn one of these cantrips: A, B or C"), up to the sentence's end; only parts
/// with two or more comma-separated pieces. Prose around spell names is no list of members.
fn list_regions(line: &str) -> Vec<&str> {
    let parts: Vec<&str> = if super::is_row(line) {
        line.split(" | ").collect()
    } else {
        line.split_once(':')
            .map(|(_, after)| after)
            .into_iter()
            .collect()
    };
    parts
        .into_iter()
        .map(|p| p.split_once(". ").map_or(p, |(head, _)| head))
        .map(|p| p.trim_end().trim_end_matches('.'))
        .filter(|p| p.matches(',').count() >= 1)
        .collect()
}

/// The members a list line names that are no spell the checker knows and that the element
/// does not grant (a spell outside the rules library, silently left out): only for a line that
/// is a list of spells (see [`list_checks`]).
fn unknown_members(
    line: &str,
    spells: &HashMap<String, Vec<String>>,
    carried: &HashSet<String>,
) -> Vec<String> {
    let joined = |n: &str| norm(n).replace(' ', "");
    let known: HashSet<String> = spells.keys().map(|k| k.replace(' ', "")).collect();
    let tails: Vec<String> = carried
        .iter()
        .filter_map(|id| {
            id.split_once("_SPELL_")
                .map(|(_, t)| t.replace('_', "").to_lowercase())
        })
        .collect();
    let mut out = Vec::new();
    for region in list_regions(line) {
        for piece in region.split([',', ';']) {
            let mut parts = vec![piece.to_string()];
            for j in LIST_JOINS {
                parts = parts
                    .iter()
                    .flat_map(|p| p.split(j).map(str::to_string).collect::<Vec<_>>())
                    .collect();
            }
            for name in parts.iter().filter_map(|p| member_name(p)) {
                let key = joined(&name);
                if !known.contains(&key) && !tails.contains(&key) && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// Lines of `e`'s own passage: its located lines without those of other non-text elements
/// that start inside it (a child's list is the child's).
fn own_lines<'a>(c: &'a Checker, draft: &ImportDraft, e: &DraftElement) -> Vec<&'a str> {
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
        .filter(|p| p.id != e.id && p.role != Role::Text)
        .filter_map(span)
        .filter(|(o, _)| *o > s && *o < end)
        .collect();
    (s..end.min(c.lines.len()))
        .filter(|i| !others.iter().any(|(o, oe)| i >= o && i < oe))
        .map(|i| c.lines[i].as_str())
        .collect()
}

/// The list members check of `e` (an element granting spells, never a class or subclass root,
/// whose lists are its features').
pub(super) fn list_checks(c: &Checker, draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if matches!(
        e.role,
        Role::Class | Role::Subclass | Role::SubclassGate | Role::Text
    ) {
        return Vec::new();
    }
    let spells = known_spells(c, draft);
    let mut carried = HashSet::new();
    ids(&e.json, &mut carried);
    // Its planned children and its parent may carry the grant.
    let kin = draft.plan.elements.iter().find(|p| p.id == e.id).map(|p| {
        draft
            .plan
            .elements
            .iter()
            .filter(|x| {
                x.parent.as_deref() == Some(e.id.as_str()) || p.parent.as_ref() == Some(&x.id)
            })
            .map(|x| x.id.clone())
            .collect::<Vec<_>>()
    });
    for k in draft
        .elements
        .iter()
        .filter(|x| kin.as_ref().is_some_and(|ks| ks.contains(&x.id)))
    {
        ids(&k.json, &mut carried);
    }
    let mut missing: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut listed = false;
    for line in own_lines(c, draft, e) {
        let names = named_in(line, &spells);
        let is_list = names.len() >= MIN_LISTED && (super::is_row(line) || line.contains(','));
        let (given, not): (Vec<&str>, Vec<&str>) = names
            .iter()
            .partition(|n| granted(n, &spells[**n], &carried));
        if is_list && !given.is_empty() {
            listed = true;
            for n in not {
                if !missing.iter().any(|m| m == n) {
                    missing.push(n.to_string());
                }
            }
            for n in unknown_members(line, &spells, &carried) {
                if !unknown.contains(&n) {
                    unknown.push(n);
                }
            }
        }
        for n in taught(line, &spells) {
            listed = true;
            if !granted(n, &spells[n], &carried) && !missing.iter().any(|m| m == n) {
                missing.push(n.to_string());
            }
        }
    }
    let mut out = Vec::new();
    if !unknown.is_empty() {
        out.push(failed(
            LIST_MEMBER_MISSING,
            format!(
                "Its source lists {} but it does not grant {}: neither your rules nor this import {} {}. Add that content or the grant",
                quoted_list(&unknown),
                if unknown.len() == 1 { "it" } else { "them" },
                if unknown.len() == 1 { "has" } else { "have" },
                if unknown.len() == 1 { "it" } else { "them" }
            ),
        ));
    }
    if missing.is_empty() {
        if !out.is_empty() {
            return out;
        }
        return if listed {
            vec![passed(
                LIST_MEMBER_MISSING,
                "Every spell its source lists is granted",
            )]
        } else {
            Vec::new()
        };
    }
    out.insert(
        0,
        failed(
            LIST_MEMBER_MISSING,
            format!(
                "Its source lists {} but it does not grant {}",
                quoted_list(&missing),
                if missing.len() == 1 { "it" } else { "them" }
            ),
        ),
    );
    out
}
