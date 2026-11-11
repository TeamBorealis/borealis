//! WB-24 (homebrew creator, spec 14.2, HC7, HC8): a pack as a `.borealis-pack.zip` through the real `AppState` on a
//! temporary data folder with the bundled content.
//!
//! * export -> import as a copy: equal apart from the ids given again (and the book's name and code), editable, a new
//!   character takes the copied class with its copied features; the original pack is untouched;
//! * export -> remove -> import: every element's form equals the form before export, `creator.json` intact;
//! * Replace: the old pack folder goes to the app's trash, the file's pack takes its place;
//! * leave text out: no copied non-SRD description in the zip; the file imports cleanly with the text empty and the
//!   pack still marked not redistributable; with it off the text goes in;
//! * a file without `creator.json` is not taken for a creator pack; a damaged one is refused with a plain message.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_packs::EventSink;
use borealis_lib::content_preview::{ImportFileInput, ImportPreview, PreviewImportRequest};
use borealis_lib::dto::Ruleset;
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::{HomebrewForm, HomebrewPackRequest};
use borealis_lib::homebrew_transfer::PackExportFile;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Read as _;
use std::path::{Path, PathBuf};

const WIZARD: &str = "ID_2024_CLASS_WIZARD";
const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";
const PACK: &str = "homebrew-ei";

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

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

impl App {
    fn new() -> Self {
        Self::with_data(tempfile::tempdir().unwrap())
    }

    fn with_data(data: tempfile::TempDir) -> Self {
        let state = AppState::new(paths(data.path())).unwrap();
        let pack = state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Ember Isles".into(),
                    code: "EI".into(),
                    edition: Ruleset::Dnd2024,
                    author: Some("Mira".into()),
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap();
        assert_eq!(pack.id, PACK);
        Self { data, state }
    }

    /// Save a copy of a loaded element into the pack: its saved ids.
    fn save_copy(&self, kind: &str, id: &str, deep: bool) -> Vec<String> {
        let v = self
            .state
            .homebrew_new_draft(PACK, kind, Some(id), None, Some(deep))
            .unwrap();
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids
    }

    fn dir(&self, pack: &str) -> PathBuf {
        self.data.path().join("library").join(pack)
    }

    fn read(&self, pack: &str, file: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.dir(pack).join(file)).unwrap()).unwrap()
    }

    fn export(&self, leave_out: bool) -> PackExportFile {
        self.state.homebrew_export_file(PACK, leave_out).unwrap()
    }

    fn preview(&self, file: &PackExportFile) -> ImportPreview {
        self.state
            .preview_import_pack(&PreviewImportRequest {
                files: vec![input(&file.file_name, &file.bytes)],
                folder: None,
            })
            .unwrap()
    }

    fn import(&self, file: &PackExportFile, as_copy: bool) -> String {
        let p = self.preview(file);
        let r = self
            .state
            .install_import_preview_as(&p.preview_id, None, as_copy, &Quiet)
            .unwrap();
        r.pack.expect("installed").id
    }

    /// Every element's form, by id, as the creator opens it (no draft is written by opening).
    fn forms(&self, pack: &str) -> BTreeMap<String, HomebrewForm> {
        self.state
            .homebrew_list_elements(pack)
            .unwrap()
            .into_iter()
            .map(|row| {
                let v = self
                    .state
                    .homebrew_open_draft(pack, Some(&row.id), None)
                    .unwrap();
                (row.id, v.form)
            })
            .collect()
    }
}

fn input(name: &str, bytes: &[u8]) -> ImportFileInput {
    serde_json::from_value(json!({"fileName": name, "contentBase64": STANDARD.encode(bytes)}))
        .unwrap()
}

/// The zip's entries: name -> text.
fn unzip(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let name = e.name().unwrap().into_owned();
            let mut text = String::new();
            e.read_to_string(&mut text).unwrap();
            (name, text)
        })
        .collect()
}

fn first_level(e: &Engine, class: &str) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, [8, 14, 13, 15, 12, 10])
        .unwrap()
        .state
}

/// An imported book (the user's own, not SRD): its spell has a description.
fn with_imported_book() -> App {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("library").join("imported-book");
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
        json!({"format": "borealis-content/1", "elements": [
            {"id": "ID_GT_SOURCE_GLOOM_TOME", "name": "Gloom Tome", "type": "Source", "source": "Gloom Tome",
             "setters": [{"name": "abbreviation", "value": "GT"}, {"name": "url", "value": ""}]},
            {"id": "ID_GT_SPELL_SHADOW_BOLT", "name": "Shadow Bolt", "type": "Spell", "source": "Gloom Tome",
             "description": "<p>A bolt of shadow.</p>",
             "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
                         {"name": "time", "value": "1 action"}, {"name": "range", "value": "60 feet"},
                         {"name": "duration", "value": "Instantaneous"}, {"name": "hasVerbalComponent", "value": "true"}]}]})
        .to_string(),
    )
    .unwrap();
    App::with_data(data)
}

