//! What the detection found, made plan input. Nothing the detection lists stops an import: a
//! source with no element kinds keeps its rules and lore as text entries (or an empty pack and a
//! report). Named options printed as the rows of one table are the options of the rule that
//! prints the table, not loose text. Narrative (an adventure's keyed areas, read-aloud text, NPC
//! and location descriptions) is reported, never imported as rules. An item the source only
//! mentions (treasure, a reference) that the library already has is referenced, not re-created.

use borealis_content::{ContentStore, Element};

use super::fewshot::in_edition;
use super::option_rows::{printed_count, says_pick, says_roll};
use std::collections::HashSet;

use super::types::{
    DetectedItem, DraftElement, Edition, ImportDraft, ImportKind, OtherItem, OutlineFeature,
    OutlinePart, PickCount, PlanGroup, PlannedElement, Role,
};
use crate::import::report::ReportEntry;

/// Plan note prefix of an item the source only mentions that the library already has:
/// `"<prefix><name> -> <library id>"` (kept across plan rebuilds, read by the report).
pub(crate) const LIBRARY_NOTE: &str = "library reference: ";
/// Fewest named rows that make a table of options element material.
const MIN_OPTION_ROWS: usize = 3;
/// Lines of a governing rule's own section read for its pick wording.
const LEAD_SPAN: usize = 15;
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

/// The source prints `name` as a passage of its own outside lines `from..to` (its table row):
/// a heading or emphasised title naming it, or a run-in label ("Name. Text") that opens a
/// paragraph. Another table row, a bullet or a bare line that only repeats the name (a list
/// of names) is no passage.
pub(crate) fn printed_apart(source: &str, name: &str, from: usize, to: usize) -> bool {
    let key = norm(name);
    if key.is_empty() {
        return false;
    }
    source.lines().enumerate().any(|(k, l)| {
        let t = l.trim();
        let titled = t.starts_with(['#', '*', '_']);
        let bare = norm(t.split(" (").next().unwrap_or(t)) == key;
        !(from..to).contains(&k)
            && first_cell(l).is_none()
            && (titled || !bare)
            && defining_line(l, &key)
    })
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
    /// How many the rule lets a character pick, as printed above the table.
    pub count: Option<u32>,
    /// A roll table (a die column, or "roll"/"determine" above it, and no pick count): never
    /// options; only the versions of an item it is printed with.
    pub rolled: bool,
    /// The title of the nearest heading above the table (its caption or its section).
    pub heading: Option<String>,
}

/// Line and title of the nearest heading above line `at`.
fn heading_above(lines: &[&str], at: usize) -> Option<(usize, String)> {
    (0..at.min(lines.len())).rev().find_map(|k| {
        let t = lines[k].trim_start();
        let n = t.chars().take_while(|c| *c == '#').count();
        let title = t[n..].trim();
        (n > 0 && t[n..].starts_with(' ') && !title.is_empty()).then(|| (k, title.to_string()))
    })
}

/// Line of a heading whose title is `name`.
fn heading_of(lines: &[&str], name: &str) -> Option<usize> {
    let key = norm(name);
    lines.iter().position(|l| {
        let t = l.trim_start();
        let n = t.chars().take_while(|c| *c == '#').count();
        n > 0 && t[n..].starts_with(' ') && norm(&t[n..]) == key
    })
}

