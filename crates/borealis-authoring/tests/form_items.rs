//! WB-12, the item forms (spec 4.9, 18.1, 18.2): Item, Weapon, Armor, Magic Item, Item Pack, Equipment Package.
//!
//! * Identity over every bundled element of these six types: each opens in its own form and comes back byte for
//!   byte (also through the wire), and no location the engine reads is left in the preserved bucket.
//! * Each row of spec 4.9 on hand-written cases: edits rewrite every bound location (2024 mirrors only when the
//!   element has them), rules a field owns go back to their place among the blocks, bad values get plain sentences.
//! * The descriptors of the six kinds (the web mock serves them from `ui/src/api/mock/homebrew-kinds.ts`, which
//!   `src-tauri/tests/contract_homebrew.rs` writes with every other kind).

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    check_fields, element_to_form, form_path_for_location, form_to_element, form_to_elements, kind,
    kind_for_type, merge_ui, to_ui, CatalogBlockCodec, FormContext, FormState, RefInfo,
};
use borealis_content::{ContentFile, Element, PackFileKind, PackManifest};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const ITEM_KINDS: [(&str, &str); 6] = [
    ("Item", "item"),
    ("Weapon", "weapon"),
    ("Armor", "armor"),
    ("Magic Item", "magicItem"),
    ("Item Pack", "itemPack"),
    ("Equipment Package", "equipmentPackage"),
];

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn bundled() -> Vec<Element> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(content_dir())
        .expect("content")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.join("pack.json").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let manifest =
            PackManifest::from_json_str(&fs::read_to_string(dir.join("pack.json")).expect("pack"))
                .expect("manifest");
        for file in manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
        {
            let text = fs::read_to_string(dir.join(&file.path)).expect("file");
            out.extend(ContentFile::from_json_str(&text).expect("content").elements);
        }
    }
    out
}

fn lookup_of(elements: &[Element]) -> HashMap<String, RefInfo> {
    elements
        .iter()
        .map(|e| {
            (
                e.id.clone(),
                RefInfo {
                    name: e.name.clone(),
                    ty: e.ty.clone(),
                },
            )
        })
        .collect()
}

/// The core elements item fields name (tags, proficiencies), for hand-written cases.
fn core_lookup() -> HashMap<String, RefInfo> {
    let r = |name: &str, ty: &str| RefInfo {
        name: name.into(),
        ty: ty.into(),
    };
    HashMap::from([
        (
            "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE".into(),
            r("Martial Melee", "Weapon Category"),
        ),
        (
            "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED".into(),
            r("Simple Ranged", "Weapon Category"),
        ),
        (
            "ID_INTERNAL_WEAPON_PROPERTY_FINESSE".into(),
            r("Finesse", "Weapon Property"),
        ),
        (
            "ID_INTERNAL_WEAPON_PROPERTY_LIGHT".into(),
            r("Light", "Weapon Property"),
        ),
        (
            "ID_INTERNAL_WEAPON_PROPERTY_RANGE".into(),
            r("Range", "Weapon Property"),
        ),
        (
            "ID_INTERNAL_DAMAGE_TYPE_SLASHING".into(),
            r("Slashing", "Damage Type"),
        ),
        (
            "ID_INTERNAL_DAMAGE_TYPE_PIERCING".into(),
            r("Piercing", "Damage Type"),
        ),
        ("ID_2024_MASTERY_SAP".into(), r("Sap", "Mastery Property")),
        ("ID_2024_MASTERY_VEX".into(), r("Vex", "Mastery Property")),
        (
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_SCIMITAR".into(),
            r("Scimitar", "Proficiency"),
        ),
        ("ID_2024_ITEM_ROPE".into(), r("Rope", "Item")),
        ("ID_2024_ITEM_TORCH".into(), r("Torch", "Item")),
        ("ID_2024_CLASS_FIGHTER".into(), r("Fighter", "Class")),
    ])
}

