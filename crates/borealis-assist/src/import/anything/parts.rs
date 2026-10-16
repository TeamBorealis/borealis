//! Named sub-items as their own elements, the way the bundled packs model them: species traits
//! (`Racial Trait`) and background features granted by their root, subraces offered by their
//! species' choice, sub-features granted by their feature, pick-N option families offered by a
//! select (gated by level, prerequisites as requirements) and rules or lore kept as text-only
//! elements. The plan decides the structure; this module writes it into the elements.

use serde_json::{json, Map, Value};

use super::types::{ImportDraft, ImportKind, ImportPlan, PlannedElement, Role};
use crate::import::report::ReportEntry;

#[path = "parts_html.rs"]
mod html;
#[path = "parts_rows.rs"]
mod rows;

/// Most source lines a text-only element keeps.
const MAX_SECTION_LINES: usize = 400;
/// `supports` of the bundled rules entries.
const RULES_SUPPORT: &str = "Rules Glossary";

fn obj(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().expect("just made an object")
}

fn rules_of(el: &mut Value) -> &mut Vec<Value> {
    let r = obj(el).entry("rules").or_insert_with(|| json!([]));
    if !r.is_array() {
        *r = json!([]);
    }
    r.as_array_mut().expect("just made an array")
}

fn children<'a>(plan: &'a ImportPlan, id: &str) -> Vec<&'a PlannedElement> {
    plan.elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(id))
        .collect()
}

fn find<'a>(plan: &'a ImportPlan, id: Option<&str>) -> Option<&'a PlannedElement> {
    id.and_then(|id| plan.elements.iter().find(|e| e.id == id))
}

/// Write the plan's structure into `el` (the element planned as `planned`): grants of its
/// parts, the selects of its options, an option's `supports` and prerequisite, a subrace's
/// `supports`, a species' subrace choice, a text-only element's shape.
pub(crate) fn apply(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    grant_parts(el, planned, plan);
    offer_options(el, planned, plan);
    offer_subraces(el, planned, plan);
    match planned.role {
        Role::Option => shape_option(el, planned, plan),
        Role::Part => {
            mark_owner(el, planned, plan);
            drop_built_caster_rules(el, planned, plan);
        }
        Role::Text => {
            let o = obj(el);
            o.insert("supports".into(), json!([text_support(&planned.ty)]));
            o.remove("rules");
        }
        _ => {}
    }
    if let Some(p) = subrace_link(plan, &planned.id) {
        obj(el).insert("supports".into(), json!([p]));
    }
    super::feature_spells::grant_spells(el, planned, plan.edition);
    super::feature_spells::table_columns(el, planned, plan);
    super::prereq::apply(el, planned, plan);
    super::units::apply(el);
}

/// A part of a casting class's spellcasting section (a feature at level 3 or lower that is
/// caster-named or has caster sub-headings) describes what the caster builder generates for
/// the class (cantrip and spell choices, slots, preparation): the model's own spellcasting
/// rules there would count them twice.
fn drop_built_caster_rules(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let Some(section) = find(plan, planned.parent.as_deref()) else {
        return;
    };
    let Some(class) = find(plan, section.owner.as_deref()).filter(|c| c.role == Role::Class) else {
        return;
    };
    let casts = plan.groups.iter().any(|g| {
        g.root_id.as_deref() == Some(class.id.as_str())
            && g.outline.as_ref().is_some_and(|o| o.caster().is_some())
    });
    let caster_section = section.level.is_some_and(|l| l <= 3)
        && (super::caster::is_caster_feature(&section.name)
            || children(plan, &section.id)
                .iter()
                .any(|c| c.role == Role::Part && super::caster::is_caster_part(&c.name)));
    if casts && caster_section && super::caster::is_caster_part(&planned.name) {
        rules_of(el).retain(|r| !super::caster::is_model_caster_rule(r));
    }
}

