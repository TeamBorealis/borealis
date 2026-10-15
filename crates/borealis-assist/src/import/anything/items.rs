//! Mundane equipment in the shape the bundled SRD packs use, so the engine reads it: a `Weapon`
//! becomes an attack (damage, properties, range, proficiency), `Armor` sets AC, ammunition and
//! gear are `Item`s. The model writes the stats (in `data`, or setters copied from the example);
//! the setters, `supports` tags and AC rules are derived here, deterministically.

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use super::types::{ImportPlan, ItemType, PlannedElement};

#[path = "items_row.rs"]
pub(crate) mod row;

#[path = "items_variant.rs"]
pub(crate) mod variant;

/// The core pack's weapon properties (the bundled vocabulary every pack shares): a printed
/// property name maps to one of these, else it is the source's own (see `derived.rs`).
const CORE_PROPERTIES: &str = include_str!("../../../../../content/core/properties.json");

/// `(lowercase name, id)` of every core `Weapon Property`.
fn core_properties() -> &'static [(String, String)] {
    static LIST: OnceLock<Vec<(String, String)>> = OnceLock::new();
    LIST.get_or_init(|| {
        let doc: Value = serde_json::from_str(CORE_PROPERTIES).unwrap_or_default();
        doc.get("elements")
            .and_then(Value::as_array)
            .map(|els| {
                els.iter()
                    .filter(|e| e.get("type").and_then(Value::as_str) == Some("Weapon Property"))
                    .filter_map(|e| {
                        let name = e.get("name")?.as_str()?.to_lowercase();
                        Some((name, e.get("id")?.as_str()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}
const TWO_HANDED: &str = "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED";
const RANGE_TAG: &str = "ID_INTERNAL_WEAPON_PROPERTY_RANGE";
/// Weapon categories: name -> (`supports` tag, proficiency that covers it, melee).
const CATEGORIES: [(&str, &str, &str, bool); 5] = [
    (
        "simple melee",
        "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE",
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_MELEE_WEAPONS",
        true,
    ),
    (
        "simple ranged",
        "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED",
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_RANGED_WEAPONS",
        false,
    ),
    (
        "martial melee",
        "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE",
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_MELEE_WEAPONS",
        true,
    ),
    (
        "martial ranged",
        "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED",
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_RANGED_WEAPONS",
        false,
    ),
    // Core has the Firearm category but no proficiency for it: martial ranged covers firearms
    // unless the source makes them a group of their own (`derived/groups.rs`).
    (
        "firearm",
        "ID_INTERNAL_WEAPON_CATEGORY_FIREARM",
        "ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_RANGED_WEAPONS",
        false,
    ),
];
/// Armor categories: name -> (`supports` tag, proficiency, equip slot).
const ARMOR: [(&str, &str, &str, &str); 4] = [
    (
        "light",
        "ID_INTERNAL_ARMOR_GROUP_LIGHT",
        "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR",
        "body",
    ),
    (
        "medium",
        "ID_INTERNAL_ARMOR_GROUP_MEDIUM",
        "ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR",
        "body",
    ),
    (
        "heavy",
        "ID_INTERNAL_ARMOR_GROUP_HEAVY",
        "ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR",
        "body",
    ),
    (
        "shield",
        "ID_INTERNAL_ARMOR_GROUP_SHIELD",
        "ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS",
        "secondary",
    ),
];
const STEALTH_DISADVANTAGE: &str = "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE";

/// What the generation prompt asks for, per element type.
pub(crate) fn guidance(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "Weapon" => "A mundane weapon, shaped like the example. Put its stats in `data`: weapon_category (Simple Melee, Simple Ranged, Martial Melee, Martial Ranged, Firearm, or the category name exactly as the source prints it when it is none of these), weapon_group (the source's own weapon group or category name when it also gives one of the standard categories; omit otherwise), melee (true/false), damage {dice, type}, properties (EVERY property name as printed, new ones included, e.g. [\"Ammunition\", \"Loading\"]), versatile_damage, range (\"normal/long\" feet, ranged and thrown weapons), ammunition_type, cost {value, currency}, weight (pounds), and mastery for 2024. Setters, supports and proficiency are derived from `data`. Describe special rules of the weapon in `description`. COLUMNS: every other column of the weapon's table row (a material, a crafting cost or DC, ...) goes in data.columns {\"<column as printed>\": \"<cell as printed>\"}; it is kept in the description. Cost and weight exactly as the row prints them. DESCRIPTION: only what the source prints for it, word for word; a table row with no prose of its own gets no sentence you write (no summary, no core rules text).",
        "Armor" => "Mundane armor or a shield, shaped like the example. Put its stats in `data`: armor_category (Light, Medium, Heavy or Shield), base_ac (the shield bonus for a shield), dex_modifier, dex_cap, strength_requirement, stealth_disadvantage, cost {value, currency}, weight (pounds). Setters, supports and AC rules are derived from `data`. COLUMNS: every other column of its table row goes in data.columns {\"<column as printed>\": \"<cell as printed>\"}; it is kept in the description. DESCRIPTION: only what the source prints for it, word for word; a table row with no prose of its own gets no sentence you write (no summary, no core rules text).",
        "Item" => "Mundane equipment (ammunition or adventuring gear), shaped like the example: description, and `data` with cost {value, currency}, weight (pounds); ammunition also data.ammunition true, amount (per bundle) and the weapon it is for in data.ammunition_for. COLUMNS: every other column of its table row goes in data.columns {\"<column as printed>\": \"<cell as printed>\"}; it is kept in the description. DESCRIPTION: only what the source prints for it, word for word; a table row with no prose of its own gets no sentence you write (no summary, no core rules text).",
        "Weapon Property" => "A weapon property: `description` in HTML paragraphs with the rules of the property kept as printed (word for word, nothing added). No rules.",
        _ => return None,
    })
}

fn obj(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().expect("just made an object")
}

fn str_of(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn setter<'a>(el: &'a Value, name: &str) -> Option<&'a Value> {
    el.get("setters")?
        .as_array()?
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
}

fn setter_value(el: &Value, name: &str) -> Option<String> {
    str_of(setter(el, name)?.get("value"))
}

/// Replace (or add) setter `name`.
fn put_setter(el: &mut Value, name: &str, value: &str, attributes: Option<Value>) {
    let o = obj(el);
    let list = o.entry("setters").or_insert_with(|| json!([]));
    if !list.is_array() {
        *list = json!([]);
    }
    let arr = list.as_array_mut().expect("just made an array");
    arr.retain(|s| s.get("name").and_then(Value::as_str) != Some(name));
    let mut s = json!({"name": name, "value": value});
    if let Some(a) = attributes {
        s["attributes"] = a;
    }
    arr.push(s);
}

fn data_mut(el: &mut Value) -> &mut Map<String, Value> {
    obj(obj(el).entry("data").or_insert_with(|| json!({})))
}

/// Property name without its parenthesised detail, lowercase: "Versatile (1d10)" -> "versatile".
pub(crate) fn property_key(name: &str) -> String {
    let head = name.split('(').next().unwrap_or(name);
    let k = head.trim().to_lowercase().replace('_', " ");
    if k == "two handed" || k == "twohanded" {
        "two-handed".into()
    } else {
        k
    }
}

/// The `supports` tag of a core property: by the full printed name ("Special (Net)"), else by the
/// name without its detail; of several of a name, the shortest id (the plain "Ammunition").
pub(crate) fn known_property(name: &str) -> Option<&'static str> {
    let full = name.trim().to_lowercase();
    let k = property_key(name);
    let core = core_properties();
    let shortest = |hit: &dyn Fn(&str) -> bool| {
        core.iter()
            .filter(|(n, _)| hit(n))
            .map(|(_, id)| id.as_str())
            .min_by_key(|id| id.len())
    };
    shortest(&|n| n == full).or_else(|| shortest(&|n| property_key(n) == k))
}

/// Text inside the parentheses of a property, e.g. "1d10" or "range 80/320".
fn property_detail(name: &str) -> Option<String> {
    let (_, rest) = name.split_once('(')?;
    Some(rest.trim_end_matches(')').trim().to_string())
}

/// `(name, id)` of the weapon properties the plan imports from the same source.
pub(crate) fn planned_properties(plan: &ImportPlan) -> Vec<(String, String)> {
    plan.elements
        .iter()
        .filter(|e| e.ty == "Weapon Property")
        .map(|e| (e.name.clone(), e.id.clone()))
        .collect()
}

/// Printed property names of a weapon element (`data.properties`, else its property tags).
pub(crate) fn weapon_properties(el: &Value) -> Vec<String> {
    let listed: Vec<String> = el
        .pointer("/data/properties")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| str_of(Some(v))).collect())
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    let tags: Vec<&str> = el
        .get("supports")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    core_properties()
        .iter()
        .filter(|(_, id)| tags.contains(&id.as_str()))
        .map(|(n, _)| capitalise(n))
        .collect()
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// `(value, currency)` from data.cost (`{value, currency}`, `"25 gp"`, `25`).
fn cost_of(v: Option<&Value>) -> Option<(String, String)> {
    let v = v?;
    if let Some(o) = v.as_object() {
        let value = str_of(o.get("value"))?;
        let cur = str_of(o.get("currency")).unwrap_or_else(|| "gp".into());
        return Some((value, cur.to_lowercase()));
    }
    let text = str_of(Some(v))?;
    let mut parts = text.split_whitespace();
    let value = parts.next()?.replace(',', "");
    let cur = parts.next().unwrap_or("gp").to_lowercase();
    Some((value, cur))
}

/// Pounds from data.weight (`3`, `"3 lb."`).
fn pounds_of(v: Option<&Value>) -> Option<String> {
    let text = str_of(v)?;
    let n = text.split_whitespace().next()?.to_string();
    n.parse::<f64>().is_ok().then_some(n)
}

/// Cost and weight setters from `data` when the model wrote none.
fn cost_and_weight(el: &mut Value) {
    let data = el.get("data").cloned().unwrap_or_default();
    if setter(el, "cost").is_none() {
        if let Some((v, c)) = cost_of(data.get("cost")) {
            put_setter(el, "cost", &v, Some(json!({"currency": c})));
        }
    }
    if setter(el, "weight").is_none() {
        if let Some(lb) = pounds_of(data.get("weight")) {
            put_setter(el, "weight", &format!("{lb} lb."), Some(json!({"lb": lb})));
        }
    }
}

fn supports_mut(el: &mut Value) -> &mut Vec<Value> {
    let o = obj(el);
    let s = o.entry("supports").or_insert_with(|| json!([]));
    if !s.is_array() {
        *s = json!([]);
    }
    s.as_array_mut().expect("just made an array")
}

fn add_support(el: &mut Value, tag: &str) {
    let s = supports_mut(el);
    if !s.iter().any(|v| v.as_str() == Some(tag)) {
        s.push(json!(tag));
    }
}

fn category_of(el: &Value) -> Option<&'static (&'static str, &'static str, &'static str, bool)> {
    let named = str_of(el.pointer("/data/weapon_category")).map(|c| c.to_lowercase());
    if let Some(c) = named {
        if let Some(hit) = CATEGORIES.iter().find(|(n, ..)| c.contains(n)) {
            return Some(hit);
        }
    }
    let tags = el.get("supports").and_then(Value::as_array)?;
    CATEGORIES
        .iter()
        .find(|(_, tag, ..)| tags.iter().any(|t| t.as_str() == Some(tag)))
}

/// Weapon: category, damage, properties, range and proficiency as setters and tags.
fn normalize_weapon(el: &mut Value, own: &[(String, String)]) {
    let props = weapon_properties(el);
    let cat = category_of(el);
    let dice = str_of(el.pointer("/data/damage/dice"))
        .or_else(|| setter_value(el, "damage"))
        .unwrap_or_default();
    let dtype = str_of(el.pointer("/data/damage/type"))
        .or_else(|| setter(el, "damage").and_then(|s| str_of(s.pointer("/attributes/type"))))
        .unwrap_or_default()
        .to_lowercase();
    let detail = |key: &str| {
        props
            .iter()
            .find(|p| property_key(p) == key)
            .and_then(|p| property_detail(p))
    };
    let range = str_of(el.pointer("/data/range"))
        .or_else(|| setter_value(el, "range"))
        .or_else(|| {
            ["ammunition", "thrown", "range"]
                .iter()
                .find_map(|k| detail(k))
                .and_then(|d| {
                    d.split_whitespace()
                        .find(|w| w.contains('/'))
                        .map(str::to_string)
                })
        });
    let versatile = str_of(el.pointer("/data/versatile_damage"))
        .or_else(|| setter_value(el, "versatile"))
        .or_else(|| detail("versatile"));
    // Tags are derived; the model's other tags (weapon groups, mastery ids) stay.
    supports_mut(el).retain(|t| {
        let t = t.as_str().unwrap_or_default();
        !(t.starts_with("ID_INTERNAL_WEAPON_CATEGORY_")
            || t.starts_with("ID_INTERNAL_WEAPON_PROPERTY_")
            || t.starts_with("ID_INTERNAL_DAMAGE_TYPE_"))
    });
    if let Some((_, tag, _, _)) = cat {
        add_support(el, tag);
    }
    if !dtype.is_empty() {
        add_support(
            el,
            &format!("ID_INTERNAL_DAMAGE_TYPE_{}", dtype.to_uppercase()),
        );
    }
    for p in &props {
        let tag = known_property(p).map(str::to_string).or_else(|| {
            own.iter()
                .find(|(n, _)| property_key(n) == property_key(p))
                .map(|(_, id)| id.clone())
        });
        if let Some(t) = tag {
            add_support(el, &t);
        }
    }
    if range.is_some() {
        add_support(el, RANGE_TAG);
    }
    let two = supports_mut(el)
        .iter()
        .any(|t| t.as_str() == Some(TWO_HANDED));
    put_setter(el, "category", "Weapons", None);
    put_setter(el, "slot", if two { "twohand" } else { "onehand" }, None);
    if !dice.is_empty() {
        put_setter(el, "damage", &dice, Some(json!({"type": dtype})));
    }
    if let Some(r) = &range {
        put_setter(el, "range", r, None);
    }
    if let Some(v) = &versatile {
        put_setter(el, "versatile", v, None);
    }
    if let Some((_, _, prof, _)) = cat {
        put_setter(el, "proficiency", prof, None);
    }
    cost_and_weight(el);
    let d = data_mut(el);
    d.insert("properties".into(), json!(props));
    if let Some((name, _, _, melee)) = cat {
        d.entry("weapon_category")
            .or_insert_with(|| json!(capitalise(name)));
        d.insert("melee".into(), json!(melee));
    }
    if !dice.is_empty() {
        d.insert("damage".into(), json!({"dice": dice, "type": dtype}));
    }
    if let Some(r) = range {
        d.insert("range".into(), json!(r));
    }
}

fn int_of(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().trim_start_matches('+').parse().ok(),
        _ => None,
    }
}

