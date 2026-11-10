//! The species forms (spec 4.4), field by field: the 2024 Species and the 2014 Race, Sub Race and Race Variant read
//! their size, speeds, senses, traits, languages and ability score increases from the rules that hold them, write
//! them back in the stored order, and write a new species from its fields. The bundled species and races are covered
//! by `tests/form_identity.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    check_fields, element_to_form, form_path_for_location, form_to_element, kind, kind_for_type,
    CatalogBlockCodec, FormContext, FormState, KindGroup, RefInfo,
};
use borealis_content::{Element, Ruleset};
use serde_json::{json, Value};
use std::collections::HashMap;

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let names = [
        ("ID_LANGUAGE_COMMON", "Common", "Language"),
        ("ID_LANGUAGE_DWARVISH", "Dwarvish", "Language"),
        ("ID_X_TRAIT_KEEN", "Keen Senses", "Racial Trait"),
        ("ID_X_TRAIT_TRANCE", "Trance", "Racial Trait"),
        ("ID_X_TRAIT_BIG", "Large Form", "Racial Trait"),
        ("ID_X_TRAIT_NEW", "Stone Skin", "Racial Trait"),
    ];
    let lookup: HashMap<String, RefInfo> = names
        .iter()
        .map(|(id, name, ty)| {
            (
                (*id).to_string(),
                RefInfo {
                    name: (*name).into(),
                    ty: (*ty).into(),
                },
            )
        })
        .collect();
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &lookup,
        blocks: &codec,
    };
    f(&cx)
}

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn one(form: &FormState, cx: &FormContext<'_>) -> Element {
    form_to_element(form, cx).expect("writes")
}

fn rules(e: &Element) -> Value {
    serde_json::to_value(&e.rules).expect("rules")
}

fn refs(ids: &[(&str, &str, &str)]) -> Value {
    Value::Array(
        ids.iter()
            .map(|(id, name, ty)| json!({"id": id, "name": name, "type": ty}))
            .collect(),
    )
}

