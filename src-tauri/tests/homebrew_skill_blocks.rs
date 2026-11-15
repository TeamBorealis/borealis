//! HC16 (2), item IpY1E: the creator's "Bonus to skills" and "Advantage on a roll" blocks offer the installed homebrew
//! skills next to the eighteen, through the real `AppState` on a temporary data folder with the bundled content. A
//! homebrew skill picked in either block is saved in the pack by its name, reopens in the block's dialog (not kept as
//! is), also after the app is started again, is applied by the engine on a character (the bonus on the skill row, the
//! advantage noted) and shows in Try it. A name no loaded skill has is refused at its field.

use borealis_authoring::blocks::SKILL_NAME_PICKER;
use borealis_authoring::form::{Block, BlockCodec, CatalogBlockCodec, FormContext, FormState};
use borealis_content::{Rule, Ruleset as ContentRuleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_creator::block_catalog;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerTarget, HomebrewTrialSetup,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const ALCHEMY: &str = "ID_OW_SKILL_ALCHEMY";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn state(data: &Path) -> AppState {
    AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    })
    .unwrap()
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
    pack: String,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = state(data.path());
        let pack = state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Old World".into(),
                    code: "OW".into(),
                    edition: Ruleset::Dnd2014,
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

    /// A new draft of `kind` with `fields` and `blocks` filled in (not saved).
    fn draft(&self, kind: &str, fields: &[(&str, Value)], blocks: Vec<Block>) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(&self.pack, kind, None, None, None)
            .unwrap();
        let mut form: FormState = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).into(), value.clone());
        }
        form.blocks = blocks;
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    fn save(&self, v: &HomebrewDraftView) -> String {
        assert_eq!(errors(v), Vec::<(String, String)>::new());
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone()
    }

    fn saved_rules(&self, id: &str) -> Vec<Value> {
        let path = self
            .data
            .path()
            .join("library")
            .join(&self.pack)
            .join("content.json");
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))["rules"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// The block a stored rule reads as, with `field` set to `value` as the block's dialog sets it.
    fn block(&self, rule: Value, field: &str, value: Value) -> Block {
        let hub = self.state.hub().unwrap();
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let rule: Rule = serde_json::from_value(rule).unwrap();
        let mut b = codec.read(&rule, format!("b-{field}"), &cx);
        b.fields.insert(field.into(), value);
        b
    }
}

/// The names the skill blocks' word picker offers.
fn skill_words(state: &AppState) -> Vec<String> {
    state
        .homebrew_reference_options(
            &HomebrewPickerTarget {
                types: vec![SKILL_NAME_PICKER.to_string()],
                tags: None,
                multiple: None,
            },
            "",
            &HomebrewPickerFilters::default(),
            HomebrewPageRequest {
                offset: 0,
                limit: 500,
            },
            None,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|r| r.name)
        .collect()
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn text(t: &str) -> Value {
    json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": t}]}]})
}

fn alchemy(app: &App) {
    let v = app.draft(
        "skill",
        &[
            ("name", json!("Alchemy")),
            ("ability", json!("Intelligence")),
            ("description", text("Brewing and reagents.")),
        ],
        Vec::new(),
    );
    assert_eq!(app.save(&v), ALCHEMY);
}

/// "Bonus to skills: +2 to Arcana and Alchemy" and "Advantage on Alchemy checks", as the dialogs write them.
fn skill_blocks(app: &App) -> Vec<Block> {
    vec![
        app.block(
            json!({"kind": "skill_bonus", "skills": ["Arcana"], "value": "2"}),
            "skills",
            json!(["Arcana", "Alchemy"]),
        ),
        app.block(
            json!({"kind": "roll_advantage", "roll": "ability_check", "skill": "arcana"}),
            "skill",
            json!("Alchemy"),
        ),
    ]
}

const BONUS_RULE: &str = r#"{"kind":"skill_bonus","skills":["Arcana","Alchemy"],"value":"2"}"#;
const ADVANTAGE_RULE: &str =
    r#"{"kind":"roll_advantage","roll":"ability_check","skill":"Alchemy"}"#;

/// The open draft of `id`: its two skill blocks are edited by their dialogs, nothing of them kept as is.
fn assert_reopens(state: &AppState, pack: &str, id: &str) {
    let v = state.homebrew_open_draft(pack, Some(id), None).unwrap();
    let by_kind = |kind: &str| {
        v.form
            .blocks
            .iter()
            .find(|b| b.kind == kind)
            .unwrap_or_else(|| panic!("{kind} block"))
            .clone()
    };
    let bonus = by_kind("skill_bonus");
    assert_eq!(bonus.fields["skills"], json!(["Arcana", "Alchemy"]));
    assert!(bonus.preserved.is_empty(), "{:?}", bonus.preserved);
    assert_eq!(bonus.sentence, "Bonus to Arcana and Alchemy: 2");
    let adv = by_kind("roll_advantage");
    assert_eq!(adv.fields["skill"], json!("Alchemy"));
    assert!(adv.preserved.is_empty(), "{:?}", adv.preserved);
    assert_eq!(adv.sentence, "Advantage on Alchemy checks");
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    state.homebrew_discard_draft(&v.draft_id).unwrap();
}

fn alchemy_row(snap: &CharacterSnapshot) -> (i32, i32) {
    let a = snap.skill("alchemy").expect("Alchemy is a skill");
    (a.misc, a.total)
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("no slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{name} {id}: {err}"))
        .state
}

