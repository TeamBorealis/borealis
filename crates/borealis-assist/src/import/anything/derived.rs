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
//! * class table columns no rule reads: a counter (`resource` with its max from the table);
//! * descriptions: page furniture and transcription markers left out, every version of an
//!   item carrying the item's shared text.
//!
//! Every change is reported (`notes`).

use std::collections::HashSet;

use borealis_content::{ContentStore, Element, Ruleset};
use serde_json::{json, Map, Value};

use super::caster::{caster_kind, CasterKind};
use super::types::{Edition, ImportPlan, Role, SpellcastingOutline};

mod cycles;
mod groups;
mod languages;
mod progression;
mod roll_tables;
mod weapon_context;

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
    derive_with_source(plan, docs, store, "")
}

/// [`derive`], also reading the import's `source` text (class marks glued to spell names).
pub(crate) fn derive_with_source(
    plan: &ImportPlan,
    docs: &[Value],
    store: &ContentStore,
    source: &str,
) -> Derived {
    let mut d = Derived {
        docs: docs.to_vec(),
        ..Derived::default()
    };
    super::lists::class_lists(plan, store, &mut d);
    super::lists::marked_lists(source, plan, store, &mut d);
    spell_lists(plan, store, &mut d);
    super::feature_spells::official_spells(plan, store, &mut d);
    groups::weapon_groups(plan, source, store, &mut d);
    groups::new_properties(plan, store, &mut d);
    weapon_context::unmatched_filters(store, &mut d);
    languages::languages(plan, store, &mut d);
    roll_tables::roll_tables(plan, source, &mut d);
    strip_furniture(source, &mut d);
    strip_markers(&mut d);
    version_texts(&mut d);
    progression::outline_columns(plan, &mut d);
    progression::table_keys(store, &mut d);
    table_counters(&mut d);
    progression::dedupe_resources(&mut d);
    proficiency_choices(plan, store, &mut d);
    ability_choices(&mut d);
    creature_types(store, &mut d);
    cycles::break_grant_cycles(store, &mut d);
    d
}

/// Page furniture of the source (a running header or footer, a web address line) copied into a
/// description is taken out of it.
fn strip_furniture(source: &str, d: &mut Derived) {
    let furniture = super::furniture::furniture(source);
    if furniture.texts().is_empty() {
        return;
    }
    for i in 0..d.docs.len() {
        let Some(html) = d.docs[i].get("description").and_then(Value::as_str) else {
            continue;
        };
        let Some(clean) = super::furniture::strip_texts(html, furniture.texts()) else {
            continue;
        };
        d.docs[i]["description"] = json!(clean);
        let id = str_at(&d.docs[i], "id").to_string();
        d.note(
            &id,
            "description: page furniture (running header, footer or web address) left out".into(),
        );
    }
}

/// Transcription markers a page transcription put on text it did not copy word for word
/// ("(Summary)", "[Paraphrased]", "[Summary: ...]") are no printed text: left out of the
/// descriptions (the import's review keeps the transcript's own notes).
fn strip_markers(d: &mut Derived) {
    for i in 0..d.docs.len() {
        let Some(clean) = d.docs[i]
            .get("description")
            .and_then(Value::as_str)
            .and_then(super::furniture::strip_markers)
        else {
            continue;
        };
        d.docs[i]["description"] = json!(clean);
        let id = str_at(&d.docs[i], "id").to_string();
        d.note(&id, "description: transcription markers left out".into());
    }
}

/// Every version of an item ("<Item>, <version>") carries the item's shared text the same way
/// ([`super::items::variant::shared_text`]), whatever its generation batch wrote.
fn version_texts(d: &mut Derived) {
    let items = super::items::variant::item_descriptions(&d.docs);
    for i in 0..d.docs.len() {
        let Some(note) = super::items::variant::shared_text_fix(&mut d.docs[i], &items) else {
            continue;
        };
        let id = str_at(&d.docs[i], "id").to_string();
        d.note(&id, note);
    }
}

/// Tag-like prefix of the bundled creature-type racial traits ("Creature Type (Construct)").
const CREATURE_TYPE_PREFIX: &str = "ID_INTERNAL_RACIAL_TRAIT_TYPE_";

