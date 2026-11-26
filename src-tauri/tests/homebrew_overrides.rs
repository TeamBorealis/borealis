//! HC20 overrides (homebrew-creator spec 22) through the real `AppState` on a temporary data folder with the bundled
//! content and a synthetic imported book (never a real non-SRD source, CS1):
//!
//! * an override of the bundled SRD Fireball keeps its id, replaces the original everywhere ("Changed by"), shows in
//!   the pack's element list, and Revert brings the original back (undoable);
//! * an override of an imported book's element wins although the book's folder loads after the pack, and survives a
//!   restart; when the book is imported again with other text the override says the original changed, and saving
//!   records the new original; a book that is gone leaves the override without effect;
//! * a book element, an element of the user's own pack, and an element another pack already changes can't be
//!   overridden;
//! * the shared pack file leaves the imported book's override out by default and keeps the SRD one.

use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";
const BOLT: &str = "ID_GT_SPELL_SHADOW_BOLT";
const BOOK_DIR: &str = "zz-imported-book";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

/// A synthetic imported book in a folder that sorts after the homebrew pack's: its spell says `text`.
fn write_book(data: &Path, text: &str) {
    let dir = data.join("library").join(BOOK_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        json!({"format": "borealis-pack/1", "id": "imported-book", "name": "Gloom Tome", "version": "1",
               "origin": "custom", "rulesets": ["dnd2024"], "files": [{"path": "content.json", "kind": "content"}]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("content.json"),
        json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": [
            {"id": "ID_GT_SOURCE_GLOOM_TOME", "name": "Gloom Tome", "type": "Source", "source": "Gloom Tome",
             "setters": [{"name": "abbreviation", "value": "GT"}, {"name": "url", "value": ""}]},
            {"id": BOLT, "name": "Shadow Bolt", "type": "Spell", "source": "Gloom Tome",
             "description": format!("<p>{text}</p>"),
             "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
                         {"name": "time", "value": "1 action"}, {"name": "range", "value": "60 feet"},
                         {"name": "duration", "value": "Instantaneous"}, {"name": "hasVerbalComponent", "value": "true"}]}]})
        .to_string(),
    )
    .unwrap();
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
    pack: String,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        write_book(data.path(), "A bolt of shadow.");
        let state = AppState::new(paths(data.path())).unwrap();
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

    /// The app started again on the same data folder.
    fn restart(self) -> Self {
        let Self { data, state, pack } = self;
        drop(state);
        let state = AppState::new(paths(data.path())).unwrap();
        Self { data, state, pack }
    }

    fn read(&self, file: &str) -> Value {
        let path = self.data.path().join("library").join(&self.pack).join(file);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn description(&self, id: &str) -> String {
        self.state
            .hub()
            .unwrap()
            .store
            .element(id)
            .expect("loaded")
            .description
            .clone()
    }

    fn range(&self, id: &str) -> String {
        let hub = self.state.hub().unwrap();
        hub.store
            .element(id)
            .expect("loaded")
            .setter_value("range")
            .unwrap_or_default()
            .to_string()
    }

    /// Override `id`, set `field` to `value`, save.
    fn override_with(&self, id: &str, field: &str, value: Value) -> HomebrewDraftView {
        let v = self.state.homebrew_new_override(&self.pack, id).unwrap();
        let mut form = v.form.clone();
        form.fields.insert(field.into(), value);
        let v = self
            .state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap();
        self.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
        v
    }
}

fn problems(v: &HomebrewDraftView, severity: &str) -> Vec<String> {
    v.problems
        .iter()
        .filter(|p| p.severity == severity)
        .map(|p| format!("{}: {}", p.path.clone().unwrap_or_default(), p.message))
        .collect()
}

#[test]
fn an_srd_override_keeps_its_id_replaces_the_original_and_reverts() {
    let app = App::new();
    let original = app.range(FIREBALL);
    let v = app
        .state
        .homebrew_new_override(&app.pack, FIREBALL)
        .unwrap();
    assert_eq!(v.element_id.as_deref(), Some(FIREBALL));
    assert_eq!(v.form.fields["name"], json!("Fireball"), "no (copy)");
    let info = v.override_info.as_ref().expect("an override draft says so");
    assert!(info.srd && !info.changed && !info.missing);
    assert_eq!(problems(&v, "error"), Vec::<String>::new());
    assert!(
        !v.problems.iter().any(|p| p.code == "id.clash"),
        "an override's own id is no accident: {:?}",
        v.problems
    );
    let mut form = v.form.clone();
    form.fields.insert("range".into(), json!("300 feet"));
    let v = app.state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, [FIREBALL]);

    // saved apart from the pack's own elements, with its id; the original's book is kept
    let content = app.read("content.json");
    assert!(!content["elements"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == FIREBALL));
    let ov = &content["overrides"][0];
    assert_eq!(ov["id"], FIREBALL);
    assert_ne!(ov["source"], json!("Ember Isles"));
    let creator = app.read("creator.json");
    assert_eq!(
        creator["overrides"][FIREBALL]["original"]["srd"],
        json!(true)
    );
    assert!(creator["overrides"][FIREBALL]["originalDigest"]
        .as_str()
        .is_some_and(|d| !d.is_empty()));
    assert!(creator["retiredIds"].as_array().unwrap().is_empty());

    // it replaces the original everywhere: the store, the description pane
    assert_eq!(app.range(FIREBALL), "300 feet");
    let detail = app.state.get_element(FIREBALL).unwrap();
    let by = detail.changed_by.expect("Changed by");
    assert_eq!(
        (by.pack_id.as_str(), by.pack_name.as_str()),
        (app.pack.as_str(), "Ember Isles")
    );
    let rows = app.state.homebrew_list_elements(&app.pack).unwrap();
    let row = rows.iter().find(|r| r.id == FIREBALL).expect("listed");
    assert!(row
        .override_info
        .as_ref()
        .is_some_and(|o| o.srd && !o.changed));
    assert!(!row.copied_text, "SRD text is shareable");

    // reopened from the list, it is the override again
    let again = app
        .state
        .homebrew_open_draft(&app.pack, Some(FIREBALL), None)
        .unwrap();
    assert!(again.override_info.is_some());
    assert_eq!(again.form.fields["range"], json!("300 feet"));
    app.state.homebrew_discard_draft(&again.draft_id).unwrap();

    // Revert: the original returns, undoably
    let reverted = app
        .state
        .homebrew_revert_override(&app.pack, FIREBALL, &Quiet)
        .unwrap();
    assert!(reverted.deleted);
    assert_eq!(app.range(FIREBALL), original);
    assert!(app
        .state
        .get_element(FIREBALL)
        .unwrap()
        .changed_by
        .is_none());
    assert!(app.read("content.json").get("overrides").is_none());
    app.state
        .homebrew_undo(reverted.undo_id.as_deref().unwrap(), &Quiet)
        .unwrap();
    assert_eq!(app.range(FIREBALL), "300 feet");
}