/// A 2024 species as the SRD writes it, with a trait from level 5 (a block).
fn elf_2024() -> Element {
    el(json!({
        "id": "ID_X_SPECIES_ELF", "name": "Elf", "type": "Species", "source": "X", "ruleset": "dnd2024",
        "description": "<p>Elf.</p>",
        "setters": [{"name": "short", "value": "Humanoid, Medium, Speed 30 ft."}],
        "rules": [
            {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
            {"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"},
            {"kind": "grant", "id": "ID_X_TRAIT_KEEN", "type": "Racial Trait"},
            {"kind": "grant", "id": "ID_X_TRAIT_BIG", "type": "Racial Trait", "level": 5},
            {"kind": "grant", "id": "ID_X_TRAIT_TRANCE", "type": "Racial Trait"}
        ],
        "data": {"creature_type": "Humanoid", "size": ["Medium"], "speed": 30,
                 "traits": ["ID_X_TRAIT_KEEN", "ID_X_TRAIT_BIG", "ID_X_TRAIT_TRANCE"], "lore": "kept"}
    }))
}

/// A 2014 race as Aurora writes it.
fn dwarf_2014() -> Element {
    el(json!({
        "id": "ID_X_RACE_DWARF", "name": "Dwarf", "type": "Race", "source": "X",
        "sheet": {"display": false},
        "setters": [{"name": "names", "value": "Bruenor, Dain", "attributes": {"type": "male"}}],
        "rules": [
            {"kind": "stat", "name": "constitution", "value": "2"},
            {"kind": "stat", "name": "innate speed", "value": "25", "bonus": "base"},
            {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
            {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
            {"kind": "grant", "id": "ID_LANGUAGE_COMMON", "type": "Language"},
            {"kind": "grant", "id": "ID_LANGUAGE_DWARVISH", "type": "Language"},
            {"kind": "stat", "name": "wisdom", "value": "1", "requirements": "!ID_X_GRANTS_MARK"},
            {"kind": "select", "type": "Language", "name": "Language (Dwarf)", "supports": "Standard||Exotic"},
            {"kind": "grant", "id": "ID_X_TRAIT_KEEN", "type": "Racial Trait"},
            {"kind": "select", "type": "Sub Race", "name": "Dwarven Subrace", "supports": "Dwarf"}
        ]
    }))
}

#[test]
fn species_types_open_in_their_forms_and_traits_are_child_kinds() {
    for (ty, id, group, editions) in [
        (
            "Species",
            "species",
            KindGroup::Main,
            &[Ruleset::Dnd2024][..],
        ),
        ("Race", "race", KindGroup::Main, &[Ruleset::Dnd2014][..]),
        (
            "Sub Race",
            "subRace",
            KindGroup::Main,
            &[Ruleset::Dnd2014][..],
        ),
        (
            "Race Variant",
            "raceVariant",
            KindGroup::Main,
            &[Ruleset::Dnd2014][..],
        ),
        (
            "Racial Trait",
            "racialTrait",
            KindGroup::Child,
            &[Ruleset::Dnd2014, Ruleset::Dnd2024][..],
        ),
        (
            "Species Trait Option",
            "speciesTraitOption",
            KindGroup::Child,
            &[Ruleset::Dnd2024][..],
        ),
    ] {
        let k = kind_for_type(ty);
        assert_eq!((k.id, k.group, k.editions), (id, group, editions), "{ty}");
    }
    assert_eq!(
        kind("racialTrait").expect("trait").parent_kinds,
        ["species", "race", "subRace", "raceVariant"]
    );
    assert_eq!(
        kind("speciesTraitOption").expect("option").parent_kinds,
        ["species", "racialTrait"]
    );
    let d = kind("race").expect("race").descriptor();
    let f = |key: &str| {
        serde_json::to_value(d.fields.iter().find(|f| f.key == key).expect(key)).expect("json")
    };
    assert_eq!(f("speed")["control"], "number");
    assert_eq!(
        (f("speed")["min"].clone(), f("speed")["max"].clone()),
        (json!(0), json!(1000))
    );
    assert_eq!(f("speed")["default"], 30);
    assert_eq!(
        f("speed")["bindings"],
        json!(["rule:stat innate speed", "data.speed"])
    );
    assert_eq!(f("languages")["control"], "references");
    assert_eq!(f("languages")["picker"], json!({"types": ["Language"]}));
    assert_eq!(f("traits")["picker"], json!({"types": ["Racial Trait"]}));
    assert_eq!(f("size")["choices"].as_array().map(Vec::len), Some(6));
    assert_eq!(f("size")["default"], json!(["Medium"]));
    assert_eq!(f("strength")["min"], -10);
    let sub = kind("subRace").expect("sub race");
    assert!(sub.field("parent").is_some_and(|p| p.required));
    assert!(sub.field("size").is_none(), "a subrace has its race's size");
    assert!(sub.field("speed").and_then(|s| s.default).is_none());
    assert!(kind("species")
        .expect("species")
        .field("strength")
        .is_none());
}

#[test]
fn a_2024_species_reads_its_rules_as_fields_and_writes_them_back() {
    with_cx(|cx| {
        let e = elf_2024();
        let form = element_to_form(&e, cx);
        assert_eq!(form.kind, "species");
        assert_eq!(form.fields["creatureType"], json!("Humanoid"));
        assert_eq!(form.fields["size"], json!(["Medium"]));
        assert_eq!(form.fields["speed"], json!(30));
        assert_eq!(form.fields["flySpeed"], json!(0));
        assert_eq!(form.fields["darkvision"], json!(0));
        assert_eq!(
            form.fields["traits"],
            refs(&[
                ("ID_X_TRAIT_KEEN", "Keen Senses", "Racial Trait"),
                ("ID_X_TRAIT_TRANCE", "Trance", "Racial Trait")
            ])
        );
        // the level-5 trait is a block; data mirrors belong to their fields, other data is kept
        assert_eq!(form.blocks.len(), 1);
        assert_eq!(form.blocks[0].kind, "grant");
        assert_eq!(form.memo.layout, ["size", "speed", "traits", "", "traits"]);
        let kept: Vec<&str> = form.preserved.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(kept, ["data.lore"]);
        assert_eq!(one(&form, cx), e);
    });
}

#[test]
fn a_size_choice_is_a_select_size() {
    with_cx(|cx| {
        let mut e = elf_2024();
        e.rules[0] = serde_json::from_value(
            json!({"kind": "select_size", "name": "Size", "options": ["Small", "Medium"]}),
        )
        .expect("rule");
        let form = element_to_form(&e, cx);
        assert_eq!(form.fields["size"], json!(["Small", "Medium"]));
        assert_eq!(one(&form, cx), e);
        // one size again: a grant in the same place, the mirror follows
        let back = one(&form.with_field("size", json!(["Small"])), cx);
        assert_eq!(
            rules(&back)[0],
            json!({"kind": "grant", "id": "ID_SIZE_SMALL", "type": "Size"})
        );
        assert_eq!(back.data["size"], json!(["Small"]));
        // the chips come back in size order
        let back = one(&form.with_field("size", json!(["Large", "Tiny"])), cx);
        assert_eq!(
            rules(&back)[0],
            json!({"kind": "select_size", "name": "Size", "options": ["Tiny", "Large"]})
        );
        // a select_size with another label stays a block
        e.rules[0] = serde_json::from_value(
            json!({"kind": "select_size", "name": "Height", "options": ["Small", "Medium"]}),
        )
        .expect("rule");
        let form = element_to_form(&e, cx);
        assert_eq!(form.fields["size"], json!([]));
        assert_eq!(form.blocks[0].kind, "select_size");
        assert_eq!(one(&form, cx), e);
    });
}

#[test]
fn a_2014_race_reads_increases_languages_darkvision_and_names() {
    with_cx(|cx| {
        let e = dwarf_2014();
        let form = element_to_form(&e, cx);
        assert_eq!(form.kind, "race");
        assert_eq!(form.fields["constitution"], json!(2));
        assert_eq!(
            form.fields["wisdom"],
            json!(0),
            "a conditional increase is a block"
        );
        assert_eq!(form.fields["speed"], json!(25));
        assert_eq!(form.fields["size"], json!(["Medium"]));
        assert_eq!(
            form.fields["darkvision"],
            json!(60),
            "the Darkvision element alone is 60 feet"
        );
        assert_eq!(
            form.fields["languages"],
            refs(&[
                ("ID_LANGUAGE_COMMON", "Common", "Language"),
                ("ID_LANGUAGE_DWARVISH", "Dwarvish", "Language")
            ])
        );
        assert_eq!(form.fields["languageChoice"], json!(1));
        assert_eq!(form.fields["maleNames"], json!(["Bruenor", "Dain"]));
        assert_eq!(form.fields["femaleNames"], json!([]));
        let blocks: Vec<&str> = form.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(
            blocks,
            ["stat", "select"],
            "the conditional increase and the subrace choice"
        );
        assert_eq!(one(&form, cx), e);
    });
}

#[test]
fn edits_rewrite_their_rules_in_place() {
    with_cx(|cx| {
        let e = dwarf_2014();
        let form = element_to_form(&e, cx);
        let edited = form
            .with_field("speed", json!(30))
            .with_field("darkvision", json!(120))
            .with_field("constitution", json!(1))
            .with_field("strength", json!(2))
            .with_field("languageChoice", json!(2))
            .with_field("maleNames", json!(["Dain", "Thorin"]))
            .with_field("name", json!("Deep Dwarf"));
        let back = one(&edited, cx);
        assert_eq!(
            rules(&back),
            json!([
                {"kind": "stat", "name": "strength", "value": "2"},
                {"kind": "stat", "name": "constitution", "value": "1"},
                {"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"},
                {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
                {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
                {"kind": "stat", "name": "darkvision:range", "value": "120", "bonus": "base"},
                {"kind": "grant", "id": "ID_LANGUAGE_COMMON", "type": "Language"},
                {"kind": "grant", "id": "ID_LANGUAGE_DWARVISH", "type": "Language"},
                {"kind": "stat", "name": "wisdom", "value": "1", "requirements": "!ID_X_GRANTS_MARK"},
                // the choice keeps its label (its persistence key) across the rename
                {"kind": "select", "type": "Language", "name": "Language (Dwarf)", "supports": "Standard||Exotic", "number": 2},
                {"kind": "grant", "id": "ID_X_TRAIT_KEEN", "type": "Racial Trait"},
                {"kind": "select", "type": "Sub Race", "name": "Dwarven Subrace", "supports": "Dwarf"}
            ])
        );
        assert_eq!(
            serde_json::to_value(&back.setters).expect("json"),
            json!([{"name": "names", "value": "Dain, Thorin", "attributes": {"type": "male"}}])
        );
        // read again, the edited fields are where they were written
        let again = element_to_form(&back, cx);
        assert_eq!(again.fields["darkvision"], json!(120));
        assert_eq!(again.fields["strength"], json!(2));
        // cleared fields write nothing, the rest stays in order
        let cleared = one(
            &form
                .with_field("darkvision", json!(0))
                .with_field("languages", json!([]))
                .with_field("languageChoice", json!(0)),
            cx,
        );
        let kinds: Vec<String> = cleared
            .rules
            .iter()
            .map(|r| {
                serde_json::to_value(r).expect("json")["kind"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert_eq!(kinds, ["stat", "stat", "grant", "stat", "grant", "select"]);
    });
}

#[test]
fn a_new_speed_or_sense_follows_the_field_before_it() {
    with_cx(|cx| {
        let e = elf_2024();
        let form = element_to_form(&e, cx);
        let back = one(
            &form
                .with_field("flySpeed", json!(50))
                .with_field("blindsight", json!(10)),
            cx,
        );
        let r = rules(&back);
        assert_eq!(r[1]["name"], "innate speed");
        assert_eq!(
            r[2],
            json!({"kind": "stat", "name": "innate speed:fly", "value": "50", "bonus": "base"})
        );
        assert_eq!(
            r[3],
            json!({"kind": "sense", "sense": "blindsight", "range": 10})
        );
        assert_eq!(r[4]["id"], "ID_X_TRAIT_KEEN");
        let again = element_to_form(&back, cx);
        assert_eq!(again.fields["flySpeed"], json!(50));
        assert_eq!(again.fields["blindsight"], json!(10));
        assert_eq!(again.blocks.len(), 1);
        // data.speed follows the walking speed only
        assert_eq!(back.data["speed"], json!(30));
    });
}

#[test]
fn the_traits_mirror_keeps_traits_other_blocks_give() {
    with_cx(|cx| {
        let e = elf_2024();
        let form = element_to_form(&e, cx);
        let edited = form.with_field(
            "traits",
            refs(&[
                ("ID_X_TRAIT_TRANCE", "Trance", "Racial Trait"),
                ("ID_X_TRAIT_NEW", "Stone Skin", "Racial Trait"),
            ]),
        );
        let back = one(&edited, cx);
        assert_eq!(
            back.data["traits"],
            json!(["ID_X_TRAIT_BIG", "ID_X_TRAIT_TRANCE", "ID_X_TRAIT_NEW"])
        );
        let ids: Vec<Value> = rules(&back)
            .as_array()
            .expect("rules")
            .iter()
            .map(|r| r["id"].clone())
            .collect();
        assert_eq!(
            ids,
            [
                json!("ID_SIZE_MEDIUM"),
                Value::Null,
                json!("ID_X_TRAIT_TRANCE"),
                json!("ID_X_TRAIT_BIG"),
                json!("ID_X_TRAIT_NEW")
            ]
        );
    });
}

#[test]
fn a_new_species_and_race_are_written_from_their_fields() {
    with_cx(|cx| {
        let mut form = kind("species").expect("species").new_form("e");
        form.memo.id = Some("ID_X_SPECIES_EMBERKIN".into());
        let form = form
            .with_field("name", json!("Emberkin"))
            .with_field("darkvision", json!(120))
            .with_field("swimSpeed", json!(30))
            .with_field(
                "traits",
                refs(&[("ID_X_TRAIT_NEW", "Stone Skin", "Racial Trait")]),
            );
        let e = one(&form, cx);
        assert_eq!(e.ty, "Species");
        assert_eq!(
            rules(&e),
            json!([
                {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
                {"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"},
                {"kind": "stat", "name": "innate speed:swim", "value": "30", "bonus": "base"},
                {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
                {"kind": "stat", "name": "darkvision:range", "value": "120", "bonus": "base"},
                {"kind": "grant", "id": "ID_X_TRAIT_NEW", "type": "Racial Trait"}
            ])
        );
        // the creature type has no other location; mirrors are written only when an element has them
        assert_eq!(
            serde_json::to_value(&e.data).expect("data"),
            json!({"creature_type": "Humanoid"})
        );
        let again = element_to_form(&e, cx);
        for key in [
            "size",
            "speed",
            "swimSpeed",
            "darkvision",
            "traits",
            "creatureType",
        ] {
            assert_eq!(again.fields[key], form.fields[key], "{key}");
        }

        let mut race = kind("race").expect("race").new_form("e");
        race.memo.id = Some("ID_X_RACE_STONEFOLK".into());
        let race = race
            .with_field("name", json!("Stonefolk"))
            .with_field("strength", json!(2))
            .with_field("languageChoice", json!(1))
            .with_field("darkvision", json!(60));
        let e = one(&race, cx);
        assert_eq!(e.ty, "Race");
        assert_eq!(
            rules(&e),
            json!([
                {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
                {"kind": "stat", "name": "strength", "value": "2"},
                {"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"},
                {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
                {"kind": "grant", "id": "ID_LANGUAGE_COMMON", "type": "Language"},
                {"kind": "select", "type": "Language", "name": "Language (Stonefolk)", "supports": "Standard||Exotic"}
            ])
        );
        assert_eq!(e.data.get("creature_type"), Some(&json!("Humanoid")));
    });
}

#[test]
fn a_subrace_names_its_race_by_its_first_tag() {
    with_cx(|cx| {
        let e = el(json!({
            "id": "ID_X_SUBRACE_HILL", "name": "Hill Dwarf", "type": "Sub Race", "source": "X",
            "supports": ["Dwarf", "Hardy"],
            "rules": [{"kind": "stat", "name": "wisdom", "value": "1"},
                      {"kind": "grant", "id": "ID_X_TRAIT_KEEN", "type": "Racial Trait"}]
        }));
        let form = element_to_form(&e, cx);
        assert_eq!(form.kind, "subRace");
        assert_eq!(form.fields["parent"], json!("Dwarf"));
        assert_eq!(form.fields["tags"], json!(["Hardy"]));
        assert_eq!(form.fields["wisdom"], json!(1));
        assert!(form.blocks.is_empty());
        assert_eq!(one(&form, cx), e);
        let moved = one(&form.with_field("parent", json!("Gnome")), cx);
        assert_eq!(moved.supports, ["Gnome", "Hardy"]);
        // a new subrace: the parent tag, no speed or size of its own
        let mut new = kind("subRace").expect("sub race").new_form("e");
        new.memo.id = Some("ID_X_SUBRACE_ASH".into());
        let new = new
            .with_field("name", json!("Ash Dwarf"))
            .with_field("parent", json!("Dwarf"))
            .with_field("charisma", json!(1));
        let e = one(&new, cx);
        assert_eq!(e.ty, "Sub Race");
        assert_eq!(e.supports, ["Dwarf"]);
        assert_eq!(
            rules(&e),
            json!([{"kind": "stat", "name": "charisma", "value": "1"}])
        );
        assert!(e.data.is_empty());
        // a race variant is the same shape
        let mut variant = kind("raceVariant").expect("variant").new_form("e");
        variant.memo.id = Some("ID_X_RACE_VARIANT_FEY".into());
        let v = one(
            &variant
                .with_field("name", json!("Fey Human"))
                .with_field("parent", json!("Human")),
            cx,
        );
        assert_eq!(
            (v.ty.as_str(), v.supports.clone()),
            ("Race Variant", vec!["Human".to_string()])
        );
    });
}

#[test]
fn bad_values_are_told_at_their_field() {
    with_cx(|cx| {
        let mut form = kind("race").expect("race").new_form("e");
        form.memo.id = Some("ID_X_RACE_A".into());
        let form = form.with_field("name", json!("A"));
        for (key, value, says) in [
            ("speed", json!(-5), "Write it in feet, from 0 to 1000."),
            (
                "darkvision",
                json!(5000),
                "Write it in feet, from 0 to 1000.",
            ),
            ("size", json!(["Enormous"]), "Choose sizes from the list."),
            ("strength", json!(11), "An increase goes from -10 to 10."),
            (
                "languageChoice",
                json!(20),
                "Choose from 0 to 10 languages.",
            ),
        ] {
            let problems = check_fields(&form.with_field(key, value), cx);
            assert_eq!(problems.len(), 1, "{key}: {problems:?}");
            assert_eq!(problems[0].path, format!("fields.{key}"));
            assert_eq!(problems[0].message, says);
        }
        let mut sub = kind("subRace").expect("sub race").new_form("e");
        sub.memo.id = Some("ID_X_SUBRACE_A".into());
        let problems = check_fields(
            &sub.with_field("name", json!("A"))
                .with_field("parent", json!("Dwarf|Elf")),
            cx,
        );
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].path, "fields.parent");
        assert!(problems[0].message.starts_with("A tag can't contain"));
    });
}

#[test]
fn validator_locations_point_at_the_species_fields() {
    with_cx(|cx| {
        let e = dwarf_2014();
        let form = element_to_form(&e, cx);
        let at = |loc: &str| form_path_for_location(&form, &e, loc, cx);
        assert_eq!(at("rules[0]").as_deref(), Some("fields.constitution"));
        assert_eq!(at("rules[3]").as_deref(), Some("fields.darkvision"));
        assert_eq!(at("rules[5]").as_deref(), Some("fields.languages"));
        assert_eq!(at("rules[6]").as_deref(), Some("blocks.0"));
        assert_eq!(at("rules[7]").as_deref(), Some("fields.languageChoice"));
        assert_eq!(at("rules[9]").as_deref(), Some("blocks.1"));
        assert_eq!(at("setters[0]").as_deref(), Some("fields.maleNames"));
        assert_eq!(at("data.speed").as_deref(), Some("fields.speed"));
        let elf = elf_2024();
        let form = element_to_form(&elf, cx);
        assert_eq!(
            form_path_for_location(&form, &elf, "data.traits", cx).as_deref(),
            Some("fields.traits")
        );
    });
}

#[test]
fn the_stored_order_survives_forgetting_the_field_memos() {
    with_cx(|cx| {
        let e = dwarf_2014();
        let mut form = element_to_form(&e, cx);
        form.memo.fields.clear();
        let back = one(&form, cx);
        assert_eq!(rules(&back), rules(&e));
        assert_eq!(back.setters, e.setters);
    });
}

#[test]
fn traits_and_trait_options_round_trip_with_the_shared_parts() {
    with_cx(|cx| {
        for e in [
            el(json!({
                "id": "ID_X_TRAIT_KEEN", "name": "Keen Senses", "type": "Racial Trait", "source": "X",
                "description": "<p>Keen.</p>",
                "rules": [{"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
                          {"kind": "stat", "name": "darkvision:range", "value": "60", "bonus": "base"}]
            })),
            el(json!({
                "id": "ID_X_OPTION_DROW", "name": "Drow", "type": "Species Trait Option", "source": "X",
                "ruleset": "dnd2024", "supports": ["Elven Lineage"],
                "data": {"lineage": "Drow"}
            })),
        ] {
            let form = element_to_form(&e, cx);
            assert!(
                form.memo.layout.is_empty(),
                "every rule of a trait is a block"
            );
            assert_eq!(one(&form, cx), e);
        }
    });
}
