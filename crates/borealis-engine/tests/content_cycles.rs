//! Malformed content never crashes the engine: grant and selection cycles (an element that
//! grants itself, two elements that grant each other, an element that may select itself) are
//! cut at the repeat and reported as a `content.grant_cycle` diagnostic, and deeply nested
//! expressions are refused as bad expressions instead of overflowing the stack.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::diag::code;
use borealis_engine::invariants::{self, kind};
use borealis_engine::state::{CharacterState, Choice, Selection};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const CLUB: &str = "ID_PROFICIENCY_WEAPON_PROFICIENCY_CLUB";
const SIMPLE_MELEE: &str = "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_MELEE_WEAPONS";

fn grant(id: &str) -> Value {
    json!({"kind": "grant", "id": id, "type": "Proficiency"})
}

fn engine(file: &Value) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(file.clone()).expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("cycles.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1).expect("slot");
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn fighter(e: &Engine, race: Option<&str>) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    if let Some(race) = race {
        s = pick(e, &s, "ID_LEVEL_1", "Race", race);
    }
    pick(e, &s, "ID_LEVEL_1", "Class", FIGHTER)
}

fn cycle_diagnostics(e: &Engine, s: &CharacterState) -> Vec<String> {
    e.derive(s)
        .diagnostics
        .iter()
        .filter(|d| d.code == code::CONTENT_GRANT_CYCLE)
        .map(|d| d.message.clone())
        .collect()
}

fn has(e: &Engine, s: &CharacterState, id: &str) -> bool {
    e.derive(s).elements.iter().any(|x| x.element.id == id)
}

/// The shape an import produced: a per-weapon proficiency appended with a grant of itself,
/// reached from a class through the bundled category proficiency.
#[test]
fn a_self_granting_append_is_cut_and_reported() {
    let e = engine(&json!({
        "format": "borealis-content/1", "ruleset": "dnd2014", "elements": [],
        "appends": [{"id": CLUB, "type": "Proficiency", "rules": [grant(CLUB)]},
                    {"id": SIMPLE_MELEE, "type": "Proficiency", "rules": [grant(CLUB)]}]
    }));
    let s = fighter(&e, None);
    assert!(
        has(&e, &s, CLUB),
        "the club proficiency itself is still granted"
    );
    let diags = cycle_diagnostics(&e, &s);
    assert!(
        diags.iter().any(|m| m.contains(CLUB)),
        "cycle reported: {diags:?}"
    );
    let found: Vec<_> = invariants::check(&e, &s, &e.derive(&s))
        .into_iter()
        .map(|v| v.kind)
        .collect();
    assert!(found.contains(&kind::GRANT_CYCLE), "{found:?}");
}

/// Two elements that grant each other, reached from a race.
#[test]
fn a_two_element_grant_cycle_is_cut_and_reported() {
    let e = engine(&json!({
        "format": "borealis-content/1", "ruleset": "dnd2014",
        "elements": [
            {"id": "ID_T_RACE_LOOP", "name": "Loop Folk", "type": "Race", "source": "Test",
             "rules": [grant("ID_T_PROF_TICK")]},
            {"id": "ID_T_PROF_TICK", "name": "Tick", "type": "Proficiency", "source": "Test",
             "rules": [grant("ID_T_PROF_TOCK")]},
            {"id": "ID_T_PROF_TOCK", "name": "Tock", "type": "Proficiency", "source": "Test",
             "rules": [grant("ID_T_PROF_TICK")]}
        ]
    }));
    let s = fighter(&e, Some("ID_T_RACE_LOOP"));
    assert!(has(&e, &s, "ID_T_PROF_TICK") && has(&e, &s, "ID_T_PROF_TOCK"));
    let diags = cycle_diagnostics(&e, &s);
    assert!(
        diags.iter().any(|m| m.contains("ID_T_PROF_TICK")),
        "cycle reported: {diags:?}"
    );
    let snap = e.derive(&s);
    let ticks = snap
        .elements
        .iter()
        .filter(|x| x.element.id == "ID_T_PROF_TICK")
        .count();
    assert_eq!(ticks, 1, "the cycle is cut at the repeat");
}

/// A select that offers its own owner: a stored pick of it must not nest forever.
#[test]
fn a_self_selecting_element_is_cut_and_reported() {
    let e = engine(&json!({
        "format": "borealis-content/1", "ruleset": "dnd2014",
        "elements": [
            {"id": "ID_T_RACE_MIRROR", "name": "Mirror Folk", "type": "Race", "source": "Test",
             "rules": [grant("ID_T_PROF_MIRROR")]},
            {"id": "ID_T_PROF_MIRROR", "name": "Mirror", "type": "Proficiency", "source": "Test",
             "rules": [{"kind": "select", "type": "Proficiency", "name": "Mirror Pick",
                        "supports": "ID_T_PROF_MIRROR||ID_T_PROF_GLASS"}]},
            {"id": "ID_T_PROF_GLASS", "name": "Glass", "type": "Proficiency", "source": "Test",
             "supports": ["ID_T_PROF_GLASS"]}
        ]
    }));
    let s = fighter(&e, Some("ID_T_RACE_MIRROR"));
    let key = slot_key(&e.derive(&s), "ID_T_PROF_MIRROR", "Mirror Pick", 1).expect("slot");
    let mut s = s;
    s.selections.push(Selection {
        slot: key,
        choice: Choice::Element("ID_T_PROF_MIRROR".into()),
        replace_level: None,
    });
    let snap = e.derive(&s);
    let mirrors = snap
        .elements
        .iter()
        .filter(|x| x.element.id == "ID_T_PROF_MIRROR")
        .count();
    assert!(mirrors <= 2, "nesting stopped, got {mirrors}");
}

/// A requirement nested far deeper than any real content is refused, not a stack overflow.
#[test]
fn a_deeply_nested_requirement_is_a_bad_expression() {
    let deep = format!("{}ID_T_X{}", "(".repeat(50_000), ")".repeat(50_000));
    let nots = format!("{}ID_T_X", "!".repeat(50_000));
    let e = engine(&json!({
        "format": "borealis-content/1", "ruleset": "dnd2014",
        "elements": [
            {"id": "ID_T_RACE_DEEP", "name": "Deep Folk", "type": "Race", "source": "Test",
             "requirements": deep},
            {"id": "ID_T_RACE_NOTS", "name": "Not Folk", "type": "Race", "source": "Test",
             "requirements": nots}
        ]
    }));
    let s = commands::new_character(&e, Ruleset::Dnd2014);
    let snap = e.derive(&s);
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == code::CONTENT_BAD_EXPRESSION));
}
