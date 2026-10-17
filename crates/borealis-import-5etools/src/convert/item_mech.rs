//! Item rules beyond the base weapon/armor stats: weapon attack/damage bonuses, 2024 weapon
//! mastery data, flat ability bonuses, speed changes and charges.

use super::super::build::{custom, stat, stat_bonus};
use super::super::mech::Mech;
use crate::core_ids;
use crate::ids::IdKind;
use crate::model::Item;
use crate::util::id_fragment;
use borealis_content::{Element, Rule};
use serde_json::{json, Value};

use super::signed_value;

/// The eight mastery properties of the 2024 rules, bundled as `ID_2024_MASTERY_<NAME>`.
const BUNDLED_MASTERIES: [&str; 8] = [
    "cleave", "graze", "nick", "push", "sap", "slow", "topple", "vex",
];

const SPEED_MODES: [&str; 5] = ["walk", "fly", "climb", "swim", "burrow"];

fn int_of(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => signed_value(s),
        _ => None,
    }
}

/// `bonusWeaponAttack` / `bonusWeaponDamage`: the per-weapon `<name>:attack` / `<name>:damage`
/// statistics the attack line adds.
pub fn weapon_bonus_rules(it: &Item, mech: &mut Mech<'_>) {
    let lname = it.name.to_lowercase();
    for (key, part) in [
        ("bonusWeaponAttack", "attack"),
        ("bonusWeaponDamage", "damage"),
    ] {
        if let Some(n) = it.extra.get(key).and_then(int_of) {
            mech.push(Rule::Stat(stat(&format!("{lname}:{part}"), n)));
        }
    }
}

/// The mastery property name and source of a `mastery` entry (`"Topple|XPHB"` or
/// `{"uid": "Vex|XPHB", ...}`).
fn mastery_ref(v: &Value) -> Option<(&str, &str)> {
    let uid = match v {
        Value::String(s) => s.as_str(),
        other => other.get("uid")?.as_str()?,
    };
    let mut parts = uid.split('|');
    let name = parts.next()?.trim();
    let source = parts.next().map_or("XPHB", str::trim);
    (!name.is_empty()).then_some((name, source))
}

/// Mastery names of a weapon entry.
pub fn mastery_names(it: &Item) -> Vec<String> {
    it.mastery
        .iter()
        .filter_map(mastery_ref)
        .map(|(n, _)| n.to_string())
        .collect()
}

/// The element id of a mastery property: the bundled 2024 element for the eight core
/// properties, else the imported `Mastery Property` element (same id `convert_mastery` gives it).
fn mastery_id(ids: &mut crate::ids::IdAlloc, name: &str, source: &str) -> String {
    let lower = name.to_lowercase();
    if BUNDLED_MASTERIES.contains(&lower.as_str()) {
        return format!("ID_2024_MASTERY_{}", id_fragment(&lower));
    }
    ids.id(
        IdKind::ItemProperty,
        source,
        &["Mastery", name],
        &format!("mastery|{name}"),
    )
}

/// `data.mastery_id` (the attack line's mastery) and, on a kind of weapon, `data.weapon_category`
/// and `data.melee` (what the Weapon Mastery choice offers). A weapon made from a base item (a
/// `+1 Longsword`) is not a kind of weapon of its own and is not offered, as in the bundled SRD.
pub fn mastery_data(ids: &mut crate::ids::IdAlloc, it: &Item, code: &str, el: &mut Element) {
    let Some((name, source)) = it.mastery.iter().find_map(mastery_ref) else {
        return;
    };
    el.data
        .insert("mastery_id".into(), json!(mastery_id(ids, name, source)));
    if it.extra.contains_key("baseItem") {
        return;
    }
    let melee = code != "R";
    let category = it.weapon_category.as_deref().unwrap_or("simple");
    let category = format!(
        "{} {}",
        crate::util::title_case(category),
        if melee { "Melee" } else { "Ranged" }
    );
    el.data.insert("weapon_category".into(), json!(category));
    el.data.insert("melee".into(), json!(melee));
}