/// The `other` entries that are the named rows of one printed table (a pipe table naming them
/// in any cell, or a table flattened into lines that start with the names), at least
/// [`MIN_OPTION_ROWS`] of them, with the rule that offers them: the entry their `what` names
/// ("option of <rule>"), else the heading the table is printed under. The rows are options
/// when the detection calls one of them an option, or the rule says a character picks
/// ("learn two of the following", "of your choice") above the table or in its own section
/// (a glossary entry printed elsewhere). A roll table is returned flagged (it can only be an
/// item's versions); a table with no owner keeps its rows as they are.
pub(crate) fn option_tables(other: &[OtherItem], source: &str) -> Vec<OptionTable> {
    let lines: Vec<&str> = source.lines().collect();
    let cands: Vec<&OtherItem> = other.iter().filter(|o| !is_narrative(&o.what)).collect();
    let names: Vec<String> = cands.iter().map(|o| o.name.clone()).collect();
    let rows = super::option_rows::find_rows(source, &names);
    super::option_rows::blocks(source, &rows)
        .into_iter()
        .filter(|b| b.len() >= MIN_OPTION_ROWS)
        .filter_map(|b| {
            let printed: Vec<&super::option_rows::Row> =
                b.iter().filter_map(|k| rows[*k].as_ref()).collect();
            let first = printed.first()?.line;
            let members: Vec<&OtherItem> = b.iter().map(|k| cands[*k]).collect();
            let heading = heading_above(&lines, first);
            // A rule the detection names but the source never prints (nor lists) is no owner.
            let named = members
                .iter()
                .find_map(|o| named_owner(&o.what))
                .and_then(|n| match other.iter().find(|o| norm(&o.name) == norm(&n)) {
                    Some(o) => Some(o.name.clone()),
                    None => defines(source, &n).then_some(n),
                });
            let lead = super::option_rows::lead_text(source, None, first);
            // The rules that may govern the table, each with its own section's opening.
            let rules: Vec<(String, String)> = named
                .iter()
                .cloned()
                .chain(governing_rules(other, &lines, &members, heading.as_ref()))
                .map(|r| {
                    let own = rule_section(&lines, source, &r, first, heading.as_ref());
                    (r, own)
                })
                .collect();
            let picker = rules.iter().find(|(_, own)| says_pick(own));
            let owner = named
                .clone()
                .or_else(|| picker.filter(|_| !says_pick(&lead)).map(|(r, _)| r.clone()))
                .or_else(|| heading.as_ref().map(|(_, t)| t.clone()))?;
            let count = printed_count(&lead).or_else(|| {
                rules
                    .iter()
                    .filter(|(r, _)| norm(r) == norm(&owner))
                    .chain(picker)
                    .find_map(|(_, own)| printed_count(own))
            });
            let picked = says_pick(&lead) || picker.is_some();
            let rolled = count.is_none()
                && (super::option_rows::rolled_rows(source, &printed) || says_roll(&lead));
            let options = members.iter().any(|o| is_option(&o.what)) || picked;
            if !(options || rolled) {
                return None;
            }
            Some(OptionTable {
                owner,
                rows: members.iter().map(|o| o.name.clone()).collect(),
                count,
                rolled,
                heading: heading.map(|(_, t)| t),
            })
        })
        .collect()
}

/// Most enclosing headings above a table read as its possible rule.
const MAX_ENCLOSING: usize = 2;

/// Listed entries the table's caption or its rows' `what` names ("Gambit Table" and rows that
/// are "a gambit" belong to "Gambits").
fn listed_rules(
    other: &[OtherItem],
    members: &[&OtherItem],
    heading: Option<&(usize, String)>,
) -> Vec<String> {
    let named: Vec<Vec<String>> = heading
        .map(|(_, t)| stems(t))
        .into_iter()
        .chain(members.iter().map(|o| stems(&o.what)))
        .collect();
    let rows: Vec<String> = members.iter().map(|o| norm(&o.name)).collect();
    other
        .iter()
        .filter(|o| !is_narrative(&o.what) && !rows.contains(&norm(&o.name)))
        .filter(|o| heading.is_none_or(|(_, t)| norm(t) != norm(&o.name)))
        .filter(|o| {
            let rule = stems(&o.name);
            named.iter().any(|n| holds(n, &rule))
        })
        .map(|o| o.name.clone())
        .collect()
}

/// Rules that may govern a table besides the one its rows name: the listed entries its caption
/// or its rows' `what` names (see [`listed_rules`]), then the sections enclosing the table's
/// own heading (up to [`MAX_ENCLOSING`] levels).
fn governing_rules(
    other: &[OtherItem],
    lines: &[&str],
    members: &[&OtherItem],
    heading: Option<&(usize, String)>,
) -> Vec<String> {
    let depth = |l: &str| l.trim_start().chars().take_while(|c| *c == '#').count();
    let mut out = listed_rules(other, members, heading);
    if let Some((k, _)) = heading {
        let mut d = depth(lines[*k]);
        for j in (0..*k).rev() {
            if out.len() >= MAX_ENCLOSING || d <= 1 {
                break;
            }
            let n = depth(lines[j]);
            if n > 0 && n < d && super::option_rows::is_heading(lines[j]) {
                d = n;
                out.push(lines[j].trim_start()[n..].trim().to_string());
            }
        }
    }
    out
}

