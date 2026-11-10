//! WB-16 (homebrew creator class form, spec 4.5) through the real `AppState` on a temporary data folder with the
//! bundled content: a new 2024 class with a d10, two saving throws, a choice of 2 skills, a level-1 feature with a
//! rule block (a child made with `homebrew_new_draft {parentId}`), the generated Ability Score Improvement (4, 8, 12,
//! 16), subclass choice (3) and epic boon (19) saves in the SRD's shape; a new character takes it and levels 1 to 20
//! with the right features and choices; its multiclass prerequisite is enforced; and a copy of an SRD class saves with
//! its own multiclass id (no collision with the original's).

use borealis_authoring::expr_build::condition::ConditionRow;
use borealis_authoring::expr_build::{Ability, ExprValue, GroupMode, Node};
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

const CLASS_ID: &str = "ID_EI_CLASS_GLOOM_KNIGHT";
const MC_ID: &str = "ID_EI_MULTICLASS_GLOOM_KNIGHT";
const FIGHTER: &str = "ID_2024_CLASS_FIGHTER";

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

    fn saved_one(&self, id: &str) -> Value {
        self.saved()
            .into_iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} saved"))
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn reference(id: &str) -> Value {
    json!({"id": id, "name": "", "type": "Proficiency"})
}

fn strength_13() -> Value {
    let model = Node::group(
        GroupMode::All,
        vec![Node::row(ConditionRow::Ability {
            ability: Ability::Strength,
            min: 13,
        })],
    );
    serde_json::to_value(ExprValue::new(model)).unwrap()
}

fn child<'f>(form: &'f FormState, name: &str) -> &'f FormState {
    form.children
        .iter()
        .find(|c| c.fields.get("name").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("child {name}"))
}

/// A 2024 character of `class` at level 1 with these scores.
fn first_level(e: &Engine, class: &str, scores: [i32; 6]) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, scores).unwrap().state
}

fn slot_names(e: &Engine, s: &CharacterState) -> Vec<String> {
    e.derive(s).slots.into_iter().map(|x| x.name).collect()
}

fn element_names(e: &Engine, s: &CharacterState) -> Vec<String> {
    e.derive(s)
        .elements
        .into_iter()
        .map(|x| x.element.name)
        .collect()
}

