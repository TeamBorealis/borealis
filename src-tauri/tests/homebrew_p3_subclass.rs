//! P3 bugs (homebrew creator, 1.5.0, HC11) through the real `AppState`: a class's subclasses follow it wherever they
//! are. A renamed class's subclasses saved in another homebrew pack, and the edited drafts of its subclasses, take its
//! new tag ("<Class> Subclass") and name, so the class keeps offering them. A subclass (draft or saved) that picked a
//! class before its first save, by the provisional id the class had then, follows the class to the id it is saved
//! with when it was renamed in between.

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

const LEVEL1: &str = "ID_LEVEL_1";

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

    fn pack(&self, name: &str, code: &str) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: name.into(),
                    code: code.into(),
                    edition: Ruleset::Dnd2024,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id
    }

    fn update(&self, v: &HomebrewDraftView, fields: &[(&str, Value)]) -> HomebrewDraftView {
        let mut form = v.form.clone();
        for (k, x) in fields {
            form.fields.insert((*k).into(), x.clone());
        }
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    fn save(&self, v: &HomebrewDraftView) -> String {
        let errors: Vec<&str> = v
            .problems
            .iter()
            .filter(|p| p.severity == "error")
            .map(|p| p.message.as_str())
            .collect();
        assert_eq!(errors, Vec::<&str>::new());
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone()
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
            .cloned()
            .unwrap_or_else(|| panic!("{id} saved"))
    }

    /// A new 2024 class draft "<name>" (d10, one skill), edited.
    fn class_draft(&self, pack: &str, name: &str) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(pack, "class", None, None, None)
            .unwrap();
        self.update(
            &v,
            &[
                ("name", json!(name)),
                ("hitDie", json!("d10")),
                (
                    "skills",
                    json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_ATHLETICS", "name": "Athletics", "type": "Proficiency"}]}),
                ),
            ],
        )
    }

    /// A subclass draft "<name>" of the class `class_id` named `class_name`, edited.
    fn subclass_draft(
        &self,
        pack: &str,
        name: &str,
        class_id: &str,
        class_name: &str,
    ) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(pack, "subclass", None, None, None)
            .unwrap();
        self.update(
            &v,
            &[
                ("name", json!(name)),
                (
                    "parentClass",
                    json!({"id": class_id, "name": class_name, "type": "Class"}),
                ),
            ],
        )
    }

    fn rename(&self, pack: &str, class_id: &str, name: &str) {
        let v = self
            .state
            .homebrew_open_draft(pack, Some(class_id), None)
            .unwrap();
        let v = self.update(&v, &[("name", json!(name))]);
        self.save(&v);
    }
}