fn is_ac_rule(r: &Value) -> bool {
    r.get("kind").and_then(Value::as_str) == Some("stat")
        && r.get("name")
            .and_then(Value::as_str)
            .is_some_and(|n| n.starts_with("ac:"))
}

/// Armor: category tag, setters and the SRD AC rules.
fn normalize_armor(el: &mut Value) {
    let named = str_of(el.pointer("/data/armor_category"))
        .or_else(|| setter_value(el, "armor"))
        .unwrap_or_default()
        .to_lowercase();
    let Some(&(cat, tag, prof, slot)) = ARMOR.iter().find(|(n, ..)| named.contains(n)) else {
        return;
    };
    let base = int_of(el.pointer("/data/base_ac"))
        .or_else(|| int_of(setter(el, "armorClass").and_then(|s| s.get("value"))));
    let dex = el
        .pointer("/data/dex_modifier")
        .and_then(Value::as_bool)
        .unwrap_or(matches!(cat, "light" | "medium"));
    let cap = int_of(el.pointer("/data/dex_cap"));
    let strength = int_of(el.pointer("/data/strength_requirement"));
    let stealth = el
        .pointer("/data/stealth_disadvantage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    supports_mut(el).retain(|t| {
        !t.as_str()
            .unwrap_or_default()
            .starts_with("ID_INTERNAL_ARMOR_GROUP_")
    });
    add_support(el, tag);
    put_setter(el, "category", "Armor", None);
    put_setter(el, "slot", slot, None);
    put_setter(el, "armor", &capitalise(cat), None);
    put_setter(el, "proficiency", prof, None);
    if let Some(s) = strength {
        put_setter(el, "strength", &s.to_string(), None);
    }
    if stealth {
        put_setter(el, "stealth", "Disadvantage", None);
    }
    cost_and_weight(el);
    let mut rules: Vec<Value> = Vec::new();
    if let Some(b) = base {
        let text = match (cat, dex, cap) {
            ("shield", ..) => format!("+{b}"),
            (_, true, Some(c)) => format!("{b} + Dex (max {c})"),
            (_, true, None) => format!("{b} + Dex"),
            _ => b.to_string(),
        };
        put_setter(el, "armorClass", &text, None);
        if cat == "shield" {
            rules.push(json!({"kind": "stat", "name": "ac:shield", "value": b.to_string(), "bonus": "shield"}));
        } else {
            // Dex (light, medium) and the medium cap of 2 come from the core armored-AC rules
            // by armor group, as for every bundled armor; repeating them counts Dex twice.
            rules.push(json!({"kind": "stat", "name": "ac:armored:armor", "value": b.to_string()}));
        }
    }
    if stealth {
        rules.push(json!({"kind": "grant", "id": STEALTH_DISADVANTAGE, "type": "Grants"}));
    }
    let o = obj(el);
    let list = o.entry("rules").or_insert_with(|| json!([]));
    if let Some(arr) = list.as_array_mut() {
        if !rules.is_empty() {
            arr.retain(|r| {
                !is_ac_rule(r) && r.get("id").and_then(Value::as_str) != Some(STEALTH_DISADVANTAGE)
            });
        }
        arr.extend(rules);
    }
    let d = data_mut(el);
    d.insert("armor_category".into(), json!(capitalise(cat)));
}

