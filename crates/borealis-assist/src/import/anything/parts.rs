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
    offer_options(el, planned);
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

fn offer_options(el: &mut Value, planned: &PlannedElement) {
    let Some(picks) = &planned.picks else { return };
    let own = planned.level.unwrap_or(1);
    // A choice among existing elements: the model's own select or grants of them go.
    let ids: Vec<&str> = if picks.key.starts_with("ID_") {
        picks.key.split('|').collect()
    } else {
        Vec::new()
    };
    let names_an_id = |r: &Value| {
        let s = r.get("supports").and_then(Value::as_str).unwrap_or("");
        let id = r.get("id").and_then(Value::as_str).unwrap_or("");
        ids.iter()
            .any(|i| *i == id || s.split('|').any(|p| p.trim() == *i))
    };
    let rules = rules_of(el);
    rules.retain(|r| {
        let kind = r.get("kind").and_then(Value::as_str);
        let same_key = kind == Some("select")
            && r.get("supports").and_then(Value::as_str) == Some(picks.key.as_str());
        let model_pick = matches!(kind, Some("select" | "grant")) && names_an_id(r);
        !(same_key || model_pick)
    });
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

/// The source lines of the section starting at the element's passage: up to the next planned
/// passage or the next heading of the same or a higher level.
pub(crate) fn section_lines<'a>(
    source: &'a str,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<&'a str> {
    let Some(ex) = planned.excerpt.as_ref() else {
        return Vec::new();
    };
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
    while end < lines.len().min(next).min(start + MAX_SECTION_LINES) {
        if let (Some(o), Some(h)) = (own, is_heading(lines[end])) {
            if h <= o {
                break;
            }
        }
        end += 1;
    }
    let end = end.max(ex.end_line.min(lines.len()));
    let mut out: Vec<&str> = lines[start..end].to_vec();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
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
