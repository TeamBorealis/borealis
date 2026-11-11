//! The creature form (spec 4.11): a stat block read into its fields and written back unchanged, edits that rewrite
//! the parts worked out from others (XP and proficiency bonus from the CR, passive Perception from Wisdom, the stat
//! block edition from the edition), a new creature in the model's shape, the problems a field can't write, and the
//! validator's locations at their fields. Every bundled creature is covered by `tests/form_identity.rs` and
//! `tests/creature_form.rs`.

use crate::expr_build::condition::ConditionContext;
use crate::form::{
    assign_ids, descriptors, element_to_form, form_path_for_location, form_to_element, kind,
    kind_for_type, new_creature_form, CatalogBlockCodec, FormContext, NoLookup, CREATURE_KIND,
};
use crate::ids::IdGenerator;
use borealis_content::{Element, MonsterData, Ruleset};
use serde_json::{json, Value};

fn el(v: Value) -> Element {
    serde_json::from_value(v).expect("element")
}

fn with_cx<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup: &NoLookup,
        blocks: &codec,
    };
    f(&cx)
}

/// A 2024 stat block in the bundled shape (a wolf-like beast with a bite).
fn wolf() -> Element {
    el(json!({
        "id": "ID_EI_CREATURE_EMBER_WOLF", "name": "Ember Wolf", "type": "Monster", "source": "Ember Isles",
        "ruleset": "dnd2024",
        "data": {
            "ruleset": "dnd2024", "size": ["medium"], "creature_type": "beast", "alignment": "unaligned",
            "ac": [{"value": 12}], "hp": {"average": 11, "formula": "2d8+2"}, "speed": {"walk": 40},
            "initiative": {"modifier": 2, "score": 12},
            "abilities": {"str": 14, "dex": 15, "con": 12, "int": 3, "wis": 12, "cha": 6},
            "skills": {"perception": 5, "stealth": 4},
            "senses": {"darkvision": 60, "passive_perception": 15},
            "cr": "1/4", "xp": 50, "proficiency_bonus": 2,
            "traits": [{"name": "Pack Tactics", "text": "Advantage when an ally is near."}],
            "actions": [{"name": "Bite", "text": "Melee Attack Roll: +4, reach 5 ft. Hit: 5 (1d6 + 2) Piercing damage.",
                "attack": {"kind": "melee", "to_hit": 4, "reach": 5,
                    "damage": [{"average": 5, "dice": "1d6+2", "type": "piercing"}]}}]
        }
    }))
}

#[test]
fn a_creature_opens_in_the_creature_form_with_its_tabs_and_controls() {
    assert_eq!(kind_for_type("Monster").id, CREATURE_KIND);
    let k = kind(CREATURE_KIND).expect("the creature form");
    let tabs: Vec<&str> = k.tabs.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        tabs,
        [
            "general",
            "statistics",
            "actions",
            "legendary",
            "rules",
            "description"
        ]
    );
    for key in ["requirements", "allowDuplicate", "sheet"] {
        assert!(k.field(key).is_none(), "{key} is not a creature's");
    }
    let d = descriptors()
        .into_iter()
        .find(|d| d.id == CREATURE_KIND)
        .expect("descriptor");
    let control = |key: &str| {
        d.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.control.clone())
    };
    assert_eq!(control("armorClass").as_deref(), Some("armorClass"));
    assert_eq!(control("actions").as_deref(), Some("entries"));
    assert_eq!(control("challenge").as_deref(), Some("challenge"));
    assert_eq!(control("saves").as_deref(), Some("bonuses"));
    let cr = d
        .fields
        .iter()
        .find(|f| f.key == "challenge")
        .expect("challenge");
    assert_eq!(
        cr.choices.as_ref().map_or(0, Vec::len),
        34,
        "0, 1/8, 1/4, 1/2 and 1 to 30"
    );
}