fn grant_parts(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let parts: Vec<&PlannedElement> = children(plan, &planned.id)
        .into_iter()
        .filter(|c| c.role == Role::Part)
        .collect();
    if parts.is_empty() {
        return;
    }
    let ids: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    let rules = rules_of(el);
    rules.retain(|r| {
        !(r.get("kind").and_then(Value::as_str) == Some("grant")
            && r.get("id")
                .and_then(Value::as_str)
                .is_some_and(|i| ids.contains(&i)))
    });
    rules.extend(
        parts
            .iter()
            .map(|p| json!({"kind": "grant", "id": p.id, "type": p.ty})),
    );
}

/// `name` without a trailing " (Level N)", lowercased and trimmed.
fn pick_name(name: &str) -> String {
    let n = name.trim();
    let base = n
        .strip_suffix(')')
        .and_then(|r| r.rsplit_once(" (Level "))
        .filter(|(_, lvl)| !lvl.is_empty() && lvl.chars().all(|c| c.is_ascii_digit()))
        .map_or(n, |(b, _)| b);
    base.trim().to_lowercase()
}

/// Whether the model's rule `r` is its own version of the planned pick: a select of the
/// pick's type named like it (or a "(Level N)" variant), or a select or grant of one of the
/// planned options (`option_ids`, or the ids the pick key itself lists).
fn replaced_pick(
    r: &Value,
    picks: &super::types::OptionPicks,
    name: &str,
    option_ids: &[&str],
) -> bool {
    let kind = r.get("kind").and_then(Value::as_str);
    let s = r.get("supports").and_then(Value::as_str).unwrap_or("");
    let id = r.get("id").and_then(Value::as_str).unwrap_or("");
    let names_an_id = option_ids
        .iter()
        .any(|i| *i == id || s.split('|').any(|p| p.trim() == *i));
    let is_select = kind == Some("select");
    let same_key = is_select && s == picks.key;
    let same_name = is_select
        && r.get("type").and_then(Value::as_str) == Some(picks.ty.as_str())
        && r.get("name").and_then(Value::as_str).map(pick_name) == Some(pick_name(name));
    let model_pick = matches!(kind, Some("select" | "grant")) && names_an_id;
    same_key || same_name || model_pick
}

fn offer_options(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let Some(picks) = &planned.picks else { return };
    let own = planned.level.unwrap_or(1);
    // A choice among existing elements, or among the planned options: the model's own
    // select (or grants) of them go, so the pick is offered once.
    let mut ids: Vec<&str> = if picks.key.starts_with("ID_") {
        picks.key.split('|').collect()
    } else {
        Vec::new()
    };
    ids.extend(
        children(plan, &planned.id)
            .into_iter()
            .filter(|c| c.role == Role::Option)
            .map(|c| c.id.as_str()),
    );
    let rules = rules_of(el);
    rules.retain(|r| !replaced_pick(r, picks, &planned.name, &ids));
    let several = picks.choose.len() > 1;
    for c in &picks.choose {
        let name = if several {
            format!("{} (Level {})", planned.name, c.level)
        } else {
            planned.name.clone()
        };
        let mut s =
            json!({"kind": "select", "type": picks.ty, "name": name, "supports": picks.key});
        if c.count > 1 {
            s["number"] = json!(c.count);
        }
        if c.level > own || (several && c.level > 1) {
            s["level"] = json!(c.level);
        }
        if let Some(sc) = &picks.spellcasting {
            s["spellcasting"] = json!(sc);
        }
        rules.push(s);
    }
}

/// The `supports` key a subrace planned as `id` carries (its parent species' choice).
fn subrace_link(plan: &ImportPlan, id: &str) -> Option<String> {
    plan.groups
        .iter()
        .find(|g| g.kind == ImportKind::Subrace && g.root_id.as_deref() == Some(id))
        .and_then(|g| g.parent.as_ref())
        .filter(|p| !p.class_id.is_empty())
        .map(|p| p.supports.clone())
}