fn with_cx<T>(lookup: &HashMap<String, RefInfo>, f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(["Fighter"]),
        lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn json_of(e: &Element) -> Value {
    serde_json::to_value(e).expect("json")
}

/// A new element of `kind` with these field values (its id given, as on its first save).
fn new_form(kind: &str, id: &str, fields: Value) -> FormState {
    let mut f = FormState::new("e", kind);
    f.memo.id = Some(id.into());
    for (k, v) in fields.as_object().expect("fields") {
        f.fields.insert(k.clone(), v.clone());
    }
    f
}

fn edited(form: &FormState, field: &str, value: Value) -> FormState {
    form.with_field(field, value)
}

/// What a preserved part is kept as: `data.<key>`, `setter <name>`, `extract`...
fn kept_location(form: &FormState) -> Vec<String> {
    form.preserved
        .iter()
        .filter_map(|p| p.kept.as_ref())
        .map(|k| {
            if k.location == "setter" {
                format!("setter {}", k.value["name"].as_str().unwrap_or_default())
            } else {
                k.location.clone()
            }
        })
        .collect()
}

/// Display-only parts of bundled items no item field edits (spec 3.3: kept untouched, dropped on copies).
fn allowed_kept(kind: &str, location: &str) -> bool {
    let item_data = [
        "tool",
        "ability",
        "utilize",
        "craft",
        "variants",
        "tool_group",
        "cost_text",
        "carrying_capacity",
        "availability",
        "spell_level",
        "contents_text",
        "speed",
        "crew",
        "passengers",
        "cargo_tons",
        "ac",
        "hp",
        "damage_threshold",
        "ammunition",
        "amount",
        "storage",
        "bundle_weight",
        "bundle_cost",
        "healing",
        "crafting",
    ];
    match kind {
        "item" => {
            location
                .strip_prefix("data.")
                .is_some_and(|k| item_data.contains(&k))
                || ["setter container", "setter capacity", "setter speed"].contains(&location)
        }
        "weapon" => ["data.ammunition_type", "data.two_handed_unless_mounted"].contains(&location),
        "armor" => ["data.don_doff", "setter type"].contains(&location),
        "magicItem" => ["data.charges", "data.raw_type_line"].contains(&location),
        _ => false,
    }
}

#[test]
fn every_bundled_item_round_trips_in_its_own_form() {
    let all = bundled();
    let lookup = lookup_of(&all);
    with_cx(&lookup, |cx| {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut failures = Vec::new();
        for e in &all {
            let Some((_, kind_id)) = ITEM_KINDS.iter().find(|(ty, _)| *ty == e.ty) else {
                continue;
            };
            *counts.entry(kind_id).or_default() += 1;
            assert_eq!(kind_for_type(&e.ty).id, *kind_id);
            let form = element_to_form(e, cx);
            let want = vec![e.clone()];
            match form_to_elements(&form, cx) {
                Ok(back) if back == want => {}
                other => failures.push(format!("{}: identity {other:?}", e.id)),
            }
            let wire: FormState =
                serde_json::from_str(&serde_json::to_string(&to_ui(&form)).expect("ui"))
                    .expect("wire");
            if form_to_elements(&merge_ui(&form, &wire), cx).ok() != Some(want) {
                failures.push(format!("{}: wire trip differs", e.id));
            }
            for loc in kept_location(&form) {
                if !allowed_kept(kind_id, &loc) {
                    failures.push(format!("{} ({kind_id}): kept {loc}", e.id));
                }
            }
            if *kind_id == "armor" {
                // base AC and stealth are fields; what is left is the heavy armor speed rule
                for b in &form.blocks {
                    assert_eq!(b.kind, "stat", "{}: {}", e.id, b.sentence);
                    assert_eq!(b.fields["name"], json!("speed"), "{}", e.id);
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        // the bundled packs: SRD items, packs (type Item with contents), weapons, armor, magic items, 2024 packages
        assert_eq!(
            counts,
            BTreeMap::from([
                ("armor", 26),
                ("equipmentPackage", 16),
                ("item", 363),
                ("magicItem", 547),
                ("weapon", 75),
            ])
        );
    });
}

fn longsword_2024() -> Element {
    el(json!({
        "id": "ID_2024_WEAPON_LONGSWORD", "name": "Longsword", "type": "Weapon", "source": "SRD 5.2.1",
        "ruleset": "dnd2024",
        "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", "ID_INTERNAL_DAMAGE_TYPE_SLASHING",
                     "ID_INTERNAL_WEAPON_PROPERTY_FINESSE", "ID_2024_MASTERY_SAP"],
        "setters": [{"name": "category", "value": "Weapons"},
                    {"name": "cost", "value": "15", "attributes": {"currency": "gp"}},
                    {"name": "weight", "value": "3 lb.", "attributes": {"lb": "3"}},
                    {"name": "slot", "value": "onehand"},
                    {"name": "damage", "value": "1d8", "attributes": {"type": "slashing"}},
                    {"name": "versatile", "value": "1d10"},
                    {"name": "mastery", "value": "sap"}],
        "data": {"weapon_category": "Martial Melee", "melee": true,
                 "damage": {"dice": "1d8", "type": "slashing"}, "properties": ["Finesse"],
                 "versatile_damage": "1d10", "range": null, "mastery": "sap",
                 "mastery_id": "ID_2024_MASTERY_SAP"}
    }))
}

fn tag_ref(id: &str, name: &str, ty: &str) -> Value {
    json!({"id": id, "name": name, "type": ty})
}

#[test]
fn a_weapon_reads_its_fields_from_tags_setters_and_mirrors() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = element_to_form(&longsword_2024(), cx);
        assert_eq!(f.kind, "weapon");
        assert_eq!(f.fields["damage"], json!("1d8"));
        assert_eq!(f.fields["damageType"]["name"], json!("Slashing"));
        assert_eq!(f.fields["weaponCategory"]["name"], json!("Martial Melee"));
        assert_eq!(f.fields["mastery"][0]["name"], json!("Sap"));
        assert_eq!(f.fields["properties"][0]["name"], json!("Finesse"));
        assert_eq!(f.fields["versatile"], json!("1d10"));
        assert_eq!(f.fields["cost"], json!("15"));
        assert_eq!(f.fields["currency"], json!("gp"));
        assert_eq!(f.fields["weight"], json!("3"));
        assert_eq!(f.fields["tags"], json!([]));
        assert!(f.preserved.is_empty(), "{:?}", f.preserved);
    });
}