/// The opening of the governing rule's own section when the source prints it under a heading
/// of its own above the table, apart from the table's section (a glossary entry that the
/// table's rows belong to); empty otherwise.
fn rule_section(
    lines: &[&str],
    source: &str,
    owner: &str,
    first: usize,
    heading: Option<&(usize, String)>,
) -> String {
    heading_of(lines, owner)
        .filter(|&h| h < first && heading.is_none_or(|(k, _)| *k != h))
        .map(|h| {
            // Its opening only: to its first sub-heading at most.
            let end = (h + 1..first.min(h + 1 + LEAD_SPAN))
                .find(|&k| super::option_rows::is_heading(lines[k]))
                .unwrap_or(first.min(h + 1 + LEAD_SPAN));
            super::option_rows::lead_text(source, Some(h), end)
        })
        .unwrap_or_default()
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

/// Tables of the same rule (one table split by a page break or a tier caption) as one.
fn merged(tables: &[OptionTable]) -> Vec<OptionTable> {
    let mut out: Vec<OptionTable> = Vec::new();
    for t in tables {
        match out.iter_mut().find(|o| norm(&o.owner) == norm(&t.owner)) {
            Some(o) => {
                let fresh: Vec<String> = t
                    .rows
                    .iter()
                    .filter(|r| !o.rows.iter().any(|x| norm(x) == norm(r)))
                    .cloned()
                    .collect();
                o.rows.extend(fresh);
                o.count = o.count.or(t.count);
                o.rolled = o.rolled && t.rolled;
            }
            None => out.push(t.clone()),
        }
    }
    out
}

/// Word stems of a name (lowercase, a plural "s" left out): "Rings of the Beast" and "ring of
/// the beast" are the same.
fn stems(s: &str) -> Vec<String> {
    norm(s)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
                w[..w.len() - 1].to_string()
            } else {
                w.to_string()
            }
        })
        .collect()
}

/// `outer` holds the words of `inner` in a row.
fn holds(outer: &[String], inner: &[String]) -> bool {
    !inner.is_empty() && outer.windows(inner.len()).any(|w| w == inner)
}

/// The planned item a table belongs to: the table's rule or caption names it ("Types of rings
/// of the beast"), or the table is printed in the item's own section.
fn owning_item(groups: &[PlanGroup], t: &OptionTable) -> Option<usize> {
    let names: Vec<Vec<String>> = std::iter::once(&t.owner)
        .chain(t.heading.as_ref())
        .map(|n| stems(n))
        .collect();
    groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.kind == ImportKind::Item)
        .find(|(_, g)| {
            let item = stems(&g.name);
            names.iter().any(|n| holds(n, &item))
        })
        .map(|(k, _)| k)
}

/// Drop the text groups of `rows` (they are planned as something else now).
fn drop_row_texts(groups: &mut Vec<PlanGroup>, rows: &[&String]) {
    let keys: Vec<String> = rows.iter().map(|r| norm(r)).collect();
    groups.retain(|g| !(g.kind == ImportKind::Rules && keys.contains(&norm(&g.name))));
}

/// The rows of a table printed with an item are the item's versions ("<Item>, <row>", the way
/// the bundled packs list them), never options; versions the plan already has are kept.
fn attach_versions(groups: &mut Vec<PlanGroup>, at: usize, t: &OptionTable) -> String {
    let item = groups[at].name.clone();
    let item_type = groups[at].item_type;
    let rows: Vec<&String> = t.rows.iter().collect();
    drop_row_texts(groups, &rows);
    let mut added = 0;
    for r in &rows {
        let name = format!("{item}, {}", r.trim());
        if groups.iter().any(|g| norm(&g.name) == norm(&name)) {
            continue;
        }
        let pos = groups
            .iter()
            .rposition(|g| {
                norm(&g.name) == norm(&item)
                    || norm(&g.name).starts_with(&norm(&format!("{item} ")))
            })
            .map_or(groups.len(), |k| k + 1);
        groups.insert(
            pos,
            super::plan::standalone(ImportKind::Item, &name, item_type),
        );
        added += 1;
    }
    format!(
        "{item}: its table of {} rows is imported as {added} versions of the item, not as options",
        rows.len()
    )
}

/// The outline feature of a class, subclass, species, background or feat named `owner`, as
/// (group, feature) indexes.
fn feature_named(groups: &[PlanGroup], owner: &str) -> Option<(usize, usize)> {
    groups.iter().enumerate().find_map(|(gi, g)| {
        if matches!(g.kind, ImportKind::Rules | ImportKind::Condition) {
            return None;
        }
        let o = g.outline.as_ref()?;
        o.features
            .iter()
            .position(|f| norm(&f.name) == norm(owner))
            .map(|fi| (gi, fi))
    })
}

/// A table of a planned feature is that feature's options: the rows it does not list yet are
/// added, typed like the feature's own options; the printed count is its pick when it has none.
fn attach_to_feature(
    groups: &mut Vec<PlanGroup>,
    (gi, fi): (usize, usize),
    t: &OptionTable,
) -> Option<String> {
    let rows: Vec<&String> = t.rows.iter().collect();
    let f = groups[gi].outline.as_ref()?.features.get(fi)?;
    let listed: Vec<String> = f
        .options
        .iter()
        .chain(&f.parts)
        .map(|p| norm(&p.name))
        .collect();
    let fresh: Vec<OutlinePart> = rows
        .iter()
        .filter(|r| !listed.contains(&norm(r)))
        .map(|r| part(r))
        .collect();
    let feature = f.name.clone();
    drop_row_texts(groups, &rows);
    let gi = groups.iter().position(|g| {
        g.outline
            .as_ref()
            .is_some_and(|o| o.features.iter().any(|x| x.name == feature))
    })?;
    let f = groups[gi]
        .outline
        .as_mut()?
        .features
        .iter_mut()
        .find(|x| x.name == feature)?;
    let added = fresh.len();
    f.options.extend(fresh);
    if f.choose.is_empty() {
        if let Some(count) = t.count {
            f.choose = vec![PickCount {
                level: f.level,
                count,
            }];
        }
    }
    Some(format!(
        "{feature}: its table of {} options is imported as options of the feature ({added} added to its outline)",
        rows.len()
    ))
}

