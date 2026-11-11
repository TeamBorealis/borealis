//! The section builder per caster type (spec 4.6, 18.1). The bundled SRD classes' stored rules against it are in
//! `tests/caster_section.rs`.

use super::*;

fn stats_of(rules: &[Value], name: &str) -> Vec<(i64, u64)> {
    rules
        .iter()
        .filter(|r| r["name"] == name)
        .map(|r| {
            (
                r["value"].as_str().unwrap().parse().unwrap(),
                r["level"].as_u64().unwrap_or(1),
            )
        })
        .collect()
}

#[test]
fn every_caster_type_has_its_srd_table() {
    let at = |t: &SlotTable, level: usize| t.grid()[level - 1];
    assert_eq!(at(&SlotTable::Full, 1), [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::Full, 20), [4, 3, 3, 3, 3, 2, 2, 1, 1]);
    assert_eq!(at(&SlotTable::HalfUp, 1), [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::HalfUp, 5), [4, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::HalfDown, 1), [0; 9]);
    assert_eq!(at(&SlotTable::HalfDown, 2), [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::Third, 2), [0; 9]);
    assert_eq!(at(&SlotTable::Third, 3), [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::Third, 20), [4, 3, 3, 1, 0, 0, 0, 0, 0]);
    assert_eq!(at(&SlotTable::Pact, 11), [0, 0, 0, 0, 3, 0, 0, 0, 0]);
    let mut grid = [[0; 9]; 20];
    grid[4][0] = 1;
    assert_eq!(
        SlotTable::from_key("custom", Some(grid)).unwrap().grid(),
        grid
    );
    assert_eq!(SlotTable::from_key("custom", None), None);
    assert_eq!(SlotTable::from_key("nonsense", None), None);
    for t in STANDARD_TABLES {
        assert_eq!(SlotTable::from_key(t.key(), None), Some(t.clone()));
    }
}

#[test]
fn slot_stats_add_up_to_the_table_in_each_edition() {
    for edition in [Edition::Y2014, Edition::Y2024] {
        for t in STANDARD_TABLES.iter().filter(|t| **t != SlotTable::Pact) {
            let rules = slot_rules(t, "Sage", edition);
            for k in 1..=9 {
                let mut total = 0;
                let stats = stats_of(&rules, &format!("sage:spellcasting:slots:{k}"));
                for level in 1..=20u64 {
                    total += stats
                        .iter()
                        .filter(|(_, l)| *l == level)
                        .map(|(v, _)| v)
                        .sum::<i64>();
                    assert_eq!(
                        total,
                        i64::from(t.grid()[level as usize - 1][k - 1]),
                        "{t:?} {edition:?} {k} {level}"
                    );
                }
            }
            let gated_first = rules.iter().any(|r| r["level"] == 1);
            assert_eq!(
                gated_first,
                edition == Edition::Y2014 && t.grid()[0][0] > 0,
                "{t:?} {edition:?}"
            );
        }
    }
}

#[test]
fn pact_magic_is_a_pool_in_2014_and_a_rule_in_2024() {
    let old = slot_rules(&SlotTable::Pact, "Hexer", Edition::Y2014);
    assert_eq!(stats_of(&old, "hexer:spellcasting:slots:count").len(), 4);
    assert!(old.iter().all(|r| r["kind"] == "stat"));
    let new = slot_rules(&SlotTable::Pact, "Hexer", Edition::Y2024);
    assert_eq!(new.last(), Some(&pact_magic_rule()));
    assert!(stats_of(&new, "hexer:spellcasting:slots:count").is_empty());
}

#[test]
fn multiclass_grants_follow_the_caster_type() {
    assert_eq!(SlotTable::Full.multiclass(), "FULL");
    assert_eq!(SlotTable::HalfUp.multiclass(), "HALF_UP");
    assert_eq!(SlotTable::HalfDown.multiclass(), "HALF");
    assert_eq!(SlotTable::Third.multiclass(), "THIRD");
    assert_eq!(SlotTable::Pact.multiclass(), "SOLO");
    assert_eq!(
        multiclass_rule("HALF_UP"),
        json!({"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP", "type": "Grants",
            "requirements": "ID_INTERNAL_GRANT_MULTICLASS"})
    );
}

