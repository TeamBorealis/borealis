//! The deterministic spellcasting builder against the engine: invented casters must get the SRD
//! classes' slot progressions.

use std::path::Path;

use borealis_assist::import::anything::caster::build_caster;
use borealis_assist::import::anything::{Edition, SpellcastingOutline};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{Choice, Engine};
use serde_json::{json, Value};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn base() -> ContentStoreBuilder {
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root().join(pack)).expect("pack loads");
    }
    b
}

fn profile(kind: &str, ability: &str, extra: Value) -> SpellcastingOutline {
    let mut v = json!({"kind": kind, "ability": ability, "spell_list_name": "Wizard"});
    v.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(v).expect("profile parses")
}

fn srd_class(pack_file: &str, id: &str) -> Value {
    let text = std::fs::read_to_string(root().join(pack_file)).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap()
        .clone()
}

/// An invented class cloned from an SRD class (engine-valid shape) whose spellcasting comes
/// from the builder.
fn invented(template: Value, name: &str, p: &SpellcastingOutline, edition: Edition) -> Vec<Value> {
    let upper = name.to_uppercase();
    let feature_id = format!("ID_TEST_CLASS_FEATURE_{upper}_SPELLCASTING");
    let built = build_caster(p, name, edition).expect("caster");
    let mut class = template;
    let old = class["name"].as_str().unwrap().to_string();
    class = serde_json::from_str(
        &class
            .to_string()
            .replace(&old, name)
            .replace(&old.to_uppercase(), &upper),
    )
    .unwrap();
    class["id"] = json!(format!("ID_TEST_CLASS_{upper}"));
    let rules = class["rules"].as_array_mut().unwrap();
    rules.retain(|r| {
        let ty = r["type"].as_str().unwrap_or("");
        matches!(r["kind"].as_str(), Some("grant" | "select"))
            && ty != "Class Feature"
            && ty != "Archetype"
    });
    rules.push(json!({"kind": "grant", "id": feature_id, "type": "Class Feature", "level": 1}));
    if edition == Edition::Y2024 {
        let d = class["data"].as_object_mut().unwrap();
        d.insert(
            "spellcasting".into(),
            built.class_spellcasting.clone().unwrap(),
        );
        let prog = d["progression"].as_object_mut().unwrap();
        prog.retain(|k, _| k == "proficiency_bonus");
        prog.extend(built.progression.clone());
    }
    let feature = json!({"id": feature_id, "name": "Spellcasting", "type": "Class Feature",
        "source": "Test", "spellcasting": built.node, "rules": built.rules,
        "data": {"class": name, "level": 1}});
    vec![class, feature]
}

type Level = ([i32; 9], Option<(i32, i32)>, Option<i32>);

fn run(elements: Vec<Value>, ruleset: Ruleset, class_id: &str) -> Vec<Level> {
    let doc = json!({"format": "borealis-content/1", "elements": elements});
    let file = ContentFile::from_json_str(&doc.to_string()).expect("loads");
    let store: ContentStore = base().add_file("t", PackOrigin::Custom, file).build();
    class_levels(&store, ruleset, class_id)
}

fn class_levels(store: &ContentStore, ruleset: Ruleset, class_id: &str) -> Vec<Level> {
    let engine = Engine::new(store);
    let mut s = commands::new_character(&engine, ruleset);
    let snap = engine.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(&engine, &s, &key, Choice::Element(class_id.into()))
        .expect("class")
        .state;
    let mut out = Vec::new();
    for level in 1..=20 {
        if level > 1 {
            s = commands::level_up_main(&engine, &s, None)
                .expect("level up")
                .state;
        }
        let snap = engine.derive(&s);
        let sc = snap.spellcasting.first();
        out.push((
            sc.map_or([0; 9], |c| c.slots),
            sc.and_then(|c| c.pact),
            sc.and_then(|c| c.prepare_limit),
        ));
    }
    out
}

fn srd(ruleset: Ruleset, class_id: &str) -> Vec<Level> {
    class_levels(&base().build(), ruleset, class_id)
}