/// Ammunition and gear: category setters (and the ammunition flag).
fn normalize_gear(el: &mut Value, ammunition: bool) {
    let ammo = ammunition
        || el
            .pointer("/data/ammunition")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if ammo {
        put_setter(el, "category", "Ammunition", None);
        put_setter(el, "type", "Ammunition", None);
        data_mut(el).insert("ammunition".into(), json!(true));
    } else if setter(el, "category").is_none() {
        put_setter(el, "category", "Adventuring Gear", None);
    }
    cost_and_weight(el);
}

fn normalize_property(el: &mut Value, name: &str) {
    obj(el).insert("supports".into(), json!(["Weapon Property"]));
    data_mut(el).insert("property".into(), json!(property_key(name)));
}

/// `data` keys the equipment shape reads (or the bundled packs carry), per element type.
const WEAPON_KEYS: [&str; 13] = [
    "weapon_category",
    "weapon_group",
    "melee",
    "damage",
    "properties",
    "versatile_damage",
    "range",
    "ammunition_type",
    "cost",
    "weight",
    "mastery",
    "proficiency",
    "columns",
];
const ARMOR_KEYS: [&str; 9] = [
    "armor_category",
    "base_ac",
    "dex_modifier",
    "dex_cap",
    "strength_requirement",
    "stealth_disadvantage",
    "cost",
    "weight",
    "columns",
];
const GEAR_KEYS: [&str; 6] = [
    "cost",
    "weight",
    "ammunition",
    "amount",
    "ammunition_for",
    "columns",
];

