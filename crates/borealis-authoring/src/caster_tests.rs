//! The spellcasting builder from a [`CasterProfile`] (the import pipeline's adapter is tested in
//! `borealis-assist/tests/caster_builder.rs` against the engine).

use super::*;

fn profile(kind: CasterKind) -> CasterProfile {
    CasterProfile {
        kind,
        ability: "Intelligence".into(),
        list: None,
        prepared: None,
        known: None,
        ritual: None,
        cantrips_by_level: None,
        spells_known_by_level: None,
        prepared_formula: None,
    }
}

fn stats<'a>(b: &'a BuiltCaster, name: &str) -> Vec<&'a Value> {
    b.rules
        .iter()
        .filter(|r| r["kind"] == "stat" && r["name"] == name)
        .collect()
}

#[test]
fn slot_rows_follow_the_srd_progressions() {
    let full = slot_rows(CasterKind::Full, Edition::Y2024);
    assert_eq!(full[0], [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(full[19], [4, 3, 3, 3, 3, 2, 2, 1, 1]);
    let half14 = slot_rows(CasterKind::Half, Edition::Y2014);
    assert_eq!(half14[0], [0; 9], "2014 half casters start at level 2");
    let half24 = slot_rows(CasterKind::Half, Edition::Y2024);
    assert_eq!(
        half24[0],
        [2, 0, 0, 0, 0, 0, 0, 0, 0],
        "2024 half casters round up"
    );
    let third = slot_rows(CasterKind::Third, Edition::Y2014);
    assert_eq!(third[1], [0; 9]);
    assert_eq!(third[2], [2, 0, 0, 0, 0, 0, 0, 0, 0]);
    let pact = slot_rows(CasterKind::Pact, Edition::Y2024);
    assert_eq!(pact[10], [0, 0, 0, 0, 3, 0, 0, 0, 0]);
    assert_eq!(slot_rows(CasterKind::Points, Edition::Y2024), [[0; 9]; 20]);
}

#[test]
fn a_non_caster_builds_nothing() {
    assert!(build_caster(&profile(CasterKind::None), "Fighter", Edition::Y2024).is_none());
}

#[test]
fn a_2024_full_caster_gets_node_slots_and_class_data() {
    let b = build_caster(&profile(CasterKind::Full), "Sage", Edition::Y2024).expect("casts");
    assert_eq!(b.node["name"], "Sage");
    assert_eq!(b.node["ability"], "Intelligence");
    assert_eq!(
        b.node["list"]["supports"], "Sage",
        "the list defaults to the class name"
    );
    assert_eq!(b.node["prepare"], true);
    let first = stats(&b, "sage:spellcasting:slots:1");
    assert_eq!(first[0]["value"], "2");
    assert!(
        first[0].get("level").is_none(),
        "2024 level-1 stats are ungated"
    );
    let sc = b.class_spellcasting.expect("2024 class data");
    assert_eq!(
        sc["multiclass_slots_grant"],
        "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL"
    );
    assert_eq!(b.progression["slot_9"][16], 1);
    assert!(b.rules.iter().all(is_built_caster_rule), "{:?}", b.rules);
}

#[test]
fn a_2014_known_caster_gets_spell_selects_and_a_ritual_grant() {
    let p = CasterProfile {
        known: Some(true),
        ritual: Some(true),
        list: Some("Wizard".into()),
        ..profile(CasterKind::Half)
    };
    let b = build_caster(&p, "Warden", Edition::Y2014).expect("casts");
    assert!(b.class_spellcasting.is_none());
    assert!(!stats(&b, "warden:spellcasting:spells known").is_empty());
    let selects: Vec<_> = b.rules.iter().filter(|r| r["kind"] == "select").collect();
    assert!(selects.iter().all(|s| s["supports"]
        .as_str()
        .is_some_and(|x| x.starts_with("$(spellcasting:list),"))));
    assert!(b
        .rules
        .iter()
        .any(|r| r["id"] == "ID_INTERNAL_RITUAL_CASTING"));
    assert!(b.rules.iter().any(|r| r["id"]
        == "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF"
        && r["requirements"] == "ID_INTERNAL_GRANT_MULTICLASS"));
}

#[test]
fn a_points_caster_gets_spells_but_no_slots() {
    let b = build_caster(&profile(CasterKind::Points), "Psion", Edition::Y2024).expect("casts");
    assert!(b
        .rules
        .iter()
        .all(|r| !r["name"].as_str().unwrap_or("").contains(":slots:")));
    let sc = b.class_spellcasting.expect("2024 class data");
    assert!(sc.get("multiclass_slots_grant").is_none());
}

#[test]
fn custom_tables_replace_the_defaults() {
    let p = CasterProfile {
        cantrips_by_level: Some(vec![1; 20]),
        ..profile(CasterKind::Full)
    };
    let b = build_caster(&p, "Sage", Edition::Y2024).expect("casts");
    assert_eq!(b.progression["cantrips"][19], 1);
    let short = CasterProfile {
        cantrips_by_level: Some(vec![1; 3]),
        ..profile(CasterKind::Full)
    };
    let b = build_caster(&short, "Sage", Edition::Y2024).expect("casts");
    assert_eq!(
        b.progression["cantrips"][19], 5,
        "a table that is not 20 long is ignored"
    );
}

#[test]
fn built_caster_rules_are_recognised() {
    assert!(is_built_caster_rule(
        &json!({"kind": "stat", "name": "x:spellcasting:slots:1"})
    ));
    assert!(is_built_caster_rule(&json!({"kind": "pact_magic"})));
    assert!(!is_built_caster_rule(
        &json!({"kind": "stat", "name": "strength"})
    ));
    assert!(!is_built_caster_rule(
        &json!({"kind": "grant", "id": "ID_SOMETHING"})
    ));
}
