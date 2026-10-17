//! Starting equipment of classes and backgrounds (`startingEquipment`, `defaultData`) as
//! `Equipment Package` elements and `equipment_package` rules (the shape of the bundled SRD
//! 5.2.1 classes and backgrounds, read by the engine's `packages`).
//!
//! 5etools groups: a `_` group is always given; a group keyed `a`/`b`/... (2014) or
//! `A`/`B`/... (2024) is a choice. The first choice group becomes the lettered options of the
//! owner's package, each with the `_` items added; every further choice group becomes a package
//! of its own (`... : choice 2`). Without a choice group the `_` items form one option `A`.

use super::build::{custom, gate};
use super::{new_element, Shared};
use crate::text::RefKind;
use crate::util::title_case;
use borealis_content::{Element, Rule};
use serde_json::{json, Map, Value};

/// 5etools' default source of an item uid without one.
const DEFAULT_ITEM_SOURCE: &str = "PHB";

/// Copper pieces per gold / silver piece.
const CP_PER_GP: u64 = 100;
const CP_PER_SP: u64 = 10;

/// Packages of one class or background.
#[derive(Debug, Default)]
pub struct Packages {
    /// `Equipment Package` elements.
    pub elements: Vec<Element>,
    /// `equipment_package` rules for the owner.
    pub rules: Vec<Rule>,
    /// What could not be converted.
    pub partial: Vec<String>,
}

/// The owner of the packages.
pub struct Owner<'a> {
    /// Element id.
    pub id: &'a str,
    /// Display name.
    pub name: &'a str,
    /// Source display name.
    pub source: &'a str,
    /// Ruleset.
    pub ruleset: borealis_content::Ruleset,
    /// Requirements of the rules (`!<multiclass id>`: only the first class gives equipment).
    pub gate: Option<&'a str>,
}

/// Display name of an `equipmentType` code.
fn equipment_type_name(code: &str) -> String {
    let name = match code {
        "weaponSimple" => "Simple Weapon",
        "weaponSimpleMelee" => "Simple Melee Weapon",
        "weaponSimpleRanged" => "Simple Ranged Weapon",
        "weaponMartial" => "Martial Weapon",
        "weaponMartialMelee" => "Martial Melee Weapon",
        "weaponMartialRanged" => "Martial Ranged Weapon",
        "instrumentMusical" => "Musical Instrument",
        "toolArtisan" => "Artisan's Tools",
        "setGaming" => "Gaming Set",
        "focusSpellcastingArcane" => "Arcane Focus",
        "focusSpellcastingDruidic" => "Druidic Focus",
        "focusSpellcastingHoly" => "Holy Symbol",
        other => return format!("{other} (any)"),
    };
    format!("{name} of your choice")
}

fn qty_of(o: &Map<String, Value>) -> u64 {
    o.get("quantity")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1)
}

/// Coins from copper pieces, largest denominations first (only the non-zero ones).
fn coins(cp: u64) -> Value {
    let mut m = Map::new();
    for (key, n) in [
        ("gp", cp / CP_PER_GP),
        ("sp", cp % CP_PER_GP / CP_PER_SP),
        ("cp", cp % CP_PER_SP),
    ] {
        if n > 0 {
            m.insert(key.into(), json!(n));
        }
    }
    Value::Object(m)
}

/// One package item: `{id?, name, qty?, note?}`.
fn item_json(id: Option<String>, name: &str, qty: u64, note: Option<&str>) -> Value {
    let mut m = Map::new();
    if let Some(id) = id {
        m.insert("id".into(), json!(id));
    }
    m.insert("name".into(), json!(name));
    if qty > 1 {
        m.insert("qty".into(), json!(qty));
    }
    if let Some(n) = note.filter(|n| !n.is_empty()) {
        m.insert("note".into(), json!(n));
    }
    Value::Object(m)
}

/// The imported item a uid (`chain mail|phb`) names.
fn item_id(sh: &Shared, uid: &str) -> (Option<String>, String) {
    let mut parts = uid.split('|');
    let name = parts.next().unwrap_or("").trim();
    let source = parts
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_ITEM_SOURCE);
    (
        sh.link(RefKind::Item, name, &source.to_uppercase()),
        title_case(name),
    )
}

/// One option: its items and its coins (copper pieces).
#[derive(Debug, Default, Clone)]
struct Opt {
    items: Vec<Value>,
    cp: u64,
}