fn label_of(key: &str) -> String {
    capitalise(key.replace('_', " ").trim())
}

fn cell_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Table columns no field of the shape holds (`data.columns`, and any other `data` key the
/// shape does not read) are kept as "Column: value" lines of the description, never dropped.
fn keep_columns(el: &mut Value, known: &[&str]) {
    let Some(data) = el.get("data").and_then(Value::as_object) else {
        return;
    };
    let mut cells: Vec<(String, String)> = data
        .get("columns")
        .and_then(Value::as_object)
        .map(|c| {
            c.iter()
                .filter_map(|(k, v)| Some((k.trim().to_string(), cell_text(v)?)))
                .collect()
        })
        .unwrap_or_default();
    cells.extend(
        data.iter()
            .filter(|(k, _)| !known.contains(&k.as_str()))
            .filter_map(|(k, v)| Some((label_of(k), cell_text(v)?))),
    );
    let mut desc = el
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let lower = desc.to_lowercase();
    let mut added = false;
    for (label, value) in cells {
        if label.is_empty() || lower.contains(&format!("{}:", label.to_lowercase())) {
            continue;
        }
        desc.push_str(&format!("<p><strong>{label}:</strong> {value}</p>"));
        added = true;
    }
    if added {
        obj(el).insert("description".into(), json!(desc));
    }
}

/// Properties whose printed cell carries the weapon's range ("Ammunition (range 80/320)").
const RANGED_PROPERTIES: [&str; 2] = ["ammunition", "thrown"];

/// The label of the properties line [`keep_properties`] writes.
const PROPERTIES_LINE: &str = "<p><strong>Properties:</strong> ";

/// The row's printed properties cell (`data.columns`, any case of "Properties").
fn printed_properties(el: &Value) -> Option<String> {
    el.pointer("/data/columns")
        .and_then(Value::as_object)?
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("properties"))
        .and_then(|(_, v)| cell_text(v))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The normalised property names, with the range on the properties that print it.
fn listed_properties(el: &Value) -> Option<String> {
    let listed: Vec<String> = el
        .pointer("/data/properties")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|v| str_of(Some(v)))
        .collect();
    if listed.is_empty() {
        return None;
    }
    let range = str_of(el.pointer("/data/range"));
    let cells: Vec<String> = listed
        .into_iter()
        .map(|p| match &range {
            Some(r)
                if !p.contains('(') && RANGED_PROPERTIES.contains(&property_key(&p).as_str()) =>
            {
                format!("{p} (range {r})")
            }
            _ => p,
        })
        .collect();
    Some(cells.join(", "))
}

