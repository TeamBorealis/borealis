//! HR3 (GitHub #24): the "Ignore feat prerequisites" and "Ignore multiclass prerequisites"
//! house rules. With a rule on, a feat or multiclass whose prerequisites are not met can be
//! picked and is marked `prerequisites_unmet`; the character's own class is still never offered
//! as a multiclass; turning the rule off rejects the pick (kept, listed as invalid) instead of
//! deleting it; the rules round-trip through the character file.

mod common;
use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::{Engine, HouseRules};
use common::*;
use std::path::Path;
use std::sync::OnceLock;

const GRAPPLER: &str = "ID_PHB_FEAT_GRAPPLER";
const FIGHTER_ASI: &str = "ID_WOTC_PHB_CLASS_FEATURE_ABILITYSCOREIMPROVEMENT_FIGHTER";
const FEAT_4: &str = "ID_INTERNAL_CLASS_FEATURE_FEAT_4_FIGHTER";

fn mc(name: &str) -> String {
    format!("ID_WOTC_PHB_MULTICLASS_{}", name.to_uppercase())
}

fn feat_slot(s: &CharacterSnapshot) -> &SlotSnapshot {
    s.slots
        .iter()
        .find(|x| x.ty == "Feat" && x.owner.id == FEAT_4)
        .expect("the level 4 feat slot")
}

/// A level 4 human fighter with Strength `str`, who took the feat alternative at 4.
fn fighter_with_feat_slot(e: &Engine, str_score: i32) -> Hero<'_> {
    let mut h = Hero::new(e, true);
    h.base([str_score, 12, 14, 8, 10, 8])
        .race_class(RACE_HUMAN, &class("Fighter"));
    h.level_to(4);
    h.pick(FIGHTER_ASI, "Improvement Option (Fighter 4)", 1, FEAT_4);
    h
}

#[test]
fn ignored_feat_prerequisites_offer_mark_and_keep_the_pick_2014() {
    each_variant("hr3-feat-2014", |e, _| {
        let mut h = fighter_with_feat_slot(e, 8);
        let snap = h.snap();
        assert!(
            !feat_slot(&snap).options.iter().any(|o| o.id == GRAPPLER),
            "Grappler needs Strength 13"
        );
        assert!(commands::select(
            e,
            &h.s,
            &feat_slot(&snap).key,
            Choice::Element(GRAPPLER.into())
        )
        .is_err());

        h.s.house_rules.ignore_feat_prerequisites = true;
        let snap = h.snap();
        let slot = feat_slot(&snap);
        let row = slot
            .options
            .iter()
            .find(|o| o.id == GRAPPLER)
            .expect("the house rule offers Grappler");
        assert!(row.prerequisites_unmet, "the row is marked");
        let key = slot.key.clone();
        h.pick_key(&key, GRAPPLER);
        let snap = h.snap();
        assert!(snap.has_element(GRAPPLER));
        assert!(feat_slot(&snap).prerequisites_unmet, "the pick is marked");
        let feature = snap
            .features
            .iter()
            .find(|f| f.id == GRAPPLER)
            .expect("Grappler is a feature");
        assert!(feature.prerequisites_unmet, "the feature list marks it too");
        assert!(snap.invalid_selections.is_empty());

        // turning the rule off rejects the pick: kept in the state, listed as invalid
        let mut off = h.s.clone();
        off.house_rules.ignore_feat_prerequisites = false;
        let s = e.derive(&off);
        assert!(!s.has_element(GRAPPLER));
        assert_eq!(s.invalid_selections.len(), 1);
        assert_eq!(
            s.invalid_selections[0].choice,
            Choice::Element(GRAPPLER.into())
        );
        let rejected = feat_slot(&s);
        assert_eq!(rejected.rejected, Some(Choice::Element(GRAPPLER.into())));
        assert!(
            rejected.prerequisites_unmet,
            "rejected only for its prerequisites"
        );
        assert!(off
            .selections
            .iter()
            .any(|x| x.choice == Choice::Element(GRAPPLER.into())));
        // and on again restores it
        off.house_rules.ignore_feat_prerequisites = true;
        assert!(e.derive(&off).has_element(GRAPPLER));
    });
}

