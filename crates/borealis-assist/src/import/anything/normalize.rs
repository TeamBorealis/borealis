//! Deterministic scaffolding applied to every model-written element: forced identity, class
//! grants, subclass links and the subclass gate. The model fills content; ids, levels and the
//! plan's structure are not left to it.

use serde_json::{json, Map, Value};

use super::caster::{build_caster, is_caster_feature, is_caster_part, is_model_caster_rule};
use super::types::{Edition, ImportPlan, ParentLink, PlannedElement, Role, SpellcastingOutline};

/// `data.progression` keys the caster builder owns (besides `slot_N`).
const CASTER_TABLES: [&str; 4] = ["cantrips", "prepared", "pact_slots", "pact_slot_level"];

const PROFICIENCY_BY_LEVEL: [u32; 20] =
    [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6];

fn obj(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().expect("just made an object")
}

fn data_of(el: &mut Value) -> &mut Map<String, Value> {
    let o = obj(el);
    let d = o.entry("data").or_insert_with(|| json!({}));
    obj(d)
}

fn grant(id: &str, ty: &str, level: Option<u32>) -> Value {
    let mut g = json!({"kind": "grant", "id": id, "type": ty});
    if let Some(l) = level.filter(|l| *l > 1) {
        g["level"] = json!(l);
    }
    g
}

fn is_grant_of(r: &Value, ty: &str) -> bool {
    r.get("kind").and_then(Value::as_str) == Some("grant")
        && r.get("type").and_then(Value::as_str) == Some(ty)
}

fn owned<'a>(plan: &'a ImportPlan, root: &str) -> Vec<&'a PlannedElement> {
    plan.elements
        .iter()
        .filter(|e| e.owner.as_deref() == Some(root))
        .collect()
}

fn gate_name(plan: &ImportPlan, planned: &PlannedElement) -> String {
    plan.elements
        .iter()
        .find(|e| e.id == planned.owner.clone().unwrap_or_default())
        .map_or_else(|| planned.name.clone(), |c| c.name.clone())
}

/// The deterministic subclass gate feature.
pub(crate) fn gate_element(plan: &ImportPlan, planned: &PlannedElement) -> Value {
    let class = gate_name(plan, planned);
    let key = format!("{class} Subclass");
    json!({
        "id": planned.id, "name": planned.name, "type": "Class Feature",
        "description": format!("<p>You gain a {class} subclass of your choice. Your subclass grants you features at certain {class} levels.</p>"),
        "rules": [{"kind": "select", "type": "Archetype", "name": key, "supports": key}],
        "data": {"class": class, "level": planned.level}
    })
}

/// The `supports` key a class's subclasses use.
pub(crate) fn subclass_key(class_name: &str) -> String {
    format!("{class_name} Subclass")
}

/// Apply the scaffolding to `el` (the model's output for `planned`).
pub(crate) fn normalize(
    el: &mut Value,
    planned: &PlannedElement,
    plan: &ImportPlan,
    parent: Option<&ParentLink>,
    source_name: &str,
) {
    let o = obj(el);
    o.insert("id".into(), json!(planned.id));
    o.insert("name".into(), json!(planned.name));
    o.insert("type".into(), json!(planned.ty));
    o.insert("source".into(), json!(source_name));
    o.remove("maps_to");
    if plan.edition == Edition::Y2024 {
        o.insert("ruleset".into(), json!("dnd2024"));
    } else {
        o.remove("ruleset");
    }
    match planned.role {
        Role::Class => normalize_class(el, planned, plan),
        Role::Subclass => normalize_subclass(el, planned, plan, parent),
        Role::ClassFeature | Role::SubclassFeature => normalize_feature(el, planned, plan, parent),
        Role::Standalone => {
            let item_type = plan
                .groups
                .iter()
                .find(|g| g.root_id.as_deref() == Some(planned.id.as_str()))
                .and_then(|g| g.item_type);
            super::items::normalize_item(el, &planned.ty, &planned.name, item_type, plan);
            super::mechanics::attunement_by(el);
        }
        Role::SubclassGate | Role::Part | Role::Option | Role::Text => {}
    }
    // Named parts, option families, subraces and text-only elements (import-fix-i2).
    super::parts::apply(el, planned, plan);
}