/// A weapon whose description has no text (a table row with no prose of its own) keeps its
/// row's properties cell as a "Properties:" line: property details such as a new property's
/// score ("Kick (12)") are printed nowhere else. The cell is kept as printed (`data.columns`);
/// only a row without it gets the normalised names. A description that is only an earlier
/// normalised line gets the printed cell. Returns a note when it changed.
pub(crate) fn keep_properties(el: &mut Value) -> Option<String> {
    let desc = el
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let printed = printed_properties(el);
    let ours = desc
        .trim()
        .strip_prefix(PROPERTIES_LINE)
        .and_then(|r| r.strip_suffix("</p>"))
        .filter(|r| !r.contains("<p>"));
    let (kept, cells) = match (ours, &printed) {
        (Some(line), Some(cell)) if line != cell => (String::new(), cell.clone()),
        _ if !super::fidelity::plain(&desc).trim().is_empty() => return None,
        _ => (desc, printed.or_else(|| listed_properties(el))?),
    };
    let line = format!("{PROPERTIES_LINE}{cells}</p>");
    obj(el).insert("description".into(), json!(format!("{kept}{line}")));
    Some(format!("description: the row's properties kept ({cells})"))
}

/// Magic item kinds as printed in an item's subtitle ("Wondrous item, rare") and the `type`
/// setter the bundled packs give them.
const MAGIC_KINDS: [(&str, &str); 9] = [
    ("wondrous item", "Wondrous item"),
    ("ring", "Ring"),
    ("rod", "Rod"),
    ("staff", "Staff"),
    ("wand", "Wand"),
    ("potion", "Potion"),
    ("scroll", "Scroll"),
    ("weapon", "Weapon"),
    ("armor", "Armor"),
];
/// The bundled packs' magic item `category` setters and the `type` they go with.
const MAGIC_CATEGORIES: [(&str, &str); 8] = [
    ("wondrous items", "Wondrous item"),
    ("rings", "Ring"),
    ("rods", "Rod"),
    ("staffs", "Staff"),
    ("wands", "Wand"),
    ("potions", "Potion"),
    ("spell scrolls", "Scroll"),
    ("scrolls", "Scroll"),
];
/// Leading paragraphs of a description searched for the subtitle.
const SUBTITLE_PARAGRAPHS: usize = 2;