/// Flat ability bonuses (`ability: {con: 2}`); `static` (set a score) is converted elsewhere.
pub fn ability_bonus_rules(it: &Item, mech: &mut Mech<'_>, partial: &mut Vec<String>) {
    let Some(Value::Object(map)) = it.extra.get("ability") else {
        return;
    };
    // a chosen ability: `choose: [{from, count, amount}]`, or the object itself (`from`, ...)
    let mut choices: Vec<Value> = match map.get("choose") {
        Some(Value::Array(list)) => list.clone(),
        Some(one) => vec![one.clone()],
        None => Vec::new(),
    };
    if map.contains_key("from") {
        choices.push(Value::Object(map.clone()));
    }
    for c in choices {
        mech.ability(
            &[json!({ "choose": c })],
            super::super::mech::AbilitySource::Feat,
        );
    }
    for (key, v) in map {
        match key.as_str() {
            "static" | "choose" | "from" | "count" | "amount" => {}
            _ => match (core_ids::ability_stat(key), int_of(v)) {
                (Some(name), Some(n)) => mech.push(Rule::Stat(stat(&name, n))),
                _ => partial.push(format!("ability entry {key:?} was not converted")),
            },
        }
    }
}

fn innate_group(mode: &str) -> String {
    if mode == "walk" {
        "innate speed".into()
    } else {
        format!("innate speed:{mode}")
    }
}

fn misc_group(mode: &str) -> String {
    if mode == "walk" {
        "speed:misc".into()
    } else {
        format!("speed:{mode}:misc")
    }
}

/// `modifySpeed`: `static` (the speed becomes at least n), `equal` (a mode equals another),
/// `bonus` (+n).
pub fn speed_rules(it: &Item, mech: &mut Mech<'_>, partial: &mut Vec<String>) {
    let Some(Value::Object(how_map)) = it.extra.get("modifySpeed") else {
        return;
    };
    for (how, modes) in how_map {
        for (mode, v) in modes.as_object().into_iter().flatten() {
            let known = |m: &str| SPEED_MODES.contains(&m);
            let rule = match (how.as_str(), v) {
                ("static", n) if known(mode) => {
                    int_of(n).map(|n| stat_bonus(&innate_group(mode), n, "base"))
                }
                ("equal", Value::String(other)) if known(mode) && known(other) => {
                    Some(stat_bonus(&innate_group(mode), innate_group(other), "base"))
                }
                ("bonus", n) if known(mode) => {
                    int_of(n).map(|n| Rule::Stat(stat(&misc_group(mode), n)))
                }
                // every speed the character has: the walking speed is the one always present
                ("bonus", n) if mode == "*" => {
                    partial.push("speed bonus applied to the walking speed only".into());
                    int_of(n).map(|n| Rule::Stat(stat(&misc_group("walk"), n)))
                }
                _ => None,
            };
            match rule {
                Some(r) => mech.push(r),
                None => partial.push(format!("speed change {how} {mode} was not converted")),
            }
        }
    }
}

/// Plain text of a `rechargeAmount` (`{@dice 1d6 + 1}` -> `1d6 + 1`).
fn amount_text(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(crate::text::strip_tags(s)),
        _ => None,
    }
}

