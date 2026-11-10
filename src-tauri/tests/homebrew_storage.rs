//! WB-06 (homebrew creator, spec 10, 11, 12.3, 14): packs and drafts through the real `AppState`, on a temporary data
//! folder with the bundled content: NEW PACK, the draft lifecycle (nothing written until the first edit, resume after
//! a restart), save with ids frozen and the fast reload, refused saves, delete with its scan of the local characters,
//! retired ids, duplicate, copies, pack settings, picker rows, and the failure modes of user data: an interrupted save,
//! damaged or missing `creator.json` and draft files, a pack edited outside the app.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackPatch, HomebrewPackRequest, HomebrewPageRequest,
    HomebrewPickerFilters, HomebrewPickerTarget,
};
use borealis_lib::homebrew_packs::{FAST_RELOAD, FULL_RELOAD};
use borealis_lib::settings::AppSettings;
use borealis_lib::state::{AppPaths, AppState, ContentHub};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
struct Reloads(Mutex<Vec<String>>);
impl EventSink for Reloads {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, r: &str) {
        self.0.lock().unwrap().push(r.to_string());
    }
}

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
    sink: Reloads,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(paths(data.path())).unwrap();
        Self {
            data,
            state,
            sink: Reloads::default(),
        }
    }

    /// The app started again on the same data folder (a restart: empty memory).
    fn restart(self) -> Self {
        let Self { data, .. } = self;
        let state = AppState::new(paths(data.path())).unwrap();
        Self {
            data,
            state,
            sink: Reloads::default(),
        }
    }

    fn pack_dir(&self, id: &str) -> PathBuf {
        self.data.path().join("library").join(id)
    }

    fn read(&self, id: &str, file: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.pack_dir(id).join(file)).unwrap())
            .unwrap()
    }

    fn drafts_dir(&self) -> PathBuf {
        self.data.path().join("homebrew-drafts")
    }

    fn draft_files(&self) -> Vec<String> {
        std::fs::read_dir(self.drafts_dir())
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn create(&self, name: &str, code: &str) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: name.into(),
                    code: code.into(),
                    edition: Ruleset::Dnd2024,
                    author: Some("Mira".into()),
                    description: None,
                    license: Some("CC0".into()),
                },
                &self.sink,
            )
            .unwrap()
            .id
    }

    /// A new element of `kind` named `name`, edited once (so it has a draft file).
    fn named(&self, pack: &str, kind: &str, name: &str) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(pack, kind, None, None, None)
            .unwrap();
        self.edit(&v, |f| {
            f.fields.insert("name".into(), json!(name));
        })
    }

    fn edit(
        &self,
        v: &HomebrewDraftView,
        change: impl FnOnce(&mut FormState),
    ) -> HomebrewDraftView {
        let mut form = v.form.clone();
        change(&mut form);
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    fn save(&self, v: &HomebrewDraftView) -> Vec<String> {
        self.state
            .homebrew_save_draft(&v.draft_id, &self.sink)
            .unwrap()
            .element_ids
    }

    fn loaded(&self, id: &str) -> bool {
        self.state.hub().unwrap().store.contains_id(id)
    }
}

#[test]
fn a_new_pack_has_its_book_creator_json_and_loads() {
    let app = App::new();
    let id = app.create("Gloom Hollow", "GH");
    assert_eq!(id, "homebrew-gh");
    for f in [
        "pack.json",
        "content.json",
        "creator.json",
        "pack-meta.json",
    ] {
        assert!(app.pack_dir(&id).join(f).is_file(), "{f}");
    }
    let content = app.read(&id, "content.json");
    assert_eq!(content["elements"][0]["type"], "Source");
    assert_eq!(content["elements"][0]["name"], "Gloom Hollow");
    let creator = app.read(&id, "creator.json");
    assert_eq!(
        (creator["format"].as_str(), creator["code"].as_str()),
        (Some("borealis-creator/1"), Some("GH"))
    );
    let pack = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(pack.kind, PackKind::Homebrew);
    assert_eq!(pack.license, "CC0");
    assert!(app.loaded("ID_GH_SOURCE_GLOOM_HOLLOW"));
    // a code in use (this pack's, a bundled book's) and a bad code are refused
    let refuse = |name: &str, code: &str| {
        app.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: name.into(),
                    code: code.into(),
                    edition: Ruleset::Any,
                    author: None,
                    description: None,
                    license: None,
                },
                &app.sink,
            )
            .unwrap_err()
    };
    assert_eq!(refuse("Again", "GH").code, ErrorCode::InvalidArgument);
    assert_eq!(
        refuse("Srd", "SRD").message,
        "Another book already uses the code SRD."
    );
    assert_eq!(refuse("Bad", "g").code, ErrorCode::InvalidArgument);
    assert_eq!(refuse("  ", "ZZ").message, "Give the pack a name.");
    assert!(app.state.homebrew_list_elements(&id).unwrap().is_empty());
}