#[test]
fn editing_a_2024_weapon_rewrites_every_bound_location() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = element_to_form(&longsword_2024(), cx);
        let f = edited(
            &f,
            "weaponCategory",
            tag_ref(
                "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED",
                "Simple Ranged",
                "Weapon Category",
            ),
        );
        let f = edited(
            &f,
            "mastery",
            json!([tag_ref("ID_2024_MASTERY_VEX", "Vex", "Mastery Property")]),
        );
        let f = edited(
            &f,
            "damageType",
            tag_ref(
                "ID_INTERNAL_DAMAGE_TYPE_PIERCING",
                "Piercing",
                "Damage Type",
            ),
        );
        let f = edited(&f, "damage", json!("1d6"));
        let f = edited(
            &f,
            "properties",
            json!([
                tag_ref(
                    "ID_INTERNAL_WEAPON_PROPERTY_LIGHT",
                    "Light",
                    "Weapon Property"
                ),
                tag_ref(
                    "ID_INTERNAL_WEAPON_PROPERTY_RANGE",
                    "Range",
                    "Weapon Property"
                )
            ]),
        );
        let f = edited(&f, "range", json!("80/320"));
        let e = json_of(&form_to_element(&f, cx).expect("writes"));
        assert_eq!(
            e["supports"],
            json!([
                "ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED",
                "ID_INTERNAL_DAMAGE_TYPE_PIERCING",
                "ID_INTERNAL_WEAPON_PROPERTY_LIGHT",
                "ID_2024_MASTERY_VEX",
                "ID_INTERNAL_WEAPON_PROPERTY_RANGE"
            ])
        );
        let setters: BTreeMap<String, Value> = e["setters"]
            .as_array()
            .expect("setters")
            .iter()
            .map(|s| (s["name"].as_str().unwrap().to_string(), s.clone()))
            .collect();
        assert_eq!(
            setters["damage"],
            json!({"name": "damage", "value": "1d6", "attributes": {"type": "piercing"}})
        );
        assert_eq!(setters["mastery"]["value"], json!("vex"));
        assert_eq!(setters["range"]["value"], json!("80/320"));
        let d = &e["data"];
        assert_eq!(d["weapon_category"], json!("Simple Ranged"));
        assert_eq!(d["melee"], json!(false));
        assert_eq!(d["damage"], json!({"dice": "1d6", "type": "piercing"}));
        assert_eq!(d["mastery"], json!("vex"));
        assert_eq!(d["mastery_id"], json!("ID_2024_MASTERY_VEX"));
        assert_eq!(
            d["properties"],
            json!(["Light"]),
            "the Range marker is not listed"
        );
        assert_eq!(d["range"], json!("80/320"));
        assert!(d.get("mastery_ids").is_none(), "no mirror it didn't have");
    });
}

