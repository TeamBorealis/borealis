//! IM8 (user, 2026-10-17) and D-22: with "allow content from the other edition" on, a 2014
//! character is offered 2024 feats, species (with their lineage options) and backgrounds, besides
//! the spells, items and optional features R4 already allowed. With it off they stay hidden.
//! Classes never cross editions, and a 2014 class never offers a 2024 subclass. Bundled core +
//! SRD 5.1 + SRD 5.2.1 packs; one synthetic 2024 subclass that names the 2014 Fighter's
//! `Martial Archetype` support, so only the edition rule can keep it out.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;

const FIGHTER_14: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const CHAMPION_14: &str = "ID_WOTC_PHB_ARCHETYPE_CHAMPION";
const FIGHTER_24: &str = "ID_2024_CLASS_FIGHTER";
const ELF_24: &str = "ID_2024_SPECIES_ELF";
const SOLDIER_24: &str = "ID_2024_BACKGROUND_SOLDIER";
const ACOLYTE_14: &str = "ID_BACKGROUND_ACOLYTE";
const GRAPPLER_24: &str = "ID_2024_FEAT_GRAPPLER";
const DWARF_14: &str = "ID_SRD_RACE_DWARF";
const SYNTHETIC_SUBCLASS_24: &str = "ID_T_2024_MARTIAL_SUBCLASS";
const ASI_14: &str = "ID_WOTC_PHB_CLASS_FEATURE_ABILITYSCOREIMPROVEMENT_FIGHTER";
const FEAT_INSTEAD_14: &str = "ID_INTERNAL_CLASS_FEATURE_FEAT_4_FIGHTER";
const BASE: [i32; 6] = [15, 14, 13, 12, 10, 8];

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let file: ContentFile = serde_json::from_value(json!({
            "format": "borealis-content/1", "ruleset": "dnd2024",
            "elements": [{"id": SYNTHETIC_SUBCLASS_24, "name": "New Martial Path",
                          "type": "Archetype", "source": "Test", "ruleset": "dnd2024",
                          "supports": ["Martial Archetype"]}]}))
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("bundled packs")
            .add_file("subclass.json", PackOrigin::Custom, file)
            .build();
        Engine::new(&store)
    })
}