#[test]
fn a_draft_is_written_by_its_first_edit_and_saved_with_a_frozen_id() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let fresh = app
        .state
        .homebrew_new_draft(&pack, "language", None, None, None)
        .unwrap();
    assert!(fresh.element_id.is_none());
    assert_eq!(fresh.form.fields["source"], "Gloom Hollow");
    assert!(fresh
        .problems
        .iter()
        .any(|p| p.path.as_deref() == Some("fields.name")));
    assert!(app.draft_files().is_empty(), "opening writes nothing");
    assert!(app.state.homebrew_list_drafts().unwrap().is_empty());
    let named = app.edit(&fresh, |f| {
        f.fields.insert("name".into(), json!("  Gloom\u{0}speak "));
        f.fields.insert("source".into(), json!("Forged"));
    });
    // only the hint that the description is still empty (info never counts as a problem)
    assert!(
        named.problems.iter().all(|p| p.severity == "info"),
        "{:?}",
        named.problems
    );
    assert_eq!(
        named.form.fields["source"], "Gloom Hollow",
        "read-only fields keep their value"
    );
    assert_eq!(app.draft_files(), [format!("{}.json", fresh.draft_id)]);
    assert_eq!(
        app.state.homebrew_list_drafts().unwrap()[0].name,
        "  Gloom\u{0}speak "
    );
    let preview = named.preview.as_ref().expect("preview");
    assert_eq!(preview.summary.name, "Gloomspeak");
    assert!(named.generated.json.contains("ID_GH_LANGUAGE_GLOOMSPEAK"));
    let saved = app.save(&named);
    assert_eq!(saved, ["ID_GH_LANGUAGE_GLOOMSPEAK"]);
    assert!(app.draft_files().is_empty(), "saving deletes the draft");
    assert!(app.loaded("ID_GH_LANGUAGE_GLOOMSPEAK"), "reloaded");
    assert_eq!(
        *app.sink.0.lock().unwrap().last().unwrap(),
        FAST_RELOAD,
        "only this pack was re-added"
    );
    assert_eq!(app.read(&pack, "pack.json")["version"], "1.0.1");
    let creator = app.read(&pack, "creator.json");
    assert_eq!(
        creator["elements"]["ID_GH_LANGUAGE_GLOOMSPEAK"]["kind"],
        "language"
    );
    // the fast reload gives the content a full load gives (the cache it wrote is set aside, so this reads every pack)
    app.state.hub().unwrap().wait_for_cache_write();
    let cache = borealis_lib::content_cache::cache_file(app.data.path());
    assert!(cache.is_file(), "the fast reload wrote the content cache");
    std::fs::rename(&cache, cache.with_extension("set-aside")).unwrap();
    let full = ContentHub::load(&app.state.paths, &AppSettings::default()).unwrap();
    assert!(app.state.hub().unwrap().builder.same_content(&full.builder));
    // renaming keeps the id
    let open = app
        .state
        .homebrew_open_draft(&pack, Some("ID_GH_LANGUAGE_GLOOMSPEAK"), None)
        .unwrap();
    assert!(
        app.draft_files().is_empty(),
        "opening to look writes nothing"
    );
    let renamed = app.edit(&open, |f| {
        f.fields.insert("name".into(), json!("Hollowspeak"));
    });
    assert_eq!(app.save(&renamed), ["ID_GH_LANGUAGE_GLOOMSPEAK"]);
    let rows = app.state.homebrew_list_elements(&pack).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].name.as_str(), rows[0].kind.as_str()),
        ("Hollowspeak", "language")
    );
    assert_eq!(rows[0].problems.errors, 0);
}