/// A species of the source offers its own subraces: one select of the edition's subrace type.
fn offer_subraces(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    if planned.role != Role::Standalone {
        return;
    }
    let key = plan
        .groups
        .iter()
        .filter(|g| g.kind == ImportKind::Subrace)
        .filter_map(|g| g.parent.as_ref())
        .find(|p| p.in_source && p.class_id == planned.id)
        .map(|p| p.supports.clone());
    let Some(key) = key else { return };
    let ty = super::plan::subrace_type(plan.edition);
    let rules = rules_of(el);
    rules.retain(|r| {
        !(r.get("kind").and_then(Value::as_str) == Some("select")
            && r.get("type").and_then(Value::as_str) == Some(ty))
    });
    // The bundled 2014 races tag their subraces with the race's own name; the choice is
    // still called "<Race> Subrace".
    let name = if key == planned.name {
        format!("{key} Subrace")
    } else {
        key.clone()
    };
    rules.push(json!({"kind": "select", "type": ty, "name": name, "supports": key}));
}

fn shape_option(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let key = find(plan, planned.parent.as_deref())
        .and_then(|p| p.picks.as_ref())
        .map(|p| p.key.clone());
    let o = obj(el);
    if let Some(k) = key {
        o.insert("supports".into(), json!([k]));
    }
    match planned.prerequisite_level {
        Some(l) => {
            o.insert("requirements".into(), json!(format!("[level:{l}]")));
        }
        None => {
            o.remove("requirements");
        }
    }
    mark_owner(el, planned, plan);
}