fn normalize_feature(
    el: &mut Value,
    planned: &PlannedElement,
    plan: &ImportPlan,
    parent: Option<&ParentLink>,
) {
    let root = plan
        .elements
        .iter()
        .find(|e| Some(e.id.as_str()) == planned.owner.as_deref());
    let level = planned.level;
    let d = data_of(el);
    match (planned.role, root, parent) {
        (Role::SubclassFeature, Some(sub), Some(p)) => {
            d.insert("class".into(), json!(p.class_name));
            d.insert("subclass".into(), json!(sub.name));
        }
        (_, Some(class), _) => {
            d.insert("class".into(), json!(class.name));
        }
        _ => {}
    }
    d.insert("level".into(), json!(level));
    apply_caster_feature(el, planned, plan);
    super::arcanum::with_selects(el, plan.edition);
}

/// The feature has spellcasting sub-headings as parts ("Cantrips", "Spell Slots", ...).
fn has_caster_parts(plan: &ImportPlan, feature: &PlannedElement) -> bool {
    plan.elements.iter().any(|e| {
        e.role == Role::Part
            && e.parent.as_deref() == Some(feature.id.as_str())
            && is_caster_part(&e.name)
    })
}

/// The id of the class's spellcasting feature: the first feature at level <= 3 that has
/// spellcasting sub-headings (whatever the source calls it, "X Magic"), else the first
/// caster-named one.
fn caster_feature_id<'a>(plan: &'a ImportPlan, root: &str) -> Option<&'a str> {
    owned(plan, root)
        .into_iter()
        .filter(|f| f.role == Role::ClassFeature && f.level.is_some_and(|l| l <= 3))
        .map(|f| (f, has_caster_parts(plan, f)))
        .filter(|(f, parts)| *parts || is_caster_feature(&f.name))
        .min_by_key(|(f, parts)| (!parts, f.level))
        .map(|(f, _)| f.id.as_str())
}

fn class_profile<'a>(plan: &'a ImportPlan, root: &str) -> Option<&'a SpellcastingOutline> {
    plan.groups
        .iter()
        .find(|g| g.root_id.as_deref() == Some(root))
        .and_then(|g| g.outline.as_ref())
        .and_then(|o| o.caster())
}

/// Replace whatever the model wrote for spellcasting with the deterministic builder's output.
fn apply_caster_feature(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let Some(root) = planned.owner.as_deref() else {
        return;
    };
    let Some(profile) = class_profile(plan, root) else {
        return;
    };
    if caster_feature_id(plan, root) != Some(planned.id.as_str()) {
        return;
    }
    let class_name = plan
        .elements
        .iter()
        .find(|e| e.id == root)
        .map_or("", |e| e.name.as_str());
    let Some(built) = build_caster(profile, class_name, plan.edition) else {
        return;
    };
    let o = obj(el);
    o.insert("spellcasting".into(), built.node);
    let rules = o.entry("rules").or_insert_with(|| json!([]));
    if let Some(arr) = rules.as_array_mut() {
        arr.retain(|r| !is_model_caster_rule(r));
        arr.extend(built.rules);
    }
}

fn normalize_class(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let key = subclass_key(&planned.name);
    let features = owned(plan, &planned.id);
    let gate_level = features
        .iter()
        .find(|f| f.role == Role::SubclassGate)
        .and_then(|f| f.level);
    let o = obj(el);
    o.insert("supports".into(), json!(["Class"]));
    let rules = o.entry("rules").or_insert_with(|| json!([]));
    if let Some(arr) = rules.as_array_mut() {
        arr.retain(|r| !is_grant_of(r, "Class Feature"));
        arr.extend(
            features
                .iter()
                .map(|f| grant(&f.id, "Class Feature", f.level)),
        );
    }
    let built =
        class_profile(plan, &planned.id).and_then(|p| build_caster(p, &planned.name, plan.edition));
    let d = data_of(el);
    d.remove("spellcasting");
    if let Some(sc) = built.as_ref().and_then(|b| b.class_spellcasting.clone()) {
        d.insert("spellcasting".into(), sc);
    }
    if let Some(l) = gate_level {
        d.insert("subclass_level".into(), json!(l));
    }
    d.entry("subclass_key").or_insert_with(|| json!(key));
    let p = d.entry("progression").or_insert_with(|| json!({}));
    let p = obj(p);
    p.entry("proficiency_bonus")
        .or_insert_with(|| json!(PROFICIENCY_BY_LEVEL));
    if let Some(b) = built {
        for k in CASTER_TABLES {
            p.remove(k);
        }
        p.retain(|k, _| !k.starts_with("slot_"));
        p.extend(b.progression);
    }
}