#[test]
fn choices_are_written_in_each_edition_s_shape() {
    let mut cantrips = [3; 20];
    for c in cantrips.iter_mut().skip(3) {
        *c = 4;
    }
    let old = cantrip_rules(
        "Sage",
        &cantrips,
        true,
        &Spelling::new(Edition::Y2014),
        Edition::Y2014,
    );
    assert_eq!(
        stats_of(&old, "sage:spellcasting:cantrips known"),
        [(3, 1), (1, 4)]
    );
    assert_eq!(
        old[2],
        json!({"kind": "select", "type": "Spell", "name": "Cantrip (Sage)", "number": 3, "level": 1,
            "supports": "$(spellcasting:list),0", "spellcasting": "Sage"})
    );
    let new = cantrip_rules(
        "Sage",
        &cantrips,
        false,
        &Spelling::new(Edition::Y2024),
        Edition::Y2024,
    );
    assert_eq!(
        new,
        [
            json!({"kind": "select", "type": "Spell", "name": "Cantrip (Sage 1)", "number": 3,
                "supports": "$(spellcasting:list), 0", "spellcasting": "Sage", "allow_replace": true}),
            json!({"kind": "select", "type": "Spell", "name": "Cantrip (Sage 4)", "level": 4,
                "supports": "$(spellcasting:list), 0", "spellcasting": "Sage", "allow_replace": true}),
        ]
    );
    let known = [2, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4];
    let grid = SlotTable::Full.grid();
    let spelling = Spelling::new(Edition::Y2014);
    let rules = known_rules(
        "Sage",
        &known,
        false,
        true,
        &spelling,
        &grid,
        Edition::Y2014,
    );
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[2]["supports"], "$(spellcasting:list),(1||2)");
    assert_eq!(rules[2]["allow_replace"], true);
    let dynamic = Spelling {
        dynamic: true,
        spaced: true,
        word: "Spell Known".into(),
    };
    let rules = known_rules(
        "Sage",
        &known,
        false,
        false,
        &dynamic,
        &grid,
        Edition::Y2014,
    );
    assert_eq!(rules[0]["name"], "Spell Known (Sage)");
    assert_eq!(
        rules[0]["supports"],
        "$(spellcasting:list), $(spellcasting:slots)"
    );
    assert!(rules[0].get("allow_replace").is_none());
}

#[test]
fn prepared_counts_formulas_and_changes() {
    assert_eq!(
        prepare_formula_rules("Sage", "Wisdom", true),
        [
            json!({"kind": "stat", "name": "sage:spellcasting:prepare", "value": "wisdom:modifier"}),
            json!({"kind": "stat", "name": "sage:spellcasting:prepare", "value": "level:sage:half"}),
        ]
    );
    let table: [u32; 20] = std::array::from_fn(|i| i as u32 + 2);
    let rules = count_rules("sage:spellcasting:prepare", &table, Edition::Y2024);
    assert_eq!(
        rules[0],
        json!({"kind": "stat", "name": "sage:spellcasting:prepare", "value": "2"})
    );
    assert_eq!(rules.len(), 20);
    assert_eq!(
        prepared_change_rule("levelOne"),
        Some(json!({"kind": "prepared_spell_change", "when": "gain_level", "count": "one"}))
    );
    assert_eq!(prepared_change_rule("sometimes"), None);
}

#[test]
fn class_table_columns_match_the_srd_classes() {
    let cantrips = [0; 20];
    let prepared = [2; 20];
    let cols = progression_columns(Some(&SlotTable::HalfUp), &cantrips, Some(&prepared));
    let keys: Vec<&str> = cols.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        ["prepared", "slot_1", "slot_2", "slot_3", "slot_4", "slot_5"]
    );
    let pact = progression_columns(Some(&SlotTable::Pact), &[1; 20], None);
    let keys: Vec<&str> = pact.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["cantrips", "pact_slots", "pact_slot_level"]);
    assert!(is_progression_column("slot_9"));
    assert!(!is_progression_column("slot_10"));
    assert!(!is_progression_column("rage_damage"));
}
