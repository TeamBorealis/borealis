//! Deterministic additions that need the whole draft and the library, applied to a copy of the
//! draft's elements wherever the draft is loaded (the engine sweep and the commit):
//!
//! * spell lists: a new class's list gets its existing spells by an `append` (the content model's
//!   extension mechanism; bundled spells carry their lists in `supports`), its own new spells by
//!   `supports`; spell choices on a list that would still be empty are left out (and reported),
//!   so the class never stalls on an empty select;
//! * weapon groups and categories no loaded pack covers, and weapon properties no pack defines
//!   (`groups.rs`): elements named from the source with the import's ids, the weapons tagged and
//!   proficient through them, the classes whose proficiency text names a group granting it;
//! * class table columns no rule reads: a counter (`resource` with its max from the table).
//!
//! Every change is reported (`notes`).

use std::collections::HashSet;

use borealis_content::{ContentStore, Element, Ruleset};
use serde_json::{json, Map, Value};

use super::caster::{caster_kind, CasterKind};
use super::types::{Edition, ImportPlan, Role, SpellcastingOutline};

mod groups;

/// `data.progression` keys that are not counters.
const NOT_COUNTERS: [&str; 5] = [
    "proficiency_bonus",
    "cantrips",
    "prepared",
    "pact_slots",
    "pact_slot_level",
];
/// The builder's spell choices read the class list through this placeholder.
const LIST_PLACEHOLDER: &str = "$(spellcasting:list)";

/// The draft's elements after the derived fixes, plus what they need.
#[derive(Debug, Clone, Default)]
pub(crate) struct Derived {
    /// The draft's elements (same order, same ids) with the fixes applied.
    pub docs: Vec<Value>,
    /// New elements (weapon groups, categories and properties).
    pub extra: Vec<Value>,
    /// `appends` records (existing spells joining a new list, existing proficiencies granting a
    /// new weapon).
    pub appends: Vec<Value>,
    /// `(element id, message)` for each change, for the import report.
    pub notes: Vec<(String, String)>,
}

impl Derived {
    fn note(&mut self, id: &str, message: String) {
        self.notes.push((id.to_string(), message));
    }
}

/// Apply every derived fix to a copy of `docs`; `store` is the library without the draft.
pub(crate) fn derive(plan: &ImportPlan, docs: &[Value], store: &ContentStore) -> Derived {
    let mut d = Derived {
        docs: docs.to_vec(),
        ..Derived::default()
    };
    super::lists::class_lists(plan, store, &mut d);
    spell_lists(plan, store, &mut d);
    groups::weapon_groups(plan, store, &mut d);
    groups::new_properties(plan, store, &mut d);
    table_counters(&mut d);
    proficiency_choices(plan, store, &mut d);
    d
}

/// Skill / tool choices of classes and bonus-cantrip choices of features (see `choices.rs`).
fn proficiency_choices(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    let mut lists: Vec<String> = store
        .by_type("Class")
        .map(|c| c.element.name.clone())
        .collect();
    lists.extend(caster_classes(plan).into_iter().map(|(_, name, p)| {
        p.list
            .clone()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or(name)
    }));
    let mut notes = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        match str_at(doc, "type") {
            "Class" => {
                let text = proficiency_text(doc);
                notes.extend(
                    super::choices::class_choices(doc, &text, store)
                        .into_iter()
                        .map(|n| (id.clone(), n)),
                );
            }
            "Spell" => {}
            _ => {
                if let Some(sel) = super::choices::cantrip_select(doc, &lists) {
                    let list = str_at(&sel, "supports").to_string();
                    if let Some(rules) = rules_mut(doc) {
                        rules.push(sel);
                    }
                    notes.push((
                        id.clone(),
                        format!("cantrip choice from the text added (\"{list}\")"),
                    ));
                }
            }
        }
    }
    d.notes.extend(notes);
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn in_edition(el: &Element, edition: Edition) -> bool {
    let is_2024 = el.ruleset == Some(Ruleset::Dnd2024);
    is_2024 == (edition == Edition::Y2024)
}