#[test]
fn a_draft_survives_a_restart_and_shows_a_base_changed_outside_the_app() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    let open = app
        .state
        .homebrew_open_draft(&pack, Some(&id), None)
        .unwrap();
    let edited = app.edit(&open, |f| {
        f.fields.insert("name".into(), json!("Mid edit"));
    });
    assert!(!edited.base_changed);
    let app = app.restart();
    let rows = app.state.homebrew_list_elements(&pack).unwrap();
    assert!(rows[0].has_draft);
    let back = app
        .state
        .homebrew_open_draft(&pack, Some(&id), None)
        .unwrap();
    assert_eq!(back.draft_id, edited.draft_id, "resumed");
    assert_eq!(back.form.fields["name"], "Mid edit");
    // a never-saved element's draft is resumed by its draft id
    let new = app.named(&pack, "condition", "Gloomed");
    let app = app.restart();
    let again = app
        .state
        .homebrew_open_draft(&pack, None, Some(&new.draft_id))
        .unwrap();
    assert_eq!(again.form.fields["name"], "Gloomed");
    // someone edits content.json by hand: the open draft's base changed
    let path = app.pack_dir(&pack).join("content.json");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\"Gloomspeak\"", "\"Edited\"");
    std::fs::write(&path, text).unwrap();
    let changed = app
        .state
        .homebrew_open_draft(&pack, Some(&id), None)
        .unwrap();
    assert!(changed.base_changed);
    assert_eq!(
        app.state.homebrew_list_elements(&pack).unwrap()[0].name,
        "Edited",
        "the list reads the disk"
    );
    // "Use the saved version": discard, open again
    app.state.homebrew_discard_draft(&changed.draft_id).unwrap();
    let saved = app
        .state
        .homebrew_open_draft(&pack, Some(&id), None)
        .unwrap();
    assert_eq!(
        (saved.form.fields["name"].as_str(), saved.base_changed),
        (Some("Edited"), false)
    );
    assert_eq!(
        app.state
            .homebrew_discard_draft("hb-nothing")
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn a_save_to_a_pack_that_is_turned_off_reloads_everything_and_keeps_it_off() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    app.state.set_pack_enabled(&pack, false, &app.sink).unwrap();
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    assert_eq!(*app.sink.0.lock().unwrap().last().unwrap(), FULL_RELOAD);
    assert!(!app.loaded(&id), "the pack stays off");
    assert!(app.read(&pack, "content.json")["elements"][1]["id"] == id.as_str());
    app.state.set_pack_enabled(&pack, true, &app.sink).unwrap();
    assert!(app.loaded(&id));
}

#[test]
fn saves_with_problems_are_refused_and_change_nothing() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let before = std::fs::read_to_string(app.pack_dir(&pack).join("content.json")).unwrap();
    // no name: the form's own problem
    let unnamed = app
        .state
        .homebrew_new_draft(&pack, "language", None, None, None)
        .unwrap();
    let e = app
        .state
        .homebrew_save_draft(&unnamed.draft_id, &app.sink)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
    assert_eq!(e.details.as_ref().unwrap()[0]["path"], "fields.name");
    // a deity without its alignment, domains and symbol: the form's required fields, in plain words (the content
    // validator's missing setters are the same fields, told once)
    let deity = app.named(&pack, "deity", "Gloomfather");
    let e = app
        .state
        .homebrew_save_draft(&deity.draft_id, &app.sink)
        .unwrap_err();
    let details = e.details.unwrap();
    let told: Vec<(&str, &str, &str)> = details
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["code"].as_str().unwrap(),
                p["path"].as_str().unwrap(),
                p["message"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        told,
        vec![
            (
                "form.required",
                "fields.alignment",
                "A deity needs an alignment."
            ),
            ("form.required", "fields.domains", "A deity needs domains."),
            ("form.required", "fields.symbol", "A deity needs a symbol."),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(app.pack_dir(&pack).join("content.json")).unwrap(),
        before
    );
    assert_eq!(app.read(&pack, "pack.json")["version"], "1.0.0");
    assert!(
        app.draft_files()
            .contains(&format!("{}.json", deity.draft_id)),
        "the draft is kept"
    );
    assert!(!app.loaded("ID_GH_DEITY_GLOOMFATHER"));
}