/// A table of a rules entry is the options the entry offers (its text group is created when
/// the detection listed no such entry); the rows' own text groups go. Rows an outline already
/// plans are left as they are; so is a table whose rule is planned as another kind.
fn attach_to_rule(groups: &mut Vec<PlanGroup>, t: &OptionTable) -> Option<String> {
    let planned = outlined(groups);
    let rows: Vec<&String> = t
        .rows
        .iter()
        .filter(|r| !planned.contains(&norm(r)) && norm(r) != norm(&t.owner))
        .collect();
    let owner_at = groups.iter().position(|g| norm(&g.name) == norm(&t.owner));
    let text_owner = owner_at
        .is_none_or(|k| matches!(groups[k].kind, ImportKind::Rules | ImportKind::Condition));
    if rows.len() < MIN_OPTION_ROWS || !text_owner {
        return None;
    }
    drop_row_texts(groups, &rows);
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
        choose: t
            .count
            .map(|count| vec![PickCount { level: 1, count }])
            .unwrap_or_default(),
        spells: Vec::new(),
    };
    if let Some(o) = groups[at].outline.as_mut() {
        o.features = vec![feature];
    }
    Some(format!(
        "{}: its table of {} options is imported as options it offers",
        groups[at].name,
        rows.len()
    ))
}

/// Place each table found by [`option_tables`] (tables of one rule taken together): a table
/// printed with a planned item gives the item's versions; a roll table is nothing more; a
/// table of a planned feature gives that feature's options (typed like its other options); a
/// table of a rules entry gives the options the entry offers. Returns a plan note per table
/// placed.
pub(crate) fn attach_option_tables(
    groups: &mut Vec<PlanGroup>,
    tables: &[OptionTable],
) -> Vec<String> {
    let mut notes = Vec::new();
    for t in merged(tables) {
        if let Some(at) = owning_item(groups, &t) {
            notes.push(attach_versions(groups, at, &t));
            continue;
        }
        if t.rolled {
            continue;
        }
        let note = match feature_named(groups, &t.owner) {
            Some(at) => attach_to_feature(groups, at, &t),
            None => attach_to_rule(groups, &t),
        };
        notes.extend(note);
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

/// The report entries of a detected `other` entry with no element: a contents page is
/// navigation, an entry only a contents page names has no section, narrative is skipped as
/// such (it is not rules; the amounts a game needs that it prints, a magic bonus, a reward,
/// XP, get a line each), anything else has no element kind.
pub(crate) fn not_imported(o: &OtherItem, source: &str, contents_page: bool) -> Vec<ReportEntry> {
    let entry = |outcome: &str, note: String| ReportEntry {
        source: o.name.clone(),
        outcome: outcome.into(),
        elements: Vec::new(),
        note: note.trim().to_string(),
    };
    if contents_page {
        return vec![entry(
            "skipped",
            "a contents page (navigation), not rules: not imported".into(),
        )];
    }
    if super::contents::listed_only(source, &o.name) {
        return vec![entry(
            "skipped",
            "listed only on a contents page; the source prints no section of its own for it".into(),
        )];
    }
    if is_narrative(&o.what) {
        let mut out = vec![entry(
            "skipped",
            format!(
                "narrative text ({}), not rules: not imported. {}",
                o.what, o.note
            ),
        )];
        out.extend(
            super::report_cover::amounts(source, &o.name)
                .into_iter()
                .map(|a| {
                    entry(
                        "skipped",
                        format!("amount printed in this narrative section: \"{a}\""),
                    )
                }),
        );
        return out;
    }
    let what = if o.what.is_empty() {
        "this"
    } else {
        o.what.as_str()
    };
    vec![entry(
        "unsupported",
        format!("no element kind for {what}; not imported. {}", o.note),
    )]
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
                    "a rule offering {} options as a select of {} (its text kept as is); the options are {} elements with supports \"{}\", and no class, feat or species grants the rule yet (grant it, or add that select, where the rule applies)",
                    options.len(),
                    picks.choose.first().map_or(1, |c| c.count),
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
