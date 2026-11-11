//! WB-18 (homebrew creator subclass and class option forms, spec 4.7, 4.8) through the real `AppState` on a
//! temporary data folder with the bundled content:
//!
//! * a new subclass of the SRD 2024 Fighter, made in the form: picking the class gives it the class's subclass tag,
//!   the level it is chosen at and the subclass feature levels; two features made inside it (`homebrew_new_draft
//!   {parentId}`, one a copy of an SRD subclass feature) on levels 3 and 7; no live or deep problems; saved in the
//!   SRD's shape; a new Fighter is offered it at its level-3 subclass choice and gets each feature on its level;
//! * a new class option in the Eldritch Invocation group with a Warlock level 2 requirement: saved in the SRD's shape
//!   and offered by the 2024 Warlock's invocation choice of level 2, not by the level-1 one.

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

const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";
const WARLOCK: &str = "ID_2024_CLASS_WARLOCK";
const SUBCLASS_ID: &str = "ID_EI_SUBCLASS_EMBER_GUARD";
const OPTION_ID: &str = "ID_EI_OPTION_EMBER_SIGHT";

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

    fn saved_one(&self, id: &str) -> Value {
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

    /// The engine checks and the level sweep, warnings and errors only.
    fn deep(&self, v: &HomebrewDraftView) -> Vec<(String, String)> {
        self.state
            .homebrew_check_draft(&v.draft_id)
            .unwrap()
            .into_iter()
            .filter(|p| p.severity != "info")
            .map(|p| (p.code, p.message))
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

/// `form` with the levelled entry of child `key` at `level`.
fn at_level(form: &mut FormState, key: &str, level: u32) {
    let levels = form
        .fields
        .get_mut("levels")
        .unwrap()
        .as_array_mut()
        .unwrap();
    for e in levels.iter_mut() {
        if e["child"] == json!(key) {
            e["level"] = json!(level);
        }
    }
}

/// A 2024 character of `class` at level 1.
fn first_level(e: &Engine, class: &str) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, [15, 14, 13, 12, 10, 16])
        .unwrap()
        .state
}

fn level_to(e: &Engine, mut s: CharacterState, level: u32) -> CharacterState {
    while s.level() < level {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

fn element_names(e: &Engine, s: &CharacterState) -> Vec<String> {
    e.derive(s)
        .elements
        .into_iter()
        .map(|x| x.element.name)
        .collect()
}

/// The open slot called `name`: its key and option ids.
fn slot(
    e: &Engine,
    s: &CharacterState,
    name: &str,
) -> Option<(borealis_engine::SlotKey, Vec<String>)> {
    e.derive(s)
        .slots
        .into_iter()
        .find(|x| x.name == name)
        .map(|x| (x.key, x.options.into_iter().map(|o| o.id).collect()))
}

#[test]
fn a_new_subclass_of_the_srd_fighter_is_offered_and_gives_its_features_on_their_levels() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "subclass", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "subclass");
    let paths: Vec<String> = errors(&v).into_iter().map(|(p, _)| p).collect();
    assert!(
        paths.contains(&"fields.parentClass".to_string()),
        "{paths:?}"
    );

    // the class: its choice's tag, its level and the subclass feature levels of its table come with it
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Ember Guard"));
    form.fields.insert(
        "parentClass".into(),
        json!({"id": FIGHTER, "name": "Fighter", "type": "Class"}),
    );
    let v = app.update(&v, &form);
    assert_eq!(v.form.fields["subclassTag"], "Fighter Subclass");
    assert_eq!(v.form.fields["levelGained"], 3);
    assert_eq!(v.form.fields["subclassLevels"], json!([3, 7, 10, 15, 18]));
    assert_eq!(errors(&v), Vec::<(String, String)>::new());

    // two features made inside it: a copy of the SRD Champion's Improved Critical, and a new one
    let v = app
        .state
        .homebrew_new_draft(
            &app.pack,
            "subclassFeature",
            Some("ID_2024_SUBCLASS_FEATURE_FIGHTER_CHAMPION_IMPROVED_CRITICAL"),
            Some(&v.draft_id),
            None,
        )
        .unwrap();
    let ward = v.form.children.last().expect("the copy").clone();
    assert_eq!(ward.kind, "subclassFeature");
    assert_eq!(ward.blocks.len(), 1, "its critical range rule is a block");
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "subclassFeature", None, Some(&v.draft_id), None)
        .unwrap();
    let surge = v.form.children.last().expect("the new feature").key.clone();
    let mut form = v.form.clone();
    at_level(&mut form, &ward.key, 3);
    at_level(&mut form, &surge, 7);
    for c in &mut form.children {
        let name = if c.key == ward.key {
            "Ember Ward"
        } else {
            "Ember Surge"
        };
        c.fields.insert("name".into(), json!(name));
        c.fields.insert(
            "description".into(),
            json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Embers rise."}]}]}),
        );
    }
    form.fields.insert(
        "description".into(),
        json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Guards of the ember."}]}]}),
    );
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    assert_eq!(app.deep(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids[0], SUBCLASS_ID);

    // the SRD's shape
    let sub = app.saved_one(SUBCLASS_ID);
    assert_eq!(sub["type"], "Archetype");
    assert_eq!(sub["ruleset"], "dnd2024");
    assert_eq!(sub["supports"], json!(["Fighter Subclass"]));
    assert_eq!(
        sub["data"],
        json!({"class": FIGHTER, "class_name": "Fighter", "level_gained": 3})
    );
    let ward_id = "ID_EI_SUBCLASS_FEATURE_EMBER_GUARD_EMBER_WARD";
    let surge_id = "ID_EI_SUBCLASS_FEATURE_EMBER_GUARD_EMBER_SURGE";
    assert_eq!(
        sub["rules"],
        json!([{"kind": "grant", "id": ward_id, "type": "Archetype Feature", "level": 3},
            {"kind": "grant", "id": surge_id, "type": "Archetype Feature", "level": 7}])
    );
    let ward = app.saved_one(ward_id);
    assert_eq!(
        ward["data"],
        json!({"class": "Fighter", "subclass": "Ember Guard", "level": 3})
    );
    assert_eq!(ward["rules"][0]["kind"], "critical_range");
    assert_eq!(app.saved_one(surge_id)["data"]["level"], 7);

    // a new Fighter is offered it at level 3 and gets its features on their levels
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = level_to(e, first_level(e, FIGHTER), 3);
    let (key, options) = slot(e, &s, "Martial Archetype").expect("the subclass choice at 3");
    assert!(options.contains(&SUBCLASS_ID.to_string()), "{options:?}");
    let s = commands::select(e, &s, &key, Choice::Element(SUBCLASS_ID.into()))
        .unwrap()
        .state;
    let names = element_names(e, &s);
    assert!(names.contains(&"Ember Ward".to_string()), "{names:?}");
    assert!(!names.contains(&"Ember Surge".to_string()), "not before 7");
    let s = level_to(e, s, 7);
    let names = element_names(e, &s);
    assert!(names.contains(&"Ember Surge".to_string()), "{names:?}");
    // reopened, it reads its class from the store
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(SUBCLASS_ID), None)
        .unwrap();
    assert_eq!(v.form.fields["parentClass"]["id"], FIGHTER);
    assert_eq!(v.form.fields["subclassLevels"], json!([3, 7, 10, 15, 18]));
    assert_eq!(v.form.children.len(), 2);
}

