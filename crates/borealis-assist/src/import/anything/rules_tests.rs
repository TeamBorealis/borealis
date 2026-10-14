//! Rule catalogue, validation and the deterministic `resource` fixes.

use super::*;
use borealis_content::rule_kinds::spec;

#[test]
fn catalogue_lists_every_registered_kind() {
    let c = catalogue();
    for s in KNOWN_KINDS {
        assert!(c.contains(&format!("\n{}", s.kind)), "{} missing", s.kind);
    }
    assert!(c.contains("resource*: id, name, max"));
}

#[test]
fn shape_hints_name_registered_kinds_and_fields() {
    for (kind, fields, _) in SHAPES {
        let s = spec(kind).unwrap_or_else(|| panic!("{kind} not registered"));
        for f in *fields {
            assert!(s.reads(f), "{kind}.{f} is not read");
        }
    }
}

#[test]
fn invented_kinds_fields_and_broken_resources_are_problems() {
    let el = json!({"rules": [
        {"kind": "grant", "id": "ID_X", "type": "Proficiency"},
        {"kind": "strike_bonus", "amount": 2},
        {"kind": "resource", "name": "Ember", "max": 3, "requires": "x"},
        {"kind": "resource", "id": "ember", "name": "Ember", "max": {"type": "fixed", "value": 3},
         "recharge": ["long_rest"]}
    ]});
    let p = rule_problems(&el);
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p[0].contains("strike_bonus"));
    assert!(p.iter().any(|m| m.contains("\"requires\"")));
    assert!(p.iter().any(|m| m.contains("no `id`")));
    assert!(p.iter().any(|m| m.contains("`max` 3")));
}

#[test]
fn ability_increase_name_lists_become_choice_objects() {
    let mut el = json!({"rules": [
        {"kind": "ability_increase", "name": "Pick", "cap": 1,
         "options": ["strength", "Dex"]},
        {"kind": "ability_increase", "name": "Any", "cap": 20,
         "options": ["strength", "dexterity", "wisdom"]},
        {"kind": "ability_increase", "options": [{"abilities": 1, "amount": 2}], "cap": 20}
    ]});
    assert_eq!(normalise_resources(&mut el).len(), 2);
    assert_eq!(
        el["rules"][0],
        json!({"kind": "ability_increase", "name": "Pick", "cap": 20,
            "options": [{"abilities": ["strength", "dexterity"], "pick": 1, "amount": 1}]})
    );
    assert_eq!(
        el["rules"][1]["options"],
        json!([{"abilities": ["strength", "dexterity", "wisdom"], "pick": 1, "amount": 1}])
    );
    assert!(rule_problems(&el).is_empty(), "{:?}", rule_problems(&el));
    let bad = json!({"rules": [{"kind": "ability_increase", "options": ["luck"]}]});
    assert_eq!(rule_problems(&bad).len(), 1);
}

#[test]
fn lasting_movement_modes_become_speed_stats() {
    let mut el = json!({"rules": [
        {"kind": "movement_mode", "mode": "climb", "speed": "walk"},
        {"kind": "movement_mode", "mode": "swim", "speed": 40},
        {"kind": "movement_mode", "mode": "fly", "speed": "speed", "duration": "10 minutes",
         "action": "bonus_action"}
    ]});
    assert_eq!(normalise_abilities(&mut el).len(), 2);
    assert_eq!(
        el["rules"][0],
        json!({"kind": "stat", "name": "speed:climb", "value": "speed"})
    );
    assert_eq!(el["rules"][1]["value"], json!("40"));
    assert_eq!(el["rules"][2]["kind"], json!("movement_mode"));
}

