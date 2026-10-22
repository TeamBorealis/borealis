//! A subclass chosen on a multiclass's own subclass select grants its features at that class's
//! level (`engine.md` 3.1: a selected or granted Archetype / Archetype Feature joins the class
//! manager whose select or tree produced it, and that manager's level gates its grants), never
//! at the character level or the main class's level. Synthetic content only.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const FIRST: &str = "ID_T_MSL_FIRST";
const SECOND: &str = "ID_T_MSL_SECOND";
const SECOND_MC: &str = "ID_T_MSL_SECOND_MC";
const PATH_FEATURE: &str = "ID_T_MSL_PATH_FEATURE";
const ARCH: &str = "ID_T_MSL_ARCH";
const ARCH_F3: &str = "ID_T_MSL_ARCH_F3";
const ARCH_F6: &str = "ID_T_MSL_ARCH_F6";
const LORE: &str = "ID_T_MSL_LORE";

fn content() -> Vec<Value> {
    vec![
        json!({"id": FIRST, "name": "First", "type": "Class", "source": "Test",
               "setters": [{"name": "hd", "value": "d10"}], "rules": [],
               "multiclass": {"id": "ID_T_MSL_FIRST_MC", "requirements": "", "rules": []}}),
        json!({"id": SECOND, "name": "Second", "type": "Class", "source": "Test",
               "setters": [{"name": "hd", "value": "d8"}],
               "rules": [{"kind": "grant", "id": PATH_FEATURE, "type": "Class Feature", "level": 3}],
               "multiclass": {"id": SECOND_MC, "requirements": "", "rules": []}}),
        json!({"id": PATH_FEATURE, "name": "Second Path", "type": "Class Feature", "source": "Test",
               "rules": [{"kind": "select", "type": "Archetype", "name": "Second Path",
                          "supports": "Second Path"}]}),
        json!({"id": ARCH, "name": "Test Path", "type": "Archetype", "source": "Test",
               "supports": ["Second Path"],
               "rules": [
                   {"kind": "grant", "id": ARCH_F3, "type": "Archetype Feature", "level": 3},
                   {"kind": "grant", "id": ARCH_F6, "type": "Archetype Feature", "level": 6}]}),
        json!({"id": ARCH_F3, "name": "Path Lore", "type": "Archetype Feature", "source": "Test",
               "rules": [{"kind": "select", "type": "Archetype Feature", "name": "Path Lore"}]}),
        json!({"id": ARCH_F6, "name": "Path Six", "type": "Archetype Feature", "source": "Test"}),
        json!({"id": LORE, "name": "Lore Pick", "type": "Archetype Feature", "source": "Test"}),
    ]
}

fn engine() -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": content()}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, 1).unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{err}"))
        .state
}

fn has_slot(e: &Engine, s: &CharacterState, owner: &str, name: &str) -> bool {
    slot_key(&e.derive(s), owner, name, 1).is_some()
}

/// `first_levels` of the main class, then `second_levels` of the multiclass, with the subclass
/// picked on the multiclass's subclass select once it appears.
fn multiclassed(e: &Engine, first_levels: u32, second_levels: u32) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s.abilities.base = [14, 14, 14, 14, 14, 14];
    s = pick(e, &s, "ID_LEVEL_1", "Class", FIRST);
    for _ in 1..first_levels {
        s = commands::level_up_main(e, &s, None).expect("main level").state;
    }
    s = commands::level_up_new_multiclass(e, &s, None)
        .expect("new multiclass")
        .state;
    let start = s.level();
    s = pick(
        e,
        &s,
        &format!("ID_LEVEL_{start}"),
        &format!("Multiclass (Level {start})"),
        SECOND_MC,
    );
    for _ in 1..second_levels {
        s = commands::level_up_multiclass(e, &s, start, None)
            .expect("multiclass level")
            .state;
    }
    if has_slot(e, &s, PATH_FEATURE, "Second Path") {
        s = pick(e, &s, PATH_FEATURE, "Second Path", ARCH);
    }
    s
}

#[test]
fn pure_second_class_gets_the_subclass_level_three_feature() {
    let e = engine();
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s = pick(&e, &s, "ID_LEVEL_1", "Class", SECOND);
    for _ in 0..2 {
        s = commands::level_up_main(&e, &s, None).expect("level").state;
    }
    s = pick(&e, &s, PATH_FEATURE, "Second Path", ARCH);
    let snap = e.derive(&s);
    assert!(snap.has_element(ARCH_F3));
    assert!(!snap.has_element(ARCH_F6));
    assert!(has_slot(&e, &s, ARCH_F3, "Path Lore"));
}

#[test]
fn multiclass_subclass_grants_at_the_multiclass_level() {
    let e = engine();
    let s = multiclassed(&e, 1, 3);
    let snap = e.derive(&s);
    assert!(snap.has_element(ARCH), "subclass on the multiclass select");
    assert!(snap.has_element(ARCH_F3), "level-3 subclass feature at second 3");
    assert!(!snap.has_element(ARCH_F6));
    assert!(has_slot(&e, &s, ARCH_F3, "Path Lore"));
    let s = pick(&e, &s, ARCH_F3, "Path Lore", LORE);
    assert!(e.derive(&s).has_element(LORE));
}

#[test]
fn multiclass_subclass_ignores_character_and_main_class_levels() {
    let e = engine();
    // first 5 / second 2: character level 7, main class 5, the subclass feature not yet granted
    let s = multiclassed(&e, 5, 2);
    let snap = e.derive(&s);
    assert!(!snap.has_element(PATH_FEATURE), "second 2 has no subclass yet");
    // first 5 / second 3: subclass level 3 only, never the 6 the main class or character has
    let s = multiclassed(&e, 5, 3);
    let snap = e.derive(&s);
    assert!(snap.has_element(ARCH_F3));
    assert!(!snap.has_element(ARCH_F6), "main class or character level used");
}
