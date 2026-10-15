//! What the detection found, made plan input. Nothing the detection lists stops an import: a
//! source with no element kinds keeps its rules and lore as text entries (or an empty pack and a
//! report). Named options printed as the rows of one table are the options of the rule that
//! prints the table, not loose text. Narrative (an adventure's keyed areas, read-aloud text, NPC
//! and location descriptions) is reported, never imported as rules. An item the source only
//! mentions (treasure, a reference) that the library already has is referenced, not re-created.

use borealis_content::{ContentStore, Element};

use super::fewshot::in_edition;
use std::collections::HashSet;

use super::types::{
    DetectedItem, DraftElement, Edition, ImportDraft, ImportKind, OtherItem, OutlineFeature,
    OutlinePart, PlanGroup, PlannedElement, Role,
};
use crate::import::report::ReportEntry;

/// Plan note prefix of an item the source only mentions that the library already has:
/// `"<prefix><name> -> <library id>"` (kept across plan rebuilds, read by the report).
pub(crate) const LIBRARY_NOTE: &str = "library reference: ";
/// Fewest named rows that make a table of options element material.
const MIN_OPTION_ROWS: usize = 3;
/// Element types of the library an item mention can stand for.
const ITEM_TYPES: [&str; 4] = ["Magic Item", "Item", "Weapon", "Armor"];
/// Words of a detection's `what` that mark a section as narrative, not rules.
const NARRATIVE_WORDS: [&str; 11] = [
    "narrative",
    "adventure",
    "encounter",
    "location",
    "npc",
    "non-player",
    "room",
    "keyed",
    "read-aloud",
    "boxed",
    "scene",
];

fn norm(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(what: &str) -> Vec<String> {
    what.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The detection's `what` says the section is narrative (see [`NARRATIVE_WORDS`]).
pub(crate) fn is_narrative(what: &str) -> bool {
    words(what)
        .iter()
        .any(|w| NARRATIVE_WORDS.contains(&w.as_str()))
}

/// The detection's `what` says the entry is a selectable option ("option", "table row
/// (option)", "option of <rule>").
fn is_option(what: &str) -> bool {
    words(what).iter().any(|w| w == "option" || w == "options")
}

/// The rule an option's `what` names ("option of Combat Tricks").
fn named_owner(what: &str) -> Option<String> {
    let lower = what.to_lowercase();
    let at = lower.find("option of ")? + "option of ".len();
    let owner = what[at..]
        .trim()
        .trim_matches(['"', '\'', '(', ')', '.', ' ']);
    (!owner.is_empty()).then(|| owner.to_string())
}

/// The first cell of a table row (`a | b`, `| a | b |`), markers and emphasis left out.
fn first_cell(line: &str) -> Option<&str> {
    let t = line.trim();
    if !t.contains('|') {
        return None;
    }
    t.trim_start_matches('|')
        .split('|')
        .next()
        .map(|c| c.trim().trim_matches(['*', '_', '†', ' ']))
}

/// A bullet line ("- a", "• a", "* a"), never a definition.
fn is_bullet(t: &str) -> bool {
    t.starts_with("- ") || t.starts_with("• ") || t.starts_with("* ") || t.starts_with("+ ")
}

/// `line` prints `key` (normalised) as a definition: a heading or bare title line naming it
/// (with a parenthesised note after it at most), a run-in label ("Name. Text", "Name: text"),
/// or the first cell of a table row.
fn defining_line(line: &str, key: &str) -> bool {
    if let Some(cell) = first_cell(line) {
        // A count or note after the name ("Javelin (2)") is no part of it.
        return norm(cell) == key || norm(cell.split(" (").next().unwrap_or(cell)) == key;
    }
    let t = line.trim();
    if t.is_empty() || is_bullet(t) {
        return false;
    }
    let bare = t.trim_start_matches('#').trim().trim_matches(['*', '_']);
    let title = bare.split(" (").next().unwrap_or(bare);
    if norm(title) == key {
        return true;
    }
    bare.split_once(['.', ':'])
        .is_some_and(|(head, rest)| norm(head) == key && !rest.trim().is_empty())
}

/// The source prints `name` as a definition of its own somewhere (see [`defining_line`]).
pub(crate) fn defines(source: &str, name: &str) -> bool {
    let key = norm(name);
    !key.is_empty() && source.lines().any(|l| defining_line(l, &key))
}

/// The library item of `edition` named `name`.
fn library_item<'a>(store: &'a ContentStore, edition: Edition, name: &str) -> Option<&'a Element> {
    let key = norm(name);
    store
        .iter()
        .map(|s| &s.element)
        .filter(|e| ITEM_TYPES.contains(&e.ty.as_str()) && in_edition(e, edition))
        .find(|e| norm(&e.name) == key)
}