#[test]
fn a_feat_whose_prerequisites_hold_is_not_marked() {
    each_variant("hr3-feat-met", |e, _| {
        let mut h = fighter_with_feat_slot(e, 13);
        h.s.house_rules.ignore_feat_prerequisites = true;
        let snap = h.snap();
        let row = feat_slot(&snap)
            .options
            .iter()
            .find(|o| o.id == GRAPPLER)
            .expect("Grappler offered");
        assert!(!row.prerequisites_unmet);
        let key = feat_slot(&snap).key.clone();
        h.pick_key(&key, GRAPPLER);
        let snap = h.snap();
        assert!(!feat_slot(&snap).prerequisites_unmet);
        assert!(
            feat_slot(&snap)
                .options
                .iter()
                .all(|o| o.id != GRAPPLER || !o.prerequisites_unmet),
            "the held pick's own row is not marked"
        );
        assert!(snap.features.iter().all(|f| !f.prerequisites_unmet));
    });
}

/// Multiclass slot of a character whose last level started a multiclass.
fn multiclass_slot(s: &CharacterSnapshot) -> &SlotSnapshot {
    s.slots
        .iter()
        .find(|x| x.ty == "Multiclass")
        .expect("a Multiclass slot")
}

#[test]
fn ignored_multiclass_prerequisites_allow_any_other_class_2014() {
    each_variant("hr3-multiclass-2014", |e, _| {
        // Fighter needs Str or Dex 13 to multiclass, Wizard Int 13: neither holds
        let mut h = Hero::new(e, true);
        h.base([8, 8, 14, 8, 10, 10])
            .race_class(RACE_HUMAN, &class("Fighter"));
        assert!(!h.snap().can_multiclass);
        assert!(commands::level_up_new_multiclass(e, &h.s, None).is_err());

        h.s.house_rules.ignore_multiclass_prerequisites = true;
        assert!(
            h.snap().can_multiclass,
            "the house rule opens multiclassing"
        );
        h.s = commands::level_up_new_multiclass(e, &h.s, None)
            .expect("new multiclass")
            .state;
        let snap = h.snap();
        let slot = multiclass_slot(&snap);
        let ids: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
        assert!(ids.contains(&mc("Wizard").as_str()), "{ids:?}");
        assert!(
            !ids.contains(&mc("Fighter").as_str()),
            "the character's own class is never a multiclass: {ids:?}"
        );
        assert!(
            slot.options.iter().all(|o| o.prerequisites_unmet),
            "the main class misses its own multiclass prerequisite, so every row is marked"
        );
        let key = slot.key.clone();
        h.pick_key(&key, &mc("Wizard"));
        let snap = h.snap();
        assert_eq!(snap.classes.len(), 2);
        assert!(multiclass_slot(&snap).prerequisites_unmet);
    });
}

#[test]
fn multiclass_marks_follow_each_class_and_reject_when_off() {
    each_variant("hr3-multiclass-marks", |e, _| {
        // Fighter (Str 13) may multiclass; Rogue (Dex 13) holds, Wizard (Int 8) does not
        let mut h = Hero::new(e, true);
        h.base([13, 13, 14, 8, 10, 10])
            .race_class(RACE_HUMAN, &class("Fighter"));
        h.s.house_rules.ignore_multiclass_prerequisites = true;
        h.s = commands::level_up_new_multiclass(e, &h.s, None)
            .expect("new multiclass")
            .state;
        let snap = h.snap();
        let slot = multiclass_slot(&snap);
        let row = |id: &str| {
            slot.options
                .iter()
                .find(|o| o.id == id)
                .unwrap_or_else(|| panic!("{id} offered"))
        };
        assert!(!row(&mc("Rogue")).prerequisites_unmet);
        assert!(row(&mc("Wizard")).prerequisites_unmet);
        let key = slot.key.clone();
        h.pick_key(&key, &mc("Wizard"));
        assert!(multiclass_slot(&h.snap()).prerequisites_unmet);

        let mut off = h.s.clone();
        off.house_rules.ignore_multiclass_prerequisites = false;
        let s = e.derive(&off);
        assert_eq!(
            s.invalid_selections
                .iter()
                .map(|i| &i.choice)
                .collect::<Vec<_>>(),
            [&Choice::Element(mc("Wizard"))]
        );
        assert!(multiclass_slot(&s).prerequisites_unmet);
    });
}