#[test]
fn delete_scans_the_characters_retires_the_id_and_keeps_a_trash_copy() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    let kept = app.save(&app.named(&pack, "language", "Brightspeak"))[0].clone();
    let chars = app.state.characters_dir();
    std::fs::create_dir_all(&chars).unwrap();
    std::fs::write(
        chars.join("mira.json"),
        json!({"meta": {"name": "Mira"}, "build": {"choices": {"x": [id.clone()]}}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        chars.join("other.json"),
        json!({"meta": {"name": "Other"}}).to_string(),
    )
    .unwrap();
    let scan = app
        .state
        .homebrew_delete_elements(&pack, std::slice::from_ref(&id), false, &app.sink)
        .unwrap();
    assert!(!scan.deleted && scan.pack.is_none());
    assert_eq!(scan.used_by.len(), 1);
    assert_eq!(scan.used_by[0].name, "Mira");
    assert!(app.loaded(&id), "nothing deleted by a scan");
    let done = app
        .state
        .homebrew_delete_elements(&pack, std::slice::from_ref(&id), true, &app.sink)
        .unwrap();
    assert!(done.deleted);
    assert_eq!(done.pack.unwrap().version, "1.0.3");
    assert!(!app.loaded(&id) && app.loaded(&kept));
    let creator = app.read(&pack, "creator.json");
    assert_eq!(creator["retiredIds"], json!([id.clone()]));
    let trash: Vec<_> = std::fs::read_dir(app.pack_dir(&pack).join(".trash"))
        .unwrap()
        .collect();
    assert_eq!(trash.len(), 1);
    // a new element of the same name never gets the retired id back
    assert_eq!(
        app.save(&app.named(&pack, "language", "Gloomspeak")),
        [format!("{id}_2")]
    );
    // unknown ids and the pack's book are refused
    let e = app
        .state
        .homebrew_delete_elements(&pack, &["ID_NOPE".into()], true, &app.sink)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
    let e = app
        .state
        .homebrew_delete_elements(
            &pack,
            &["ID_GH_SOURCE_GLOOM_HOLLOW".into()],
            true,
            &app.sink,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
    // a delete without characters needs no confirmation
    let free = app
        .state
        .homebrew_delete_elements(&pack, std::slice::from_ref(&kept), false, &app.sink)
        .unwrap();
    assert!(free.deleted && free.used_by.is_empty());
}

#[test]
fn duplicates_and_copies_are_new_elements_with_their_provenance() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    let dup = app.state.homebrew_duplicate_element(&pack, &id).unwrap();
    assert_eq!(
        (dup.element_id.as_deref(), dup.form.fields["name"].as_str()),
        (None, Some("Gloomspeak (copy)"))
    );
    assert_eq!(app.save(&dup), ["ID_GH_LANGUAGE_GLOOMSPEAK_COPY"]);
    // a copy of a bundled SRD element (spec 11): new id, the provenance in creator.json, SRD text is shareable
    let hub = app.state.hub().unwrap();
    let srd = hub
        .store
        .by_type("Language")
        .find(|s| s.file.starts_with("srd-5.2.1/"))
        .map(|s| s.element.clone())
        .expect("an SRD language");
    drop(hub);
    let copy = app
        .state
        .homebrew_new_draft(&pack, "language", Some(&srd.id), None, None)
        .unwrap();
    assert_eq!(copy.form.fields["source"], "Gloom Hollow");
    let saved = app.save(&copy)[0].clone();
    assert!(saved.starts_with("ID_GH_LANGUAGE_"), "{saved}");
    let creator = app.read(&pack, "creator.json");
    let entry = &creator["elements"][&saved];
    assert_eq!(
        entry["copiedFrom"],
        json!({"id": srd.id, "packId": "srd-5.2.1", "srd": true})
    );
    assert_eq!(entry["nonSrdText"], false);
    let listed = app
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == pack)
        .unwrap();
    assert_eq!(listed.redistributable, None);
    // copying the pack's book, a child, or a deep copy are not offered
    assert_eq!(
        app.state
            .homebrew_new_draft(
                &pack,
                "language",
                Some("ID_GH_SOURCE_GLOOM_HOLLOW"),
                None,
                None
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        app.state
            .homebrew_new_draft(&pack, "language", None, Some(&id), None)
            .unwrap_err()
            .code,
        ErrorCode::RequirementFailed
    );
}

#[test]
fn pack_settings_rename_the_book_and_bump_the_version() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    let p = app
        .state
        .homebrew_update_pack(
            &pack,
            &HomebrewPackPatch {
                name: Some("Bright Hollow".into()),
                ..HomebrewPackPatch::default()
            },
            &app.sink,
        )
        .unwrap();
    assert_eq!(
        (p.name.as_str(), p.version.as_str()),
        ("Bright Hollow", "1.0.2")
    );
    let hub = app.state.hub().unwrap();
    assert_eq!(hub.store.element(&id).unwrap().source, "Bright Hollow");
    assert_eq!(
        hub.store.element("ID_GH_SOURCE_GLOOM_HOLLOW").unwrap().name,
        "Bright Hollow"
    );
    let e = app
        .state
        .homebrew_update_pack(
            &pack,
            &HomebrewPackPatch {
                name: Some(" ".into()),
                ..HomebrewPackPatch::default()
            },
            &app.sink,
        )
        .unwrap_err();
    assert_eq!(e.message, "Give the pack a name.");
}