#[test]
fn resources_are_normalised_only_when_unambiguous() {
    let mut el = json!({"rules": [
        {"kind": "resource", "name": "Ember  Coals", "max": 2, "recharge": "Short Rest"},
        {"kind": "resource", "name": "Odd", "max": "lots", "recharge": "whenever"},
        {"kind": "resource_cost", "resource": "EMBER COALS", "cost": 1}
    ]});
    let mut notes = normalise_resources(&mut el);
    let known = resources_of(&el);
    notes.extend(normalise_references(&mut el, &known));
    let r = &el["rules"];
    assert_eq!(r[0]["id"], "ember coals");
    assert_eq!(r[0]["max"], json!({"type": "fixed", "value": 2}));
    assert_eq!(r[0]["recharge"], json!(["short_rest", "long_rest"]));
    assert_eq!(r[1]["id"], "odd");
    assert_eq!(r[1]["max"], "lots");
    assert_eq!(r[1]["recharge"], "whenever");
    assert_eq!(r[2]["resource"], "ember coals");
    assert_eq!(notes.len(), 5, "{notes:?}");
    assert_eq!(rule_problems(&el).len(), 2, "{:?}", rule_problems(&el));
}

/// `(resource name, max)` of a level-`level` fighter of a species whose only rules are `rules`.
fn species_pool(rules: Value, level: u32) -> Vec<(String, i32)> {
    use borealis_engine::{commands, Choice, Engine};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = borealis_content::ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let doc = json!({"format": borealis_content::file::ContentFormat::ID, "elements": [
        {"id": "ID_T_RACE_LANTERNFOLK", "name": "Lanternfolk", "type": "Race", "source": "T",
         "ruleset": "dnd2024", "rules": rules}]});
    let file = borealis_content::file::ContentFile::from_json_str(&doc.to_string()).unwrap();
    let store = b
        .add_file("t", borealis_content::file::PackOrigin::Custom, file)
        .build();
    let engine = Engine::new(&store);
    let mut s = commands::new_character(&engine, borealis_content::Ruleset::Dnd2024);
    for (ty, id) in [
        ("Race", "ID_T_RACE_LANTERNFOLK"),
        ("Class", "ID_2024_CLASS_FIGHTER"),
    ] {
        let key = commands::slot_key(&engine.derive(&s), "ID_LEVEL_1", ty, 1).expect(ty);
        s = commands::select(&engine, &s, &key, Choice::Element(id.into()))
            .unwrap()
            .state;
    }
    for _ in 1..level {
        s = commands::level_up_main(&engine, &s, None).unwrap().state;
    }
    engine
        .derive(&s)
        .resources
        .iter()
        .map(|r| (r.name.clone(), r.max))
        .collect()
}

#[test]
fn level_scaled_species_pools_in_both_written_shapes_grow_with_level() {
    // Self-replacing level-gated steps, and a `stat` max over level-gated stat rules.
    let steps = json!([
        {"kind": "resource", "id": "sparks", "name": "Sparks", "level": 1,
         "max": {"type": "fixed", "value": 1}, "recharge": ["long_rest"]},
        {"kind": "resource", "id": "sparks", "name": "Sparks", "level": 3, "replaces": "sparks",
         "max": {"type": "fixed", "value": 2}, "recharge": ["long_rest"]},
        {"kind": "resource", "id": "sparks", "name": "Sparks", "level": 5, "replaces": "sparks",
         "max": {"type": "fixed", "value": 3}, "recharge": ["long_rest"]}
    ]);
    let stats = json!([
        {"kind": "stat", "name": "sparks:max", "value": "1"},
        {"kind": "stat", "name": "sparks:max", "value": "1", "level": 3},
        {"kind": "stat", "name": "sparks:max", "value": "1", "level": 5},
        {"kind": "resource", "id": "sparks", "name": "Sparks",
         "max": {"type": "stat", "stat": "sparks:max"}, "recharge": ["long_rest"]}
    ]);
    for rules in [steps, stats] {
        assert!(rule_problems(&json!({"rules": rules.clone()})).is_empty());
        for (level, want) in [(1, 1), (3, 2), (5, 3)] {
            let pools = species_pool(rules.clone(), level);
            assert!(
                pools.contains(&("Sparks".to_string(), want)),
                "level {level}: {pools:?}"
            );
        }
    }
}