/// `charges` -> a `resource` the size of the charges; `recharge` / `rechargeAmount` set how it
/// comes back (a full daily recharge counts as a long rest, a partial one is described).
pub fn charges_rule(it: &Item, el: &Element, partial: &mut Vec<String>) -> Option<Rule> {
    let raw = it.extra.get("charges")?;
    let Some(max) = raw.as_i64().filter(|n| *n > 0) else {
        partial.push(format!("charges {raw} were not converted"));
        return None;
    };
    let when = it.extra.get("recharge").and_then(Value::as_str);
    let amount = it.extra.get("rechargeAmount").and_then(amount_text);
    let moment = match when {
        Some("dawn") => Some("daily at dawn"),
        Some("dusk") => Some("daily at dusk"),
        Some("midnight") => Some("daily at midnight"),
        Some("restLong") => Some("on a long rest"),
        Some("restShort") => Some("on a short rest"),
        _ => None,
    };
    let (recharge, alt) = match (when, moment, amount) {
        (Some("restShort"), _, None) => ("short_rest", None),
        (_, Some(m), None) => (
            "long_rest",
            (when != Some("restLong")).then(|| format!("Regains all charges {m}")),
        ),
        (_, Some(m), Some(a)) => ("none", Some(format!("Regains {a} charges {m}"))),
        (Some(other), None, _) => ("none", Some(format!("Recharge: {other}"))),
        (None, None, _) => ("none", None),
    };
    let mut fields = json!({
        "id": el.id,
        "name": format!("{} charges", el.name),
        "max": {"type": "fixed", "value": max},
        "recharge": [recharge],
    });
    if let Some(text) = alt {
        fields["alt_regain"] = json!(text);
    }
    Some(custom("resource", fields))
}

#[cfg(test)]
mod tests {
    use super::super::super::Shared;
    use super::*;

    fn item(raw: Value) -> Item {
        crate::model::parse(&raw).unwrap()
    }

    fn stats(rules: &[Rule]) -> Vec<(String, String, Option<String>)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Stat(s) => Some((s.name.clone(), s.value.clone(), s.bonus.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn weapon_attack_and_damage_bonuses_become_named_stats() {
        let sh = Shared::default();
        let mut mech = Mech::new(&sh, "x");
        let it = item(json!({"name": "Glimmer Pick", "source": "ZZZ",
            "bonusWeaponAttack": "+2", "bonusWeaponDamage": 1}));
        weapon_bonus_rules(&it, &mut mech);
        assert_eq!(
            stats(&mech.rules),
            vec![
                ("glimmer pick:attack".into(), "2".into(), None),
                ("glimmer pick:damage".into(), "1".into(), None)
            ]
        );
    }

    #[test]
    fn mastery_data_links_bundled_or_imported_property() {
        let mut ids = crate::ids::IdAlloc::default();
        let mut el = Element::default();
        let it = item(json!({"name": "Glimmer Pick", "source": "ZZZ", "type": "M",
            "weaponCategory": "martial", "mastery": ["Topple|XPHB"]}));
        mastery_data(&mut ids, &it, "M", &mut el);
        assert_eq!(el.data["mastery_id"], "ID_2024_MASTERY_TOPPLE");
        assert_eq!(el.data["weapon_category"], "Martial Melee");
        assert_eq!(el.data["melee"], true);
        let mut el = Element::default();
        let it = item(json!({"name": "Odd Sling", "source": "ZZZ", "type": "R",
            "mastery": [{"uid": "Ricochet|ZZZ", "note": "n"}]}));
        mastery_data(&mut ids, &it, "R", &mut el);
        assert_eq!(
            el.data["mastery_id"],
            "ID_5ET_ZZZ_ITEMPROP_MASTERY_RICOCHET"
        );
        assert_eq!(el.data["weapon_category"], "Simple Ranged");
        assert_eq!(el.data["melee"], false);
        assert_eq!(mastery_names(&it), vec!["Ricochet".to_string()]);
        // a weapon made from a base item keeps its mastery but is not a kind of its own
        let mut el = Element::default();
        let it = item(
            json!({"name": "+1 Glimmer Pick", "source": "ZZZ", "type": "M",
            "weaponCategory": "martial", "mastery": ["Vex|XPHB"], "baseItem": "glimmer pick|zzz"}),
        );
        mastery_data(&mut ids, &it, "M", &mut el);
        assert_eq!(el.data["mastery_id"], "ID_2024_MASTERY_VEX");
        assert!(!el.data.contains_key("weapon_category") && !el.data.contains_key("melee"));
        let mut el = Element::default();
        mastery_data(
            &mut ids,
            &item(json!({"name": "Plain", "source": "ZZZ"})),
            "M",
            &mut el,
        );
        assert!(el.data.is_empty());
    }

    #[test]
    fn flat_ability_bonuses_and_choices() {
        let sh = Shared::default();
        let mut mech = Mech::new(&sh, "x");
        let mut partial = Vec::new();
        let it = item(json!({"name": "Band", "source": "ZZZ",
            "ability": {"con": 2, "static": {"str": 19}, "choose": [{"from": ["str", "dex"], "count": 1}]}}));
        ability_bonus_rules(&it, &mut mech, &mut partial);
        assert_eq!(
            stats(&mech.rules),
            vec![("constitution".into(), "2".into(), None)]
        );
        assert!(partial.is_empty(), "{partial:?}");
        assert!(mech
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Select(s) if s.ty == "Ability Score Improvement")));
        // +2 to one ability of your choice, written at the top level or under `choose`
        for ability in [
            json!({"from": ["str", "dex", "con", "int", "wis", "cha"], "count": 1, "amount": 2}),
            json!({"choose": [{"from": ["str", "dex", "con", "int", "wis", "cha"], "count": 1, "amount": 2}]}),
        ] {
            let mut mech = Mech::new(&sh, "x");
            let it = item(json!({"name": "Tome", "source": "ZZZ", "ability": ability}));
            ability_bonus_rules(&it, &mut mech, &mut partial);
            assert!(
                matches!(&mech.rules[..], [Rule::Custom(c)] if c.kind == "ability_increase"),
                "{:?}",
                mech.rules
            );
        }
    }