#[test]
fn picker_rows_hold_loaded_elements_and_the_packs_unsaved_ones() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let spells = |q: &str, draft: Option<&str>| {
        app.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: vec!["Spell".into()],
                    tags: None,
                    multiple: None,
                },
                q,
                &HomebrewPickerFilters {
                    edition: Some(Ruleset::Dnd2024),
                    ..HomebrewPickerFilters::default()
                },
                HomebrewPageRequest {
                    offset: 0,
                    limit: 20,
                },
                draft,
            )
            .unwrap()
    };
    let page = spells("fireball", None);
    let fireball = page
        .items
        .iter()
        .find(|r| r.name == "Fireball")
        .expect("Fireball");
    assert_eq!(
        (fireball.level, fireball.school.as_deref()),
        (Some(3), Some("Evocation"))
    );
    assert!(fireball.spell.is_some() && !fireball.draft);
    let mut draft = app.named(&pack, "other", "Gloombolt");
    draft = app.edit(&draft, |f| {
        f.fields.insert("type".into(), json!("Spell"));
    });
    let page = spells("gloom", Some(&draft.draft_id));
    assert_eq!(page.total, 1);
    assert!(page.items[0].draft);
    assert_eq!(page.items[0].id, "ID_GH_SPELL_GLOOMBOLT");
}

#[test]
fn damaged_or_missing_creator_files_and_packs_edited_outside_the_app() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let id = app.save(&app.named(&pack, "language", "Gloomspeak"))[0].clone();
    let dir = app.pack_dir(&pack);
    // a damaged creator.json: the pack still opens (code from its book), the save sets the bad file aside
    std::fs::write(dir.join("creator.json"), "{ \"format\": \"borealis-creat").unwrap();
    assert_eq!(app.state.homebrew_list_elements(&pack).unwrap().len(), 1);
    let next = app.save(&app.named(&pack, "language", "Brightspeak"));
    assert_eq!(next, ["ID_GH_LANGUAGE_BRIGHTSPEAK"]);
    assert!(dir.join("creator.json.bad").is_file());
    assert_eq!(app.read(&pack, "creator.json")["code"], "GH");
    // an interrupted save left a temporary file and creator.json knows an element content.json doesn't hold
    std::fs::write(
        dir.join(".content.json.tmp"),
        "{ \"format\": \"borealis-content/1\", \"elem",
    )
    .unwrap();
    let mut creator = app.read(&pack, "creator.json");
    creator["elements"]["ID_GH_LANGUAGE_GHOST"] =
        json!({"kind": "language", "createdAt": "", "nonSrdText": false});
    std::fs::write(dir.join("creator.json"), creator.to_string()).unwrap();
    let app = app.restart();
    assert!(app.loaded(&id), "the pack still loads");
    assert_eq!(app.state.homebrew_list_elements(&pack).unwrap().len(), 2);
    app.save(&app.named(&pack, "language", "Thirdspeak"));
    let creator = app.read(&pack, "creator.json");
    assert!(
        creator["elements"].get("ID_GH_LANGUAGE_GHOST").is_none(),
        "reconciled"
    );
    // a content.json damaged outside the app: nothing is written, the draft is kept
    let pending = app.named(&pack, "language", "Lostspeak");
    std::fs::write(dir.join("content.json"), "{ not json").unwrap();
    let e = app
        .state
        .homebrew_save_draft(&pending.draft_id, &app.sink)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ContentError);
    assert_eq!(
        std::fs::read_to_string(dir.join("content.json")).unwrap(),
        "{ not json"
    );
    assert!(app
        .draft_files()
        .contains(&format!("{}.json", pending.draft_id)));
    // a damaged draft file is set aside, the others still list
    std::fs::write(app.drafts_dir().join("hb-broken.json"), "{ half").unwrap();
    let listed = app.state.homebrew_list_drafts().unwrap();
    assert!(listed.iter().any(|d| d.draft_id == pending.draft_id));
    assert!(listed.iter().all(|d| d.draft_id != "hb-broken"));
    assert!(app.drafts_dir().join("hb-broken.json.bad").is_file());
}