#[test]
fn a_new_2024_class_saves_levels_to_20_and_enforces_its_multiclass_prerequisite() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "class", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "class");
    assert_eq!(v.form.fields["edition"], json!("dnd2024"));
    // the generated features: Ability Score Improvement at 4, the subclass choice at 3, the epic boon at 19
    let names: Vec<&str> = v
        .form
        .children
        .iter()
        .filter_map(|c| c.fields.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(
        names,
        ["Ability Score Improvement", "Subclass", "Epic Boon"]
    );
    assert_eq!(
        child(&v.form, "Ability Score Improvement").fields["asiLevels"],
        json!([4, 8, 12, 16])
    );

    // a level-1 feature with a rule block: a child made inside the class (a copy of the SRD Extra Attack's rule)
    let v = app
        .state
        .homebrew_new_draft(
            &app.pack,
            "classFeature",
            Some("ID_2024_CLASS_FEATURE_FIGHTER_EXTRA_ATTACK"),
            Some(&v.draft_id),
            None,
        )
        .unwrap();
    let feature = v.form.children.last().expect("the new child").clone();
    assert_eq!(feature.blocks.len(), 1, "the copied stat rule is a block");
    assert!(
        v.form.fields["levels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["child"] == json!(feature.key) && e["level"] == json!(1)),
        "the new feature is on the LEVELS grid at level 1: {}",
        v.form.fields["levels"]
    );

    // the starting equipment: a package made inside the class (its EQUIPMENT tab)
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "equipmentPackage", None, Some(&v.draft_id), None)
        .unwrap();
    let package = v.form.children.last().expect("the package").key.clone();
    assert_eq!(v.form.fields["equipment"], json!([{"child": package}]));

    let mut form = v.form.clone();
    for c in &mut form.children {
        if c.key == package {
            c.fields
                .insert("name".into(), json!("Starting Equipment (Gloom Knight)"));
            c.fields.insert(
                "options".into(),
                json!([{"label": "A", "items": [], "coins": {"cp": 0, "sp": 0, "ep": 0, "gp": 50, "pp": 0}}]),
            );
        }
    }
    let set = |f: &mut FormState, k: &str, v: Value| {
        f.fields.insert(k.into(), v);
    };
    set(&mut form, "name", json!("Gloom Knight"));
    set(&mut form, "hitDie", json!("d10"));
    set(
        &mut form,
        "savingThrows",
        json!(["Strength", "Constitution"]),
    );
    set(
        &mut form,
        "skills",
        json!({"count": 2, "skills": [
            reference("ID_PROFICIENCY_SKILL_ATHLETICS"),
            reference("ID_PROFICIENCY_SKILL_PERCEPTION"),
            reference("ID_PROFICIENCY_SKILL_STEALTH")]}),
    );
    set(
        &mut form,
        "armor",
        json!([reference("ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR")]),
    );
    set(
        &mut form,
        "weapons",
        json!([reference(
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS"
        )]),
    );
    set(&mut form, "mcRequirements", strength_13());
    set(
        &mut form,
        "mcProficiencies",
        json!([reference("ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR")]),
    );
    for c in &mut form.children {
        match c.fields.get("name").and_then(Value::as_str) {
            Some("Subclass") => {
                c.fields.insert("name".into(), json!("Gloom Knight Order"));
                c.fields
                    .insert("subclassLabel".into(), json!("Knightly Order"));
            }
            Some("Extra Attack (copy)") => {
                c.fields.insert("name".into(), json!("Gloom Strike"));
            }
            _ => {}
        }
    }
    let v = app.update(&v, &form);
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    // the engine checks and the 1-20 level sweep (spec 8.1 step 6) find nothing wrong
    let deep: Vec<(String, String)> = app
        .state
        .homebrew_check_draft(&v.draft_id)
        .unwrap()
        .into_iter()
        .filter(|p| p.severity != "info")
        .map(|p| (p.code, p.message))
        .collect();
    assert_eq!(deep, Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids[0], CLASS_ID);
    assert_eq!(saved.element_ids.len(), 6, "{:?}", saved.element_ids);

    // the SRD's shape
    let class = app.saved_one(CLASS_ID);
    assert_eq!(class["ruleset"], "dnd2024");
    assert_eq!(class["setters"], json!([{"name": "hd", "value": "d10"}]));
    let fco = format!("!{MC_ID}");
    let rules = class["rules"].as_array().unwrap();
    let grants: Vec<&str> = rules
        .iter()
        .filter(|r| r["kind"] == "grant" && r["type"] == "Proficiency")
        .map(|r| {
            assert_eq!(r["requirements"], json!(fco), "first class only: {r}");
            r["id"].as_str().unwrap()
        })
        .collect();
    assert_eq!(
        grants,
        [
            "ID_PROFICIENCY_SAVINGTHROW_STRENGTH",
            "ID_PROFICIENCY_SAVINGTHROW_CONSTITUTION",
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS",
            "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR",
        ]
    );
    let skills = rules.iter().find(|r| r["kind"] == "select").unwrap();
    assert_eq!(
        skills,
        &json!({"kind": "select", "type": "Proficiency", "name": "Skill Proficiency (Gloom Knight)", "number": 2,
            "requirements": fco, "supports": "ID_PROFICIENCY_SKILL_ATHLETICS|ID_PROFICIENCY_SKILL_PERCEPTION|ID_PROFICIENCY_SKILL_STEALTH"})
    );
    let features: Vec<(String, u64)> = rules
        .iter()
        .filter(|r| r["type"] == "Class Feature")
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_string(),
                r["level"].as_u64().unwrap_or(1),
            )
        })
        .collect();
    assert_eq!(
        features,
        [
            (
                "ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_GLOOM_KNIGHT_ORDER".into(),
                3
            ),
            (
                "ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_ABILITY_SCORE_IMPROVEMENT".into(),
                4
            ),
            ("ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_EPIC_BOON".into(), 19),
            ("ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_GLOOM_STRIKE".into(), 1),
        ]
    );
    assert_eq!(
        class["multiclass"],
        json!({"id": MC_ID, "prerequisite": "Strength 13", "requirements": "[str:13]",
            "setters": [{"name": "multiclass proficiencies", "value": "Light Armor"}],
            "rules": [{"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS", "type": "Grants"},
                {"kind": "grant", "id": "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR", "type": "Proficiency"}]})
    );
    let package_id = "ID_EI_EQUIPMENT_PACKAGE_GLOOM_KNIGHT_STARTING_EQUIPMENT_GLOOM_KNIGHT";
    assert_eq!(
        rules.iter().find(|r| r["kind"] == "equipment_package"),
        Some(
            &json!({"kind": "equipment_package", "requirements": fco, "ref": package_id,
            "name": "Starting Equipment (Gloom Knight)"})
        )
    );
    let pack = app.saved_one(package_id);
    assert_eq!(
        pack["data"]["owner"], CLASS_ID,
        "the package belongs to the class"
    );
    assert_eq!(pack["data"]["options"][0]["coins"], json!({"gp": 50}));
    let asi = app.saved_one("ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_ABILITY_SCORE_IMPROVEMENT");
    assert_eq!(asi["data"], json!({"class": "Gloom Knight", "level": 4}));
    assert_eq!(
        asi["rules"][3],
        json!({"kind": "select_feat", "level": 16, "name": "Ability Score Improvement (Gloom Knight 16)",
            "category": "General", "default": "ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT"})
    );
    let sub = app.saved_one("ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_GLOOM_KNIGHT_ORDER");
    assert_eq!(
        sub["rules"],
        json!([{"kind": "select", "type": "Archetype", "name": "Knightly Order", "supports": "Gloom Knight Subclass"}])
    );
    let strike = app.saved_one("ID_EI_CLASS_FEATURE_GLOOM_KNIGHT_GLOOM_STRIKE");
    assert_eq!(
        strike["data"]["class"], "Gloom Knight",
        "the copied feature follows its class"
    );
    assert_eq!(strike["data"]["level"], 1);

    // a new character takes it and levels 1-20 with the right features and choices
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let mut s = first_level(e, CLASS_ID, [15, 14, 13, 12, 10, 8]);
    let snap = e.derive(&s);
    assert_eq!(snap.classes[0].hit_die, Some(10));
    assert!(element_names(e, &s).contains(&"Gloom Strike".to_string()));
    assert!(slot_names(e, &s).contains(&"Skill Proficiency (Gloom Knight)".to_string()));
    let mut seen = Vec::new();
    while s.level() < 20 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
        let names = slot_names(e, &s);
        for want in [
            "Knightly Order",
            "Ability Score Improvement (Gloom Knight 4)",
            "Ability Score Improvement (Gloom Knight 16)",
            "Epic Boon (Gloom Knight)",
        ] {
            if names.iter().any(|n| n == want) && !seen.iter().any(|(w, _)| *w == want) {
                seen.push((want, s.level()));
            }
        }
    }
    assert_eq!(
        seen,
        [
            ("Knightly Order", 3),
            ("Ability Score Improvement (Gloom Knight 4)", 4),
            ("Ability Score Improvement (Gloom Knight 16)", 16),
            ("Epic Boon (Gloom Knight)", 19),
        ]
    );
    assert_eq!(e.derive(&s).level, 20);

    // the multiclass prerequisite: a Fighter with Strength 8 can't take it, one with Strength 15 can
    let offered = |scores: [i32; 6]| {
        let f = first_level(e, FIGHTER, scores);
        let opened = commands::level_up_new_multiclass(e, &f, None)
            .unwrap()
            .state;
        e.derive(&opened)
            .slots
            .into_iter()
            .find(|x| x.name.starts_with("Multiclass"))
            .expect("multiclass slot")
            .options
            .iter()
            .any(|o| o.id == MC_ID)
    };
    assert!(offered([15, 14, 13, 12, 10, 8]), "Strength 15 qualifies");
    assert!(!offered([8, 15, 14, 13, 12, 10]), "Strength 8 doesn't");
}