/// A race's printed creature type (`data.creature_type`) grants the bundled creature-type racial
/// trait of that type, when the library has it and the race does not grant it yet.
fn creature_types(store: &ContentStore, d: &mut Derived) {
    for i in 0..d.docs.len() {
        if !matches!(str_at(&d.docs[i], "type"), "Race" | "Sub Race") {
            continue;
        }
        let Some(kind) = d.docs[i]
            .pointer("/data/creature_type")
            .and_then(Value::as_str)
            .map(|k| norm(k).replace(' ', "_").to_uppercase())
            .filter(|k| !k.is_empty())
        else {
            continue;
        };
        let trait_id = format!("{CREATURE_TYPE_PREFIX}{kind}");
        if store.get(&trait_id).is_none() {
            continue;
        }
        let id = str_at(&d.docs[i], "id").to_string();
        let Some(rules) = rules_mut(&mut d.docs[i]) else {
            continue;
        };
        if rules.iter().any(|r| str_at(r, "id") == trait_id) {
            continue;
        }
        rules.push(json!({"kind": "grant", "type": "Racial Trait", "id": trait_id}));
        d.note(
            &id,
            format!("grants its printed creature type ({trait_id})"),
        );
    }
}

/// `ability_increase` options written as a bare list of ability names (no choice for the engine)
/// as choice objects, and lasting `movement_mode` speeds as `speed:<mode>` stats.
fn ability_choices(d: &mut Derived) {
    let mut notes = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        for m in super::rules::normalise_abilities(doc) {
            notes.push((id.clone(), m));
        }
    }
    d.notes.extend(notes);
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
    let rid = super::rules::resource_id(&label);
    if repair_counter(class, &rid, key, d) {
        return;
    }
    let home_id = str_at(&d.docs[home], "id").to_string();
    let rule = json!({"kind": "resource", "id": rid, "name": label,
        "max": {"type": "table", "key": key}});
    if let Some(rules) = rules_mut(&mut d.docs[home]) {
        rules.push(rule);
    }
    d.note(&home_id, format!("class table column \"{key}\" became the counter \"{label}\" (its value by {class} level)"));
}

/// A `resource` the model already wrote under the counter's id on one of the class's features
/// reads the column instead of getting a twin (the engine would keep only the first of the two).
fn repair_counter(class: &str, rid: &str, key: &str, d: &mut Derived) -> bool {
    for doc in &mut d.docs {
        if str_at(doc, "type") != "Class Feature"
            || doc.pointer("/data/class").and_then(Value::as_str) != Some(class)
        {
            continue;
        }
        let id = str_at(doc, "id").to_string();
        let Some(rules) = doc.get_mut("rules").and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(r) = rules
            .iter_mut()
            .find(|r| str_at(r, "kind") == "resource" && str_at(r, "id") == rid)
        else {
            continue;
        };
        let old = r.get("max").cloned().unwrap_or(Value::Null);
        r["max"] = json!({"type": "table", "key": key});
        d.notes.push((
            id,
            format!("resource \"{rid}\" reads the class table column \"{key}\" (was {old})"),
        ));
        return true;
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_races_printed_creature_type_grants_the_bundled_type_trait() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = borealis_content::ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .expect("core pack")
            .build();
        let race = |id: &str, kind: &str| {
            json!({"id": id, "name": id, "type": "Race", "data": {"creature_type": kind},
                "rules": []})
        };
        let mut d = Derived {
            docs: vec![
                race("ID_T_RACE_A", "Construct"),
                race("ID_T_RACE_B", "Moss Spirit"),
            ],
            ..Derived::default()
        };
        creature_types(&store, &mut d);
        creature_types(&store, &mut d);
        let grants = |i: usize| -> Vec<String> {
            d.docs[i]["rules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| str_at(r, "id").to_string())
                .collect()
        };
        assert_eq!(grants(0), ["ID_INTERNAL_RACIAL_TRAIT_TYPE_CONSTRUCT"]);
        assert!(grants(1).is_empty(), "an unknown type grants nothing");
        assert_eq!(d.notes.len(), 1);
    }
}