#[test]
fn a_pack_without_creator_json_is_adopted_and_other_packs_are_copy_only() {
    let app = App::new();
    // a homebrew pack made outside the creator (as `ai_add_homebrew_to_pack` makes them): no book, no creator.json
    let dir = app.pack_dir("homebrew");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        json!({"format": "borealis-pack/1", "id": "homebrew", "name": "My Homebrew", "version": "1.0.0",
               "origin": "custom", "rulesets": ["dnd2024"], "files": [{"path": "content.json", "kind": "content"}]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("content.json"),
        json!({"format": "borealis-content/1", "elements": [
            {"id": "ID_HB_LANGUAGE_OLD", "name": "Oldspeak", "type": "Language", "source": "My Homebrew"}]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("pack-meta.json"), r#"{"kind":"homebrew"}"#).unwrap();
    // an imported pack (an upstream would overwrite edits): copy-only (HC5)
    let imp = app.pack_dir("imported-book");
    std::fs::create_dir_all(&imp).unwrap();
    std::fs::write(
        imp.join("pack.json"),
        json!({"format": "borealis-pack/1", "id": "imported-book", "name": "Imported", "version": "1",
               "origin": "custom", "rulesets": ["dnd2014"], "files": []})
        .to_string(),
    )
    .unwrap();
    let app = app.restart();
    let rows = app.state.homebrew_list_elements("homebrew").unwrap();
    assert_eq!(
        (rows[0].name.as_str(), rows[0].kind.as_str()),
        ("Oldspeak", "language")
    );
    let saved = app.save(&app.named("homebrew", "language", "Newspeak"));
    assert_eq!(
        saved,
        ["ID_MH_LANGUAGE_NEWSPEAK"],
        "code from the pack name"
    );
    let creator = app.read("homebrew", "creator.json");
    assert_eq!(creator["code"], "MH");
    assert!(creator["elements"].get("ID_HB_LANGUAGE_OLD").is_some());
    assert_eq!(
        app.state
            .hub()
            .unwrap()
            .store
            .element("ID_MH_LANGUAGE_NEWSPEAK")
            .unwrap()
            .source,
        "My Homebrew"
    );
    let e = app
        .state
        .homebrew_list_elements("imported-book")
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::RequirementFailed);
    let e = app.state.homebrew_list_elements("srd-5.2.1").unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
}

/// Spec 14.1 / 18.5 budget: a save re-adds one pack instead of reading every pack. Measured here (debug build, the
/// bundled library of about 4,000 elements) and reported with `--nocapture`; the release-build budget over a
/// 20k-element library is WB-27's measurement.
#[test]
fn a_save_is_timed_against_a_full_load() {
    let app = App::new();
    let pack = app.create("Gloom Hollow", "GH");
    let draft = app.named(&pack, "language", "Gloomspeak");
    let t = std::time::Instant::now();
    app.save(&draft);
    let save = t.elapsed();
    assert_eq!(*app.sink.0.lock().unwrap().last().unwrap(), FAST_RELOAD);
    app.state.hub().unwrap().wait_for_cache_write();
    let cache = borealis_lib::content_cache::cache_file(app.data.path());
    let _ = std::fs::remove_file(&cache);
    let t = std::time::Instant::now();
    let _full = ContentHub::load(&app.state.paths, &AppSettings::default()).unwrap();
    let full = t.elapsed();
    println!("homebrew save with fast reload: {save:?}; full load without cache: {full:?}");
}