#[test]
fn the_skill_blocks_offer_the_installed_homebrew_skills_after_the_eighteen() {
    let catalog = block_catalog();
    for (kind, key, control) in [
        ("skill_bonus", "skills", "chips"),
        ("roll_advantage", "skill", "combo"),
    ] {
        let f = catalog
            .iter()
            .find(|b| b.kind == kind)
            .and_then(|b| b.fields.iter().find(|f| f.base.key == key))
            .unwrap_or_else(|| panic!("{kind}.{key}"));
        assert_eq!(f.base.control, control, "{kind}.{key}");
        assert_eq!(
            f.base.choices.as_ref().map(Vec::len),
            Some(18),
            "{kind}.{key}"
        );
        assert_eq!(f.base.allow_custom, None, "{kind}.{key}: a closed set");
        assert_eq!(
            f.base.picker.as_ref().map(|p| p.types.clone()),
            Some(vec![SKILL_NAME_PICKER.to_string()]),
            "{kind}.{key}: the installed skills' names"
        );
    }
    let app = App::new();
    assert_eq!(
        skill_words(&app.state),
        Vec::<String>::new(),
        "none installed"
    );
    alchemy(&app);
    // a Skill named like one of the eighteen is that skill: offered once, as one of them
    let v = app.draft(
        "skill",
        &[
            ("name", json!("Arcana")),
            ("ability", json!("Intelligence")),
            ("description", text("Lore.")),
        ],
        Vec::new(),
    );
    app.save(&v);
    assert_eq!(skill_words(&app.state), ["Alchemy"]);
}

#[test]
fn a_homebrew_skill_in_both_blocks_is_saved_reopens_and_shows_in_try_it() {
    let app = App::new();
    alchemy(&app);
    let feat = app.draft(
        "feat",
        &[
            ("name", json!("Brewer's Knack")),
            ("description", text("A knack for brewing.")),
        ],
        skill_blocks(&app),
    );
    assert_eq!(errors(&feat), Vec::<(String, String)>::new());
    // Try it on the draft: the trial character has the bonus on its Alchemy row and the advantage noted
    let trial = app
        .state
        .homebrew_trial_start(&feat.draft_id, &HomebrewTrialSetup::default())
        .unwrap();
    let int = trial.snapshot.abilities.modifier.int;
    let row = trial
        .snapshot
        .stats
        .skills
        .iter()
        .find(|s| s.key == "alchemy")
        .expect("the Alchemy row");
    assert_eq!(row.name, "Alchemy");
    assert_eq!(row.bonus, int + 2, "Intelligence and the feat's +2");
    let advantages = serde_json::to_value(&trial.snapshot.play.advantages).unwrap();
    assert!(
        advantages
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["text"] == "Advantage on Alchemy checks" && a["source"] == "Brewer's Knack"),
        "{advantages}"
    );
    app.state.homebrew_trial_end(&trial.trial_id).unwrap();
    // saved in the pack by name
    let id = app.save(&feat);
    let rules: Vec<String> = app.saved_rules(&id).iter().map(Value::to_string).collect();
    assert_eq!(rules, [BONUS_RULE, ADVANTAGE_RULE]);
    assert_reopens(&app.state, &app.pack, &id);
    // and after the app starts again on the same data folder
    let again = state(app.data.path());
    assert_reopens(&again, &app.pack, &id);
}

#[test]
fn a_character_with_the_blocks_gets_the_bonus_and_the_advantage() {
    let app = App::new();
    alchemy(&app);
    let bg = app.draft(
        "background",
        &[
            ("name", json!("Apothecary")),
            (
                "skills",
                json!([{"id": ALCHEMY, "name": "", "type": "Skill"}]),
            ),
            ("description", text("Herbs and tinctures.")),
        ],
        skill_blocks(&app),
    );
    let bg = app.save(&bg);
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2014);
    s.abilities.base[3] = 16;
    let before = e.derive(&s);
    assert_eq!(
        alchemy_row(&before),
        (0, 3),
        "Intelligence 16, nothing else"
    );
    let s = pick(e, &s, "ID_LEVEL_1", "Background", &bg);
    let snap = e.derive(&s);
    assert_eq!(
        alchemy_row(&snap),
        (2, 3 + 2 + 2),
        "Intelligence, proficiency and the +2"
    );
    assert_eq!(snap.skill("arcana").unwrap().misc, 2);
    assert!(
        snap.advantages
            .iter()
            .any(|a| a.text == "Advantage on Alchemy checks" && a.source == "Apothecary"),
        "{:?}",
        snap.advantages
    );
}

#[test]
fn a_skill_no_loaded_content_adds_is_refused_at_its_field() {
    let app = App::new();
    let v = app.draft(
        "feat",
        &[
            ("name", json!("Brewer's Knack")),
            ("description", text("A knack for brewing.")),
        ],
        skill_blocks(&app),
    );
    assert_eq!(
        errors(&v),
        [
            (
                "blocks.0.fields.skills".to_string(),
                "Bonus to skills: pick skills from the list.".to_string()
            ),
            (
                "blocks.1.fields.skill".to_string(),
                "Advantage on a roll: pick skill from the list.".to_string()
            ),
        ]
    );
}