#[test]
fn an_imported_override_wins_after_its_book_and_notices_a_new_import() {
    let app = App::new();
    assert!(app
        .state
        .homebrew_list_elements(&app.pack)
        .unwrap()
        .is_empty());
    let v = app.override_with(BOLT, "range", json!("90 feet"));
    assert!(v
        .override_info
        .as_ref()
        .is_some_and(|o| !o.srd && o.source_name == "Gloom Tome"));
    assert_eq!(app.range(BOLT), "90 feet");
    let creator = app.read("creator.json");
    assert_eq!(creator["overrides"][BOLT]["nonSrdText"], json!(true));

    // the book's folder loads after the pack's; after a restart the override still wins
    let app = app.restart();
    assert_eq!(app.range(BOLT), "90 feet");
    let row = |app: &App| {
        app.state
            .homebrew_list_elements(&app.pack)
            .unwrap()
            .into_iter()
            .find(|r| r.id == BOLT)
            .and_then(|r| r.override_info)
            .expect("an override row")
    };
    assert!(!row(&app).changed);
    assert!(app
        .state
        .list_packs()
        .unwrap()
        .iter()
        .any(|p| p.id == app.pack));

    // the book is imported again with other text: the override notices
    write_book(app.data.path(), "A newer bolt of shadow.");
    let app = app.restart();
    assert_eq!(app.range(BOLT), "90 feet", "the override still wins");
    assert!(
        app.description(BOLT).contains("A bolt of shadow."),
        "the override keeps the text it was saved with"
    );
    assert!(row(&app).changed);
    let v = app
        .state
        .homebrew_open_draft(&app.pack, Some(BOLT), None)
        .unwrap();
    assert!(v.override_info.as_ref().is_some_and(|o| o.changed));
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert!(
        !row(&app).changed,
        "saving keeps my version and records the new original"
    );

    // the book is removed: the override has no effect and says why
    std::fs::remove_dir_all(app.data.path().join("library").join(BOOK_DIR)).unwrap();
    let app = app.restart();
    assert!(app.state.hub().unwrap().store.element(BOLT).is_none());
    assert!(row(&app).missing);
}

#[test]
fn books_own_elements_and_elements_changed_elsewhere_are_refused() {
    let app = App::new();
    let refused = |id: &str| {
        app.state
            .homebrew_new_override(&app.pack, id)
            .err()
            .map(|e| e.message)
            .unwrap_or_default()
    };
    assert_eq!(
        refused("ID_GT_SOURCE_GLOOM_TOME"),
        "A book element can't be changed."
    );
    // an element of the user's own pack is edited in place
    let own = app
        .state
        .homebrew_new_draft(&app.pack, "language", None, None, None)
        .unwrap();
    let mut form = own.form.clone();
    form.fields.insert("name".into(), json!("Emberspeak"));
    let own = app
        .state
        .homebrew_update_draft(&own.draft_id, &form)
        .unwrap();
    let saved = app
        .state
        .homebrew_save_draft(&own.draft_id, &Quiet)
        .unwrap();
    assert_eq!(
        refused(&saved.element_ids[0]),
        "That element is in your pack Ember Isles. Open it there to change it."
    );
    // another pack already changes Fireball
    let other = app
        .state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: "Frost Reach".into(),
                code: "FR".into(),
                edition: Ruleset::Dnd2024,
                author: None,
                description: None,
                license: None,
            },
            &Quiet,
        )
        .unwrap()
        .id;
    let v = app.state.homebrew_new_override(&other, FIREBALL).unwrap();
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(
        refused(FIREBALL),
        "Frost Reach already changes this element. Change it there, or revert that change first."
    );
}

#[test]
fn sharing_leaves_the_imported_override_out_and_keeps_the_srd_one() {
    let app = App::new();
    app.override_with(BOLT, "range", json!("90 feet"));
    app.override_with(FIREBALL, "range", json!("300 feet"));
    let file = app.state.homebrew_export_file(&app.pack, true).unwrap();
    assert_eq!(file.left_out, 1);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file.bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(&format!("{}/content.json", app.pack))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let content: Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<&str> = content["overrides"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["id"].as_str())
        .collect();
    assert_eq!(ids, [FIREBALL]);
    let kept = app.state.homebrew_export_file(&app.pack, false).unwrap();
    assert_eq!(kept.left_out, 0);
}