impl Opt {
    fn add(&mut self, sh: &Shared, entry: &Value, partial: &mut Vec<String>) {
        let o = match entry {
            Value::String(uid) => {
                let (id, name) = item_id(sh, uid);
                self.items.push(item_json(id, &name, 1, None));
                return;
            }
            Value::Object(o) => o,
            other => {
                partial.push(format!(
                    "starting equipment entry {other} was not converted"
                ));
                return;
            }
        };
        let qty = qty_of(o);
        let display = o.get("displayName").and_then(Value::as_str);
        self.cp += o.get("containsValue").and_then(Value::as_u64).unwrap_or(0);
        if let Some(v) = o.get("value").and_then(Value::as_u64) {
            self.cp += v;
        } else if let Some(uid) = o.get("item").and_then(Value::as_str) {
            let (id, name) = item_id(sh, uid);
            let note = display.filter(|d| !d.eq_ignore_ascii_case(&name));
            self.items.push(item_json(id, &name, qty, note));
        } else if let Some(special) = o.get("special").and_then(Value::as_str) {
            let worth = o
                .get("worthValue")
                .and_then(Value::as_u64)
                .map(|v| format!("worth {} gp", v / CP_PER_GP));
            self.items
                .push(item_json(None, &title_case(special), qty, worth.as_deref()));
        } else if let Some(code) = o.get("equipmentType").and_then(Value::as_str) {
            let name = display.map_or_else(|| equipment_type_name(code), title_case);
            self.items.push(item_json(None, &name, qty, None));
        } else if let Some(codes) = o.get("equipmentTypes").and_then(Value::as_array) {
            let names: Vec<String> = codes
                .iter()
                .filter_map(Value::as_str)
                .map(equipment_type_name)
                .collect();
            self.items
                .push(item_json(None, &names.join(" or "), qty, None));
        } else {
            partial.push(format!(
                "starting equipment entry {} was not converted",
                Value::Object(o.clone())
            ));
        }
    }

    fn to_json(&self, label: &str) -> Value {
        json!({"id": label, "items": self.items, "coins": coins(self.cp)})
    }
}

/// The groups of a `startingEquipment` value: a background's array, or a class's
/// `defaultData`.
#[must_use]
pub fn groups_of(value: &Value) -> Vec<Map<String, Value>> {
    let list = match value {
        Value::Array(a) => a.as_slice(),
        Value::Object(o) => o
            .get("defaultData")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice),
        _ => &[],
    };
    list.iter().filter_map(|g| g.as_object().cloned()).collect()
}