    #[test]
    fn speed_changes() {
        let sh = Shared::default();
        let mut mech = Mech::new(&sh, "x");
        let mut partial = Vec::new();
        let it = item(json!({"name": "Fins", "source": "ZZZ", "modifySpeed": {
            "static": {"fly": 60, "walk": 30}, "equal": {"climb": "walk"},
            "bonus": {"swim": 10, "*": 5}, "multiply": {"walk": 2}}}));
        speed_rules(&it, &mut mech, &mut partial);
        let got = stats(&mech.rules);
        let base = Some("base".to_string());
        assert!(got.contains(&("innate speed:fly".into(), "60".into(), base.clone())));
        assert!(got.contains(&("innate speed".into(), "30".into(), base.clone())));
        assert!(got.contains(&("innate speed:climb".into(), "innate speed".into(), base)));
        assert!(got.contains(&("speed:swim:misc".into(), "10".into(), None)));
        assert!(got.contains(&("speed:misc".into(), "5".into(), None)));
        assert_eq!(got.len(), 5);
        // `*` is applied to walking only; `multiply` is not converted
        assert_eq!(partial.len(), 2);
    }

    fn resource(raw: Value) -> (Option<Rule>, Vec<String>) {
        let it = item(raw);
        let el = Element {
            id: "ID_TEST_WAND".into(),
            name: "Test Wand".into(),
            ..Element::default()
        };
        let mut partial = Vec::new();
        (charges_rule(&it, &el, &mut partial), partial)
    }

    fn fields(r: Option<Rule>) -> serde_json::Map<String, Value> {
        match r {
            Some(Rule::Custom(c)) if c.kind == "resource" => c.fields,
            other => panic!("not a resource: {other:?}"),
        }
    }