fn supports_of(v: &Value) -> Vec<String> {
    v.get("supports")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn push_support(v: &mut Value, tag: &str) -> bool {
    let mut tags = supports_of(v);
    if tags.iter().any(|t| t == tag) {
        return false;
    }
    tags.push(tag.to_string());
    if let Some(o) = v.as_object_mut() {
        o.insert("supports".into(), json!(tags));
    }
    true
}

fn rules_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    let o = v.as_object_mut()?;
    o.entry("rules").or_insert_with(|| json!([])).as_array_mut()
}

/// Class roots of the plan with their caster profile.
fn caster_classes(plan: &ImportPlan) -> Vec<(String, String, &SpellcastingOutline)> {
    plan.groups
        .iter()
        .filter_map(|g| {
            let root = plan
                .elements
                .iter()
                .find(|e| Some(e.id.as_str()) == g.root_id.as_deref())?;
            if root.role != Role::Class {
                return None;
            }
            let p = g.outline.as_ref()?.caster()?;
            Some((root.id.clone(), root.name.clone(), p))
        })
        .collect()
}

fn spell_lists(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    for (root_id, class_name, p) in caster_classes(plan) {
        let list = p
            .list
            .clone()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| class_name.clone());
        add_list_members(plan.edition, store, &root_id, &list, p, d);
        if caster_kind(p) == CasterKind::Points {
            d.note(&root_id, format!("{class_name} casts from a pool of points: the engine has no point pool, so no spell slots were made; spellcasting, spell choices and preparation are modelled and the pool is in the text"));
        }
        let listed = store.by_type("Spell").any(|s| {
            in_edition(&s.element, plan.edition) && s.element.supports.iter().any(|t| t == &list)
        }) || d.appends.iter().any(|a| supports_of(a).contains(&list))
            || d.docs
                .iter()
                .any(|e| str_at(e, "type") == "Spell" && supports_of(e).iter().any(|t| t == &list));
        if !listed {
            drop_list_choices(&class_name, &list, d);
        }
    }
}

fn add_list_members(
    edition: Edition,
    store: &ContentStore,
    root_id: &str,
    list: &str,
    p: &SpellcastingOutline,
    d: &mut Derived,
) {
    let mut added = 0;
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for name in p.spells.iter().flatten() {
        let key = norm(name);
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if let Some(doc) = d
            .docs
            .iter_mut()
            .find(|e| str_at(e, "type") == "Spell" && norm(str_at(e, "name")) == key)
        {
            push_support(doc, list);
            continue;
        }
        let found = store
            .by_type("Spell")
            .find(|s| in_edition(&s.element, edition) && norm(&s.element.name) == key);
        match found {
            Some(s) if !s.element.supports.iter().any(|t| t == list) => {
                d.appends
                    .push(json!({"id": s.element.id, "type": "Spell", "supports": [list]}));
                added += 1;
            }
            Some(_) => {}
            None => missing.push(name.trim().to_string()),
        }
    }
    if added > 0 {
        d.note(
            root_id,
            format!("{added} existing spell(s) added to the {list} spell list"),
        );
    }
    if !missing.is_empty() {
        d.note(root_id, format!("{} spell(s) of the {list} list are neither in the library nor in this import, so they are not on the list: {}", missing.len(), missing.join(", ")));
    }
}

/// Remove the generated spell choices of a list no spell is on (they would offer nothing and
/// stall the class); reported so the user imports the spells.
fn drop_list_choices(class_name: &str, list: &str, d: &mut Derived) {
    let mut notes = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        let Some(rules) = rules_mut(doc) else {
            continue;
        };
        let before = rules.len();
        rules.retain(|r| {
            !(str_at(r, "kind") == "select"
                && str_at(r, "type") == "Spell"
                && str_at(r, "spellcasting") == class_name
                && str_at(r, "supports").starts_with(LIST_PLACEHOLDER))
        });
        if rules.len() < before {
            notes.push((id, before - rules.len()));
        }
    }
    for (id, n) in notes {
        d.note(&id, format!("no spell is on the {list} spell list yet, so its {n} spell choice(s) are left out; import the list's spells and they come back"));
    }
}

