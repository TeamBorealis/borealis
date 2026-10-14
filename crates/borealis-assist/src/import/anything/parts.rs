//! Named sub-items as their own elements, the way the bundled packs model them: species traits
//! (`Racial Trait`) and background features granted by their root, subraces offered by their
//! species' choice, sub-features granted by their feature, pick-N option families offered by a
//! select (gated by level, prerequisites as requirements) and rules or lore kept as text-only
//! elements. The plan decides the structure; this module writes it into the elements.

use serde_json::{json, Map, Value};

use super::types::{ImportDraft, ImportKind, ImportPlan, PlannedElement, Role};
use crate::import::report::ReportEntry;

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

fn is_list_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with('|')
        || t.split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
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

/// A heading of the same level that only continues a text-only section the PDF flattened (a
/// question of a FAQ, a sub-rule): unplanned, no boundary word, within reach.
fn continues_section(
    plan: &ImportPlan,
    planned: &PlannedElement,
    line: &str,
    own_end: Option<usize>,
    at: usize,
) -> bool {
    let Some((_, title)) = heading_text(line) else {
        return false;
    };
    planned.role == Role::Text
        && own_end.map_or(true, |e| at < e + MAX_EXTENSION_LINES)
        && !title.split(' ').any(|w| BOUNDARY_WORDS.contains(&w))
        && !names_planned(plan, &planned.id, &title)
}

/// The 0-based line range of the section starting at the element's passage: up to the next
/// planned passage or the next heading of the same or a higher level. A text-only element
/// also reaches over the unplanned headings of its own level that follow (a section the PDF
/// printed flat), up to the next planned element. The passage end is never cut short.
pub(crate) fn section_range(
    source: &str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Option<(usize, usize)> {
    let ex = planned.excerpt.as_ref()?;
    let lines: Vec<&str> = source.lines().collect();
    let start = ex.start_line.saturating_sub(1).min(lines.len());
    let next = plan
        .elements
        .iter()
        .filter(|e| e.id != planned.id)
        .filter_map(|e| e.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1)))
        .filter(|l| *l > start)
        .min()
        .unwrap_or(lines.len());
    let own = lines.get(start).and_then(|l| is_heading(l));
    let mut end = start + 1;
    let mut own_end: Option<usize> = None;
    while end < lines.len().min(next).min(start + MAX_SECTION_LINES) {
        if let (Some(o), Some(h)) = (own, is_heading(lines[end])) {
            let same = h == o;
            if h < o || (same && !continues_section(plan, planned, lines[end], own_end, end)) {
                break;
            }
            if same {
                own_end.get_or_insert(end);
            }
        }
        end += 1;
    }
    let mut end = end.max(ex.end_line.min(lines.len()));
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    Some((start, end))
}

/// The source lines of [`section_range`].
pub(crate) fn section_lines<'a>(
    source: &'a str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<&'a str> {
    let Some((start, end)) = section_range(source, planned, plan) else {
        return Vec::new();
    };
    let lines: Vec<&str> = source.lines().collect();
    lines[start..end.min(lines.len())].to_vec()
}

/// Source lines as HTML paragraphs (headings bold, list and table lines kept apart).
pub(crate) fn to_html(lines: &[&str]) -> String {
    let mut out = String::new();
    for block in lines.split(|l| l.trim().is_empty()) {
        if block.is_empty() {
            continue;
        }
        let mut para: Vec<String> = Vec::new();
        let flush = |para: &mut Vec<String>, out: &mut String, list: bool| {
            if !para.is_empty() {
                let sep = if list { "<br>" } else { " " };
                out.push_str(&format!("<p>{}</p>", para.join(sep)));
                para.clear();
            }
        };
        let list = block.iter().any(|l| is_list_line(l));
        for l in block {
            if let Some(n) = is_heading(l) {
                flush(&mut para, &mut out, list);
                let text = l.trim_start()[n..].trim();
                out.push_str(&format!("<p><strong>{}</strong></p>", escape(text)));
            } else {
                para.push(escape(l.trim()));
            }
        }
        flush(&mut para, &mut out, list);
    }
    out
}

/// The text-only element of a rules or lore section, built from its source passage (no model
/// call, nothing paraphrased away).
pub(crate) fn text_element(planned: &PlannedElement, plan: &ImportPlan, source: &str) -> Value {
    let lines = section_lines(source, planned, plan);
    let description = if lines.is_empty() {
        format!("<p>{}</p>", escape(&planned.brief))
    } else {
        to_html(&lines)
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