#[test]
fn a_new_weapon_with_a_mastery_is_written_in_the_tag_form() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = new_form(
            "weapon",
            "ID_HB_WEAPON_SICKLE_SWORD",
            json!({
                "name": "Sickle Sword", "category": "Weapons",
                "weaponCategory": tag_ref("ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", "Martial Melee", "Weapon Category"),
                "damage": "1d6", "damageType": tag_ref("ID_INTERNAL_DAMAGE_TYPE_SLASHING", "Slashing", "Damage Type"),
                "mastery": [tag_ref("ID_2024_MASTERY_SAP", "Sap", "Mastery Property")],
                "proficiency": tag_ref("ID_PROFICIENCY_WEAPON_PROFICIENCY_SCIMITAR", "Scimitar", "Proficiency"),
                "cost": "25", "currency": "gp", "weight": "2.5", "slot": "onehand"
            }),
        );
        let e = form_to_element(&f, cx).expect("writes");
        assert_eq!(e.ty, "Weapon");
        assert_eq!(
            e.supports,
            [
                "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE",
                "ID_INTERNAL_DAMAGE_TYPE_SLASHING",
                "ID_2024_MASTERY_SAP"
            ]
        );
        assert!(
            e.data.is_empty(),
            "a new weapon has no 2024 mirrors: {:?}",
            e.data
        );
        let v = json_of(&e);
        assert!(v["setters"]
            .as_array()
            .unwrap()
            .contains(&json!({"name": "weight", "value": "2.5 lb.", "attributes": {"lb": "2.5"}})));
        assert!(v["setters"]
            .as_array()
            .unwrap()
            .contains(&json!({"name": "cost", "value": "25", "attributes": {"currency": "gp"}})));
        assert!(v["setters"]
            .as_array()
            .unwrap()
            .contains(&json!({"name": "mastery", "value": "sap"})));
        assert!(v["setters"].as_array().unwrap().contains(
            &json!({"name": "proficiency", "value": "ID_PROFICIENCY_WEAPON_PROFICIENCY_SCIMITAR"})
        ));
        // read back: the same values
        let again = element_to_form(&e, cx);
        for k in [
            "mastery",
            "damageType",
            "weaponCategory",
            "weight",
            "cost",
            "currency",
        ] {
            assert_eq!(again.fields[k], f.fields[k], "{k}");
        }
    });
}

