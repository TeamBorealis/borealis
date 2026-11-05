//! Sub races and race variants from imported content attach to the bundled SRD 5.1 races (B-170,
//! issue #23). Imported Aurora content often brings sub races and variants for a race it does not
//! define itself: they name the parent with `<supports>Tiefling</supports>` and expect the race to
//! carry the matching select (`<select type="Sub Race" supports="Tiefling" optional="true"/>`), as
//! Aurora's own race elements with the same ids do. The bundled races must carry those selects too,
//! and drop the traits a sub race or variant replaces (the Aurora markers
//! `ID_INTERNAL_GRANTS_TIEFLING_SUBRACE` and `ID_INTERNAL_GRANTS_DRAGONMARK`). Synthetic content
//! on the bundled core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use serde_json::{json, Value};
use std::path::Path;

const TIEFLING: &str = "ID_RACE_TIEFLING";
const HUMAN: &str = "ID_RACE_HUMAN";
const HALF_ORC: &str = "ID_RACE_HALFORC";
const INFERNAL_LEGACY: &str = "ID_RACIAL_TRAIT_INFERNAL_LEGACY";
const MENACING: &str = "ID_RACIAL_TRAIT_MENACING";
const TIEFLING_MARKER: &str = "ID_INTERNAL_GRANTS_TIEFLING_SUBRACE";
const MARK_MARKER: &str = "ID_INTERNAL_GRANTS_DRAGONMARK";
/// A sub race that keeps the race's traits (no marker).
const PLAIN_LINE: &str = "ID_T_SUB_RACE_TIEFLING_PLAIN_LINE";
/// A sub race that replaces the race's ability scores and legacy (grants the marker).
const ALT_LINE: &str = "ID_T_SUB_RACE_TIEFLING_ALT_LINE";
const ALT_LEGACY: &str = "ID_T_RACIAL_TRAIT_ALT_LEGACY";
const MARK: &str = "ID_T_MARK_OF_TESTING";
const HUMAN_MARKED: &str = "ID_T_RACE_VARIANT_HUMAN_MARKED";
const HALF_ORC_MARKED: &str = "ID_T_RACE_VARIANT_HALF_ORC_MARKED";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const SCORES: [i32; 6] = [10, 10, 10, 10, 10, 10];

fn sub_race(id: &str, name: &str, rules: Value) -> Value {
    json!({"id": id, "name": name, "type": "Sub Race", "source": "Test",
           "supports": ["Tiefling"], "rules": rules})
}

fn variant(id: &str, name: &str, parent: &str) -> Value {
    json!({"id": id, "name": name, "type": "Race Variant", "source": "Test",
           "supports": [parent],
           "rules": [{"kind": "grant", "type": "Dragonmark", "id": MARK},
                     {"kind": "stat", "name": "wisdom", "value": "2"},
                     {"kind": "stat", "name": "constitution", "value": "1"}]})
}