#[test]
fn a_stat_block_reads_into_its_fields_and_writes_back_unchanged() {
    with_cx(|cx| {
        let e = wolf();
        let form = element_to_form(&e, cx);
        assert_eq!(form.kind, CREATURE_KIND);
        assert!(form.preserved.is_empty(), "{:?}", form.preserved);
        let f = &form.fields;
        assert_eq!(f["size"], json!(["medium"]));
        assert_eq!(f["creatureType"], json!("beast"));
        assert_eq!(
            f["armorClass"],
            json!({"entries": [{"value": 12}], "note": ""})
        );
        assert_eq!(
            f["hitPoints"],
            json!({"average": 11, "formula": "2d8+2", "note": ""})
        );
        // the usual passive Perception (10 + Perception 5), the CR's XP and proficiency bonus: left to the form
        assert_eq!(
            f["senses"],
            json!({"darkvision": 60, "passive_perception": null})
        );
        assert_eq!(
            f["challenge"],
            json!({"cr": "1/4", "note": "", "xp": null, "xpLair": null, "proficiencyBonus": null})
        );
        assert_eq!(f["actions"][0]["attack"]["to_hit"], json!(4));
        assert_eq!(form_to_element(&form, cx).expect("writes"), e);
    });
}

#[test]
fn the_cr_wisdom_and_edition_rewrite_the_parts_that_follow_them() {
    with_cx(|cx| {
        let form = element_to_form(&wolf(), cx);
        let mut f = form.with_field(
            "challenge",
            json!({"cr": "5", "note": "", "xp": null, "xpLair": null, "proficiencyBonus": null}),
        );
        f = f.with_field("skills", json!({"stealth": 4}));
        f = f.with_field(
            "abilities",
            json!({"str": 14, "dex": 15, "con": 12, "int": 3, "wis": 16, "cha": 6}),
        );
        f = f.with_field("edition", json!("dnd2014"));
        let e = form_to_element(&f, cx).expect("writes");
        assert_eq!(e.data["cr"], json!("5"));
        assert_eq!(e.data["xp"], json!(1800));
        assert_eq!(e.data["proficiency_bonus"], json!(3));
        assert_eq!(
            e.data["senses"]["passive_perception"],
            json!(13),
            "10 + Wisdom +3"
        );
        assert_eq!(e.data["ruleset"], json!("dnd2014"));
        // an XP of its own is kept (the validator warns), and no CR means no XP
        let own = f.with_field(
            "challenge",
            json!({"cr": "5", "note": "", "xp": 2000, "xpLair": null, "proficiencyBonus": null}),
        );
        let e = form_to_element(&own, cx).expect("writes");
        assert_eq!(e.data["xp"], json!(2000));
        let m = MonsterData::from_element(&e).expect("stat block");
        assert_eq!(m.warnings().len(), 1, "a non-standard XP");
        let none = f.with_field(
            "challenge",
            json!({"cr": "", "note": "Scales with the spell's level", "xp": 900, "xpLair": null, "proficiencyBonus": null}),
        );
        let e = form_to_element(&none, cx).expect("writes");
        assert_eq!(e.data.get("cr"), None);
        assert_eq!(e.data.get("xp"), None);
        assert_eq!(e.data["cr_note"], json!("Scales with the spell's level"));
        assert!(MonsterData::from_element(&e)
            .expect("reads")
            .validate()
            .is_empty());
    });
}