#[test]
fn a_copied_srd_class_saves_with_its_own_multiclass_id() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "class", Some("ID_2024_CLASS_WIZARD"), None, None)
        .unwrap();
    assert_eq!(v.form.kind, "class");
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let id = &saved.element_ids[0];
    assert_eq!(id, "ID_EI_CLASS_WIZARD_COPY");
    let class = app.saved_one(id);
    let mc = class["multiclass"]["id"].as_str().unwrap();
    assert_eq!(mc, "ID_EI_MULTICLASS_WIZARD_COPY");
    // every "first class only" names the copy's own section, never the original's
    let text = class.to_string();
    assert!(!text.contains("ID_2024_MULTICLASS_WIZARD"), "{text}");
    assert!(text.contains("!ID_EI_MULTICLASS_WIZARD_COPY"));
    // both are loaded: the original's multiclass entry and the copy's are two entries
    let hub = app.state.hub().unwrap();
    assert!(hub.store.element(id).is_some());
    let f = first_level(&hub.engine, FIGHTER, [15, 14, 13, 15, 10, 8]);
    let opened = commands::level_up_new_multiclass(&hub.engine, &f, None)
        .unwrap()
        .state;
    let options: Vec<String> = hub
        .engine
        .derive(&opened)
        .slots
        .into_iter()
        .find(|x| x.name.starts_with("Multiclass"))
        .unwrap_or_else(|| panic!("multiclass slot in {:?}", slot_names(&hub.engine, &opened)))
        .options
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(options.contains(&"ID_2024_MULTICLASS_WIZARD".to_string()));
    assert!(options.contains(&mc.to_string()), "{options:?}");
}
