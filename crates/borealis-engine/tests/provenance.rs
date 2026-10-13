//! Stat provenance and feature details.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine, EvaluateOptions};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

fn content(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(name)
}

fn srd_engine(extra: Vec<Value>) -> Engine {
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&content("core"))
        .and_then(|b| b.add_pack_dir(&content("srd-5.1")))
        .expect("packs")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn fighter(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(
        e,
        &s,
        &key,
        Choice::Element("ID_WOTC_PHB_CLASS_FIGHTER".into()),
    )
    .expect("fighter")
    .state
}

fn give_and_equip(e: &Engine, s: &CharacterState, id: &str) -> CharacterState {
    let s = commands::add_item(e, s, id, 1).expect("add").state;
    let uid = s.inventory.items.last().expect("item").uid.clone();
    commands::equip(e, &s, &uid, None).expect("equip").state
}

fn flat(steps: &[borealis_engine::TraceStep]) -> Vec<&borealis_engine::TraceStep> {
    steps
        .iter()
        .flat_map(|s| std::iter::once(s).chain(flat(&s.inputs)))
        .collect()
}

fn ring(id: &str, name: &str, value: i32, bonus: Option<&str>) -> Value {
    let mut rule = json!({"kind":"stat","name":"ac:misc","value": value.to_string()});
    if let Some(b) = bonus {
        rule["bonus"] = json!(b);
    }
    json!({"id": id, "name": name, "type":"Magic Item", "source":"Test",
           "setters":[{"name":"slot","value":"onehand"}], "rules":[rule]})
}