/// `(type, base item)` of a subtitle line: "Weapon (longbow), rare" -> ("Weapon", "longbow").
fn subtitle_type(line: &str) -> Option<(&'static str, Option<String>)> {
    let t = line.trim().trim_start_matches(['*', '_']).trim_start();
    let low = t.to_lowercase();
    MAGIC_KINDS.iter().find_map(|(word, value)| {
        let rest = low.strip_prefix(word)?;
        if !rest.starts_with([',', ' ']) {
            return None;
        }
        let rest = rest.trim_start();
        if let Some(inner) = rest.strip_prefix('(') {
            // The bundled packs write the base item lowercase ("longsword").
            let base = inner.split(')').next().unwrap_or("").trim();
            return Some((*value, (!base.is_empty()).then(|| base.to_string())));
        }
        rest.starts_with(',').then_some((*value, None))
    })
}

/// A magic item with no `type` setter gets one: from its printed subtitle (in its description,
/// else its located passage), else from its `category` setter.
pub(crate) fn magic_item_type(el: &mut Value, name: &str, plan: &ImportPlan) {
    if setter(el, "type").is_some() {
        return;
    }
    let desc = el.get("description").and_then(Value::as_str).unwrap_or("");
    let from_desc = desc
        .split_inclusive("</p>")
        .take(SUBTITLE_PARAGRAPHS)
        .find_map(|p| subtitle_type(&super::fidelity::plain(p)));
    let from_passage = || {
        let snippet = &plan
            .elements
            .iter()
            .find(|p| p.name == name && p.ty == "Magic Item")?
            .excerpt
            .as_ref()?
            .snippet;
        let t = snippet.trim_start_matches(['#', ' ']);
        let rest = t.get(name.len()..).filter(|_| {
            t.get(..name.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(name))
        })?;
        subtitle_type(rest)
    };
    let from_category = || {
        let cat = setter_value(el, "category")?.to_lowercase();
        MAGIC_CATEGORIES
            .iter()
            .find(|(c, _)| *c == cat.trim())
            .map(|(_, v)| (*v, None))
    };
    let Some((value, base)) = from_desc.or_else(from_passage).or_else(from_category) else {
        return;
    };
    put_setter(el, "type", value, base.map(|b| json!({"addition": b})));
}

