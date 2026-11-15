//! HC1 (WB-19 Open, item HtCY8): a Skill made in the creator's Skill form is a character skill, through the real
//! `AppState` on a temporary data folder with the bundled content. The skill pickers (a background's skill
//! proficiencies, a class's skill choice) offer it with the SRD skills; a background that grants it gives a level-1
//! character proficiency in it and reopens with it among its skills; a class's skill choice offers it; the
//! character's skill list has it with its ability and name.

use borealis_authoring::form::{descriptors, FormState};
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerTarget,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const ALCHEMY: &str = "ID_OW_SKILL_ALCHEMY";
const MEDICINE: &str = "ID_PROFICIENCY_SKILL_MEDICINE";
const ATHLETICS: &str = "ID_PROFICIENCY_SKILL_ATHLETICS";

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

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    /// Fill `fields` into a new draft of `kind`, save it and return its id.
    fn make(&self, kind: &str, fields: &[(&str, Value)]) -> String {
        let v = self
            .state
            .homebrew_new_draft(&self.pack, kind, None, None, None)
            .unwrap();
        let mut form = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).into(), value.clone());
        }
        let v = self.update(&v, &form);
        assert_eq!(errors(&v), Vec::<(String, String)>::new(), "{kind}");
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone()
    }

    fn saved(&self, id: &str) -> Value {
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
            .cloned()
            .unwrap_or_else(|| panic!("{id} saved"))
    }

    /// The ids a picker of `types` and `tags` offers.
    fn offered(&self, types: &[String], tags: Option<Vec<String>>) -> Vec<String> {
        self.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: types.to_vec(),
                    tags,
                    multiple: Some(true),
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
            .map(|r| r.id)
            .collect()
    }
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

fn reference(id: &str, ty: &str) -> Value {
    json!({"id": id, "name": "", "type": ty})
}

fn alchemy(app: &App) -> String {
    app.make(
        "skill",
        &[
            ("name", json!("Alchemy")),
            ("ability", json!("Intelligence")),
            ("description", text("Brewing and reagents.")),
        ],
    )
}

/// The open slot of `owner` named `name` and what it offers.
fn offered_by(snap: &CharacterSnapshot, name: &str) -> Vec<String> {
    snap.slots
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("no slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{name} {id}: {err}"))
        .state
}

#[test]
fn the_skill_pickers_offer_a_homebrew_skill() {
    let app = App::new();
    let skill = alchemy(&app);
    assert_eq!(skill, ALCHEMY);
    let kinds = descriptors();
    for (kind, field) in [("background", "skills"), ("class", "skills")] {
        let picker = kinds
            .iter()
            .find(|k| k.id == kind)
            .and_then(|k| k.fields.iter().find(|f| f.key == field))
            .and_then(|f| f.picker.clone())
            .unwrap_or_else(|| panic!("{kind}.{field} has a picker"));
        let offered = app.offered(&picker.types, picker.tags.clone());
        for id in [ALCHEMY, MEDICINE] {
            assert!(offered.iter().any(|o| o == id), "{kind}.{field}: {id}");
        }
    }
}

#[test]
fn a_background_that_grants_a_homebrew_skill_makes_a_character_proficient() {
    let app = App::new();
    alchemy(&app);
    let bg = app.make(
        "background",
        &[
            ("name", json!("Apothecary")),
            (
                "skills",
                json!([
                    reference(ALCHEMY, "Skill"),
                    reference(MEDICINE, "Proficiency")
                ]),
            ),
            ("description", text("Herbs and tinctures.")),
        ],
    );
    let saved = app.saved(&bg);
    let grants: Vec<&Value> = saved["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "grant")
        .collect();
    assert_eq!(
        grants,
        [
            &json!({"kind": "grant", "id": ALCHEMY, "type": "Skill"}),
            &json!({"kind": "grant", "id": MEDICINE, "type": "Proficiency"})
        ]
    );
    // it reopens with both among its skills, none among its tools
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(&bg), None)
        .unwrap();
    let skills: Vec<&str> = v.form.fields["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(skills, [ALCHEMY, MEDICINE]);
    assert_eq!(v.form.fields["tools"], json!([]));
    app.state.homebrew_discard_draft(&v.draft_id).unwrap();

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = commands::new_character(e, ContentRuleset::Dnd2014);
    s.abilities.base[3] = 16;
    let s = pick(e, &s, "ID_LEVEL_1", "Background", &bg);
    let snap = e.derive(&s);
    let a = snap.skill("alchemy").expect("Alchemy is a skill");
    assert_eq!(a.label.as_deref(), Some("Alchemy"));
    assert_eq!(a.ability, "intelligence");
    assert!(a.proficient && snap.skill("medicine").unwrap().proficient);
    assert_eq!(a.total, 3 + 2, "Intelligence 16 and the proficiency bonus");
}

#[test]
fn a_class_skill_choice_offers_a_homebrew_skill() {
    let app = App::new();
    alchemy(&app);
    let class = app.make(
        "class",
        &[
            ("name", json!("Brewer")),
            ("hitDie", json!("d8")),
            (
                "skills",
                json!({"count": 1, "skills": [reference(ATHLETICS, "Proficiency"), reference(ALCHEMY, "Skill")]}),
            ),
            ("description", text("Potion makers.")),
        ],
    );
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = commands::new_character(e, ContentRuleset::Dnd2014);
    let s = pick(e, &s, "ID_LEVEL_1", "Class", &class);
    let snap = e.derive(&s);
    let name = "Skill Proficiency (Brewer)";
    assert_eq!(offered_by(&snap, name), [ALCHEMY, ATHLETICS], "by name");
    let s = pick(e, &s, &class, name, ALCHEMY);
    assert!(e.derive(&s).skill("alchemy").unwrap().proficient);
}
