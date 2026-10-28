//! Decision IM12: a 5etools creature printed without a challenge rating (a summoned spirit, a
//! level-scaled NPC) imports, through the real app state, on invented homebrew data only. The
//! compendium shows the note that stands in for its CR and no XP; a companion select that
//! restricts by CR never offers it, one that does not offers it; a character that picked it
//! saves, closes and loads back with it. Decision IM12a: a summon whose armor class and hit
//! points are formulas imports with the text, shown on its compendium card and companion tab in
//! place of the numbers, and keeps it through save, close and load.

use borealis_lib::content_dto::{FiveToolsImportReport, FiveToolsImportRequest};
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const CLASS: &str = "ID_5ET_HBX_CLASS_TAMER";
const HOUND: &str = "ID_5ET_HBX_MONSTER_PROBE_HOUND";
const SPIRIT: &str = "ID_5ET_HBX_MONSTER_PROBE_EMBER_SPIRIT";
const NPC: &str = "ID_5ET_HBX_MONSTER_PROBE_LEVELLED_GUIDE";
const WARD: &str = "ID_5ET_HBX_MONSTER_PROBE_WARD_SPIRIT";
const WARD_AC: &str = "9 + the kindling level";
const WARD_HP: &str = "15 + 5 for each kindling level";
/// The select whose filter restricts by challenge rating, and the one that does not.
const GATED: &str = "Beast Bond";
const UNGATED: &str = "Spirit Bond";

fn app(data: &Path) -> AppState {
    borealis_engine::invariants::enable_for_thread();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    };
    AppState::new(paths).expect("state")
}

fn creature(name: &str, extra: &Value) -> Value {
    let mut c = json!({"name": name, "source": "HBX", "size": ["S"], "type": "beast",
        "alignment": ["U"], "ac": [12], "hp": {"average": 9, "formula": "2d8"},
        "speed": {"walk": 40}, "str": 12, "dex": 14, "con": 11, "int": 3, "wis": 12,
        "cha": 6, "passive": 11,
        "action": [{"name": "Bite", "entries": ["{@atk mw} {@hit 3} to hit, reach 5 ft., \
            one target. {@h}4 ({@damage 1d6 + 1}) piercing damage."]}]});
    for (k, v) in extra.as_object().expect("object") {
        c[k] = v.clone();
    }
    c
}

/// An invented homebrew book: a class with a CR-gated and an ungated companion select, a
/// creature with a CR, a summoned one without, and a level-scaled one without.
fn homebrew(dir: &Path) {
    let feature = |name: &str, text: &str| {
        json!({"name": name, "source": "HBX", "className": "Tamer", "classSource": "HBX",
               "level": 1, "entries": [text]})
    };
    let brew = json!({
        "_meta": {"sources": [{"json": "HBX", "full": "Probe Homebrew"}]},
        "class": [{"name": "Tamer", "source": "HBX", "hd": {"number": 1, "faces": 8},
                   "proficiency": ["wis", "cha"],
                   "classFeatures": ["Beast Bond|Tamer|HBX|1", "Spirit Bond|Tamer|HBX|1"]}],
        "classFeature": [
            feature(GATED, "A beast joins you. Choose a {@filter small beast|bestiary|\
                challenge rating=[&0;&1/4]|type=beast|size=t;s;m}."),
            feature(UNGATED, "A spirit joins you. Choose a {@filter beast|bestiary|type=beast}."),
        ],
        "monster": [
            creature("Probe Hound", &json!({"cr": "1/8"})),
            creature("Probe Ember Spirit", &json!({
                "summonedBySpell": "Probe Kindling|HBX", "summonedBySpellLevel": 2})),
            creature("Probe Levelled Guide", &json!({"level": 4, "type": "humanoid"})),
            creature("Probe Ward Spirit", &json!({
                "summonedBySpell": "Probe Kindling|HBX", "summonedBySpellLevel": 2,
                "ac": [{"special": WARD_AC}], "hp": {"special": WARD_HP}})),
        ]
    });
    std::fs::create_dir_all(dir.join("homebrew")).unwrap();
    std::fs::write(dir.join("homebrew/probe.json"), brew.to_string()).unwrap();
}

fn import(app: &AppState, dir: &Path) -> FiveToolsImportReport {
    let folder = app
        .picked
        .grant(dir.to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token;
    let request = FiveToolsImportRequest {
        folder,
        sources: vec!["HBX".into()],
        include_fluff: true,
    };
    app.import_5etools(&request, &NullSink).expect("import")
}

fn fact(app: &AppState, id: &str, label: &str) -> String {
    app.get_element(id)
        .unwrap_or_else(|e| panic!("{id}: {e:?}"))
        .facts
        .into_iter()
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("{id} has no {label} fact"))
        .value
}

fn challenge(app: &AppState, id: &str) -> String {
    fact(app, id, "Challenge")
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| {
            let all: Vec<_> = snap.slots.iter().map(|s| s.title.as_str()).collect();
            panic!("no {title} slot in {all:?}")
        })
        .id
        .clone()
}

fn offered(app: &AppState, snap: &CharacterSnapshot, title: &str) -> Vec<String> {
    app.get_slot_options(&snap.id, &slot_id(snap, title), None, Some(500), None)
        .expect("options")
        .items
        .iter()
        .flat_map(|o| o.all_versions().iter().map(|v| v.summary.id.clone()))
        .collect()
}

