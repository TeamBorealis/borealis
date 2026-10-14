//! Mundane equipment in the shape the bundled SRD packs use, so the engine reads it: a `Weapon`
//! becomes an attack (damage, properties, range, proficiency), `Armor` sets AC, ammunition and
//! gear are `Item`s. The model writes the stats (in `data`, or setters copied from the example);
//! the setters, `supports` tags and AC rules are derived here, deterministically.

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use super::types::{ImportPlan, ItemType};

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
        "Magic Item" => magic_item_type(el, name, plan),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magic_item_without_a_type_setter_gets_one_from_its_subtitle_or_category() {
        let plan: ImportPlan = serde_json::from_value(json!({
            "edition": "2024", "groups": [], "elements": [], "notes": []}))
        .unwrap();
        let type_of = |el: &Value| {
            let s = setter(el, "type").cloned().unwrap_or(Value::Null);
            (
                s.get("value").and_then(Value::as_str).map(str::to_string),
                s.pointer("/attributes/addition")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            )
        };
        let mut wondrous = json!({"description": "<p><em>Wondrous item, rare (requires attunement)</em></p><p>A lamp.</p>",
            "setters": [{"name": "rarity", "value": "Rare"}]});
        magic_item_type(&mut wondrous, "Moth Lamp", &plan);
        assert_eq!(type_of(&wondrous), (Some("Wondrous item".into()), None));
        let mut blade =
            json!({"description": "<p>Weapon (longsword), very rare</p><p>It hums.</p>"});
        magic_item_type(&mut blade, "Hum Blade", &plan);
        assert_eq!(
            type_of(&blade),
            (Some("Weapon".into()), Some("longsword".into()))
        );
        let mut ring = json!({"description": "<p>A ring of reeds.</p>",
            "setters": [{"name": "category", "value": "Rings"}]});
        magic_item_type(&mut ring, "Reed Ring", &plan);
        assert_eq!(type_of(&ring), (Some("Ring".into()), None));
        // The model's own setter stays.
        let mut kept = json!({"description": "<p>Wand, rare</p>",
            "setters": [{"name": "type", "value": "Rod"}]});
        magic_item_type(&mut kept, "Odd Rod", &plan);
        assert_eq!(type_of(&kept), (Some("Rod".into()), None));
    }

    #[test]
    fn property_names_match_bundled_tags_with_details() {
        assert_eq!(
            known_property("Versatile (1d10)"),
            Some("ID_INTERNAL_WEAPON_PROPERTY_VERSATILE")
        );
        assert_eq!(known_property("two handed"), Some(TWO_HANDED));
        assert_eq!(known_property("Sparkburst"), None);
        assert_eq!(
            known_property("Ammunition (range 80/320)"),
            Some("ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION")
        );
        assert_eq!(
            known_property("Special (Net)"),
            Some("ID_INTERNAL_WEAPON_PROPERTY_SPECIAL_NET")
        );
        // Every core weapon property is known by its own name, nothing else is built in.
        for (name, id) in core_properties() {
            assert!(known_property(name).is_some(), "{name} ({id})");
        }
        assert!(core_properties().len() >= 15);
        assert_eq!(
            property_detail("Ammunition (range 80/320)").unwrap(),
            "range 80/320"
        );
    }

    #[test]
    fn unmapped_table_columns_stay_in_the_description() {
        let mut el = json!({"description": "<p>A cudgel.</p>", "data": {
            "weapon_category": "Simple Melee", "damage": {"dice": "1d4", "type": "bludgeoning"},
            "cost": {"value": 1, "currency": "sp"}, "craft_dc": 10,
            "columns": {"Material": "Oak/Iron", "Notes": ""}}});
        keep_columns(&mut el, &WEAPON_KEYS);
        let d = el["description"].as_str().unwrap().to_string();
        assert!(d.contains("<strong>Material:</strong> Oak/Iron"), "{d}");
        assert!(d.contains("<strong>Craft dc:</strong> 10"), "{d}");
        assert!(!d.contains("Notes"), "{d}");
        assert!(!d.contains("Weapon category"), "{d}");
        // Idempotent: a second pass adds nothing.
        keep_columns(&mut el, &WEAPON_KEYS);
        assert_eq!(el["description"].as_str().unwrap(), d);
    }

    #[test]
    fn armor_gets_srd_ac_rules() {
        let mut el = json!({"data": {"armor_category": "Medium", "base_ac": 13, "dex_cap": 2,
            "cost": "40 gp", "weight": 18}});
        normalize_armor(&mut el);
        assert_eq!(setter_value(&el, "armorClass").unwrap(), "13 + Dex (max 2)");
        assert_eq!(setter_value(&el, "slot").unwrap(), "body");
        assert_eq!(setter_value(&el, "cost").unwrap(), "40");
        let rules = el["rules"].as_array().unwrap();
        assert!(rules.iter().any(|r| r["name"] == "ac:armored:armor"));
        // Dex and its cap come from the core rules by group; never a second time from the item.
        assert!(!rules.iter().any(|r| r["name"]
            .as_str()
            .unwrap_or("")
            .starts_with("ac:armored:dexterity")));
        assert_eq!(el["supports"], json!(["ID_INTERNAL_ARMOR_GROUP_MEDIUM"]));
    }
}