#[test]
fn the_pack_file_holds_the_pack_folder_without_drafts_or_trash() {
    let app = App::new();
    let ember = app.save_copy("spell", FIREBALL, false);
    // a deleted element (a `.trash` copy) and an open draft with an edit (a draft file) never go in the file
    let doomed = app.save_copy("spell", "ID_2024_SPELL_BLESS", false);
    app.state
        .homebrew_delete_elements(PACK, &doomed, true, &Quiet)
        .unwrap();
    assert!(app.dir(PACK).join(".trash").is_dir());
    let v = app
        .state
        .homebrew_open_draft(PACK, Some(&ember[0]), None)
        .unwrap();
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Unsaved Ember"));
    app.state.homebrew_update_draft(&v.draft_id, &form).unwrap();

    let file = app.export(true);
    assert_eq!(file.file_name, "Ember_Isles.borealis-pack.zip");
    let entries = unzip(&file.bytes);
    assert_eq!(
        entries.keys().collect::<Vec<_>>(),
        [
            "homebrew-ei/content.json",
            "homebrew-ei/creator.json",
            "homebrew-ei/pack.json"
        ]
    );
    let on_disk = |f: &str| std::fs::read_to_string(app.dir(PACK).join(f)).unwrap();
    assert_eq!(entries["homebrew-ei/content.json"], on_disk("content.json"));
    assert_eq!(entries["homebrew-ei/creator.json"], on_disk("creator.json"));
    assert!(!entries.values().any(|t| t.contains("Unsaved Ember")));
    assert_eq!(file.left_out, 0, "SRD text is the user's to share");
}