fn tamer(app: &AppState) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Probe".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let s = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let class_slot = s
        .slots
        .iter()
        .find(|x| x.element_type.eq_ignore_ascii_case("Class"))
        .expect("class slot")
        .id
        .clone();
    app.apply_selection(&s.id, &class_slot, CLASS)
        .expect("pick the class")
}

#[test]
fn a_creature_without_a_challenge_rating_imports_shows_its_note_and_fills_only_ungated_selects() {
    let data = tempfile::tempdir().expect("data dir");
    homebrew(data.path());
    let home = tempfile::tempdir().expect("app dir");
    let app = app(home.path());
    let report = import(&app, data.path());
    assert_eq!(report.packs.len(), 1);
    let skipped: Vec<&str> = report.skipped.iter().map(|n| n.reason.as_str()).collect();
    assert!(skipped.is_empty(), "nothing is skipped: {skipped:?}");

    // display: the CR with its XP, else the note that stands in for it and no XP
    assert_eq!(challenge(&app, HOUND), "1/8 (25 XP)");
    assert_eq!(
        challenge(&app, SPIRIT),
        "Scales with spell level (Probe Kindling)"
    );
    assert_eq!(challenge(&app, NPC), "Level 4");

    // a CR-gated select never offers it, an ungated one does
    let s = tamer(&app);
    let gated = offered(&app, &s, GATED);
    assert!(gated.iter().any(|id| id == HOUND), "{gated:?}");
    assert!(!gated.iter().any(|id| id == SPIRIT), "{gated:?}");
    let ungated = offered(&app, &s, UNGATED);
    assert!(ungated.iter().any(|id| id == HOUND), "{ungated:?}");
    assert!(ungated.iter().any(|id| id == SPIRIT), "{ungated:?}");
    assert!(
        !ungated.iter().any(|id| id == NPC),
        "a humanoid is no beast"
    );

    // round trip: picked, saved, closed, loaded back with the same companion
    let s = app
        .apply_selection(&s.id, &slot_id(&s, UNGATED), SPIRIT)
        .expect("pick the spirit");
    let picked = |snap: &CharacterSnapshot| {
        snap.companion
            .as_ref()
            .and_then(|c| c.creature.as_ref())
            .map(|c| c.id.clone())
    };
    assert_eq!(picked(&s).as_deref(), Some(SPIRIT));
    assert_eq!(s.companion.as_ref().map(|c| c.hp), Some(9));
    app.save_character(&s.id).expect("save");
    app.close_character(&s.id, false).expect("close");
    let reloaded = app.load_character(&s.id).expect("load");
    assert_eq!(picked(&reloaded).as_deref(), Some(SPIRIT));
    let chosen = reloaded
        .slots
        .iter()
        .find(|x| x.title == UNGATED)
        .and_then(|x| x.chosen.as_ref())
        .map(|c| c.id.clone());
    assert_eq!(chosen.as_deref(), Some(SPIRIT));

    // a fresh app on the same data folder reads the stored pack and the character back
    drop(app);
    let again = self::app(home.path());
    assert_eq!(
        challenge(&again, SPIRIT),
        "Scales with spell level (Probe Kindling)"
    );
    let reopened = again.load_character(&s.id).expect("load in a new app");
    assert_eq!(picked(&reopened).as_deref(), Some(SPIRIT));
}

#[test]
fn a_summon_whose_armor_class_and_hit_points_are_formulas_imports_and_shows_the_text() {
    let data = tempfile::tempdir().expect("data dir");
    homebrew(data.path());
    let home = tempfile::tempdir().expect("app dir");
    let app = app(home.path());
    let report = import(&app, data.path());
    assert!(report.skipped.is_empty(), "nothing is skipped");

    // the card shows the text in place of the numbers; a numbered stat block its numbers
    assert_eq!(fact(&app, WARD, "Armor Class"), WARD_AC);
    assert_eq!(fact(&app, WARD, "Hit Points"), WARD_HP);
    assert_eq!(fact(&app, HOUND, "Armor Class"), "12");
    assert_eq!(fact(&app, HOUND, "Hit Points"), "9 (2d8)");

    // the companion tab shows the text, not a number it would have to invent
    let s = tamer(&app);
    assert!(offered(&app, &s, UNGATED).iter().any(|id| id == WARD));
    assert!(!offered(&app, &s, GATED).iter().any(|id| id == WARD));
    let s = app
        .apply_selection(&s.id, &slot_id(&s, UNGATED), WARD)
        .expect("pick the ward spirit");
    let notes = |snap: &CharacterSnapshot| {
        let c = snap.companion.as_ref().expect("companion");
        (c.armor_class_note.clone(), c.hp_note.clone())
    };
    let want = (Some(WARD_AC.to_string()), Some(WARD_HP.to_string()));
    assert_eq!(notes(&s), want);
    // the challenge on the tab is the one the card shows (a note or an em dash, never a CR)
    assert_eq!(
        s.companion.as_ref().map(|c| c.challenge.clone()),
        Some(challenge(&app, WARD))
    );
    app.save_character(&s.id).expect("save");
    app.close_character(&s.id, false).expect("close");
    assert_eq!(notes(&app.load_character(&s.id).expect("load")), want);

    // a fresh app reads the stored stat block back with its text
    drop(app);
    let again = self::app(home.path());
    assert_eq!(fact(&again, WARD, "Hit Points"), WARD_HP);
    assert_eq!(notes(&again.load_character(&s.id).expect("load")), want);
}