fn plate_2024() -> Element {
    el(json!({
        "id": "ID_2024_ARMOR_PLATE_ARMOR", "name": "Plate Armor", "type": "Armor", "source": "SRD 5.2.1",
        "ruleset": "dnd2024", "supports": ["ID_INTERNAL_ARMOR_GROUP_HEAVY"],
        "setters": [{"name": "category", "value": "Armor"}, {"name": "slot", "value": "body"},
                    {"name": "armor", "value": "Heavy"}, {"name": "armorClass", "value": "18"},
                    {"name": "strength", "value": "15"}, {"name": "stealth", "value": "Disadvantage"}],
        "rules": [{"kind": "grant", "id": "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE", "type": "Grants"},
                  {"kind": "stat", "name": "ac:armored:armor", "value": "18"},
                  {"kind": "stat", "name": "speed", "value": "-10", "bonus": "armor", "requirements": "![str:15]"}],
        "data": {"armor_category": "Heavy", "base_ac": 18, "dex_modifier": false, "dex_cap": null,
                 "strength_requirement": 15, "stealth_disadvantage": true}
    }))
}

#[test]
fn armor_fields_own_their_rules_and_put_them_back_in_place() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = element_to_form(&plate_2024(), cx);
        assert_eq!(f.fields["baseAc"], json!(18));
        assert_eq!(f.fields["stealth"], json!(true));
        assert_eq!(f.fields["strength"], json!(15));
        assert_eq!(f.fields["armorType"], json!("Heavy"));
        assert_eq!(f.blocks.len(), 1, "only the speed rule is a block");
        // an edited base AC stays where it was among the rules
        let e = form_to_element(&edited(&f, "baseAc", json!(19)), cx).expect("writes");
        let rules = serde_json::to_value(&e.rules).unwrap();
        assert_eq!(
            rules[0]["id"],
            json!("ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE")
        );
        assert_eq!(
            rules[1],
            json!({"kind": "stat", "name": "ac:armored:armor", "value": "19"})
        );
        assert_eq!(rules[2]["name"], json!("speed"));
        assert_eq!(e.data["base_ac"], json!(19));
        // stealth off: no grant, the 2024 "—" setter and mirror
        let e = form_to_element(&edited(&f, "stealth", json!(false)), cx).expect("writes");
        let v = json_of(&e);
        assert_eq!(v["rules"].as_array().unwrap().len(), 2);
        assert!(v["setters"]
            .as_array()
            .unwrap()
            .contains(&json!({"name": "stealth", "value": "\u{2014}"})));
        assert_eq!(e.data["stealth_disadvantage"], json!(false));
        // a shield: its AC is the shield rule, and the group tag and mirrors follow
        let shield = edited(
            &edited(&f, "armorType", json!("Shield")),
            "baseAc",
            json!(2),
        );
        let e = json_of(&form_to_element(&shield, cx).expect("writes"));
        assert!(e["rules"].as_array().unwrap().contains(
            &json!({"kind": "stat", "name": "ac:shield", "value": "2", "bonus": "shield"})
        ));
        assert_eq!(e["supports"], json!(["ID_INTERNAL_ARMOR_GROUP_SHIELD"]));
        assert_eq!(e["data"]["armor_category"], json!("Shield"));
        assert_eq!(e["data"]["dex_cap"], Value::Null);
        // the armor type alone changing still moves an unchanged base AC to the shield rule
        let e = json_of(&form_to_element(&edited(&f, "armorType", json!("Shield")), cx).unwrap());
        assert!(e["rules"].as_array().unwrap().contains(
            &json!({"kind": "stat", "name": "ac:shield", "value": "18", "bonus": "shield"})
        ));
        // the validator's location of the AC rule is the Armor Class field
        let saved = form_to_element(&f, cx).unwrap();
        assert_eq!(
            form_path_for_location(&f, &saved, "rules[1]", cx).as_deref(),
            Some("fields.baseAc")
        );
        assert_eq!(
            form_path_for_location(&f, &saved, "rules[2]", cx).as_deref(),
            Some("blocks.0")
        );
    });
}