#[test]
fn export_then_import_as_a_copy_is_equal_apart_from_the_ids_and_a_new_character_takes_it() {
    let app = App::new();
    let class_ids = app.save_copy("class", WIZARD, true);
    let ember = app.save_copy("spell", FIREBALL, false);
    app.state
        .homebrew_set_spell_list(PACK, "Wizard", &[ember[0].clone()], &[], &Quiet)
        .unwrap();
    let class = class_ids[0].clone();
    let before_files: Vec<Vec<u8>> = ["pack.json", "content.json", "creator.json"]
        .iter()
        .map(|f| std::fs::read(app.dir(PACK).join(f)).unwrap())
        .collect();
    let original = app.read(PACK, "content.json");

    let file = app.export(true);
    let p = app.preview(&file);
    assert!(p.replaces_existing, "the pack is installed here");
    let hb = p.homebrew.as_ref().expect("a creator pack file");
    assert_eq!(
        (
            hb.code.as_str(),
            hb.copy_code.as_str(),
            hb.copy_name.as_str()
        ),
        ("EI", "EI2", "Ember Isles (copy)")
    );
    assert_eq!(p.summary.errors, 0, "{:?}", p.diagnostics);
    assert_eq!(p.unresolved_total, 0, "{:?}", p.unresolved);
    let r = app
        .state
        .install_import_preview_as(&p.preview_id, None, true, &Quiet)
        .unwrap();
    let copy_pack = r.pack.expect("installed");
    assert_eq!(copy_pack.id, "homebrew-ei2");
    assert_eq!(copy_pack.name, "Ember Isles (copy)");
    assert_eq!(copy_pack.kind, PackKind::Homebrew);

    // the original is untouched
    for (f, before) in ["pack.json", "content.json", "creator.json"]
        .iter()
        .zip(&before_files)
    {
        assert_eq!(
            &std::fs::read(app.dir(PACK).join(f)).unwrap(),
            before,
            "{f}"
        );
    }

    // equal apart from the ids (and the book's name and code)
    let copy = app.read("homebrew-ei2", "content.json");
    let text = copy.to_string();
    let left: Vec<&str> = text
        .match_indices("ID_EI_")
        .map(|(i, _)| &text[i.saturating_sub(40)..(i + 60).min(text.len())])
        .collect();
    assert!(
        left.is_empty(),
        "every id of the pack given again: {left:#?}"
    );
    let creator = app.read("homebrew-ei2", "creator.json");
    assert_eq!(creator["code"], "EI2");
    let map: HashMap<String, String> = copy["elements"]
        .as_array()
        .unwrap()
        .iter()
        .zip(original["elements"].as_array().unwrap())
        .map(|(n, o)| {
            (
                n["id"].as_str().unwrap().to_string(),
                o["id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(map
        .iter()
        .all(|(n, o)| n.replacen("ID_EI2_", "ID_EI_", 1) == *o));
    // the pack's own ids are `ID_EI_...`, so the copy read back with the old code is the original
    let restored = text
        .replace("ID_EI2_", "ID_EI_")
        .replace("Ember Isles (copy)", "Ember Isles")
        .replace("\"value\":\"EI2\"", "\"value\":\"EI\"");
    assert_eq!(serde_json::from_str::<Value>(&restored).unwrap(), original);
    assert_eq!(
        copy["appends"][0]["id"], original["appends"][0]["id"],
        "an append to a bundled list keeps its target"
    );
    // creator.json entries follow their elements, provenance intact
    let new_class = format!("ID_EI2_{}", class.strip_prefix("ID_EI_").unwrap());
    assert_eq!(
        creator["elements"][&new_class]["copiedFrom"],
        json!({"id": WIZARD, "packId": "srd-5.2.1", "srd": true})
    );

    // editable in the creator
    let rows = app.state.homebrew_list_elements("homebrew-ei2").unwrap();
    assert_eq!(
        rows.len(),
        app.state.homebrew_list_elements(PACK).unwrap().len()
    );
    assert!(app
        .state
        .homebrew_open_draft("homebrew-ei2", Some(&new_class), None)
        .is_ok());

    // a new character takes the copied class and gets the copied features
    let hub = app.state.hub().unwrap();
    let e = &hub.engine;
    let s = first_level(e, &new_class);
    let ids: Vec<String> = e
        .derive(&s)
        .elements
        .into_iter()
        .map(|x| x.element.id)
        .collect();
    assert!(ids.contains(&new_class), "{ids:?}");
    assert!(
        ids.iter().any(|i| i.starts_with("ID_EI2_CLASS_FEATURE_")),
        "the copied features: {ids:?}"
    );
    assert!(
        !ids.iter().any(|i| i.starts_with("ID_EI_")),
        "nothing of the original: {ids:?}"
    );
}

#[test]
fn export_remove_import_round_trips_every_form() {
    let app = App::new();
    app.save_copy("class", WIZARD, true);
    app.save_copy("spell", FIREBALL, false);
    app.save_copy("background", "ID_2024_BACKGROUND_ACOLYTE", true);
    let forms = app.forms(PACK);
    let creator = app.read(PACK, "creator.json");
    let file = app.export(true);

    app.state.remove_pack(PACK, &Quiet).unwrap();
    assert!(!app.dir(PACK).exists());
    let p = app.preview(&file);
    assert!(!p.replaces_existing);
    assert!(!p.homebrew.as_ref().unwrap().code_taken);
    assert_eq!(p.converted, forms.len(), "the counted elements");
    let id = app.import(&file, false);
    assert_eq!(id, PACK);
    assert_eq!(app.forms(PACK), forms);
    assert_eq!(app.read(PACK, "creator.json"), creator);
    assert_eq!(
        app.read(PACK, "pack-meta.json")["kind"],
        "homebrew",
        "editable"
    );
}

#[test]
fn replace_puts_the_old_pack_in_the_trash() {
    let app = App::new();
    app.save_copy("spell", FIREBALL, false);
    let file = app.export(true);
    // after the export the pack gains an element: Replace brings back the file's pack
    let later = app.save_copy("spell", "ID_2024_SPELL_BLESS", false);
    let before = std::fs::read_to_string(app.dir(PACK).join("content.json")).unwrap();
    let p = app.preview(&file);
    assert!(p.replaces_existing);
    let id = app.import(&file, false);
    assert_eq!(id, PACK);
    let now = std::fs::read_to_string(app.dir(PACK).join("content.json")).unwrap();
    assert!(!now.contains(&later[0]), "the file's pack replaced it");
    let trash = app.data.path().join("sync").join("trash");
    let kept: Vec<PathBuf> = std::fs::read_dir(&trash)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].to_string_lossy().ends_with("-homebrew-ei"));
    assert_eq!(
        std::fs::read_to_string(kept[0].join("content.json")).unwrap(),
        before,
        "the old copy is kept whole"
    );
    assert_eq!(
        app.state
            .list_packs()
            .unwrap()
            .iter()
            .filter(|p| p.id == PACK)
            .count(),
        1
    );
}

#[test]
fn leave_text_out_exports_no_copied_text_and_imports_cleanly() {
    let app = with_imported_book();
    let shadow = app.save_copy("spell", "ID_GT_SPELL_SHADOW_BOLT", false);
    let own = app.save_copy("spell", FIREBALL, false);
    let rows = app.state.homebrew_list_elements(PACK).unwrap();
    let copied: Vec<&str> = rows
        .iter()
        .filter(|r| r.copied_text)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(copied, [shadow[0].as_str()], "the dialog's count");

    // with the text left out (the default): no copied description anywhere in the file
    let file = app.export(true);
    assert_eq!(file.left_out, 1);
    let entries = unzip(&file.bytes);
    assert!(
        !entries.values().any(|t| t.contains("A bolt of shadow")),
        "{entries:?}"
    );
    let content: Value = serde_json::from_str(&entries["homebrew-ei/content.json"]).unwrap();
    let el = |id: &str| {
        content["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(el(&shadow[0]).get("description"), None);
    assert!(
        el(&own[0])["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()),
        "SRD text stays"
    );
    let creator: Value = serde_json::from_str(&entries["homebrew-ei/creator.json"]).unwrap();
    assert_eq!(creator["elements"][&shadow[0]]["nonSrdText"], true);
    // the pack on disk keeps its text
    assert!(std::fs::read_to_string(app.dir(PACK).join("content.json"))
        .unwrap()
        .contains("A bolt of shadow"));

    // it imports cleanly: the description empty, the pack still not redistributable, editable
    let p = app.preview(&file);
    assert_eq!(p.summary.errors, 0, "{:?}", p.diagnostics);
    let id = app.import(&file, true);
    let pack = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(pack.redistributable, Some(false));
    let copy_id = shadow[0].replacen("ID_EI_", "ID_EI2_", 1);
    let rows = app.state.homebrew_list_elements(&id).unwrap();
    assert!(
        rows.iter().all(|r| !r.copied_text),
        "nothing left to leave out"
    );
    let v = app
        .state
        .homebrew_open_draft(&id, Some(&copy_id), None)
        .unwrap();
    assert_eq!(v.form.fields["name"], json!("Shadow Bolt (copy)"));
    assert!(
        v.problems.iter().all(|p| p.severity != "error"),
        "{:?}",
        v.problems
    );

    // with "Leave that text out" off, the text goes in
    let full = app.export(false);
    assert_eq!(full.left_out, 0);
    assert!(unzip(&full.bytes)["homebrew-ei/content.json"].contains("A bolt of shadow"));
}

#[test]
fn other_zips_are_not_creator_packs_and_damaged_ones_are_refused() {
    let app = App::new();
    app.save_copy("spell", FIREBALL, false);
    let file = app.export(true);
    let mut entries = unzip(&file.bytes);

    // no creator.json: the XML import reads the zip (and finds nothing to import)
    let rezip = |entries: &BTreeMap<String, String>| {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (n, t) in entries {
            z.start_file(n, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut z, t.as_bytes()).unwrap();
        }
        z.finish().unwrap().into_inner()
    };
    let mut plain = entries.clone();
    plain.remove("homebrew-ei/creator.json");
    let err = app
        .state
        .preview_import_pack(&PreviewImportRequest {
            files: vec![input("x.zip", &rezip(&plain))],
            folder: None,
        })
        .unwrap_err();
    assert!(
        err.message.contains("No .xml or .index files"),
        "{}",
        err.message
    );

    // a damaged content file: a plain message, nothing installed
    entries.insert("homebrew-ei/content.json".into(), "{ not json".into());
    let err = app
        .state
        .preview_import_pack(&PreviewImportRequest {
            files: vec![input("x.borealis-pack.zip", &rezip(&entries))],
            folder: None,
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    assert_eq!(
        err.message,
        "This pack file is damaged: its content.json can't be read. Nothing was imported."
    );

    // a pack file naming a bundled pack is refused
    let mut bundled = unzip(&file.bytes);
    let manifest = bundled["homebrew-ei/pack.json"].replace("\"homebrew-ei\"", "\"srd-5.1\"");
    bundled.insert("homebrew-ei/pack.json".into(), manifest);
    let err = app
        .state
        .preview_import_pack(&PreviewImportRequest {
            files: vec![input("x.borealis-pack.zip", &rezip(&bundled))],
            folder: None,
        })
        .unwrap_err();
    assert!(err.message.contains("can't install"), "{}", err.message);
}

#[test]
fn only_homebrew_packs_export() {
    let app = with_imported_book();
    let err = app
        .state
        .homebrew_export_file("imported-book", false)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
}
