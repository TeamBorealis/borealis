//! WB-11 (homebrew creator spell form, spec 4.1) through the real `AppState` on a temporary data folder with the bundled
//! content: a new spell starts as a spell and asks only for its school, goes on the Wizard list, saves as Aurora content
//! writes a spell (setters + `supports`), is offered by a Wizard-list spell picker with its Magic columns, and opens
//! again in the spell form unchanged. A bundled 2024 spell opens in the spell form too.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerRow, HomebrewPickerTarget,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

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

    fn edit(&self, v: &HomebrewDraftView, fields: &[(&str, Value)]) -> HomebrewDraftView {
        let mut form: FormState = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).to_string(), value.clone());
        }
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    /// The pack's saved elements, as on disk.
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

    fn spells(&self, query: &str, tags: &[&str]) -> Vec<HomebrewPickerRow> {
        self.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: vec!["Spell".into()],
                    tags: Some(tags.iter().map(|t| (*t).to_string()).collect()),
                    multiple: None,
                },
                query,
                &HomebrewPickerFilters::default(),
                HomebrewPageRequest {
                    offset: 0,
                    limit: 20,
                },
                None,
            )
            .unwrap()
            .items
    }
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

#[test]
fn a_new_spell_on_the_wizard_list_saves_and_is_offered_by_a_wizard_spell_picker() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "spell", None, None, None)
        .unwrap();
    assert_eq!(v.form.kind, "spell");
    assert_eq!(v.form.fields["level"], json!("1"));
    assert_eq!(v.form.fields["time"], json!("Action"));
    assert_eq!(
        errors(&v),
        [
            ("fields.name".into(), "Give it a name.".into()),
            ("fields.school".into(), "A spell needs a school.".into())
        ]
    );
    let v = app.edit(
        &v,
        &[
            ("name", json!("Ember Lance")),
            ("school", json!("Evocation")),
            ("range", json!("120 feet")),
            ("spellLists", json!(["Wizard"])),
        ],
    );
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, ["ID_EI_SPELL_EMBER_LANCE"]);

    // stored as Aurora content stores a spell: the engine's setters, the lists as tags
    let spell = app
        .saved()
        .into_iter()
        .find(|e| e["id"] == "ID_EI_SPELL_EMBER_LANCE")
        .expect("saved");
    assert_eq!(spell["type"], "Spell");
    assert_eq!(spell["supports"], json!(["Wizard"]));
    let setter = |n: &str| {
        spell["setters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == n)
            .map(|s| s["value"].clone())
    };
    assert_eq!(setter("level"), Some(json!("1")));
    assert_eq!(setter("range"), Some(json!("120 feet")));
    assert_eq!(setter("hasVerbalComponent"), Some(json!("true")));
    assert_eq!(setter("hasMaterialComponent"), Some(json!("false")));
    assert!(
        spell.get("data").is_none(),
        "a new spell gets no 2024 mirror"
    );

    // pickable: a Wizard-list spell picker offers it with its Magic columns; a Cleric-list one doesn't
    let rows = app.spells("ember lance", &["Wizard"]);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(
        (row.level, row.school.as_deref()),
        (Some(1), Some("Evocation"))
    );
    let facts = row.spell.as_ref().expect("spell facts");
    assert!(facts.verbal && facts.somatic && !facts.material);
    assert!(app.spells("ember lance", &["Cleric"]).is_empty());

    // it opens again in the spell form, with its lists; an unchanged save writes the same element
    let again = app
        .state
        .homebrew_open_draft(&app.pack, Some("ID_EI_SPELL_EMBER_LANCE"), None)
        .unwrap();
    assert_eq!(again.form.kind, "spell");
    assert_eq!(again.form.fields["spellLists"], json!(["Wizard"]));
    let touched = app.edit(&again, &[]);
    app.state
        .homebrew_save_draft(&touched.draft_id, &Quiet)
        .unwrap();
    let after = app
        .saved()
        .into_iter()
        .find(|e| e["id"] == "ID_EI_SPELL_EMBER_LANCE")
        .unwrap();
    assert_eq!(after, spell);
}

#[test]
fn a_copied_srd_spell_opens_in_the_spell_form_with_its_lists() {
    let app = App::new();
    let v = app
        .state
        .homebrew_new_draft(&app.pack, "spell", Some("ID_2024_SPELL_BLESS"), None, None)
        .unwrap();
    assert_eq!(v.form.kind, "spell");
    assert_eq!(v.form.fields["spellLists"], json!(["Cleric", "Paladin"]));
    assert_eq!(v.form.fields["materialCost"], json!("5"));
    let v = app.edit(
        &v,
        &[("spellLists", json!(["Cleric", "Paladin", "Wizard"]))],
    );
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let copy = app
        .saved()
        .into_iter()
        .find(|e| e["name"] == "Bless (copy)")
        .expect("copy saved");
    assert_eq!(copy["supports"], json!(["Cleric", "Paladin", "Wizard"]));
    assert_eq!(
        copy["data"]["classes"],
        json!(["Cleric", "Paladin", "Wizard"])
    );
}

#[test]
fn the_spell_lists_offered_are_the_lists_loaded_classes_declare() {
    let app = App::new();
    let page = app
        .state
        .homebrew_reference_options(
            &HomebrewPickerTarget {
                types: vec![borealis_lib::homebrew_refs::SPELL_LIST_TYPE.into()],
                tags: None,
                multiple: None,
            },
            "",
            &HomebrewPickerFilters::default(),
            HomebrewPageRequest {
                offset: 0,
                limit: 50,
            },
            None,
        )
        .unwrap();
    let names: Vec<&str> = page.items.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["Bard", "Cleric", "Druid", "Paladin", "Ranger", "Sorcerer", "Warlock", "Wizard"],
        "the SRD casters' lists, once each; no Fighter or Rogue"
    );
    assert!(page
        .items
        .iter()
        .all(|r| r.ty == "Spell List" && r.id == r.name));
}