#[test]
fn ac_of_an_armored_shield_bearer_traces_each_part() {
    let e = srd_engine(vec![]);
    let mut s = fighter(&e);
    s = give_and_equip(&e, &s, "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
    s = give_and_equip(&e, &s, "ID_WOTC_GEAR_SHIELD");
    let t = e.explain(&s, "ac");
    assert_eq!(t.value, e.derive(&s).armor_class);
    let top: i32 = t.steps.iter().filter(|x| x.included).map(|x| x.value).sum();
    assert_eq!(top, t.value, "included steps add up to the final value");
    let all = flat(&t.steps);
    let armor = all
        .iter()
        .find(|x| x.element_id.as_deref() == Some("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL"))
        .expect("the armor rule is traced");
    assert!(
        armor.included && armor.value == 16 && armor.element_name.as_deref() == Some("Chain Mail")
    );
    let shield = all
        .iter()
        .find(|x| x.element_id.as_deref() == Some("ID_WOTC_GEAR_SHIELD"))
        .expect("the shield rule is traced");
    assert!(shield.included && shield.value == 2 && shield.group == "ac:shield");
    assert_eq!(shield.bonus.as_deref(), Some("shield"));
    let unarmored = t
        .steps
        .iter()
        .find(|x| x.label == "Unarmored")
        .expect("the unarmored calculation is listed");
    assert!(!unarmored.included);
    assert!(unarmored
        .reason
        .as_deref()
        .unwrap_or("")
        .contains("equipped"));
    assert_eq!(
        unarmored.condition.as_deref(),
        Some("equipped [armor:none]")
    );
}

#[test]
fn a_shared_bonus_label_shows_the_excluded_lower_value() {
    let e = srd_engine(vec![
        ring("ID_T_RING_1", "Ring of Warding", 1, Some("deflection")),
        ring("ID_T_RING_2", "Ring of Shielding", 2, Some("deflection")),
    ]);
    let mut s = fighter(&e);
    s = give_and_equip(&e, &s, "ID_T_RING_1");
    s = give_and_equip(&e, &s, "ID_T_RING_2");
    let t = e.explain(&s, "ac");
    let on_misc: Vec<_> = flat(&t.steps)
        .into_iter()
        .filter(|x| x.group == "ac:misc")
        .collect();
    let high = on_misc.iter().find(|x| x.value == 2).expect("+2 traced");
    let low = on_misc.iter().find(|x| x.value == 1).expect("+1 traced");
    assert!(high.included);
    assert!(!low.included);
    let why = low.reason.as_deref().unwrap_or("");
    assert!(
        why.contains("deflection") && why.contains("highest"),
        "{why}"
    );
    assert_eq!(low.element_id.as_deref(), Some("ID_T_RING_1"));
    assert_eq!(
        e.derive(&s).armor_class,
        e.derive(&fighter(&e)).armor_class + 2
    );
}

#[test]
fn a_cursed_minus_one_ac_misc_carries_the_e3_diagnostic() {
    let e = srd_engine(vec![ring("ID_T_CURSE", "Cursed Band", -1, None)]);
    let mut s = fighter(&e);
    s = give_and_equip(&e, &s, "ID_T_CURSE");
    let t = e.explain(&s, "ac");
    let curse = flat(&t.steps)
        .into_iter()
        .find(|x| x.element_id.as_deref() == Some("ID_T_CURSE"))
        .expect("the curse is traced");
    assert!(curse.included && curse.value == -1);
    assert!(
        t.diagnostics
            .iter()
            .any(|d| d.code == "legacy.nonpositive_merge"),
        "{:?}",
        t.diagnostics
    );
    let snap = e.derive_opts(&s, false, EvaluateOptions { provenance: true });
    let from_snapshot = &snap.provenance["ac"];
    assert_eq!(from_snapshot.value, snap.armor_class);
    assert!(from_snapshot
        .diagnostics
        .iter()
        .any(|d| d.code == "legacy.nonpositive_merge"));
}

#[test]
fn provenance_is_opt_in_deterministic_and_changes_no_results() {
    let e = srd_engine(vec![]);
    let s = fighter(&e);
    let plain = e.derive(&s);
    assert!(plain.provenance.is_empty());
    let a = e.derive_opts(&s, true, EvaluateOptions { provenance: true });
    let b = e.derive_opts(&s, true, EvaluateOptions { provenance: true });
    assert_eq!(a, b, "traces are deterministic");
    assert_eq!(
        a.provenance.keys().collect::<Vec<_>>(),
        a.stats.keys().collect::<Vec<_>>()
    );
    let mut stripped = a.clone();
    stripped.provenance.clear();
    assert_eq!(stripped, plain, "tracing never changes a result");
    for (name, trace) in &a.provenance {
        let top: i32 = trace
            .steps
            .iter()
            .filter(|x| x.included)
            .map(|x| x.value)
            .sum();
        assert_eq!(top, a.stats[name].value, "{name}");
        assert_eq!(trace.value, a.stats[name].value, "{name}");
    }
    let score = e.explain(&s, "Strength:Score");
    assert_eq!(score.stat, "strength:score");
    assert_eq!(score.base, plain.abilities[0].base);
    assert_eq!(score.value, plain.abilities[0].score);
    assert_eq!(e.explain(&s, "no:such:stat").value, 0);
}

#[test]
fn srd_fighter_features_carry_descriptions_source_and_level() {
    let e = srd_engine(vec![]);
    let s = fighter(&e);
    let snap = e.derive(&s);
    let wind = snap
        .features
        .iter()
        .find(|f| f.id == "ID_WOTC_PHB_CLASS_FEATURE_SECONDWIND")
        .expect("Second Wind");
    assert_eq!(wind.name, "Second Wind");
    assert_eq!(wind.source, "SRD 5.1");
    assert_eq!(wind.level, 1);
    assert!(wind.description.starts_with("<p>"), "{}", wind.description);
    assert_eq!(
        wind.sheet_summary.as_deref(),
        Some("You regain 1d10+1 hit points.")
    );
    assert!(snap
        .sheet_summary
        .contains("Second Wind: You regain 1d10+1"));
    assert!(snap
        .features
        .iter()
        .all(|f| !f.description.contains("<script")));
    assert!(
        snap.features
            .iter()
            .all(|f| f.id != "ID_WOTC_PHB_CLASS_FEATURE_ACTIONSURGE"),
        "level 2 features are not there yet"
    );
    let json = serde_json::to_string(&snap).unwrap();
    let back: borealis_engine::CharacterSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
}

#[test]
fn level_20_provenance_cost() {
    let e = srd_engine(vec![]);
    let mut s = fighter(&e);
    for _ in 0..19 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = give_and_equip(&e, &s, "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
    s = give_and_equip(&e, &s, "ID_WOTC_GEAR_SHIELD");
    let time = |opts: EvaluateOptions| {
        let t = Instant::now();
        for _ in 0..20 {
            std::hint::black_box(e.derive_opts(&s, false, opts));
        }
        t.elapsed() / 20
    };
    let off = time(EvaluateOptions::default());
    let on = time(EvaluateOptions { provenance: true });
    eprintln!("level 20 derive: provenance off {off:?}, on {on:?}");
    let snap = e.derive_opts(&s, false, EvaluateOptions { provenance: true });
    let bytes = serde_json::to_string(&snap.provenance).unwrap().len();
    eprintln!(
        "provenance json: {} stats, {bytes} bytes",
        snap.provenance.len()
    );
}