fn pick(s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let e = engine();
    let key = slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn try_pick(s: &CharacterState, ty: &str, id: &str) -> bool {
    let e = engine();
    let snap = e.derive(s);
    let slot = open_slot(&snap, ty);
    commands::select(e, s, &slot, Choice::Element(id.into())).is_ok()
}

fn open_slot(snap: &CharacterSnapshot, ty: &str) -> borealis_engine::state::SlotKey {
    snap.slots
        .iter()
        .find(|k| k.ty == ty && k.chosen.is_none())
        .unwrap_or_else(|| panic!("no open {ty} slot"))
        .key
        .clone()
}

fn fighter(ruleset: Ruleset, class: &str, allow: Option<bool>, level: u32) -> CharacterState {
    let e = engine();
    let mut s = commands::new_character(e, ruleset);
    s.allow_cross_edition = allow;
    s = commands::set_base_scores(e, &s, BASE).unwrap().state;
    s = pick(&s, "ID_LEVEL_1", "Class", class);
    while s.level() < level {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    if class == FIGHTER_14 && level >= 4 {
        // take a feat instead of the ability score improvement
        s = pick(
            &s,
            ASI_14,
            "Improvement Option (Fighter 4)",
            FEAT_INSTEAD_14,
        );
    }
    s
}

/// Ids offered by the open slots of a type.
fn offered(snap: &CharacterSnapshot, ty: &str) -> Vec<String> {
    snap.slots
        .iter()
        .filter(|k| k.ty == ty && k.chosen.is_none())
        .flat_map(|k| k.options.iter().map(|o| o.id.clone()))
        .collect()
}

fn has(list: &[String], id: &str) -> bool {
    list.iter().any(|x| x == id)
}

#[test]
fn a_2014_fighter_with_the_setting_on_is_offered_2024_feats_species_and_backgrounds() {
    let s = fighter(Ruleset::Dnd2014, FIGHTER_14, Some(true), 4);
    let snap = engine().derive(&s);
    let races = offered(&snap, "Race");
    assert!(has(&races, ELF_24), "2024 species offered: {races:?}");
    assert!(has(&races, DWARF_14), "2014 races still offered");
    assert!(has(&offered(&snap, "Background"), SOLDIER_24));
    let feats = offered(&snap, "Feat");
    assert!(has(&feats, GRAPPLER_24), "2024 feat offered: {feats:?}");
    // every 2024 option carries the edition badge
    let badge = snap
        .slots
        .iter()
        .flat_map(|k| k.options.iter())
        .find(|o| o.id == ELF_24)
        .and_then(|o| o.edition.as_ref())
        .expect("badge");
    assert_eq!(badge.from, Ruleset::Dnd2024);
}

#[test]
fn the_2024_picks_hold_and_appear_in_the_compatibility_report() {
    let mut s = fighter(Ruleset::Dnd2014, FIGHTER_14, Some(true), 4);
    s = pick(&s, "ID_LEVEL_1", "Race", ELF_24);
    // the species' own lineage choice offers the 2024 lineage options
    let snap = engine().derive(&s);
    let lineage = snap
        .slots
        .iter()
        .find(|k| k.name == "Elven Lineage")
        .expect("lineage slot");
    assert!(!lineage.options.is_empty(), "lineage options offered");
    let lineage_pick = lineage.options[0].id.clone();
    s = commands::select(
        engine(),
        &s,
        &lineage.key,
        Choice::Element(lineage_pick.clone()),
    )
    .expect("lineage pick")
    .state;
    let bg = open_slot(&engine().derive(&s), "Background");
    s = commands::select(engine(), &s, &bg, Choice::Element(SOLDIER_24.into()))
        .expect("2024 background")
        .state;
    let feat = open_slot(&engine().derive(&s), "Feat");
    s = commands::select(engine(), &s, &feat, Choice::Element(GRAPPLER_24.into()))
        .expect("2024 feat")
        .state;

    let snap = engine().derive(&s);
    for id in [ELF_24, lineage_pick.as_str(), SOLDIER_24, GRAPPLER_24] {
        assert!(snap.has_element(id), "{id} registered");
        let entry = snap
            .compatibility
            .entries
            .iter()
            .find(|c| c.element.id == id)
            .unwrap_or_else(|| panic!("{id} in the compatibility report"));
        assert_eq!(entry.from, Ruleset::Dnd2024);
    }
    assert!(!snap.compatibility.strict);
    for k in &snap.slots {
        assert!(k.rejected.is_none(), "{} rejected {:?}", k.name, k.rejected);
    }
    assert_eq!(snap.speed.walk, 30, "the 2024 elf's speed applies");
}

#[test]
fn with_the_setting_off_2024_feats_species_and_backgrounds_stay_hidden() {
    for allow in [None, Some(false)] {
        let s = fighter(Ruleset::Dnd2014, FIGHTER_14, allow, 4);
        let snap = engine().derive(&s);
        assert!(!has(&offered(&snap, "Race"), ELF_24), "{allow:?}");
        assert!(has(&offered(&snap, "Race"), DWARF_14), "{allow:?}");
        assert!(!has(&offered(&snap, "Background"), SOLDIER_24), "{allow:?}");
        assert!(!has(&offered(&snap, "Feat"), GRAPPLER_24), "{allow:?}");
        assert!(!try_pick(&s, "Race", ELF_24), "{allow:?}");
        assert!(!try_pick(&s, "Background", SOLDIER_24), "{allow:?}");
        assert!(!try_pick(&s, "Feat", GRAPPLER_24), "{allow:?}");
    }
}

#[test]
fn classes_and_subclasses_never_cross_into_a_2014_character() {
    let s = fighter(Ruleset::Dnd2014, FIGHTER_14, Some(true), 3);
    let snap = engine().derive(&s);
    let subclasses = offered(&snap, "Archetype");
    assert!(has(&subclasses, CHAMPION_14), "{subclasses:?}");
    assert!(
        !has(&subclasses, SYNTHETIC_SUBCLASS_24),
        "a 2014 class never offers a 2024 subclass: {subclasses:?}"
    );
    assert!(!try_pick(&s, "Archetype", SYNTHETIC_SUBCLASS_24));
    let fresh = engine().derive(&{
        let mut c = commands::new_character(engine(), Ruleset::Dnd2014);
        c.allow_cross_edition = Some(true);
        c
    });
    let classes = offered(&fresh, "Class");
    assert!(has(&classes, FIGHTER_14));
    assert!(!has(&classes, FIGHTER_24), "2024 classes never offered");
}

#[test]
fn a_2024_character_is_unchanged() {
    // default open: 2014 races and backgrounds offered; strict hides them
    let open = engine().derive(&fighter(Ruleset::Dnd2024, FIGHTER_24, None, 1));
    assert!(has(&offered(&open, "Race"), DWARF_14));
    assert!(has(&offered(&open, "Background"), ACOLYTE_14));
    assert!(has(&offered(&open, "Race"), ELF_24));
    let strict = engine().derive(&fighter(Ruleset::Dnd2024, FIGHTER_24, Some(false), 1));
    assert!(!has(&offered(&strict, "Race"), DWARF_14));
    assert!(!has(&offered(&strict, "Background"), ACOLYTE_14));
    let fresh = engine().derive(&commands::new_character(engine(), Ruleset::Dnd2024));
    assert!(!has(&offered(&fresh, "Class"), FIGHTER_14));
}

/// Ability scores when a 2014 character takes a 2024 species: the species itself has no
/// increase (2024 moves it to the background), so IM9 offers a flexible +2/+1 or +1/+1/+1
/// (`cross_edition_species_asi.rs`); left unchosen, nothing applies. A 2024 background brings
/// its own ability distribution instead (IM9a: the species' choice is then not offered).
#[test]
fn a_2024_species_on_a_2014_character_adds_nothing_until_the_increase_is_chosen() {
    let mut s = fighter(Ruleset::Dnd2014, FIGHTER_14, Some(true), 1);
    s = pick(&s, "ID_LEVEL_1", "Race", ELF_24);
    let bg = open_slot(&engine().derive(&s), "Background");
    let with_14_bg = commands::select(engine(), &s, &bg, Choice::Element(ACOLYTE_14.into()))
        .expect("2014 background")
        .state;
    let snap = engine().derive(&with_14_bg);
    let scores: Vec<i32> = snap.abilities.iter().map(|a| a.score).collect();
    assert_eq!(
        scores,
        BASE.to_vec(),
        "no increase from the background, the species' IM9 choice still open"
    );

    let with_24_bg = commands::select(engine(), &s, &bg, Choice::Element(SOLDIER_24.into()))
        .expect("2024 background")
        .state;
    let snap = engine().derive(&with_24_bg);
    assert!(
        snap.has_element("ID_2024_FEAT_SAVAGE_ATTACKER"),
        "the background's origin feat"
    );
    let dist = snap
        .slots
        .iter()
        .find(|k| k.owner.id == SOLDIER_24 && k.name == "Ability Scores (Soldier)")
        .expect("the 2024 background's ability distribution");
    let (item, _) = dist
        .list_items
        .iter()
        .find(|(_, t)| t == "Strength +2, Dexterity +1")
        .expect("list entry")
        .clone();
    let s = commands::select(engine(), &with_24_bg, &dist.key, Choice::ListItem(item))
        .expect("distribution")
        .state;
    let scores: Vec<i32> = engine()
        .derive(&s)
        .abilities
        .iter()
        .map(|a| a.score)
        .collect();
    assert_eq!(scores, vec![17, 15, 13, 12, 10, 8]);
}