/// The proficiency line(s) of a class: description and `data.core_traits`, tags removed.
fn proficiency_text(class: &Value) -> String {
    let mut text = str_at(class, "description")
        .replace("</p>", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n");
    if let Some(traits) = class
        .pointer("/data/core_traits")
        .and_then(Value::as_object)
    {
        for (k, v) in traits {
            text.push_str(&format!("\n{k}: {}", v.as_str().unwrap_or("")));
        }
    }
    let mut plain = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    plain
}

fn label_of(key: &str) -> String {
    key.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Numbers of a progression column, numeric strings read as numbers; None if any entry is not.
fn numbers(col: &Value) -> Option<Vec<i64>> {
    col.as_array()?
        .iter()
        .map(|v| match v {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        })
        .collect()
}

fn table_counters(d: &mut Derived) {
    let all: Vec<String> = d.docs.iter().map(Value::to_string).collect();
    let mut adds: Vec<(String, String, Vec<i64>, String)> = Vec::new();
    for c in d.docs.iter().filter(|e| str_at(e, "type") == "Class") {
        let class = str_at(c, "name").to_string();
        let Some(prog) = c.pointer("/data/progression").and_then(Value::as_object) else {
            continue;
        };
        for (key, col) in prog {
            if NOT_COUNTERS.contains(&key.as_str()) || key.starts_with("slot_") {
                continue;
            }
            let quoted = format!("\"{key}\"");
            if all
                .iter()
                .map(|t| t.matches(&quoted).count())
                .sum::<usize>()
                > 1
            {
                continue; // a rule already reads it
            }
            let Some(vals) = numbers(col) else {
                continue;
            };
            if vals.iter().all(|v| *v == 0) {
                continue;
            }
            adds.push((
                str_at(c, "id").to_string(),
                class.clone(),
                vals,
                key.clone(),
            ));
        }
    }
    for (class_id, class, vals, key) in adds {
        add_counter(&class_id, &class, &vals, &key, d);
    }
}

/// The feature that carries a column's counter: the one named like it, else the class's first.
fn counter_home(d: &Derived, class: &str, label: &str) -> Option<usize> {
    let features: Vec<usize> = (0..d.docs.len())
        .filter(|i| {
            str_at(&d.docs[*i], "type") == "Class Feature"
                && d.docs[*i].pointer("/data/class").and_then(Value::as_str) == Some(class)
        })
        .collect();
    let want = norm(label);
    features
        .iter()
        .copied()
        .find(|i| norm(str_at(&d.docs[*i], "name")) == want)
        .or_else(|| {
            features.iter().copied().min_by_key(|i| {
                d.docs[*i]
                    .pointer("/data/level")
                    .and_then(Value::as_u64)
                    .unwrap_or(99)
            })
        })
}

fn add_counter(class_id: &str, class: &str, vals: &[i64], key: &str, d: &mut Derived) {
    let label = label_of(key);
    if let Some(prog) = d
        .docs
        .iter_mut()
        .find(|e| str_at(e, "id") == class_id)
        .and_then(|c| c.pointer_mut("/data/progression"))
        .and_then(Value::as_object_mut)
    {
        prog.insert(key.to_string(), json!(vals));
    }
    let Some(home) = counter_home(d, class, &label) else {
        d.note(class_id, format!("class table column \"{key}\" has no feature to carry its counter; kept as a table column only"));
        return;
    };
    let home_id = str_at(&d.docs[home], "id").to_string();
    let rule = json!({"kind": "resource", "id": super::rules::resource_id(&label), "name": label,
        "max": {"type": "table", "key": key}});
    if let Some(rules) = rules_mut(&mut d.docs[home]) {
        rules.push(rule);
    }
    d.note(&home_id, format!("class table column \"{key}\" became the counter \"{label}\" (its value by {class} level)"));
}

/// The derived elements and appends as a content file document next to the draft's own.
pub(crate) fn content_doc(d: &Derived) -> Value {
    let mut elements = d.docs.clone();
    elements.extend(d.extra.iter().cloned());
    let mut doc = Map::new();
    doc.insert(
        "format".into(),
        json!(borealis_content::file::ContentFormat::ID),
    );
    doc.insert("elements".into(), json!(elements));
    if !d.appends.is_empty() {
        doc.insert("appends".into(), json!(d.appends));
    }
    Value::Object(doc)
}