/// Parts and options of class and subclass features name their class (and subclass) like the
/// features themselves.
fn mark_owner(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let mut up = find(plan, planned.parent.as_deref());
    while let Some(p) = up.filter(|p| p.owner.is_none()) {
        up = find(plan, p.parent.as_deref());
    }
    let Some(feature) = up else { return };
    let Some(root) = find(plan, feature.owner.as_deref()) else {
        return;
    };
    let mut data = json!({"level": feature.level});
    match root.role {
        Role::Class => data["class"] = json!(root.name),
        Role::Subclass => {
            let class = plan
                .groups
                .iter()
                .find(|g| g.root_id.as_deref() == Some(root.id.as_str()))
                .and_then(|g| g.parent.as_ref())
                .map(|p| p.class_name.clone());
            data["class"] = json!(class);
            data["subclass"] = json!(root.name);
        }
        _ => return,
    }
    let d = obj(el).entry("data").or_insert_with(|| json!({}));
    for (k, v) in data.as_object().cloned().unwrap_or_default() {
        obj(d).insert(k, v);
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn is_heading(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then_some(n)
}

fn heading_text(line: &str) -> Option<(usize, String)> {
    let n = is_heading(line)?;
    let words: String = line.trim_start()[n..]
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    Some((
        n,
        words
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    ))
}

/// Does `part` have a heading of its own inside the section headed `parent` (a deeper
/// heading before the next one at the parent's depth)? Such a sub-item is its own element,
/// never folded into its parent.
pub(crate) fn headed_within(source: &str, parent: &str, part: &str) -> bool {
    let key = |s: &str| heading_text(&format!("# {s}")).map(|(_, t)| t);
    let (Some(parent), Some(part)) = (key(parent), key(part)) else {
        return false;
    };
    let mut depth: Option<usize> = None;
    for line in source.lines() {
        let Some((n, text)) = heading_text(line) else {
            continue;
        };
        match depth {
            None if text == parent => depth = Some(n),
            Some(d) if n <= d => {
                depth = (text == parent).then_some(n);
            }
            Some(_) if text == part => return true,
            _ => {}
        }
    }
    false
}

/// The headings (as printed) one level below or deeper inside the section headed `parent`,
/// each with prose under it before the next heading (a caption over a bare table is not a
/// sub-item). An outline that skipped them still gets them as their own elements.
pub(crate) fn sub_headings(source: &str, parent: &str) -> Vec<String> {
    let Some(want) = heading_text(&format!("# {parent}")).map(|(_, t)| t) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let mut depth: Option<usize> = None;
    let mut open: Option<(String, bool)> = None;
    let close = |open: &mut Option<(String, bool)>, out: &mut Vec<String>| {
        if let Some((name, true)) = open.take() {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    };
    for line in source.lines() {
        let Some((n, text)) = heading_text(line) else {
            if let Some((_, prose)) = open.as_mut() {
                let t = line.trim();
                *prose |= !t.is_empty() && !t.starts_with('|');
            }
            continue;
        };
        close(&mut open, &mut out);
        match depth {
            None if text == want => depth = Some(n),
            Some(d) if n <= d => depth = (text == want).then_some(n),
            Some(_) => {
                let raw = line.trim_start()[n..].trim().trim_end_matches('#').trim();
                open = Some((raw.to_string(), false));
            }
            None => {}
        }
    }
    close(&mut open, &mut out);
    out
}

/// Most lines a text-only section reaches past its own heading's section over unplanned
/// headings of its level.
const MAX_EXTENSION_LINES: usize = 150;
/// Heading words that open another part of the document, never a sub-section of the text
/// before it.
const BOUNDARY_WORDS: [&str; 8] = [
    "appendix",
    "license",
    "licence",
    "credits",
    "contents",
    "index",
    "chapter",
    "introduction",
];

/// Is `title` (a heading's words) the name of a planned element other than `except`, alone or
/// with a level after it ("Second Wind 2nd")?
pub(crate) fn names_planned(plan: &ImportPlan, except: &str, title: &str) -> bool {
    let t = heading_text(&format!("# {title}"))
        .map(|(_, t)| t)
        .unwrap_or_default();
    plan.elements.iter().filter(|e| e.id != except).any(|e| {
        let n = heading_text(&format!("# {}", e.name))
            .map(|(_, t)| t)
            .unwrap_or_default();
        let level = t
            .strip_prefix(&format!("{n} "))
            .and_then(|rest| rest.split(' ').next())
            .is_some_and(|w| w.starts_with(|c: char| c.is_ascii_digit()));
        !n.is_empty() && (t == n || level)
    })
}

/// Is `title` (a heading's words) a section the detection called narrative (see
/// `plan::narrative_notes`)? Narrative is never imported, so it is no text section's either.
fn names_narrative(plan: &ImportPlan, title: &str) -> bool {
    super::plan::narrative_sections(plan)
        .any(|n| heading_text(&format!("# {n}")).is_some_and(|(_, n)| !n.is_empty() && n == title))
}

/// A heading of the same level that only continues a text-only section the PDF flattened (a
/// question of a FAQ, a sub-rule): unplanned, no boundary word, no end matter or narrative,
/// within reach.
fn continues_section(
    lines: &[&str],
    plan: &ImportPlan,
    planned: &PlannedElement,
    own_end: Option<usize>,
    at: usize,
) -> bool {
    let Some((_, title)) = heading_text(lines[at]) else {
        return false;
    };
    planned.role == Role::Text
        && own_end.map_or(true, |e| at < e + MAX_EXTENSION_LINES)
        && !title.split(' ').any(|w| BOUNDARY_WORDS.contains(&w))
        && !super::passage::end_matter(&title)
        && !names_planned(plan, &planned.id, &title)
        && !names_narrative(plan, &title)
        && !root_sub_section(lines, plan, at)
        && !nested(lines, plan, planned)
}

/// `planned` starts inside the heading section of another planned text-only element of a
/// higher heading level: the unplanned sections after it are that section's, not its own.
fn nested(lines: &[&str], plan: &ImportPlan, planned: &PlannedElement) -> bool {
    let Some(at) = planned
        .excerpt
        .as_ref()
        .map(|x| x.start_line.saturating_sub(1))
    else {
        return false;
    };
    let Some(level) = lines.get(at).and_then(|l| is_heading(l)) else {
        return false;
    };
    plan.elements
        .iter()
        .filter(|e| e.id != planned.id && e.role == Role::Text)
        .filter_map(|e| e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
        .filter(|&s| s < at)
        .any(|s| {
            lines.get(s).and_then(|l| is_heading(l)).is_some_and(|h| {
                h < level && !(s + 1..at).any(|k| is_heading(lines[k]).is_some_and(|k| k <= h))
            })
        })
}

/// The heading at `at` opens an unplanned sub-section a class or subclass root carries in its
/// description (see [`super::passage::unplanned_sections`]).
fn root_sub_section(lines: &[&str], plan: &ImportPlan, at: usize) -> bool {
    plan.elements
        .iter()
        .filter(|e| matches!(e.role, Role::Class | Role::Subclass))
        .filter(|e| {
            e.excerpt
                .as_ref()
                .is_some_and(|x| x.start_line.saturating_sub(1) < at)
        })
        .any(|e| {
            super::passage::unplanned_sections_in(lines, e, plan)
                .iter()
                .any(|s| s.at == at)
        })
}

/// Where an element's section reaches (0-based): its first line; the end of its own heading's
/// section, up to the next planned passage or the next heading of the same or a higher level
/// (a text-only element also reaches over the unplanned headings of its own level that follow:
/// a section the PDF printed flat); the end of its located passage; and the next planned
/// passage's first line (a passage without a heading of its own: up to the first heading after
/// its located passage).
struct Reach {
    start: usize,
    own_end: usize,
    located_end: usize,
}

fn reach(lines: &[&str], planned: &PlannedElement, plan: &ImportPlan) -> Option<Reach> {
    let ex = planned.excerpt.as_ref()?;
    let start = ex.start_line.saturating_sub(1).min(lines.len());
    // Never into the document's end matter (a legal notice, thanks), its title marked as a
    // heading or not.
    let tail = (start + 1..lines.len())
        .find(|&k| super::passage::end_matter_title(lines, k))
        .unwrap_or(lines.len());
    let next = next_start(lines, plan, &planned.id, start)
        .unwrap_or(lines.len())
        .min(tail);
    let own = lines.get(start).and_then(|l| is_heading(l));
    let mut end = start + 1;
    let mut own_end: Option<usize> = None;
    let located_end = ex.end_line.min(lines.len()).min(tail.max(start + 1));
    while end < lines.len().min(next).min(start + MAX_SECTION_LINES) {
        // A passage without a heading of its own (a name line, a wrapped title) ends at the
        // first heading after its located passage: what follows is another section.
        if own.is_none() && end >= located_end && is_heading(lines[end]).is_some() {
            break;
        }
        if let (Some(o), Some(h)) = (own, is_heading(lines[end])) {
            let same = h == o;
            if h < o || (same && !continues_section(lines, plan, planned, own_end, end)) {
                break;
            }
            if same {
                own_end.get_or_insert(end);
            }
        }
        end += 1;
    }
    Some(Reach {
        start,
        own_end: end.min(lines.len()),
        located_end,
    })
}

/// The first line (0-based) of the next planned passage after `start`, `except` left out; a
/// row element's table row is no passage boundary (the table is printed whole).
fn next_start(lines: &[&str], plan: &ImportPlan, except: &str, start: usize) -> Option<usize> {
    plan.elements
        .iter()
        .filter(|e| e.id != except)
        .filter_map(|e| e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
        .filter(|l| *l > start && !lines.get(*l).is_some_and(|t| t.contains(" | ")))
        .min()
}

/// `end` moved back over trailing blank lines (never before `start + 1`).
fn trimmed(lines: &[&str], start: usize, mut end: usize) -> usize {
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    end
}

/// The end (exclusive, 0-based) of the lines a planned element claims as its own text, ONE
/// range used both for the element's own text and for leaving it out of the section it is
/// printed in: a text-only element its section to the end of its located passage (see
/// [`Reach`]); a headed generated one its heading's section (to the next heading of its level
/// or higher) or its located passage, whichever reaches further (it is generated from that
/// passage); any other its located passage up to the next planned passage.
fn claimed_end(lines: &[&str], e: &PlannedElement, plan: &ImportPlan) -> Option<usize> {
    let ex = e.excerpt.as_ref()?;
    let at = ex.start_line.saturating_sub(1).min(lines.len());
    let located = ex.end_line.min(lines.len());
    let end = match lines.get(at).and_then(|l| is_heading(l)) {
        _ if e.role == Role::Text => {
            let r = reach(lines, e, plan)?;
            trimmed(lines, r.start, r.own_end.max(r.located_end))
        }
        Some(h) => (at + 1..lines.len())
            .find(|&k| is_heading(lines[k]).is_some_and(|k| k <= h))
            .unwrap_or(lines.len())
            .max(located),
        None => {
            // Up to the next planned start of any kind (a row element's row included).
            let next = plan
                .elements
                .iter()
                .filter(|o| o.id != e.id)
                .filter_map(|o| o.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
                .filter(|l| *l > at)
                .min()
                .unwrap_or(lines.len());
            located.min(next)
        }
    };
    Some(end.max(at + 1))
}

/// The 0-based line ranges a text-only element's description is built from: its claimed lines
/// (see [`claimed_end`]) without the claimed lines of the other planned elements that start
/// inside it (they carry their text themselves; the unplanned sub-sections between them stay
/// the section's), and without the headings left with nothing under them. Table rows stay: a
/// row element does not take its row out of the table.
pub(crate) fn section_blocks(
    source: &str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(r) = reach(&lines, planned, plan) else {
        return Vec::new();
    };
    let end = claimed_end(&lines, planned, plan).unwrap_or(r.start + 1);
    let mut inner: Vec<(usize, usize)> = plan
        .elements
        .iter()
        .filter(|e| e.id != planned.id)
        .flat_map(|e| {
            let Some(at) = e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)) else {
                return Vec::new();
            };
            // A row element keeps its row in the table too (a table is printed whole).
            let row = lines.get(at).is_some_and(|l| l.contains(" | "));
            if at <= r.start || at >= end || row {
                return Vec::new();
            }
            let mut own = vec![(at, claimed_end(&lines, e, plan).unwrap_or(at + 1).min(end))];
            // A class or subclass root's description also carries its unplanned sub-sections.
            if matches!(e.role, Role::Class | Role::Subclass) {
                own.extend(
                    super::passage::unplanned_sections_in(&lines, e, plan)
                        .into_iter()
                        .filter(|s| s.at > r.start && s.at < end)
                        .map(|s| (s.at, s.end.min(end))),
                );
            }
            own
        })
        .collect();
    inner.sort_unstable();
    let mut out = Vec::new();
    let mut from = r.start;
    for (s, e) in inner {
        if s > from {
            out.push((from, s));
        }
        from = from.max(e);
    }
    if end > from {
        out.push((from, end));
    }
    let blocks: Vec<(usize, usize)> = out
        .into_iter()
        .map(|(s, e)| (s, without_bare_headings(&lines, s, e)))
        .filter(|(s, e)| lines[*s..*e].iter().any(|l| !l.trim().is_empty()))
        .collect();
    // A block of headings only (a sub-heading whose body is all planned children) says nothing.
    let has_body = |(s, e): &(usize, usize)| {
        lines[*s..*e]
            .iter()
            .any(|l| !l.trim().is_empty() && is_heading(l).is_none())
    };
    if blocks.iter().any(has_body) {
        blocks.into_iter().filter(has_body).collect()
    } else {
        blocks.into_iter().take(1).collect()
    }
}

/// `end` moved back over trailing blank lines and over the headings at the end of the block
/// `start..end` that have nothing under them (their body is a planned child's), never before
/// `start + 1`.
fn without_bare_headings(lines: &[&str], start: usize, end: usize) -> usize {
    let mut end = trimmed(lines, start, end);
    while end > start + 1 && is_heading(lines[end - 1]).is_some() {
        end = trimmed(lines, start, end - 1);
    }
    end
}

/// The source lines of [`section_blocks`], a blank line between blocks, page furniture left out.
pub(crate) fn section_lines<'a>(
    source: &'a str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<&'a str> {
    let lines: Vec<&str> = source.lines().collect();
    let furniture = super::passage::page_marks::page_furniture(source);
    let mut out = Vec::new();
    for (s, e) in section_blocks(source, planned, plan) {
        if !out.is_empty() {
            out.push("");
        }
        // Page furniture (a page number, a running header or footer) is no part of it.
        out.extend((s..e).filter(|i| !furniture.has(*i)).map(|i| lines[i]));
    }
    out
}

/// Source lines as HTML: paragraphs, bold headings, list lines apart (a wrapped list line joins
/// its item), pipe-table lines as a table (a wrapped cell joins its row), `**bold**` as bold.
pub(crate) fn to_html(lines: &[&str]) -> String {
    html::to_html(lines)
}

/// The text-only element of a rules or lore section, built from its source passage (no model
/// call, nothing paraphrased away).
pub(crate) fn text_element(planned: &PlannedElement, plan: &ImportPlan, source: &str) -> Value {
    let lines = section_lines(source, planned, plan);
    let description = if lines.is_empty() {
        format!("<p>{}</p>", escape(&planned.brief))
    } else {
        // Rows of elements imported on their own keep only their roll and name here.
        let blocks = section_blocks(source, planned, plan);
        let rows = rows::rows_of_others(&lines, &planned.id, &blocks, plan, source);
        to_html(&rows.iter().map(AsRef::as_ref).collect::<Vec<&str>>())
    };
    json!({"id": planned.id, "name": planned.name, "type": planned.ty,
        "supports": [text_support(&planned.ty)], "description": description})
}

/// `supports` of a text-only element: the bundled conditions are `Condition`, every other
/// entry is in the rules glossary.
fn text_support(ty: &str) -> &'static str {
    if ty == "Condition" {
        "Condition"
    } else {
        RULES_SUPPORT
    }
}

/// Report entries for what this module changed about the source's objects: text-only rules
/// and lore, subraces imported as species of their own.
pub(crate) fn report_entries(
    draft: &ImportDraft,
    final_id: &dyn Fn(&str) -> String,
) -> Vec<ReportEntry> {
    let mut out = Vec::new();
    for g in &draft.plan.groups {
        let Some(root) = g.root_id.as_deref().and_then(|id| draft.element(id)) else {
            continue;
        };
        match g.kind {
            ImportKind::Rules | ImportKind::Condition => {
                let what = g
                    .outline
                    .as_ref()
                    .map(|o| o.summary.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "rules or lore".into());
                out.push(ReportEntry {
                    source: g.name.clone(),
                    outcome: "text_only".into(),
                    elements: vec![final_id(&root.id)],
                    note: format!("{what}: kept as a text-only element with the source text; not mechanised (the engine applies no rules for it)"),
                });
            }
            ImportKind::Subrace if root.ty == ImportKind::Species.element_type() => {
                out.push(ReportEntry {
                    source: g.name.clone(),
                    outcome: "element".into(),
                    elements: vec![final_id(&root.id)],
                    note: "subrace whose parent species was not found; imported as a species of its own".into(),
                });
            }
            _ => {}
        }
    }
    out.extend(super::passage::uncovered_prose(draft));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_text_becomes_paragraphs_and_lists() {
        let lines = [
            "## Old Ways",
            "",
            "First line",
            "continues here & on.",
            "",
            "- one",
            "- two",
        ];
        let html = to_html(&lines);
        assert_eq!(
            html,
            "<p><strong>Old Ways</strong></p><p>First line continues here &amp; on.</p><p>- one<br>- two</p>"
        );
    }
}

#[cfg(test)]
#[path = "parts_tests.rs"]
mod pick_tests;