#[test]
fn a_new_armor_writes_its_rules_in_the_srd_order() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = new_form(
            "armor",
            "ID_HB_ARMOR_BONE_PLATE",
            json!({"name": "Bone Plate", "armorType": "Medium", "baseAc": 15, "stealth": true,
                   "strength": 0, "armorClassText": "15 + Dex (max 2)"}),
        );
        let e = json_of(&form_to_element(&f, cx).expect("writes"));
        assert_eq!(
            e["rules"],
            json!([
                {"kind": "grant", "id": "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE", "type": "Grants"},
                {"kind": "stat", "name": "ac:armored:armor", "value": "15"}
            ])
        );
        assert_eq!(e["supports"], json!(["ID_INTERNAL_ARMOR_GROUP_MEDIUM"]));
        assert!(e.get("data").is_none());
    });
}

#[test]
fn a_magic_items_bonus_and_base_items() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let element = |id: &str| json!({"node": "row", "row": {"row": "element", "id": id}});
        let any_armor = json!({"value": {"shape": "model", "model": {"specific": true, "root": {"node": "group",
            "mode": "any", "items": [element("ID_INTERNAL_ARMOR_GROUP_LIGHT"), element("ID_INTERNAL_ARMOR_GROUP_HEAVY")]}}}});
        let f = new_form(
            "magicItem",
            "ID_HB_MAGIC_ITEM_ARMOR_2",
            json!({"name": "Armor of the Hearth", "magicType": "Armor", "rarity": "Rare",
                   "attunement": true, "attunementBy": "by a cleric",
                   "appliesToArmor": any_armor, "enhancement": 2, "nameFormat": "{{parent}} +{{enhancement}}"}),
        );
        let problems = check_fields(&f, cx);
        assert!(problems.is_empty(), "{problems:?}");
        let e = json_of(&form_to_element(&f, cx).expect("writes"));
        let setters = e["setters"].as_array().unwrap();
        assert!(setters.contains(&json!({"name": "attunement", "value": "true", "attributes": {"addition": "by a cleric"}})));
        assert!(setters.contains(&json!({"name": "enhancement", "value": "2"})));
        assert!(setters.contains(&json!({"name": "armor", "value": "ID_INTERNAL_ARMOR_GROUP_LIGHT|ID_INTERNAL_ARMOR_GROUP_HEAVY"})), "{setters:?}");
        assert_eq!(
            e["rules"],
            json!([{"kind": "stat", "name": "ac:armored:enhancement", "value": "2", "bonus": "enhancement"}])
        );
        // a +1 weapon: the bonus is the setter alone (the engine adds it to attack and damage)
        let w = new_form(
            "magicItem",
            "ID_HB_MAGIC_ITEM_WEAPON_1",
            json!({"name": "Weapon, +1", "magicType": "Weapon", "enhancement": 1}),
        );
        assert!(form_to_element(&w, cx).unwrap().rules.is_empty());
        // a shield-only item gets the shield rule
        let shields = json!({"value": {"shape": "model", "model": {"specific": true,
            "root": element("ID_INTERNAL_ARMOR_GROUP_SHIELD")}}});
        let s = new_form(
            "magicItem",
            "ID_HB_MAGIC_ITEM_SHIELD_1",
            json!({"name": "Shield, +1", "appliesToArmor": shields, "enhancement": 1}),
        );
        assert_eq!(
            json_of(&form_to_element(&s, cx).unwrap())["rules"],
            json!([{"kind": "stat", "name": "ac:shield", "value": "1", "bonus": "enhancement"}])
        );
    });
}

