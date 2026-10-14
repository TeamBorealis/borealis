//! Mundane equipment in the shape the bundled SRD packs use, so the engine reads it: a `Weapon`
//! becomes an attack (damage, properties, range, proficiency), `Armor` sets AC, ammunition and
//! gear are `Item`s. The model writes the stats (in `data`, or setters copied from the example);
//! the setters, `supports` tags and AC rules are derived here, deterministically.

use serde_json::{json, Map, Value};

use super::types::{ImportPlan, ItemType};

/// Bundled weapon properties: printed name (lowercase) -> `supports` tag.
const KNOWN_PROPERTIES: [(&str, &str); 15] = [
    ("ammunition", "ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION"),
    ("finesse", "ID_INTERNAL_WEAPON_PROPERTY_FINESSE"),
    ("heavy", "ID_INTERNAL_WEAPON_PROPERTY_HEAVY"),
    ("light", "ID_INTERNAL_WEAPON_PROPERTY_LIGHT"),
    ("loading", "ID_INTERNAL_WEAPON_PROPERTY_LOADING"),
    ("range", "ID_INTERNAL_WEAPON_PROPERTY_RANGE"),
    ("reach", "ID_INTERNAL_WEAPON_PROPERTY_REACH"),
    ("thrown", "ID_INTERNAL_WEAPON_PROPERTY_THROWN"),
    ("two-handed", "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED"),
    ("versatile", "ID_INTERNAL_WEAPON_PROPERTY_VERSATILE"),
    ("improvised", "ID_INTERNAL_WEAPON_PROPERTY_IMPROVISED"),
    ("silvered", "ID_INTERNAL_WEAPON_PROPERTY_SILVERED"),
    ("special", "ID_INTERNAL_WEAPON_PROPERTY_SPECIAL"),
    ("reload", "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_RELOAD"),
    (
        "burst fire",
        "ID_INTERNAL_WEAPON_PROPERTY_FIREARM_BURST_FIRE",
    ),
];
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
    // No bundled firearm proficiency: martial ranged proficiency covers firearms.
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
        "Weapon" => "A mundane weapon, shaped like the example. Put its stats in `data`: weapon_category (Simple Melee, Simple Ranged, Martial Melee, Martial Ranged or Firearm), melee (true/false), damage {dice, type}, properties (EVERY property name as printed, new ones included, e.g. [\"Ammunition\", \"Loading\"]), versatile_damage, range (\"normal/long\" feet, ranged and thrown weapons), ammunition_type, cost {value, currency}, weight (pounds), and mastery for 2024. Setters, supports and proficiency are derived from `data`. Describe special rules of the weapon in `description`.",
        "Armor" => "Mundane armor or a shield, shaped like the example. Put its stats in `data`: armor_category (Light, Medium, Heavy or Shield), base_ac (the shield bonus for a shield), dex_modifier, dex_cap, strength_requirement, stealth_disadvantage, cost {value, currency}, weight (pounds). Setters, supports and AC rules are derived from `data`.",
        "Item" => "Mundane equipment (ammunition or adventuring gear), shaped like the example: description, and `data` with cost {value, currency}, weight (pounds); ammunition also data.ammunition true, amount (per bundle) and the weapon it is for in data.ammunition_for.",
        "Weapon Property" => "A weapon property: `description` in HTML paragraphs restating the rules of the property in your own words. No rules.",
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
fn property_key(name: &str) -> String {
    let head = name.split('(').next().unwrap_or(name);
    let k = head.trim().to_lowercase().replace('_', " ");
    if k == "two handed" || k == "twohanded" {
        "two-handed".into()
    } else {
        k
    }
}

/// The `supports` tag of a bundled property.
pub(crate) fn known_property(name: &str) -> Option<&'static str> {
    let k = property_key(name);
    KNOWN_PROPERTIES
        .iter()
        .find(|(n, _)| *n == k)
        .map(|(_, id)| *id)
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
    KNOWN_PROPERTIES
        .iter()
        .filter(|(_, id)| tags.contains(id))
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
            rules.push(json!({"kind": "stat", "name": "ac:armored:armor", "value": b.to_string()}));
            let dexv = if dex { "dexterity:modifier" } else { "0" };
            rules.push(json!({"kind": "stat", "name": "ac:armored:dexterity", "value": dexv}));
            if let Some(c) = cap.filter(|_| dex) {
                rules.push(json!({"kind": "stat", "name": "ac:armored:dexterity:cap", "value": c.to_string()}));
            }
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

/// Apply the equipment shape to a standalone element of type `ty`.
pub(crate) fn normalize_item(
    el: &mut Value,
    ty: &str,
    name: &str,
    item_type: Option<ItemType>,
    plan: &ImportPlan,
) {
    match ty {
        "Weapon" => normalize_weapon(el, &planned_properties(plan)),
        "Armor" => normalize_armor(el),
        "Item" => normalize_gear(el, item_type == Some(ItemType::Ammunition)),
        "Weapon Property" => normalize_property(el, name),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_names_match_bundled_tags_with_details() {
        assert_eq!(
            known_property("Versatile (1d10)"),
            Some("ID_INTERNAL_WEAPON_PROPERTY_VERSATILE")
        );
        assert_eq!(known_property("two handed"), Some(TWO_HANDED));
        assert_eq!(known_property("Sparkburst"), None);
        assert_eq!(
            property_detail("Ammunition (range 80/320)").unwrap(),
            "range 80/320"
        );
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
        assert!(rules
            .iter()
            .any(|r| r["name"] == "ac:armored:dexterity:cap"));
        assert_eq!(el["supports"], json!(["ID_INTERNAL_ARMOR_GROUP_MEDIUM"]));
    }
}