fn normalize_subclass(
    el: &mut Value,
    planned: &PlannedElement,
    plan: &ImportPlan,
    parent: Option<&ParentLink>,
) {
    let features = owned(plan, &planned.id);
    let first = features.iter().filter_map(|f| f.level).min();
    let o = obj(el);
    if let Some(p) = parent {
        o.insert("supports".into(), json!([p.supports]));
    }
    let rules = o.entry("rules").or_insert_with(|| json!([]));
    if let Some(arr) = rules.as_array_mut() {
        arr.retain(|r| !is_grant_of(r, "Archetype Feature"));
        arr.extend(
            features
                .iter()
                .map(|f| grant(&f.id, "Archetype Feature", f.level.or(Some(2)))),
        );
    }
    let d = data_of(el);
    if let Some(p) = parent {
        d.insert("class".into(), json!(p.class_id));
        d.insert("class_name".into(), json!(p.class_name));
    }
    if let Some(l) = first {
        d.insert("level_gained".into(), json!(l));
    }
}

/// Structural checks the schema cannot express; each message goes to the model on repair.
pub(crate) fn static_checks(
    el: &Value,
    planned: &PlannedElement,
    plan: &ImportPlan,
) -> Vec<String> {
    let mut out = Vec::new();
    if planned.role != Role::Class {
        return out;
    }
    let data = el.get("data").and_then(Value::as_object);
    let prog = data
        .and_then(|d| d.get("progression"))
        .and_then(Value::as_object);
    match prog {
        None => out.push("data.progression must be an object of per-level arrays".to_string()),
        Some(p) => {
            for (k, v) in p {
                match v.as_array() {
                    Some(a) if a.len() == 20 => {}
                    _ => out.push(format!("data.progression.{k} must be an array with exactly 20 entries (levels 1-20)")),
                }
            }
        }
    }
    let wants_mc = if plan.edition == Edition::Y2024 {
        data.is_some_and(|d| d.get("multiclass").is_some_and(|m| m.is_object()))
    } else {
        el.get("multiclass").is_some_and(Value::is_object)
    };
    if !wants_mc {
        let where_ = if plan.edition == Edition::Y2024 {
            "data.multiclass"
        } else {
            "multiclass"
        };
        out.push(format!(
            "the multiclass definition ({where_}: prerequisite and requirements) is missing"
        ));
    }
    let casts = plan.groups.iter().any(|g| {
        g.root_id.as_deref() == Some(&planned.id)
            && g.outline.as_ref().is_some_and(|o| o.caster().is_some())
    });
    if casts
        && plan.edition == Edition::Y2024
        && !data.is_some_and(|d| d.get("spellcasting").is_some_and(Value::is_object))
    {
        out.push("the class casts spells but data.spellcasting is missing".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(id: &str, name: &str, role: &str, parent: Option<&str>) -> Value {
        json!({"id": id, "ty": "Class Feature", "name": name, "role": role, "level": 1,
            "owner": "C", "brief": "", "excerpt": null, "parent": parent})
    }

    /// A source spellcasting section under another name ("Moss Magic" with "Cantrips" and
    /// "Spell Slots" sub-headings) is the caster feature, not the generated "<Class>
    /// Spellcasting" beside it.
    #[test]
    fn a_feature_with_caster_parts_is_the_spellcasting_feature() {
        let plan: ImportPlan = serde_json::from_value(json!({"edition": "2014", "notes": [],
        "groups": [], "elements": [
            planned("GEN", "Tidecaller Spellcasting", "class_feature", None),
            planned("MOSS", "Moss Magic", "class_feature", None),
            planned("P1", "Cantrips", "part", Some("MOSS")),
            planned("P2", "Spell Slots", "part", Some("MOSS")),
            planned("OTHER", "Tide Sense", "class_feature", None),
        ]}))
        .expect("plan");
        assert_eq!(caster_feature_id(&plan, "C"), Some("MOSS"));
        let without: ImportPlan = serde_json::from_value(json!({"edition": "2014", "notes": [],
        "groups": [], "elements": [
            planned("OTHER", "Tide Sense", "class_feature", None),
            planned("GEN", "Tidecaller Spellcasting", "class_feature", None),
        ]}))
        .expect("plan");
        assert_eq!(caster_feature_id(&without, "C"), Some("GEN"));
    }
}