/// The `data` keys the equipment shape of `ty` reads.
fn known_keys(ty: &str) -> Option<&'static [&'static str]> {
    match ty {
        "Weapon" => Some(&WEAPON_KEYS),
        "Armor" => Some(&ARMOR_KEYS),
        "Item" => Some(&GEAR_KEYS),
        _ => None,
    }
}

/// A table-row weapon, armor or item gets its printed row as its column lines
/// ([`row::keep_printed_row`]); a weapon whose row is not located keeps the properties fallback
/// ([`keep_properties`]). Run at finalize, where the source is at hand. Returns notes.
pub(crate) fn printed_row_fixes(
    el: &mut Value,
    planned: Option<&PlannedElement>,
    source: &str,
) -> Vec<String> {
    let ty = el
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some(known) = known_keys(&ty) else {
        return Vec::new();
    };
    let name = el
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match planned.and_then(|p| row::printed_row(source, p)) {
        Some(r) => row::keep_printed_row(el, &name, known, &r),
        None if ty == "Weapon" => keep_properties(el).into_iter().collect(),
        None => Vec::new(),
    }
}

/// The SRD shape of a magic armor's or shield's own "+N to AC" ("Armor, +1", "Arrow-Catching
/// Shield"): the armored enhancement bonus. The model sometimes writes it as a plain `ac` stat.
const ENHANCEMENT_STAT: &str = "ac:armored:enhancement";