/// The subclasses a new 2024 character of `class` is offered at level 3.
fn offered_subclasses(e: &Engine, class: &str) -> Vec<String> {
    let mut s: CharacterState = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, LEVEL1, "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    for _ in 1..3 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    e.derive(&s)
        .slots
        .iter()
        .find(|x| x.name == "Subclass")
        .map(|x| x.options.iter().map(|o| o.id.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_renamed_class_s_subclasses_in_other_packs_and_drafts_take_its_new_tag() {
    // Arrange: the class in one pack, a saved subclass of it in another, and an edited draft of a saved subclass
    let app = App::new();
    let isles = app.pack("Ember Isles", "EI");
    let wastes = app.pack("Ash Wastes", "AW");
    let class_id = app.save(&app.class_draft(&isles, "Ember Knight"));
    let other = app.save(&app.subclass_draft(&wastes, "Order of Ash", &class_id, "Ember Knight"));
    assert_eq!(
        app.saved(&wastes, &other)["supports"],
        json!(["Ember Knight Subclass"])
    );
    let own = app.save(&app.subclass_draft(&isles, "Order of Cinders", &class_id, "Ember Knight"));
    let opened = app
        .state
        .homebrew_open_draft(&isles, Some(&own), None)
        .unwrap();
    let draft = app.update(&opened, &[("name", json!("Order of the Cinders"))]);
    assert_eq!(
        draft.form.fields["subclassTag"],
        json!("Ember Knight Subclass")
    );

    // Act
    app.rename(&isles, &class_id, "Cinder Knight");

    // Assert: the other pack's subclass follows the class, and the class offers it
    let followed = app.saved(&wastes, &other);
    assert_eq!(followed["supports"], json!(["Cinder Knight Subclass"]));
    assert_eq!(followed["data"]["class_name"], "Cinder Knight");
    assert_eq!(followed["data"]["class"], json!(class_id));
    let hub = app.state.hub().unwrap();
    let offered = offered_subclasses(&hub.engine, &class_id);
    drop(hub);
    assert!(offered.contains(&other), "{offered:?}");
    assert!(offered.contains(&own), "{offered:?}");

    // the draft of the own pack's subclass carries the new tag and name, and saves them
    let reopened = app
        .state
        .homebrew_open_draft(&isles, None, Some(&draft.draft_id))
        .unwrap();
    assert_eq!(
        reopened.form.fields["subclassTag"],
        json!("Cinder Knight Subclass")
    );
    assert_eq!(
        reopened.form.fields["parentClass"]["name"],
        json!("Cinder Knight")
    );
    app.save(&reopened);
    assert_eq!(
        app.saved(&isles, &own)["supports"],
        json!(["Cinder Knight Subclass"])
    );
}

#[test]
fn a_subclass_of_a_class_renamed_before_its_first_save_follows_it_to_its_id() {
    // Arrange: a class not saved yet, and a subclass draft that picked it by its provisional id
    let app = App::new();
    let isles = app.pack("Ember Isles", "EI");
    let class = app.class_draft(&isles, "Gloom Knight");
    let provisional = "ID_EI_CLASS_GLOOM_KNIGHT";
    let sub = app.subclass_draft(&isles, "Order of Dusk", provisional, "Gloom Knight");
    assert_eq!(
        sub.form.fields["subclassTag"],
        json!("Gloom Knight Subclass")
    );

    // Act: the class renamed, then saved for the first time
    let class = app.update(&class, &[("name", json!("Dusk Knight"))]);
    let class_id = app.save(&class);
    assert_eq!(class_id, "ID_EI_CLASS_DUSK_KNIGHT");

    // Assert: the subclass draft names the saved class, with its tag, and saves them
    let reopened = app
        .state
        .homebrew_open_draft(&isles, None, Some(&sub.draft_id))
        .unwrap();
    assert_eq!(reopened.form.fields["parentClass"]["id"], json!(class_id));
    assert_eq!(
        reopened.form.fields["parentClass"]["name"],
        json!("Dusk Knight")
    );
    assert_eq!(
        reopened.form.fields["subclassTag"],
        json!("Dusk Knight Subclass")
    );
    let sub_id = app.save(&reopened);
    let saved = app.saved(&isles, &sub_id);
    assert_eq!(saved["data"]["class"], json!(class_id));
    assert_eq!(saved["supports"], json!(["Dusk Knight Subclass"]));
    let hub = app.state.hub().unwrap();
    assert!(offered_subclasses(&hub.engine, &class_id).contains(&sub_id));
}

#[test]
fn a_subclass_saved_before_its_class_follows_the_class_renamed_before_its_first_save() {
    // Arrange: a subclass saved while its class (picked by its provisional id) is not saved yet
    let app = App::new();
    let isles = app.pack("Ember Isles", "EI");
    let class = app.class_draft(&isles, "Gloom Knight");
    let sub = app.subclass_draft(
        &isles,
        "Order of Dusk",
        "ID_EI_CLASS_GLOOM_KNIGHT",
        "Gloom Knight",
    );
    let sub_id = app.save(&sub);
    assert_eq!(
        app.saved(&isles, &sub_id)["data"]["class"],
        json!("ID_EI_CLASS_GLOOM_KNIGHT")
    );

    // Act: the class renamed, then saved for the first time
    let class = app.update(&class, &[("name", json!("Dusk Knight"))]);
    let class_id = app.save(&class);

    // Assert: the saved subclass names the class's id, name and tag, and the class offers it
    let saved = app.saved(&isles, &sub_id);
    assert_eq!(saved["data"]["class"], json!(class_id));
    assert_eq!(saved["data"]["class_name"], json!("Dusk Knight"));
    assert_eq!(saved["supports"], json!(["Dusk Knight Subclass"]));
    let hub = app.state.hub().unwrap();
    assert!(offered_subclasses(&hub.engine, &class_id).contains(&sub_id));
}
