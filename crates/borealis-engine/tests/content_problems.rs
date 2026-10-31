//! GitHub #11 (C1): a content problem (an expression that can't be read) is reported only where the
//! character meets it. A fresh character carries no diagnostic about content it never touches; an
//! element it holds keeps its diagnostic; a choice whose own filter can't be read, or whose
//! options can't be offered because their requirements can't be read, says so on the slot
//! (`problem`), as Aurora fails such a select only when it is shown. Synthetic homebrew over the
//! bundled core + SRD 5.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::diag::code;
use borealis_engine::snapshot::{CharacterSnapshot, SlotProblem, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

const CLASS: &str = "ID_T_GRAVE_CLASS";

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": "dnd2014",
        "elements": [
            {"id": CLASS, "name": "Test Gravecaller", "type": "Class", "source": "Test Book",
             "setters": [{"name": "hd", "value": "d8"}], "rules": [
                {"kind": "select", "type": "Class Feature", "name": "Broken Gift", "supports": "Broken (Gift", "level": 1},
                {"kind": "select", "type": "Class Feature", "name": "Odd Pick", "supports": "T Odd", "level": 1},
                {"kind": "grant", "type": "Class Feature", "id": "ID_T_ULTIMATE", "level": 1}]},
            {"id": "ID_T_ODD_A", "name": "Odd A", "type": "Class Feature", "source": "Test Book", "supports": ["T Odd"]},
            {"id": "ID_T_ODD_B", "name": "Odd B", "type": "Class Feature", "source": "Test Book", "supports": ["T Odd"],
             "requirements": "Ultimate Fighter"},
            {"id": "ID_T_ULTIMATE", "name": "Ultimate Style", "type": "Class Feature", "source": "Test Book",
             "requirements": "Ultimate Fighter"},
            {"id": "ID_T_ELSEWHERE", "name": "Elsewhere", "type": "Feat", "source": "Test Book",
             "requirements": "Some Prose Name"}
        ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("grave.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn bad(snap: &CharacterSnapshot) -> Vec<String> {
    let mut out: Vec<String> = snap
        .diagnostics
        .iter()
        .filter(|d| d.code == code::CONTENT_BAD_EXPRESSION)
        .filter_map(|d| d.element.clone())
        .filter(|e| e.starts_with("ID_T_"))
        .collect();
    out.sort();
    out
}

fn slot<'a>(snap: &'a CharacterSnapshot, name: &str) -> &'a SlotSnapshot {
    snap.slots
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
}

fn with_class(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(CLASS.into()))
        .expect("class")
        .state
}

#[test]
fn a_fresh_character_has_no_diagnostic_about_content_it_never_touches() {
    let e = engine();
    let snap = e.derive(&commands::new_character(&e, Ruleset::Dnd2014));
    assert_eq!(bad(&snap), Vec::<String>::new());
    // the content-wide list still knows every broken element (the import report shows it)
    assert!(slot(&snap, "Class").problem.is_none());
}

#[test]
fn a_held_element_keeps_its_diagnostic_and_the_choices_say_what_cannot_be_read() {
    let e = engine();
    let snap = e.derive(&with_class(&e));
    assert_eq!(
        slot(&snap, "Broken Gift").problem,
        Some(SlotProblem {
            message: "This choice's filter, \"Broken (Gift\", can't be read, so it offers nothing."
                .into(),
            elements: vec![CLASS.into()],
            own_rule: Some(0),
        })
    );
    let odd = slot(&snap, "Odd Pick");
    assert_eq!(
        odd.options
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        ["Odd A"]
    );
    assert_eq!(
        odd.problem,
        Some(SlotProblem {
            message: "1 option can't be offered: its requirements can't be read (Odd B).".into(),
            elements: vec!["ID_T_ODD_B".into()],
            own_rule: None,
        })
    );
    // the held class, the feature it grants (withheld: its requirements can't be read) and the
    // option the character is offered keep their diagnostics; the feat nothing offers is left out
    assert_eq!(bad(&snap), [CLASS, "ID_T_ODD_B", "ID_T_ULTIMATE"]);
}

#[test]
fn a_derive_without_option_lists_keeps_only_held_content() {
    let e = engine();
    let snap = e.derive_with(&with_class(&e), false);
    assert_eq!(bad(&snap), [CLASS, "ID_T_ULTIMATE"]);
}