fn engine_2024() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

fn select(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("{owner}/{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("select {id}: {err}"))
        .state
}

/// A level 4 2024 human fighter with Str 8 / Dex 8.
fn fighter_2024(level: u32) -> CharacterState {
    borealis_engine::invariants::enable_for_thread();
    let e = engine_2024();
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    s = select(e, &s, LEVEL1, "Race", "ID_2024_SPECIES_HUMAN");
    s = select(e, &s, LEVEL1, "Class", "ID_2024_CLASS_FIGHTER");
    s = select(e, &s, LEVEL1, "Background", "ID_2024_BACKGROUND_SOLDIER");
    s.abilities.base = [8, 8, 14, 10, 12, 10];
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

#[test]
fn ignored_feat_prerequisites_2024_offer_general_feats() {
    let e = engine_2024();
    let mut s = fighter_2024(4);
    let asi = "ID_2024_CLASS_FEATURE_FIGHTER_ABILITY_SCORE_IMPROVEMENT";
    let name = "Ability Score Improvement (Fighter 4)";
    let offered = |s: &CharacterState| -> Vec<(String, bool)> {
        e.derive(s)
            .slot(asi, name)
            .expect("ASI slot")
            .options
            .iter()
            .map(|o| (o.id.clone(), o.prerequisites_unmet))
            .collect()
    };
    let strict = offered(&s);
    assert!(strict.iter().all(|(id, _)| id != "ID_2024_FEAT_GRAPPLER"));
    assert!(strict.iter().all(|(id, _)| !id.contains("BOON_OF")));
    assert!(strict.iter().all(|(_, unmet)| !unmet));

    s.house_rules.ignore_feat_prerequisites = true;
    let relaxed = offered(&s);
    let mark = |id: &str| relaxed.iter().find(|(x, _)| x == id).map(|(_, u)| *u);
    assert_eq!(mark("ID_2024_FEAT_GRAPPLER"), Some(true), "{relaxed:?}");
    assert_eq!(
        mark("ID_2024_FEAT_BOON_OF_COMBAT_PROWESS"),
        None,
        "the slot's own filter (General feats) still applies"
    );
    assert_eq!(
        mark("ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT"),
        Some(false),
        "level 4 meets the ASI feat's prerequisite"
    );
    s = select(e, &s, asi, name, "ID_2024_FEAT_GRAPPLER");
    let snap = e.derive(&s);
    assert!(snap.has_element("ID_2024_FEAT_GRAPPLER"));
    assert!(snap.slot(asi, name).expect("slot").prerequisites_unmet);
}

#[test]
fn ignored_multiclass_prerequisites_2024_keep_the_own_class_out() {
    let e = engine_2024();
    let mut s = fighter_2024(1);
    assert!(!e.derive(&s).can_multiclass);
    s.house_rules.ignore_multiclass_prerequisites = true;
    s = commands::level_up_new_multiclass(e, &s, None)
        .expect("new multiclass")
        .state;
    let snap = e.derive(&s);
    let slot = multiclass_slot(&snap);
    let ids: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"ID_2024_MULTICLASS_WIZARD"), "{ids:?}");
    assert!(!ids.contains(&"ID_2024_MULTICLASS_FIGHTER"), "{ids:?}");
}

#[test]
fn house_rules_round_trip_and_stay_out_of_files_when_off() {
    let off = HouseRules::default();
    let json = serde_json::to_string(&off).expect("serialize");
    assert_eq!(json, "{}", "old files stay byte-identical");
    let on = HouseRules {
        ignore_feat_prerequisites: true,
        ignore_multiclass_prerequisites: true,
        ..HouseRules::default()
    };
    let json = serde_json::to_string(&on).expect("serialize");
    assert!(
        json.contains("\"ignore_feat_prerequisites\":true"),
        "{json}"
    );
    assert!(
        json.contains("\"ignore_multiclass_prerequisites\":true"),
        "{json}"
    );
    let back: HouseRules = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, on);
    let old: HouseRules = serde_json::from_str("{\"attunement_limit\":4}").expect("old file");
    assert!(!old.ignore_feat_prerequisites && !old.ignore_multiclass_prerequisites);
}