/// Packages for `groups` (see the module documentation).
#[must_use]
pub fn packages(sh: &Shared, owner: &Owner<'_>, groups: &[Map<String, Value>]) -> Packages {
    let mut out = Packages::default();
    let mut fixed = Opt::default();
    let mut choices: Vec<Vec<(String, Opt)>> = Vec::new();
    for g in groups {
        let mut options: Vec<(String, Opt)> = Vec::new();
        for (key, entries) in g {
            let mut opt = Opt::default();
            for e in entries.as_array().map_or(&[][..], Vec::as_slice) {
                opt.add(sh, e, &mut out.partial);
            }
            if key == "_" {
                fixed.items.extend(opt.items);
                fixed.cp += opt.cp;
            } else {
                options.push((key.to_uppercase(), opt));
            }
        }
        if !options.is_empty() {
            options.sort_by(|a, b| a.0.cmp(&b.0));
            choices.push(options);
        }
    }
    if choices.is_empty() {
        if fixed.items.is_empty() && fixed.cp == 0 {
            return out;
        }
        choices.push(vec![("A".to_string(), Opt::default())]);
    }
    let total = choices.len();
    for (i, options) in choices.into_iter().enumerate() {
        let options: Vec<Value> = options
            .into_iter()
            .map(|(label, mut opt)| {
                if i == 0 {
                    let mut items = fixed.items.clone();
                    items.extend(opt.items);
                    opt.items = items;
                    opt.cp += fixed.cp;
                }
                opt.to_json(&label)
            })
            .collect();
        let (pid, name) = if i == 0 {
            (
                format!("{}_EQUIPMENT_PACKAGE", owner.id),
                format!("Starting Equipment ({})", owner.name),
            )
        } else {
            (
                format!("{}_EQUIPMENT_PACKAGE_{}", owner.id, i + 1),
                format!(
                    "Starting Equipment ({}): choice {} of {total}",
                    owner.name,
                    i + 1
                ),
            )
        };
        let mut el = new_element(
            &pid,
            &name,
            "Equipment Package",
            owner.source,
            owner.ruleset,
        );
        el.data.insert("owner".into(), json!(owner.id));
        el.data.insert("options".into(), Value::Array(options));
        let mut rule = custom("equipment_package", json!({"ref": pid, "name": name}));
        if let Some(g) = owner.gate {
            if let Err(e) = gate(&mut rule, g) {
                out.partial.push(e);
            }
        }
        out.rules.push(rule);
        out.elements.push(el);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Ruleset;

    fn shared() -> Shared {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Item, "javelin".into(), "XPHB".into()),
            "ID_5ET_XPHB_ITEM_JAVELIN".into(),
        );
        sh
    }

    fn owner(gate: Option<&str>) -> Owner<'_> {
        Owner {
            id: "ID_X_BACKGROUND_ACOLYTE",
            name: "Acolyte",
            source: "Test",
            ruleset: Ruleset::Dnd2014,
            gate,
        }
    }

    fn opts(el: &Element) -> &Vec<Value> {
        el.data["options"].as_array().unwrap()
    }

    #[test]
    fn fixed_items_join_every_option_of_the_first_choice() {
        let sh = shared();
        let groups = groups_of(&json!([
            {"_": [
                {"item": "holy symbol|phb", "displayName": "holy symbol (a gift)"},
                {"special": "sticks of incense", "quantity": 5},
                "common clothes|phb",
                {"item": "pouch|phb", "containsValue": 1500}
            ]},
            {"a": [{"item": "book|phb", "displayName": "prayer book"}], "b": [{"special": "prayer wheel"}]}
        ]));
        let p = packages(&sh, &owner(None), &groups);
        assert!(p.partial.is_empty(), "{:?}", p.partial);
        assert_eq!(p.elements.len(), 1);
        let el = &p.elements[0];
        assert_eq!(el.id, "ID_X_BACKGROUND_ACOLYTE_EQUIPMENT_PACKAGE");
        assert_eq!(el.ty, "Equipment Package");
        assert_eq!(el.data["owner"], "ID_X_BACKGROUND_ACOLYTE");
        let o = opts(el);
        assert_eq!(o.len(), 2);
        assert_eq!(o[0]["id"], "A");
        assert_eq!(o[1]["id"], "B");
        // 4 fixed items (pouch included) + the choice
        assert_eq!(o[0]["items"].as_array().unwrap().len(), 5);
        assert_eq!(o[1]["items"][4]["name"], "Prayer Wheel");
        assert_eq!(o[0]["items"][1]["qty"], 5);
        assert_eq!(o[0]["items"][0]["note"], "holy symbol (a gift)");
        assert_eq!(o[0]["coins"], json!({"gp": 15}));
        assert_eq!(o[1]["coins"], json!({"gp": 15}));
        let Rule::Custom(r) = &p.rules[0] else {
            panic!("custom rule expected")
        };
        assert_eq!(r.kind, "equipment_package");
        assert_eq!(r.fields["ref"], "ID_X_BACKGROUND_ACOLYTE_EQUIPMENT_PACKAGE");
    }

    #[test]
    fn lettered_2024_options_coins_and_further_choices() {
        let sh = shared();
        let groups = groups_of(&json!({"defaultData": [
            {"A": [{"item": "chain mail|xphb"}, {"item": "javelin|xphb", "quantity": 8}, {"value": 400}],
             "C": [{"value": 15555}], "B": [{"equipmentType": "weaponMartial", "quantity": 2}]},
            {"a": [{"equipmentType": "setGaming", "displayName": "gaming set (any)"}],
             "b": [{"equipmentTypes": ["instrumentMusical", "toolArtisan"]}]}
        ]}));
        let p = packages(&sh, &owner(Some("!ID_MC")), &groups);
        assert_eq!(p.elements.len(), 2);
        let o = opts(&p.elements[0]);
        let labels: Vec<&str> = o.iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(labels, ["A", "B", "C"]);
        assert_eq!(o[0]["coins"], json!({"gp": 4}));
        assert_eq!(o[2]["coins"], json!({"gp": 155, "sp": 5, "cp": 5}));
        assert_eq!(o[1]["items"][0]["name"], "Martial Weapon of your choice");
        assert_eq!(o[1]["items"][0]["qty"], 2);
        assert_eq!(o[0]["items"][1]["name"], "Javelin");
        assert_eq!(o[0]["items"][1]["id"], "ID_5ET_XPHB_ITEM_JAVELIN");
        assert!(o[0]["items"][0].get("id").is_none());
        let second = &p.elements[1];
        assert_eq!(second.id, "ID_X_BACKGROUND_ACOLYTE_EQUIPMENT_PACKAGE_2");
        assert_eq!(second.name, "Starting Equipment (Acolyte): choice 2 of 2");
        let o2 = opts(second);
        assert_eq!(o2[0]["items"][0]["name"], "Gaming Set (Any)");
        assert_eq!(
            o2[1]["items"][0]["name"],
            "Musical Instrument of your choice or Artisan's Tools of your choice"
        );
        for r in &p.rules {
            let Rule::Custom(c) = r else {
                panic!("custom rule expected")
            };
            assert_eq!(c.fields["requirements"], "!ID_MC");
        }
    }

    #[test]
    fn fixed_only_is_one_option_and_empty_gives_nothing() {
        let sh = shared();
        let p = packages(
            &sh,
            &owner(None),
            &groups_of(&json!([{"_": ["scroll case|phb", {"value": 500}]}])),
        );
        let o = opts(&p.elements[0]);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0]["id"], "A");
        assert_eq!(o[0]["coins"], json!({"gp": 5}));
        assert!(packages(&sh, &owner(None), &groups_of(&json!([])))
            .elements
            .is_empty());
        let odd = packages(&sh, &owner(None), &groups_of(&json!([{"_": [{"odd": 1}]}])));
        assert_eq!(odd.partial.len(), 1);
    }
}