#[test]
fn once_a_day_recharges_are_long_rests_without_a_repair() {
    for recharge in [
        json!("dawn"),
        json!(["dawn"]),
        json!("at dawn"),
        json!("Daily"),
        json!(["Each Day at Dawn"]),
        json!("once per day"),
    ] {
        let mut el = json!({"rules": [{"kind": "resource", "id": "glow", "name": "Glow",
            "max": {"type": "fixed", "value": 1}, "recharge": recharge}]});
        let notes = normalise_resources(&mut el);
        assert_eq!(
            el["rules"][0]["recharge"],
            json!(["long_rest"]),
            "{recharge}"
        );
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(rule_problems(&el).is_empty(), "{:?}", rule_problems(&el));
    }
    let mut both = json!({"rules": [{"kind": "resource", "id": "glow", "name": "Glow",
        "max": {"type": "fixed", "value": 1}, "recharge": ["short_rest", "dawn"]}]});
    normalise_resources(&mut both);
    assert_eq!(
        both["rules"][0]["recharge"],
        json!(["short_rest", "long_rest"])
    );
}

#[test]
fn resource_formulas_use_the_engine_dialect_or_are_problems() {
    let mut el = json!({"data": {"class": "Tidecaller"}, "rules": [
        {"kind": "resource", "id": "brine", "name": "Brine",
         "max": {"type": "formula", "expr": "1 + wisdom"}, "recharge": ["long_rest"]},
        {"kind": "resource", "id": "spray", "name": "Spray", "max": "max(1, dex mod)"},
        {"kind": "resource", "id": "tide", "name": "Tide",
         "max": {"type": "formula", "expr": "half your speed"}}
    ]});
    let notes = normalise_resources(&mut el);
    let r = &el["rules"];
    assert_eq!(r[0]["max"]["expr"], "1 + wisdom:modifier");
    assert_eq!(
        r[1]["max"],
        json!({"type": "formula", "expr": "max(1, dexterity:modifier)"})
    );
    assert_eq!(r[2]["max"]["expr"], "half your speed");
    assert_eq!(notes.len(), 2, "{notes:?}");
    let p = rule_problems(&el);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("\"half your speed\"") && p[0].contains("dexterity:modifier"));
}

#[test]
fn strip_removes_and_reports_everything_the_engine_ignores() {
    let mut el = json!({"rules": [
        {"kind": "strike_bonus", "amount": 2},
        {"kind": "resource", "id": "ember", "name": "Ember", "max": {"type": "fixed", "value": 1},
         "requires": "x"},
        {"kind": "resource", "name": "Nameless", "max": "lots"},
        {"kind": "stat", "name": "speed", "value": "5"}
    ]});
    let notes = strip_invalid(&mut el);
    assert_eq!(notes.len(), 3, "{notes:?}");
    let rules = el["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2);
    assert!(rules[0].get("requires").is_none());
    assert!(rule_problems(&el).is_empty());
}

#[test]
fn a_damage_type_select_param_on_a_resistance_is_a_select_over_the_resistance_conditions() {
    let mut el = json!({"description": "<p>You have resistance to acid, cold or fire damage \
    (your choice).</p>", "rules": [
        {"kind": "select_param", "name": "Wand Resistance", "options": ["Acid", "cold", "fire"],
         "level": 3},
        {"kind": "select_param", "name": "Element", "options": ["acid", "sunlight"]}
    ]});
    let notes = normalise_abilities(&mut el);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        el["rules"][0],
        json!({"kind": "select", "type": "Condition", "name": "Wand Resistance", "level": 3,
            "supports": "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_ACID|\
ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_COLD|ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE"})
    );
    assert_eq!(
        el["rules"][1]["kind"], "select_param",
        "not all damage types"
    );
    assert!(rule_problems(&el).is_empty(), "{:?}", rule_problems(&el));
    // Damage types without resistance (a breath weapon's element) stay a free parameter.
    let mut breath = json!({"description": "<p>Choose the damage type of your breath.</p>",
        "rules": [{"kind": "select_param", "name": "Breath", "options": ["fire", "cold"]}]});
    assert!(normalise_resources(&mut breath).is_empty());
    assert_eq!(breath["rules"][0]["kind"], "select_param");
}
