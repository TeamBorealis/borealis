//! WB-13 (homebrew creator feat form, spec 4.2) through the real `AppState` on a temporary data folder with the bundled
//! content: a new 2024 General feat with a prerequisite and an ability score increase saves in the 2024 shape (category
//! setter, `"General Feat"` tag, `requirements` and the display sentence written from one condition), and a 2024
//! fighter's level-4 feat choice offers it when the character meets the prerequisite and not when it doesn't. Taking it
//! there gives its ability score increase choice. It opens again in the feat form unchanged.

use borealis_authoring::expr_build::condition::ConditionRow;
use borealis_authoring::expr_build::{Ability, ExprValue, GroupMode, Node};
use borealis_authoring::form::FormState;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine, SlotKey};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const FEAT_ID: &str = "ID_EI_FEAT_EMBER_HEART";
const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";
const FEAT_CHOICE: &str = "Ability Score Improvement (Fighter 4)";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
    pack: String,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        let pack = state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Ember Isles".into(),
                    code: "EI".into(),
                    edition: Ruleset::Dnd2024,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id;
        Self { data, state, pack }
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn saved(&self) -> Vec<Value> {
        let path = self
            .data
            .path()
            .join("library")
            .join(&self.pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"].as_array().unwrap().clone()
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

/// "Character level at least 4, and Strength at least 13."
fn prerequisite() -> Value {
    let model = Node::group(
        GroupMode::All,
        vec![
            Node::row(ConditionRow::CharacterLevel { min: 4 }),
            Node::row(ConditionRow::Ability {
                ability: Ability::Strength,
                min: 13,
            }),
        ],
    );
    serde_json::to_value(ExprValue::new(model)).unwrap()
}

/// A 2024 fighter at level 4 with these scores, before its level-4 choice.
fn fighter_4(e: &Engine, scores: [i32; 6]) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(FIGHTER.into()))
        .unwrap()
        .state;
    s = commands::set_base_scores(e, &s, scores).unwrap().state;
    while s.level() < 4 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

/// The level-4 feat choice, and whether it offers the new feat.
fn feat_choice(e: &Engine, s: &CharacterState) -> (SlotKey, bool) {
    let snap = e.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name == FEAT_CHOICE)
        .unwrap_or_else(|| panic!("no {FEAT_CHOICE}"));
    (
        slot.key.clone(),
        slot.options.iter().any(|o| o.id == FEAT_ID),
    )
}

#[test]
fn a_new_2024_feat_saves_and_is_offered_at_a_level_4_feat_choice() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "feat", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "feat");
    assert_eq!(
        errors(&v),
        [("fields.name".to_string(), "Give it a name.".to_string())]
    );

    // what it gives: the SRD Grappler's ability score increase, taken from a copy of it
    let grappler = app
        .state
        .homebrew_new_draft(&app.pack, "feat", Some("ID_2024_FEAT_GRAPPLER"), None, None)
        .unwrap();
    assert_eq!(
        grappler.form.kind, "feat",
        "a copied feat opens in the feat form"
    );
    assert_eq!(grappler.form.fields["category"], json!("General"));
    let increase = grappler
        .form
        .blocks
        .iter()
        .find(|b| b.kind == "ability_increase")
        .expect("increase block")
        .clone();
    app.state
        .homebrew_discard_draft(&grappler.draft_id)
        .unwrap();

    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Ember Heart"));
    form.fields.insert("category".into(), json!("General"));
    form.fields.insert("requirements".into(), prerequisite());
    form.blocks = vec![increase];
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [FEAT_ID]);

    let feat = app
        .saved()
        .into_iter()
        .find(|e| e["id"] == FEAT_ID)
        .expect("saved");
    assert_eq!(feat["type"], "Feat");
    assert_eq!(feat["supports"], json!(["General Feat", "Feat"]));
    assert_eq!(
        feat["setters"],
        json!([{"name": "category", "value": "General"}])
    );
    assert_eq!(feat["requirements"], "[character:4]&&[str:13]");
    assert_eq!(feat["prerequisite"], "Level 4+, Strength 13+");
    assert_eq!(feat["rules"][0]["kind"], "ability_increase");

    // a 2024 fighter's level-4 feat choice offers it, but only to a character that meets the prerequisite
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let strong = fighter_4(e, [15, 14, 13, 12, 10, 8]);
    let (key, offered) = feat_choice(e, &strong);
    assert!(offered, "the level-4 feat choice offers the new feat");
    let weak = fighter_4(e, [8, 15, 14, 13, 12, 10]);
    assert!(!feat_choice(e, &weak).1, "Strength 8 doesn't qualify");

    // taken there, it is on the character and brings its increase choice
    let taken = commands::select(e, &strong, &key, Choice::Element(FEAT_ID.into()))
        .unwrap()
        .state;
    let snap = e.derive(&taken);
    assert!(
        snap.slots
            .iter()
            .any(|x| x.owner.id == FEAT_ID && !x.list_items.is_empty()),
        "the feat's ability score increase is a choice on the character"
    );

    // it opens again in the feat form; an unchanged save writes the same element
    let again = app
        .state
        .homebrew_open_draft(&app.pack, Some(FEAT_ID), None)
        .unwrap();
    assert_eq!(again.form.kind, "feat");
    assert_eq!(again.form.fields["category"], json!("General"));
    assert_eq!(
        again.form.fields["prerequisiteText"],
        json!("Level 4+, Strength 13+")
    );
    let touched = app.update(&again, &again.form);
    app.state
        .homebrew_save_draft(&touched.draft_id, &Quiet)
        .unwrap();
    let after = app
        .saved()
        .into_iter()
        .find(|e| e["id"] == FEAT_ID)
        .unwrap();
    assert_eq!(after, feat);
}