#[test]
fn a_new_eldritch_invocation_is_offered_by_the_warlock_s_level_2_choice() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "classOption", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "classOption");
    let paths: Vec<String> = errors(&v).into_iter().map(|(p, _)| p).collect();
    assert!(paths.contains(&"fields.group".to_string()), "{paths:?}");
    // the level requirement from a copy of an SRD invocation's condition (Warlock level 2)
    let model = app
        .state
        .homebrew_new_draft(
            &app.pack,
            "classOption",
            Some("ID_2024_OPTION_ELDRITCH_INVOCATION_AGONIZING_BLAST"),
            None,
            None,
        )
        .unwrap();
    let mut form = v.form.clone();
    let set = |f: &mut FormState, k: &str, v: Value| {
        f.fields.insert(k.into(), v);
    };
    set(&mut form, "name", json!("Ember Sight"));
    set(&mut form, "group", json!("Eldritch Invocation"));
    set(&mut form, "className", json!("Warlock"));
    set(
        &mut form,
        "requirements",
        model.form.fields["requirements"].clone(),
    );
    set(
        &mut form,
        "description",
        json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "You see embers."}]}]}),
    );
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let option = app.saved_one(OPTION_ID);
    assert_eq!(option["type"], "Option");
    assert_eq!(option["supports"], json!(["Eldritch Invocation"]));
    assert_eq!(option["requirements"], "[warlock:2]");
    assert_eq!(option["data"], json!({"class_name": "Warlock"}));

    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = first_level(e, WARLOCK);
    let (_, at_1) = slot(e, &s, "Eldritch Invocation (Warlock 1)").expect("the level-1 choice");
    assert!(!at_1.contains(&OPTION_ID.to_string()), "needs Warlock 2");
    let s = level_to(e, s, 2);
    let (_, at_2) = slot(e, &s, "Eldritch Invocation (Warlock 2)").expect("the level-2 choice");
    assert!(at_2.contains(&OPTION_ID.to_string()), "{at_2:?}");
}

#[test]
fn a_subclass_of_a_class_of_the_pack_not_saved_yet_takes_its_choice() {
    use borealis_lib::homebrew_dto::{
        HomebrewPageRequest, HomebrewPickerFilters, HomebrewPickerTarget,
    };
    let app = App::new();
    // a new class: its generated subclass choice is at level 3
    let class = app
        .state
        .homebrew_new_draft(&app.pack, "class", None, None, None)
        .unwrap();
    let mut form = class.form.clone();
    form.fields.insert("name".into(), json!("Gloom Knight"));
    form.fields.insert("hitDie".into(), json!("d10"));
    form.fields.insert(
        "skills".into(),
        json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_ATHLETICS", "name": "", "type": "Proficiency"}]}),
    );
    let class = app.update(&class, &form);
    assert_eq!(errors(&class), Vec::<(String, String)>::new());
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "subclass", None, None, None)
        .unwrap();
    // the class picker offers the unsaved class
    let rows = app
        .state
        .homebrew_reference_options(
            &HomebrewPickerTarget {
                types: vec!["Class".into()],
                tags: None,
                multiple: None,
            },
            "gloom",
            &HomebrewPickerFilters::default(),
            HomebrewPageRequest {
                offset: 0,
                limit: 10,
            },
            Some(&v.draft_id),
        )
        .unwrap();
    let row = rows.items.first().expect("the unsaved class is offered");
    assert!(row.draft);
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Order of Dusk"));
    form.fields.insert(
        "parentClass".into(),
        json!({"id": row.id, "name": row.name, "type": "Class"}),
    );
    let v = app.update(&v, &form);
    assert_eq!(v.form.fields["subclassTag"], "Gloom Knight Subclass");
    assert_eq!(v.form.fields["levelGained"], 3);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
}