/// The bundled packs plus a content file that brings only sub races and variants (no race of
/// its own), the way a supplement imported without the core race file does. The markers it grants
/// are not defined in it: the bundled packs must define them.
fn engine_with(elements: Vec<Value>) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("supplement.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn engine() -> Engine {
    engine_with(vec![
        sub_race(PLAIN_LINE, "Tiefling (Plain Line)", json!([])),
        sub_race(
            ALT_LINE,
            "Tiefling (Alt Line)",
            json!([{"kind": "grant", "type": "Grants", "id": TIEFLING_MARKER},
                   {"kind": "stat", "name": "dexterity", "value": "2"},
                   {"kind": "stat", "name": "intelligence", "value": "1"},
                   {"kind": "grant", "type": "Racial Trait", "id": ALT_LEGACY}]),
        ),
        json!({"id": ALT_LEGACY, "name": "Alt Legacy", "type": "Racial Trait", "source": "Test"}),
        json!({"id": MARK, "name": "Mark of Testing", "type": "Dragonmark", "source": "Test",
               "rules": [{"kind": "grant", "type": "Grants", "id": MARK_MARKER}]}),
        variant(HUMAN_MARKED, "Mark of Testing (Human)", "Human"),
        variant(
            HALF_ORC_MARKED,
            "Mark of Testing (Half-Orc)",
            "Half-Orc Variant",
        ),
    ])
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1)
        .unwrap_or_else(|| panic!("no slot {name} on {owner}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn with_race(e: &Engine, race: &str) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = commands::set_base_scores(e, &s, SCORES).unwrap().state;
    let s = pick(e, &s, "ID_LEVEL_1", "Class", FIGHTER);
    pick(e, &s, "ID_LEVEL_1", "Race", race)
}

fn options(snap: &CharacterSnapshot, owner: &str, name: &str) -> Vec<String> {
    snap.slot(owner, name)
        .unwrap_or_else(|| panic!("no slot {name} on {owner}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn score(snap: &CharacterSnapshot, ability: &str) -> i32 {
    snap.abilities
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(ability))
        .unwrap_or_else(|| panic!("no {ability}"))
        .score
}

fn has(snap: &CharacterSnapshot, id: &str) -> bool {
    snap.elements.iter().any(|x| x.element.id == id)
}

#[test]
fn the_bundled_tiefling_offers_sub_races_from_other_content() {
    let e = engine();
    let snap = e.derive(&with_race(&e, TIEFLING));
    let slot = snap
        .slot(TIEFLING, "Tiefling Subrace")
        .expect("the bundled Tiefling has a Sub Race select");
    assert_eq!(slot.ty, "Sub Race");
    assert!(slot.optional, "Aurora's Tiefling sub race is optional");
    assert!(!slot.requires_selection);
    let mut offered = options(&snap, TIEFLING, "Tiefling Subrace");
    offered.sort();
    assert_eq!(offered, [ALT_LINE, PLAIN_LINE]);
}

#[test]
fn a_tiefling_without_a_sub_race_keeps_its_traits() {
    let e = engine();
    let snap = e.derive(&with_race(&e, TIEFLING));
    assert_eq!(score(&snap, "intelligence"), 11);
    assert_eq!(score(&snap, "charisma"), 12);
    assert!(has(&snap, INFERNAL_LEGACY));
}

#[test]
fn a_sub_race_without_the_marker_keeps_the_race_traits() {
    let e = engine();
    let s = pick(
        &e,
        &with_race(&e, TIEFLING),
        TIEFLING,
        "Tiefling Subrace",
        PLAIN_LINE,
    );
    let snap = e.derive(&s);
    assert!(has(&snap, PLAIN_LINE));
    assert_eq!(score(&snap, "intelligence"), 11);
    assert_eq!(score(&snap, "charisma"), 12);
    assert!(has(&snap, INFERNAL_LEGACY));
}

#[test]
fn a_sub_race_with_the_marker_replaces_the_race_scores_and_legacy() {
    let e = engine();
    let s = pick(
        &e,
        &with_race(&e, TIEFLING),
        TIEFLING,
        "Tiefling Subrace",
        ALT_LINE,
    );
    let snap = e.derive(&s);
    assert!(has(&snap, ALT_LINE));
    assert!(
        has(&snap, TIEFLING_MARKER),
        "the bundled packs define the marker the sub race grants"
    );
    assert_eq!(score(&snap, "dexterity"), 12);
    assert_eq!(
        score(&snap, "intelligence"),
        11,
        "+1 from the sub race only"
    );
    assert_eq!(score(&snap, "charisma"), 10, "the race's +2 is replaced");
    assert!(
        !has(&snap, INFERNAL_LEGACY),
        "the race's legacy is replaced"
    );
    assert!(has(&snap, ALT_LEGACY));
}

#[test]
fn the_tiefling_sub_race_select_stays_hidden_without_sub_races() {
    let e = engine_with(Vec::new());
    let snap = e.derive(&with_race(&e, TIEFLING));
    let slot = snap
        .slot(TIEFLING, "Tiefling Subrace")
        .expect("the select is there");
    assert!(slot.options.is_empty());
    assert!(slot.optional && !slot.requires_selection);
    assert!(snap.invalid_selections.is_empty());
}

#[test]
fn the_bundled_human_offers_variants_from_other_content() {
    let e = engine();
    let s = with_race(&e, HUMAN);
    let snap = e.derive(&s);
    assert_eq!(options(&snap, HUMAN, "Human Variant"), [HUMAN_MARKED]);
    assert_eq!(score(&snap, "strength"), 11);
    assert_eq!(score(&snap, "wisdom"), 11);
    let snap = e.derive(&pick(&e, &s, HUMAN, "Human Variant", HUMAN_MARKED));
    assert!(has(&snap, MARK_MARKER));
    assert_eq!(score(&snap, "strength"), 10, "the race's +1s are replaced");
    assert_eq!(score(&snap, "wisdom"), 12);
    assert_eq!(score(&snap, "constitution"), 11);
}

#[test]
fn the_bundled_half_orc_offers_variants_from_other_content() {
    let e = engine();
    let s = with_race(&e, HALF_ORC);
    let snap = e.derive(&s);
    assert_eq!(
        options(&snap, HALF_ORC, "Half-Orc Variant"),
        [HALF_ORC_MARKED]
    );
    assert_eq!(score(&snap, "strength"), 12);
    assert!(has(&snap, MENACING));
    let snap = e.derive(&pick(&e, &s, HALF_ORC, "Half-Orc Variant", HALF_ORC_MARKED));
    assert_eq!(
        score(&snap, "strength"),
        10,
        "the race's scores are replaced"
    );
    assert_eq!(score(&snap, "constitution"), 11);
    assert_eq!(score(&snap, "wisdom"), 12);
    assert!(!has(&snap, MENACING), "the race's traits are replaced");
}