/// Detected items the source only mentions (it prints no definition of its own for them)
/// that the library already has: `(name, library id)` each. They are referenced, not planned.
pub(crate) fn library_references(
    store: &ContentStore,
    edition: Edition,
    items: &[DetectedItem],
    source: &str,
) -> Vec<(String, String)> {
    items
        .iter()
        .filter(|i| i.kind == ImportKind::Item && !defines(source, &i.name))
        .filter_map(|i| {
            Some((
                i.name.clone(),
                library_item(store, edition, &i.name)?.id.clone(),
            ))
        })
        .collect()
}

/// The plan note of a library reference.
pub(crate) fn library_note(name: &str, id: &str) -> String {
    format!("{LIBRARY_NOTE}{name} -> {id}")
}

/// `(name, library id)` of a plan note written by [`library_note`].
pub(crate) fn parse_library_note(note: &str) -> Option<(&str, &str)> {
    note.strip_prefix(LIBRARY_NOTE)?.rsplit_once(" -> ")
}

/// Named options printed as rows of one table, and the rule that offers them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OptionTable {
    /// The rule (an `other` entry, or the heading the table is printed under).
    pub owner: String,
    /// The options, as the detection named them, in table order.
    pub rows: Vec<String>,
}

/// 0-based `[start, end)` line ranges of the source's tables (runs of `|` lines).
fn tables(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < lines.len() {
        if first_cell(lines[k]).is_none() {
            k += 1;
            continue;
        }
        let start = k;
        while k < lines.len() && first_cell(lines[k]).is_some() {
            k += 1;
        }
        out.push((start, k));
    }
    out
}

/// The title of the nearest heading above line `at`.
fn heading_above(lines: &[&str], at: usize) -> Option<String> {
    lines[..at].iter().rev().find_map(|l| {
        let t = l.trim_start();
        let n = t.chars().take_while(|c| *c == '#').count();
        let title = t[n..].trim();
        (n > 0 && t[n..].starts_with(' ') && !title.is_empty()).then(|| title.to_string())
    })
}

/// The `other` entries the detection calls options that are rows of one printed table, at
/// least [`MIN_OPTION_ROWS`] of them, with the rule that offers them: the entry their `what`
/// names ("option of <rule>"), else the heading the table is printed under. A table with no
/// owner keeps its rows as they are.
pub(crate) fn option_tables(other: &[OtherItem], source: &str) -> Vec<OptionTable> {
    let lines: Vec<&str> = source.lines().collect();
    let blocks = tables(&lines);
    let mut found: Vec<(usize, Vec<&OtherItem>)> = Vec::new();
    for o in other.iter().filter(|o| is_option(&o.what)) {
        let key = norm(&o.name);
        let Some(b) = blocks.iter().position(|(s, e)| {
            lines[*s..*e]
                .iter()
                .any(|l| first_cell(l).is_some_and(|c| norm(c) == key))
        }) else {
            continue;
        };
        match found.iter_mut().find(|(x, _)| *x == b) {
            Some((_, rows)) => rows.push(o),
            None => found.push((b, vec![o])),
        }
    }
    found
        .into_iter()
        .filter(|(_, rows)| rows.len() >= MIN_OPTION_ROWS)
        .filter_map(|(b, rows)| {
            let named = rows.iter().find_map(|o| named_owner(&o.what)).map(|n| {
                other
                    .iter()
                    .find(|o| norm(&o.name) == norm(&n))
                    .map_or(n, |o| o.name.clone())
            });
            let owner = named.or_else(|| heading_above(&lines, blocks[b].0))?;
            let rows = rows.iter().map(|o| o.name.clone()).collect();
            Some(OptionTable { owner, rows })
        })
        .collect()
}

/// Names a non-text group stands for: its own and those its outline plans (features, parts,
/// options).
fn outlined(groups: &[PlanGroup]) -> Vec<String> {
    let own = groups
        .iter()
        .filter(|g| !matches!(g.kind, ImportKind::Rules | ImportKind::Condition));
    own.clone()
        .map(|g| &g.name)
        .chain(
            own.filter_map(|g| g.outline.as_ref())
                .flat_map(|o| &o.features)
                .flat_map(|f| {
                    std::iter::once(&f.name)
                        .chain(f.parts.iter().chain(&f.options).map(|p| &p.name))
                }),
        )
        .map(|n| norm(n))
        .collect()
}

