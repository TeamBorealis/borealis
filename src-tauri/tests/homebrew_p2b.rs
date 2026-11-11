//! P2b (homebrew creator P2 items, 1.5.0) through the real `AppState` on a temporary data folder with the bundled
//! content:
//!
//! * Try it's level ladder tells a subclass's level problems in the creator's plain words (WB-07's sentences);
//! * a homebrew weapon tried out is a weapon: carried and equipped (never attuned when it needs no attunement), an
//!   attack, never a feature, and offered by the Weapon Mastery choice;
//! * a weapon's live preview gives its damage and type, properties, mastery, weapon category, cost with its coin and
//!   weight;
//! * a creature's live preview (the compendium's monster entry) is the whole stat block: size, type and alignment,
//!   armor class, hit points, speed, the six scores with their modifiers, saving throws, skills, damage and condition
//!   lines, senses, languages and challenge, then the sections;
//! * SPELL LISTS edits a subclass's expanded list: the own subclass's `spellcasting.extends`, or the pack's append
//!   for a loaded subclass, and a character of that subclass is offered the spell.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewTrialSetup, HomebrewTrialView,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const CHAMPION: &str = "ID_2024_SUBCLASS_FIGHTER_CHAMPION";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    _data: tempfile::TempDir,
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
        Self { _data: data, state }
    }

    fn pack(&self, edition: Ruleset) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Gloom Hollow".into(),
                    code: "GH".into(),
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

    /// A deep copy of a loaded element (its features made inside it).
    fn deep_copy(&self, pack: &str, id: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, "other", Some(id), None, Some(true))
            .unwrap()
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn try_it(&self, v: &HomebrewDraftView, level: u32) -> HomebrewTrialView {
        self.state
            .homebrew_trial_start(
                &v.draft_id,
                &HomebrewTrialSetup {
                    level: Some(level),
                    ..Default::default()
                },
            )
            .unwrap()
    }
}

/// The child form named `name`.
fn child_named<'f>(form: &'f mut FormState, name: &str) -> &'f mut FormState {
    form.children
        .iter_mut()
        .find(|c| c.fields.get("name") == Some(&json!(name)))
        .unwrap_or_else(|| panic!("a child named {name}"))
}

#[test]
fn the_ladder_tells_a_subclass_s_level_problems_in_plain_words() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let copy = app.deep_copy(&pack, CHAMPION);
    let mut form = copy.form.clone();
    // the copied level-3 feature gets a choice of something no proficiency is: it offers nothing
    let athlete = child_named(&mut form, "Remarkable Athlete");
    athlete.blocks.push(
        serde_json::from_value(json!({"key": "b-new", "kind": "select",
            "fields": {"type": "Proficiency", "name": "Moonlit Lore", "level": 3, "number": 1,
                "supports": {"value": {"shape": "kept", "text": "ID_GH_NO_SUCH_SUPPORT", "reason": "test"}}}}))
        .unwrap(),
    );
    let draft = app.update(&copy, &form);
    // Try it loads the unsaved copy as a save writes it: the copy grants its own (edited) feature, not the SRD's
    let view = app.try_it(&draft, 3);
    let features: Vec<String> = serde_json::to_value(&view.snapshot.play.features)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["id"].as_str().map(str::to_string))
        .collect();
    assert!(
        features
            .iter()
            .any(|id| id.starts_with("ID_GH_") && id.ends_with("_REMARKABLE_ATHLETE")),
        "the copied feature is on the test character: {features:?}"
    );
    let at3: Vec<&str> = view.ladder[2]
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    assert!(
        at3.contains(&"At level 3 the choice \"Moonlit Lore\" has nothing to offer."),
        "the creator's sentence: {at3:?}"
    );
    let every: Vec<&str> = view
        .ladder
        .iter()
        .flat_map(|r| r.problems.iter().map(|p| p.message.as_str()))
        .collect();
    for raw in [
        "offers no options",
        "level-up to",
        "is missing at level",
        "select \"",
    ] {
        assert!(
            every.iter().all(|m| !m.contains(raw)),
            "no engine text ({raw}): {every:?}"
        );
    }
}