#[test]
fn invented_half_caster_matches_the_srd_paladin_slots() {
    let template = srd_class("srd-5.1/classes/paladin.json", "ID_WOTC_PHB_CLASS_PALADIN");
    let p = profile("half", "charisma", json!({"prepared": true}));
    let got = run(
        invented(template, "Zealot", &p, Edition::Y2014),
        Ruleset::Dnd2014,
        "ID_TEST_CLASS_ZEALOT",
    );
    let want = srd(Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_PALADIN");
    for level in [2usize, 5, 9, 13, 17] {
        assert_eq!(
            got[level - 1].0,
            want[level - 1].0,
            "slots at level {level}"
        );
    }
    let slots = |v: &[Level]| v.iter().map(|l| l.0).collect::<Vec<_>>();
    assert_eq!(slots(&got), slots(&want), "every level matches");
    assert_eq!(
        got[0].0, [0; 9],
        "a 2014 half caster has no slot at level 1"
    );
}

#[test]
fn invented_pact_caster_matches_the_srd_warlock() {
    let template = srd_class("srd-5.1/classes/warlock.json", "ID_WOTC_PHB_CLASS_WARLOCK");
    let p = profile("pact", "charisma", json!({"known": true}));
    let got = run(
        invented(template, "Binder", &p, Edition::Y2014),
        Ruleset::Dnd2014,
        "ID_TEST_CLASS_BINDER",
    );
    let want = srd(Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_WARLOCK");
    assert!(want[0].1.is_some(), "the SRD warlock has a pact pool");
    for level in 1..=20 {
        assert_eq!(
            got[level - 1].1,
            want[level - 1].1,
            "pact (count, level) at {level}"
        );
        assert_eq!(
            got[level - 1].0,
            want[level - 1].0,
            "ordinary slots at {level}"
        );
    }
}

#[test]
fn invented_third_caster_gets_its_first_slot_at_level_three() {
    let template = srd_class("srd-5.1/classes/paladin.json", "ID_WOTC_PHB_CLASS_PALADIN");
    let p = profile("third", "intelligence", json!({"prepared": true}));
    let got = run(
        invented(template, "Duelist", &p, Edition::Y2014),
        Ruleset::Dnd2014,
        "ID_TEST_CLASS_DUELIST",
    );
    assert_eq!(got[0].0, [0; 9]);
    assert_eq!(got[1].0, [0; 9]);
    assert_eq!(
        got[2].0,
        [2, 0, 0, 0, 0, 0, 0, 0, 0],
        "first slots at level 3"
    );
    assert_eq!(got[3].0[0], 3);
    assert_eq!(got[6].0[..2], [4, 2], "level 7 gains 2nd-level slots");
    assert_eq!(got[18].0[..4], [4, 3, 3, 1]);
}

#[test]
fn a_2024_prepared_caster_takes_its_prepared_count_from_the_progression() {
    let template = srd_class("srd-5.2.1/classes/cleric.json", "ID_2024_CLASS_CLERIC");
    let default = profile("full", "wisdom", json!({"prepared": true}));
    let got = run(
        invented(template.clone(), "Zealot", &default, Edition::Y2024),
        Ruleset::Dnd2024,
        "ID_TEST_CLASS_ZEALOT",
    );
    let want = srd(Ruleset::Dnd2024, "ID_2024_CLASS_CLERIC");
    for level in 1..=20 {
        assert_eq!(got[level - 1].0, want[level - 1].0, "slots at {level}");
        assert_eq!(got[level - 1].2, want[level - 1].2, "prepared at {level}");
    }
    let table: Vec<u32> = (1..=20).map(|n| n + 2).collect();
    let custom = profile(
        "full",
        "wisdom",
        json!({"prepared": true, "spells_known_by_level": table}),
    );
    let got = run(
        invented(template, "Zealot", &custom, Edition::Y2024),
        Ruleset::Dnd2024,
        "ID_TEST_CLASS_ZEALOT",
    );
    for level in 1..=20 {
        assert_eq!(
            got[level - 1].2,
            Some(level as i32 + 2),
            "custom prepared at {level}"
        );
    }
}

#[test]
fn a_2024_half_caster_matches_the_srd_ranger() {
    let template = srd_class("srd-5.2.1/classes/ranger.json", "ID_2024_CLASS_RANGER");
    let p = profile("half", "wisdom", json!({"prepared": true}));
    let got = run(
        invented(template, "Strider", &p, Edition::Y2024),
        Ruleset::Dnd2024,
        "ID_TEST_CLASS_STRIDER",
    );
    let want = srd(Ruleset::Dnd2024, "ID_2024_CLASS_RANGER");
    for level in 1..=20 {
        assert_eq!(got[level - 1].0, want[level - 1].0, "slots at {level}");
        assert_eq!(got[level - 1].2, want[level - 1].2, "prepared at {level}");
    }
}
