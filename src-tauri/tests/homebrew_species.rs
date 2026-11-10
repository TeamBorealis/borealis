//! WB-15 (homebrew creator species forms, spec 4.4) through the real `AppState` on a temporary data folder with the
//! bundled content:
//!
//! * a new 2024 species with a speed of 35 feet and darkvision of 120 feet saves with those rules, a new 2024
//!   character's species slot offers it, and the character who takes it walks 35 feet and has darkvision 120;
//! * a new 2014 subrace whose parent race is the SRD Dwarf is offered by the Dwarf's subrace choice and gives its
//!   ability score increase.
//!
//! Both open again in their forms, and an unchanged save writes the same element.

use borealis_authoring::form::FormState;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const SPECIES_ID: &str = "ID_EI_SPECIES_EMBERKIN";
const SUBRACE_ID: &str = "ID_OW_SUBRACE_ASH_DWARF";
const DWARF: &str = "ID_SRD_RACE_DWARF";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        Self { data, state }
    }

    fn pack(&self, name: &str, code: &str, edition: Ruleset) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: name.into(),
                    code: code.into(),
                    edition,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn saved(&self, pack: &str, id: &str) -> Value {
        let path = self
            .data
            .path()
            .join("library")
            .join(pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))
            .clone()
    }

    /// Open the saved element again, save it unchanged, and give back what was written.
    fn resave(&self, pack: &str, id: &str, kind: &str) -> Value {
        let again = self
            .state
            .homebrew_open_draft(pack, Some(id), None)
            .unwrap();
        assert_eq!(again.form.kind, kind);
        let touched = self.update(&again, &again.form);
        self.state
            .homebrew_save_draft(&touched.draft_id, &Quiet)
            .unwrap();
        self.saved(pack, id)
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn warnings(v: &HomebrewDraftView) -> Vec<String> {
    v.problems
        .iter()
        .filter(|p| p.severity == "warning")
        .map(|p| p.code.clone())
        .collect()
}

/// A new character of `ruleset` with `id` in its species (race) slot, if the slot offers it.
fn with_species(e: &Engine, ruleset: ContentRuleset, id: &str) -> Option<CharacterState> {
    let s = commands::new_character(e, ruleset);
    let snap = e.derive(&s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.owner.id == "ID_LEVEL_1" && x.name == "Race")
        .expect("a species slot");
    if !slot.options.iter().any(|o| o.id == id) {
        return None;
    }
    let key = slot_key(&snap, "ID_LEVEL_1", "Race", 1).expect("slot key");
    Some(
        commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap()
            .state,
    )
}

#[test]
fn a_new_2024_species_is_offered_in_the_species_slot_with_its_speed_and_darkvision() {
    let app = App::new();
    let pack = app.pack("Ember Isles", "EI", Ruleset::Dnd2024);
    let v = app
        .state
        .homebrew_new_draft(&pack, "species", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "species");
    assert_eq!(v.form.fields["speed"], json!(30));
    assert_eq!(v.form.fields["size"], json!(["Medium"]));

    let form = v
        .form
        .with_field("name", json!("Emberkin"))
        .with_field("speed", json!(35))
        .with_field("darkvision", json!(120));
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [SPECIES_ID]);

    let species = app.saved(&pack, SPECIES_ID);
    assert_eq!(species["type"], "Species");
    assert_eq!(
        species["rules"],
        json!([
            {"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"},
            {"kind": "stat", "name": "innate speed", "value": "35", "bonus": "base"},
            {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
            {"kind": "stat", "name": "darkvision:range", "value": "120", "bonus": "base"}
        ])
    );
    assert_eq!(species["data"], json!({"creature_type": "Humanoid"}));

    // a new 2024 character's species slot offers it; taken, the character walks 35 feet and sees 120 in the dark
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = with_species(e, ContentRuleset::Dnd2024, SPECIES_ID)
        .expect("the 2024 species slot offers the new species");
    let snap = e.derive(&s);
    assert_eq!(snap.speed.walk, 35);
    let darkvision = snap
        .senses
        .iter()
        .find(|x| x.name == "darkvision")
        .expect("darkvision");
    assert_eq!(darkvision.range, 120);
    drop(hub);

    // it opens again in the species form with its fields; an unchanged save writes the same element
    let again = app
        .state
        .homebrew_open_draft(&pack, Some(SPECIES_ID), None)
        .unwrap();
    assert_eq!(again.form.fields["speed"], json!(35));
    assert_eq!(again.form.fields["darkvision"], json!(120));
    assert!(again.form.blocks.is_empty());
    app.state.homebrew_discard_draft(&again.draft_id).unwrap();
    assert_eq!(app.resave(&pack, SPECIES_ID, "species"), species);
}

#[test]
fn a_new_2014_subrace_is_offered_under_its_parent_race() {
    let app = App::new();
    let pack = app.pack("Old World", "OW", Ruleset::Dnd2014);
    let v = app
        .state
        .homebrew_new_draft(&pack, "subRace", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "subRace");
    assert!(errors(&v).contains(&(
        "fields.parent".to_string(),
        "A subrace needs a parent race.".to_string()
    )));

    // a parent tag no race's choice offers is a warning at the field
    let typo = app.update(
        &v,
        &v.form
            .with_field("name", json!("Ash Dwarf"))
            .with_field("parent", json!("Dwarrf")),
    );
    assert_eq!(errors(&typo), Vec::<(String, String)>::new());
    assert!(warnings(&typo).contains(&"lint.not_offered".to_string()));

    let form = typo
        .form
        .with_field("parent", json!("Dwarf"))
        .with_field("charisma", json!(1))
        .with_field("darkvision", json!(120));
    let v = app.update(&typo, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    assert!(!warnings(&v).contains(&"lint.not_offered".to_string()));
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [SUBRACE_ID]);

    let sub = app.saved(&pack, SUBRACE_ID);
    assert_eq!(sub["type"], "Sub Race");
    assert_eq!(sub["supports"], json!(["Dwarf"]));
    assert_eq!(
        sub["rules"],
        json!([
            {"kind": "stat", "name": "charisma", "value": "1"},
            {"kind": "grant", "id": "ID_VISION_DARKVISION", "type": "Vision"},
            {"kind": "stat", "name": "darkvision:range", "value": "120", "bonus": "base"}
        ])
    );

    // a 2014 dwarf's subrace choice offers it; taken, it gives +1 Charisma and darkvision 120
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let dwarf = with_species(e, ContentRuleset::Dnd2014, DWARF).expect("the SRD dwarf");
    let snap = e.derive(&dwarf);
    let choice = snap
        .slots
        .iter()
        .find(|x| x.ty == "Sub Race")
        .expect("the dwarf's subrace choice");
    assert!(
        choice.options.iter().any(|o| o.id == SUBRACE_ID),
        "the subrace choice offers the new subrace"
    );
    let charisma = |snap: &borealis_engine::snapshot::CharacterSnapshot| {
        snap.abilities
            .iter()
            .find(|a| a.name == "charisma")
            .map(|a| a.additional)
            .unwrap_or_default()
    };
    let before = charisma(&snap);
    let taken = commands::select(e, &dwarf, &choice.key, Choice::Element(SUBRACE_ID.into()))
        .unwrap()
        .state;
    let after = e.derive(&taken);
    assert_eq!(charisma(&after), before + 1);
    assert_eq!(
        after
            .senses
            .iter()
            .find(|x| x.name == "darkvision")
            .map(|x| x.range),
        Some(120)
    );
    drop(hub);

    assert_eq!(app.resave(&pack, SUBRACE_ID, "subRace"), sub);
}