#[test]
fn a_2024_magic_item_keeps_its_mirrors_in_step() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let wand = el(json!({
            "id": "ID_2024_MAGIC_ITEM_WAND_OF_FEAR", "name": "Wand of Fear", "type": "Magic Item",
            "source": "SRD 5.2.1", "ruleset": "dnd2024",
            "setters": [{"name": "category", "value": "Wands"}, {"name": "type", "value": "Wand"},
                        {"name": "rarity", "value": "Rare"}, {"name": "attunement", "value": "true"}],
            "data": {"item_type": "Wand", "item_subtype": null, "rarity_text": "Rare", "rarities": ["Rare"],
                     "requires_attunement": true, "attunement_by": null, "charges": 7,
                     "raw_type_line": "Wand, Rare (Requires Attunement)"}
        }));
        let f = element_to_form(&wand, cx);
        assert_eq!(f.fields["rarity"], json!("Rare"));
        assert_eq!(f.fields["attunement"], json!(true));
        assert_eq!(f.fields["attunementBy"], json!(""));
        let f = edited(
            &edited(&f, "rarity", json!("Very Rare")),
            "attunementBy",
            json!("by a wizard"),
        );
        let e = form_to_element(&f, cx).unwrap();
        assert_eq!(e.data["rarity_text"], json!("Very Rare"));
        assert_eq!(e.data["rarities"], json!(["Very Rare"]));
        assert_eq!(e.data["attunement_by"], json!("by a wizard"));
        assert_eq!(e.data["charges"], json!(7), "kept as it was");
        let att = e.setters.iter().find(|s| s.name == "attunement").unwrap();
        assert_eq!(att.attribute("addition"), Some("by a wizard"));
        let off = form_to_element(&edited(&f, "attunement", json!(false)), cx);
        let problems = off.expect_err("by whom without attunement");
        assert_eq!(problems[0].path, "fields.attunementBy");
        assert!(problems[0].message.starts_with("Tick"), "{problems:?}");
    });
}

#[test]
fn pack_contents_and_starting_equipment() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let pack = new_form(
            "itemPack",
            "ID_HB_ITEM_PACK_CAMP",
            json!({"name": "Camp Pack", "cost": "2",
                   "contents": [{"item": tag_ref("ID_2024_ITEM_ROPE", "Rope", "Item"), "amount": 1},
                                {"item": tag_ref("ID_2024_ITEM_TORCH", "Torch", "Item"), "amount": 10},
                                {"item": null, "amount": 3}]}),
        );
        let e = form_to_element(&pack, cx).unwrap();
        assert_eq!(e.ty, "Item Pack");
        assert_eq!(
            serde_json::to_value(&e.extract).unwrap(),
            json!({"items": [{"id": "ID_2024_ITEM_ROPE"}, {"id": "ID_2024_ITEM_TORCH", "amount": 10}]})
        );
        assert!(
            element_to_form(&e, cx).preserved.is_empty(),
            "contents are a field"
        );
        let pkg = new_form(
            "equipmentPackage",
            "ID_HB_EQUIPMENT_PACKAGE_WARDEN",
            json!({"name": "Starting Equipment (Warden)",
                   "owner": tag_ref("ID_2024_CLASS_FIGHTER", "Fighter", "Class"),
                   "options": [
                       {"label": "A", "items": [{"item": tag_ref("ID_2024_ITEM_ROPE", "Rope", "Item"), "qty": 2, "note": "hemp"},
                                                {"name": "A trinket", "qty": 1, "note": ""}], "coins": {"gp": 5}},
                       {"label": "B", "items": [], "coins": {"gp": 75, "sp": 2}}]}),
        );
        let e = form_to_element(&pkg, cx).unwrap();
        assert_eq!(e.data["owner"], json!("ID_2024_CLASS_FIGHTER"));
        assert_eq!(
            e.data["options"],
            json!([
                {"id": "A", "items": [{"id": "ID_2024_ITEM_ROPE", "qty": 2, "note": "hemp"}, {"name": "A trinket"}],
                 "coins": {"gp": 5}},
                {"id": "B", "items": [], "coins": {"sp": 2, "gp": 75}}
            ])
        );
        let again = element_to_form(&e, cx);
        assert_eq!(
            again.fields["options"][0]["items"][1]["name"],
            json!("A trinket")
        );
        assert_eq!(again.fields["options"][1]["coins"]["gp"], json!(75));
        let bad = edited(
            &pkg,
            "options",
            json!([{"label": " ", "items": [], "coins": {}}]),
        );
        let p = form_to_element(&bad, cx).unwrap_err();
        assert_eq!(p[0].path, "fields.options");
    });
}

