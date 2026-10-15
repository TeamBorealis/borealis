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
        }
        for n in taught(line, &spells) {
            listed = true;
            if !granted(n, &spells[n], &carried) && !missing.iter().any(|m| m == n) {
                missing.push(n.to_string());
            }
        }
    }
    if missing.is_empty() {
        return if listed {
            vec![passed(
                LIST_MEMBER_MISSING,
                "Every spell its source lists is granted",
            )]
        } else {
            Vec::new()
        };
    }
    vec![failed(
        LIST_MEMBER_MISSING,
        format!(
            "Its source lists {} but it does not grant {}",
            quoted_list(&missing),
            if missing.len() == 1 { "it" } else { "them" }
        ),
    )]
}
