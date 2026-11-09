//! Tester B11 (B-135, TV2): Ability Scores offers "Suggested for <class>", that class's standard
//! array from the ruleset's tables, for the user to apply.

use borealis_character::Character;
use borealis_content::Ruleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::dto::AbilityRecord;
use borealis_lib::session::new_id;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, "ID_LEVEL_1", name, 1).unwrap_or_else(|| panic!("slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn open(app: &AppState, build: CharacterState) -> String {
    let mut c = Character::new("Hero");
    c.build = build;
    let id = new_id();
    let path = app.character_path(&id).unwrap();
    c.save(&path).unwrap();
    app.insert_session(&id, c, path, false).unwrap();
    id
}

#[test]
fn a_2024_barbarian_is_suggested_the_barbarian_standard_array() {
    let (_dir, app) = app();
    let hub = app.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    let none = app.get_snapshot(&open(&app, s.clone())).unwrap();
    assert_eq!(
        none.abilities.suggested, None,
        "no class, nothing to suggest"
    );
    s = pick(e, &s, "Class", "ID_2024_CLASS_BARBARIAN");
    let snap = app.get_snapshot(&open(&app, s)).unwrap();
    let array = e
        .universe(Ruleset::Dnd2024)
        .tables()
        .standard_array_for("Barbarian")
        .expect("barbarian array");
    let suggested = snap.abilities.suggested.expect("a suggestion");
    assert_eq!(suggested.class_name, "Barbarian");
    assert_eq!(suggested.scores, AbilityRecord::from_fn(|i| array[i]));
    assert!(
        suggested.scores.str >= 15 && suggested.scores.con >= 13,
        "a Barbarian leads with Strength and Constitution"
    );
}

#[test]
fn a_ruleset_without_class_arrays_suggests_nothing() {
    let (_dir, app) = app();
    let hub = app.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    let class = e
        .derive(&s)
        .slots
        .iter()
        .find(|x| x.name == "Class")
        .and_then(|x| x.options.first())
        .map(|o| o.id.clone())
        .expect("a 2014 class on offer");
    s = pick(e, &s, "Class", &class);
    let snap = app.get_snapshot(&open(&app, s)).unwrap();
    assert_eq!(snap.abilities.suggested, None);
}