/// Make each option table's rows the options of its owner's text group (created when the
/// detection listed no such entry): the rows' own text groups go, the owner keeps its text
/// (the rule paragraph and the table) and offers them. Rows an outline already plans, and
/// tables whose owner is planned as another kind, are left as they are. Returns a plan note
/// per table.
pub(crate) fn attach_option_tables(
    groups: &mut Vec<PlanGroup>,
    tables: &[OptionTable],
) -> Vec<String> {
    let mut notes = Vec::new();
    for t in tables {
        let planned = outlined(groups);
        let rows: Vec<&String> = t
            .rows
            .iter()
            .filter(|r| !planned.contains(&norm(r)) && norm(r) != norm(&t.owner))
            .collect();
        let owner_at = groups.iter().position(|g| norm(&g.name) == norm(&t.owner));
        let text_owner = owner_at.map_or(true, |k| groups[k].kind == ImportKind::Rules);
        if rows.len() < MIN_OPTION_ROWS || !text_owner {
            continue;
        }
        let keys: Vec<String> = rows.iter().map(|r| norm(r)).collect();
        groups.retain(|g| !(matches!(g.kind, ImportKind::Rules) && keys.contains(&norm(&g.name))));
        let at = match groups.iter().position(|g| norm(&g.name) == norm(&t.owner)) {
            Some(k) => k,
            None => {
                groups.push(super::plan::text_group(&t.owner, "rules option", ""));
                groups.len() - 1
            }
        };
        let feature = OutlineFeature {
            level: 1,
            name: groups[at].name.clone(),
            summary: String::new(),
            quote: String::new(),
            parts: Vec::new(),
            options: rows.iter().map(|r| part(r)).collect(),
            choose: Vec::new(),
            spells: Vec::new(),
        };
        if let Some(o) = groups[at].outline.as_mut() {
            o.features = vec![feature];
        }
        notes.push(format!(
            "{}: its table of {} options is imported as options it offers",
            groups[at].name,
            rows.len()
        ));
    }
    notes
}

fn part(name: &str) -> OutlinePart {
    OutlinePart {
        name: name.to_string(),
        summary: String::new(),
        quote: String::new(),
        level: None,
    }
}

/// The report entry of a detected `other` entry with no element: narrative is skipped as
/// such (it is not rules), anything else has no element kind.
pub(crate) fn not_imported(o: &OtherItem) -> ReportEntry {
    let (outcome, note) = if super::intake::is_narrative(&o.what) {
        (
            "skipped",
            format!(
                "narrative text ({}), not rules: not imported. {}",
                o.what, o.note
            ),
        )
    } else {
        let what = if o.what.is_empty() {
            "this"
        } else {
            o.what.as_str()
        };
        (
            "unsupported",
            format!("no element kind for {what}; not imported. {}", o.note),
        )
    };
    ReportEntry {
        source: o.name.clone(),
        outcome: outcome.into(),
        elements: Vec::new(),
        note: note.trim().to_string(),
    }
}

/// Report entries for what the intake decided: items the source only mentions that the library
/// has (referenced, not re-created), and rules entries whose options are elements no element
/// offers yet.
pub(crate) fn intake_entries(
    draft: &ImportDraft,
    left_out: &HashSet<String>,
    final_id: &dyn Fn(&str) -> String,
) -> Vec<ReportEntry> {
    let references = draft
        .plan
        .notes
        .iter()
        .filter_map(|n| super::intake::parse_library_note(n))
        .map(|(name, id)| ReportEntry {
            source: name.to_string(),
            outcome: "skipped".into(),
            elements: Vec::new(),
            note: format!(
                "already in the library as {id}; the source only mentions it, so it is referenced, not re-created"
            ),
        });
    let offered = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.role == Role::Text && !left_out.contains(&p.id))
        .filter_map(|p| Some((p, p.picks.as_ref()?)))
        .map(|(p, picks)| {
            let options: Vec<&PlannedElement> = draft
                .plan
                .elements
                .iter()
                .filter(|c| c.parent.as_deref() == Some(p.id.as_str()) && !left_out.contains(&c.id))
                .collect();
            ReportEntry {
                source: p.name.clone(),
                outcome: "text_only".into(),
                elements: std::iter::once(p)
                    .chain(options.iter().copied())
                    .map(|e| final_id(&e.id))
                    .collect(),
                note: format!(
                    "a rule offering {} options, kept as text; the options are {} elements with supports \"{}\", offered by no element yet (add a select of that supports to the class, feat or species that gets the rule)",
                    options.len(),
                    picks.ty,
                    picks.key
                ),
            }
        });
    references.chain(offered).collect()
}

/// A source warning when the pack has no element beyond rules and lore text (or none at all).
pub(crate) fn nothing_mechanical(
    draft: &ImportDraft,
    left_out: &HashSet<String>,
) -> Option<String> {
    let kept: Vec<&DraftElement> = draft
        .elements
        .iter()
        .filter(|e| !left_out.contains(&e.id) && !e.json.is_null())
        .collect();
    if kept.is_empty() {
        return Some(
            "nothing mechanical was found: the source defines no class, species, background, feat, spell, item or monster, and no rules text with a heading of its own, so the pack is empty".into(),
        );
    }
    kept.iter().all(|e| e.role == Role::Text).then(|| {
        format!(
            "nothing mechanical was found: the source defines no class, species, background, feat, spell, item or monster; its rules and lore are kept as {} text entries",
            kept.len()
        )
    })
}

#[cfg(test)]
#[path = "intake_tests.rs"]
mod tests;