    #[test]
    fn charges_become_a_resource() {
        let (r, _) = resource(json!({"name": "W", "source": "ZZZ", "charges": 7,
            "recharge": "dawn", "rechargeAmount": "{@dice 1d6 + 1}"}));
        let f = fields(r);
        assert_eq!(f["id"], "ID_TEST_WAND");
        assert_eq!(f["max"], json!({"type": "fixed", "value": 7}));
        assert_eq!(f["recharge"], json!(["none"]));
        assert_eq!(f["alt_regain"], "Regains 1d6 + 1 charges daily at dawn");
        let f = fields(
            resource(json!({"name": "W", "source": "ZZZ", "charges": 3, "recharge": "dawn"})).0,
        );
        assert_eq!(f["recharge"], json!(["long_rest"]));
        assert_eq!(f["alt_regain"], "Regains all charges daily at dawn");
        let f = fields(
            resource(json!({"name": "W", "source": "ZZZ", "charges": 2, "recharge": "restShort"}))
                .0,
        );
        assert_eq!(f["recharge"], json!(["short_rest"]));
        assert!(!f.contains_key("alt_regain"));
        let f = fields(resource(json!({"name": "W", "source": "ZZZ", "charges": 5})).0);
        assert_eq!(f["recharge"], json!(["none"]));
        let (r, partial) =
            resource(json!({"name": "W", "source": "ZZZ", "charges": "{@dice 1d4}"}));
        assert!(r.is_none());
        assert_eq!(partial.len(), 1);
        assert!(resource(json!({"name": "W", "source": "ZZZ"})).0.is_none());
    }

    fn convert(raw: Value) -> (Element, Vec<String>) {
        let sh = Shared::default();
        let mut st = super::super::super::State::default();
        let ent = super::super::super::Ent {
            name: raw["name"].as_str().unwrap().to_string(),
            source: raw["source"].as_str().unwrap().to_string(),
            ruleset: crate::source::ruleset_of_entry(&raw),
            is_srd: false,
            file: "items.json".into(),
            raw,
        };
        let conv = super::super::convert(&sh, &mut st, &ent);
        (conv.elements[0].clone(), conv.partial)
    }

    #[test]
    fn converted_items_carry_the_new_rules() {
        // a named martial weapon with no core base: category proficiency, mastery data
        let (el, _) = convert(
            json!({"name": "Glimmer Pick", "source": "ZZZ", "type": "M|XPHB",
            "edition": "one", "rarity": "rare", "weaponCategory": "martial", "dmg1": "1d8",
            "dmgType": "P", "mastery": ["Sap|XPHB"], "bonusWeapon": "+1", "bonusWeaponAttack": "+1"}),
        );
        assert_eq!(
            el.setter_value("proficiency"),
            Some("ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS")
        );
        assert_eq!(el.setter_value("enhancement"), Some("1"));
        assert_eq!(el.data["mastery_id"], "ID_2024_MASTERY_SAP");
        assert!(stats(&el.rules).contains(&("glimmer pick:attack".into(), "1".into(), None)));
        // a firearm gets no category fallback (firearms have their own proficiency)
        let (el, _) = convert(json!({"name": "Odd Musket", "source": "ZZZ", "type": "R",
            "weaponCategory": "martial", "firearm": true, "dmg1": "1d12", "dmgType": "P"}));
        assert!(el.setter_value("proficiency").is_none());
        // a +N shield adds to the shield's own AC
        let (el, _) = convert(json!({"name": "Ward Shield", "source": "ZZZ", "type": "S",
            "rarity": "rare", "ac": 2, "bonusAc": "+2"}));
        let got = stats(&el.rules);
        let shield = Some("shield".to_string());
        assert!(got.contains(&("ac:shield".into(), "2".into(), shield)));
        assert!(got.contains(&(
            "ac:shield".into(),
            "2".into(),
            Some("enhancement:shield".into())
        )));
        assert!(!got.iter().any(|(n, _, _)| n.starts_with("ac:armored")));
        // ability bonus, speed and charges on a wondrous item
        let (el, partial) = convert(
            json!({"name": "Odd Charm", "source": "ZZZ", "wondrous": true,
            "rarity": "rare", "ability": {"wis": 2}, "modifySpeed": {"static": {"swim": 40}},
            "charges": 4, "recharge": "dawn"}),
        );
        let got = stats(&el.rules);
        assert!(got.contains(&("wisdom".into(), "2".into(), None)));
        assert!(got.contains(&("innate speed:swim".into(), "40".into(), Some("base".into()))));
        assert!(el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Custom(c) if c.kind == "resource")));
        assert!(partial.is_empty(), "{partial:?}");
    }
}