/// A Magic Item whose `type` setter is Armor and whose one AC rule is a bare `stat ac N`
/// (no condition) gets the SRD enhancement rule instead (bonus `enhancement`, a shield's
/// `enhancement:shield`). Returns a note when it changed.
pub(crate) fn armor_enhancement(el: &mut Value) -> Option<String> {
    let ty = setter(el, "type")?;
    if str_of(ty.get("value"))?.to_lowercase() != "armor" {
        return None;
    }
    let shield = str_of(ty.pointer("/attributes/addition"))
        .into_iter()
        .chain(setter_value(el, "armor"))
        .any(|v| v.to_lowercase().contains("shield"));
    let rules = el.get("rules")?.as_array()?;
    if rules
        .iter()
        .any(|r| r.get("name").and_then(Value::as_str) == Some(ENHANCEMENT_STAT))
    {
        return None;
    }
    let bare = |r: &Value| {
        r.get("kind").and_then(Value::as_str) == Some("stat")
            && r.get("name").and_then(Value::as_str) == Some("ac")
            && r.as_object().is_some_and(|o| {
                o.keys()
                    .all(|k| matches!(k.as_str(), "kind" | "name" | "value"))
            })
    };
    let found: Vec<usize> = (0..rules.len()).filter(|&i| bare(&rules[i])).collect();
    let [i] = found[..] else {
        return None;
    };
    let n = int_of(rules[i].get("value")).filter(|n| *n > 0)?;
    let bonus = if shield {
        "enhancement:shield"
    } else {
        "enhancement"
    };
    let rule =
        json!({"kind": "stat", "name": ENHANCEMENT_STAT, "value": n.to_string(), "bonus": bonus});
    el.get_mut("rules")?.as_array_mut()?[i] = rule;
    Some(format!(
        "rules: the armor's +{n} to AC is its {ENHANCEMENT_STAT} bonus"
    ))
}

/// Apply the equipment shape to a standalone element of type `ty`.
pub(crate) fn normalize_item(
    el: &mut Value,
    ty: &str,
    name: &str,
    item_type: Option<ItemType>,
    plan: &ImportPlan,
) {
    match ty {
        "Weapon" => {
            normalize_weapon(el, &planned_properties(plan));
            keep_properties(el);
            keep_columns(el, &WEAPON_KEYS);
        }
        "Armor" => {
            normalize_armor(el);
            keep_columns(el, &ARMOR_KEYS);
        }
        "Item" => {
            normalize_gear(el, item_type == Some(ItemType::Ammunition));
            keep_columns(el, &GEAR_KEYS);
        }
        "Weapon Property" => normalize_property(el, name),
        "Magic Item" => {
            magic_item_type(el, name, plan);
            armor_enhancement(el);
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