#[test]
fn bad_item_values_get_plain_sentences_at_their_fields() {
    let lookup = core_lookup();
    with_cx(&lookup, |cx| {
        let f = new_form(
            "weapon",
            "ID_HB_WEAPON_X",
            json!({"name": "X", "damage": "a lot", "versatile": "2x", "range": "far", "cost": "cheap",
                   "currency": "doubloons"}),
        );
        let problems = check_fields(&f, cx);
        let at: BTreeMap<&str, &str> = problems
            .iter()
            .map(|p| (p.path.as_str(), p.message.as_str()))
            .collect();
        assert_eq!(
            at["fields.damage"],
            "Write the damage as dice, like 1d8 or 2d6."
        );
        assert_eq!(
            at["fields.versatile"],
            "Write the two-handed damage as dice, like 1d10."
        );
        assert_eq!(
            at["fields.range"],
            "Write the range in feet as normal/long, like 20/60."
        );
        assert_eq!(
            at["fields.cost"],
            "Write the cost as a number, like 15 or 0.5."
        );
        assert_eq!(at["fields.currency"], "Pick a coin from the list.");
        for p in &problems {
            assert!(
                !p.message.contains("ID_") && !p.message.contains('{') && p.message.ends_with('.'),
                "plain: {p:?}"
            );
        }
        let w = new_form(
            "item",
            "ID_HB_ITEM_Y",
            json!({"name": "Y", "weight": "Varies"}),
        );
        let e = json_of(&form_to_element(&w, cx).expect("a word is a weight"));
        assert_eq!(e["setters"], json!([{"name": "weight", "value": "Varies"}]));
        let a = new_form(
            "armor",
            "ID_HB_ARMOR_Z",
            json!({"name": "Z", "armorType": "Mithral"}),
        );
        let p = check_fields(&a, cx);
        assert_eq!(p[0].path, "fields.armorType");
    });
}

#[test]
fn the_six_item_kinds_describe_their_fields() {
    let mut types = BTreeSet::new();
    for (ty, id) in ITEM_KINDS {
        let k = kind(id).expect("kind");
        assert_eq!(k.element_types, [ty]);
        types.insert(ty);
        let d = k.descriptor();
        let v = serde_json::to_value(&d).unwrap();
        for f in v["fields"].as_array().unwrap() {
            let control = f["control"].as_str().unwrap();
            if control == "reference" || control == "references" {
                assert!(
                    f["picker"]["types"]
                        .as_array()
                        .is_some_and(|t| !t.is_empty()),
                    "{f}"
                );
                assert_eq!(
                    f["picker"].get("multiple").is_some(),
                    control == "references",
                    "{f}"
                );
            }
            if control == "number" {
                assert!(f["min"].is_i64() && f["max"].is_i64(), "{f}");
            }
            assert!(!f["bindings"].as_array().unwrap().is_empty(), "{f}");
        }
    }
    assert_eq!(types.len(), 6);
    let weapon = kind("weapon").unwrap().descriptor();
    let mastery = weapon.fields.iter().find(|f| f.key == "mastery").unwrap();
    assert_eq!(
        mastery.bindings,
        [
            "setter:mastery",
            "supports:Mastery Property",
            "data.mastery",
            "data.mastery_id",
            "data.mastery_ids"
        ]
    );
}

#[test]
fn name_when_made_is_text_with_value_chips() {
    let magic = borealis_authoring::form::descriptors()
        .into_iter()
        .find(|k| k.id == "magicItem")
        .expect("magic item kind");
    let f = magic
        .fields
        .iter()
        .find(|f| f.key == "nameFormat")
        .expect("nameFormat");
    assert_eq!(f.control, "nameFormat", "text with value chips (spec 7.4)");
    assert!(
        !f.help.as_deref().unwrap_or_default().contains('{'),
        "the help never asks for braces: {:?}",
        f.help
    );
}