#[test]
fn a_new_creature_is_written_in_the_stat_block_shape() {
    with_cx(|cx| {
        let mut form = new_creature_form(Ruleset::Dnd2024)
            .with_field("name", json!("Cinder Boar"))
            .with_field("creatureType", json!("beast"))
            .with_field("armorClass", json!({"entries": [{"value": 11, "source": " natural armor "}], "note": ""}))
            .with_field("hitPoints", json!({"average": null, "formula": "2d8 + 2", "note": ""}))
            .with_field(
                "challenge",
                json!({"cr": "1/4", "note": "", "xp": null, "xpLair": null, "proficiencyBonus": null}),
            )
            .with_field(
                "actions",
                json!([{"name": "Tusk", "text": "Melee Attack Roll: +3, reach 5 ft. Hit: 4 (1d6 + 1) Slashing damage.",
                    "condition": "",
                    "attack": {"kind": "melee", "to_hit": 3, "reach": 5,
                        "damage": [{"average": 4, "dice": "1d6 + 1", "type": "slashing"}]}}]),
            );
        let mut ids =
            IdGenerator::new("EI", Vec::<String>::new(), Vec::<String>::new()).expect("generator");
        form = assign_ids(&form, &mut ids);
        let e = form_to_element(&form, cx).expect("writes");
        assert_eq!(e.ty, "Monster");
        assert_eq!(e.id, "ID_EI_CREATURE_CINDER_BOAR");
        assert_eq!(e.ruleset, Some(Ruleset::Dnd2024));
        let m = MonsterData::from_element(&e).expect("a typed stat block");
        assert!(m.validate().is_empty(), "{:?}", m.validate());
        assert!(m.warnings().is_empty());
        assert_eq!(e.data["hp"], json!({"average": 11, "formula": "2d8+2"}));
        assert_eq!(
            e.data["ac"],
            json!([{"value": 11, "source": "natural armor"}])
        );
        assert_eq!(e.data["speed"], json!({"walk": 30}));
        assert_eq!(e.data["senses"], json!({"passive_perception": 10}));
        assert_eq!(e.data["xp"], json!(50));
        assert_eq!(e.data["actions"][0].get("condition"), None);
        assert_eq!(
            e.data["actions"][0]["attack"]["damage"][0]["dice"],
            json!("1d6+1")
        );
        // read again: the same values
        let again = element_to_form(&e, cx);
        assert_eq!(
            again.fields["hitPoints"],
            json!({"average": 11, "formula": "2d8+2", "note": ""})
        );
        assert_eq!(again.fields["challenge"]["xp"], Value::Null);
    });
}

#[test]
fn values_the_stat_block_cant_hold_are_problems_at_their_fields() {
    with_cx(|cx| {
        let mut ids =
            IdGenerator::new("EI", Vec::<String>::new(), Vec::<String>::new()).expect("generator");
        let form = assign_ids(
            &new_creature_form(Ruleset::Dnd2014).with_field("name", json!("Mote")),
            &mut ids,
        );
        let problem = |f: &crate::form::FormState| -> Vec<(String, String)> {
            form_to_element(f, cx)
                .expect_err("can't write")
                .into_iter()
                .map(|p| (p.path, p.message))
                .collect()
        };
        assert_eq!(
            problem(&form),
            [(
                "fields.creatureType".to_string(),
                "Choose the creature type.".to_string()
            )]
        );
        let typed = form.with_field("creatureType", json!("fey"));
        let bad_dice = typed.with_field(
            "hitPoints",
            json!({"average": null, "formula": "2d", "note": ""}),
        );
        assert_eq!(
            problem(&bad_dice),
            [(
                "fields.hitPoints".to_string(),
                "Write the hit dice as dice, like 2d8+2.".to_string()
            )]
        );
        let bad_cr = typed.with_field(
            "challenge",
            json!({"cr": "3/4", "note": "", "xp": null, "xpLair": null, "proficiencyBonus": null}),
        );
        assert_eq!(
            problem(&bad_cr)[0].1,
            "Choose a challenge rating from the list."
        );
        // a 2014 creature writes no initiative and a 2014 stat block
        let e = form_to_element(&typed, cx).expect("writes");
        assert_eq!(e.data["ruleset"], json!("dnd2014"));
        assert_eq!(e.data.get("initiative"), None);
    });
}

#[test]
fn the_validator_s_locations_point_at_the_creature_s_fields() {
    with_cx(|cx| {
        let e = wolf();
        let form = element_to_form(&e, cx);
        let at = |loc: &str| form_path_for_location(&form, &e, loc, cx);
        assert_eq!(at("data.ac").as_deref(), Some("fields.armorClass"));
        assert_eq!(at("data.hp.formula").as_deref(), Some("fields.hitPoints"));
        assert_eq!(at("data.xp").as_deref(), Some("fields.challenge"));
        assert_eq!(
            at("data.abilities.str").as_deref(),
            Some("fields.abilities")
        );
        assert_eq!(at("data.actions[0]").as_deref(), Some("fields.actions.0"));
        assert_eq!(
            at("data.traits[0].damage[1]").as_deref(),
            Some("fields.traits.0")
        );
        assert_eq!(
            at("data.legendary.per_round").as_deref(),
            Some("fields.legendary")
        );
    });
}
