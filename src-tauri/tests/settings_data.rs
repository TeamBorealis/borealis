//! Settings > DATA FILES and the general settings flow: token-only folders, folder validation,
//! secondary content, automatic content updates (fake index, no network), Restore Defaults and
//! clearing caches.

use borealis_lib::content_auto::{spawn_startup_task, MSG_UPDATED};
use borealis_lib::content_dto::PackKind;
use borealis_lib::content_index::{sha256_hex, Fetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::error::ErrorCode;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::settings::AppSettings;
use borealis_lib::settings_dto::ContentUpdateState;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Side Book</name><update version="1.0.0"><file name="side-book.xml" url="https://example.invalid/side-book.xml"/></update></info>
  <element name="Side Gift" type="Feat" source="Side Book" id="ID_SIDE_FEAT_GIFT">
    <description><p>Test.</p></description>
    <sheet display="true"><description>Test.</description></sheet>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#;

const INDEX_URL: &str = "https://example.invalid/side.index.json";
const GOOD_URL: &str = "https://example.invalid/good.xml";
const BAD_URL: &str = "https://example.invalid/bad.xml";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// A folder outside the system temp dir (which user folders may not live in).
fn user_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("user dir")
}

fn grant(app: &AppState, path: &Path, purpose: PickPurpose) -> String {
    app.picked.grant(path.to_path_buf(), purpose).unwrap().token
}

fn code<T: std::fmt::Debug>(r: Result<T, borealis_lib::error::ApiError>) -> ErrorCode {
    r.expect_err("should fail").code
}

// ---- documents directory ----

#[test]
fn documents_dir_needs_a_documents_token() {
    let (_d, app) = app();
    let dir = user_dir();
    let raw = dir.path().to_string_lossy().into_owned();
    assert_eq!(
        code(app.set_documents_dir(Some(&raw))),
        ErrorCode::InvalidArgument
    );
    let wrong = grant(&app, dir.path(), PickPurpose::FivetoolsFolder);
    assert_eq!(
        code(app.set_documents_dir(Some(&wrong))),
        ErrorCode::InvalidArgument
    );
    assert!(app.data_dirs().unwrap().is_default);
    // A raw path can never arrive through save_settings either.
    let patch = json!({ "documentsDir": raw });
    assert_eq!(code(app.save_settings(&patch)), ErrorCode::InvalidArgument);
}

#[test]
fn documents_dir_moves_new_saves_but_not_existing_files() {
    let (_d, app) = app();
    let req = |name: &str| NewCharacterRequest {
        name: name.into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::StandardArray,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let old = app.create_character(Ruleset::Dnd2014, &req("Old")).unwrap();
    let dir = user_dir();
    let token = grant(&app, dir.path(), PickPurpose::DocumentsDir);
    let dirs = app.set_documents_dir(Some(&token)).unwrap();
    assert!(!dirs.is_default);
    let chars = dir.path().join("characters");
    assert_eq!(
        Path::new(&dirs.characters_dir),
        chars.canonicalize().unwrap()
    );
    // Tokens are single-use.
    assert!(app.set_documents_dir(Some(&token)).is_err());
    let new = app.create_character(Ruleset::Dnd2014, &req("New")).unwrap();
    assert!(chars.join(format!("{}.json", new.id)).is_file());
    assert!(!chars.join(format!("{}.json", old.id)).exists());
    let listed = app.list_characters().unwrap();
    assert_eq!(listed.len(), 1);
    // The unchanged value may be resent with a full save.
    let mut full = serde_json::to_value(app.get_settings().unwrap()).unwrap();
    full["playerName"] = json!("Ada");
    assert_eq!(app.save_settings(&full).unwrap().player_name, "Ada");
    // Back to the default folder.
    assert!(app.set_documents_dir(None).unwrap().is_default);
    assert_eq!(app.list_characters().unwrap().len(), 1);
}

#[test]
fn unusable_documents_dirs_are_rejected() {
    let (d, app) = app();
    let try_dir =
        |p: &Path| code(app.set_documents_dir(Some(&grant(&app, p, PickPurpose::DocumentsDir))));
    assert_eq!(try_dir(Path::new("/")), ErrorCode::InvalidArgument);
    #[cfg(unix)]
    assert_eq!(try_dir(Path::new("/usr")), ErrorCode::InvalidArgument);
    assert_eq!(try_dir(&std::env::temp_dir()), ErrorCode::InvalidArgument);
    let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content/core");
    assert_eq!(try_dir(&content), ErrorCode::InvalidArgument);
    assert_eq!(
        try_dir(&d.path().join("library")),
        ErrorCode::InvalidArgument
    );
    let user = user_dir();
    assert_eq!(
        try_dir(&user.path().join("missing")),
        ErrorCode::InvalidArgument
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = user.path().join("read-only");
        std::fs::create_dir(&ro).unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        assert_eq!(try_dir(&ro), ErrorCode::InvalidArgument);
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(app.data_dirs().unwrap().is_default);
}

// ---- secondary content directory ----

#[test]
fn secondary_dir_loads_files_as_user_packs_and_unlink_removes_them() {
    let (d, app) = app();
    let dir = user_dir();
    std::fs::write(dir.path().join("side.xml"), XML).unwrap();
    // Named like a bundled pack: installed under a `sec-` id, never shadowing `core`.
    std::fs::write(dir.path().join("core.xml"), XML.replace("SIDE", "CORESIDE")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
    std::fs::write(dir.path().join("broken.json"), "{ nope").unwrap();

    let raw = dir.path().to_string_lossy().into_owned();
    assert!(app.set_secondary_content_dir(&raw, &NullSink).is_err());
    let inside_data = grant(&app, d.path(), PickPurpose::SecondaryContentDir);
    assert!(app
        .set_secondary_content_dir(&inside_data, &NullSink)
        .is_err());

    let token = grant(&app, dir.path(), PickPurpose::SecondaryContentDir);
    let r = app.set_secondary_content_dir(&token, &NullSink).unwrap();
    assert_eq!(r.report.installed, vec!["sec-core", "sec-side"]);
    assert_eq!(r.report.skipped.len(), 1, "{:?}", r.report.skipped);
    assert!(r.dirs.secondary_content_dir.is_some());
    let packs = app.list_packs().unwrap();
    let core = packs.iter().find(|p| p.id == "core").unwrap();
    assert_eq!(core.kind, PackKind::Bundled);
    let side = packs.iter().find(|p| p.id == "sec-side").unwrap();
    assert_eq!(side.kind, PackKind::Imported);
    assert!(app.hub().unwrap().store.get("ID_SIDE_FEAT_GIFT").is_some());

    // Unchanged files are not reinstalled; a removed file drops its pack.
    std::fs::remove_file(dir.path().join("core.xml")).unwrap();
    let again = app.sync_secondary_content(&NullSink).unwrap();
    assert!(again.installed.is_empty());
    assert_eq!(again.removed, vec!["sec-core"]);

    let r = app.unlink_secondary_content_dir(&NullSink).unwrap();
    assert_eq!(r.report.removed, vec!["sec-side"]);
    assert!(r.dirs.secondary_content_dir.is_none());
    assert!(app.hub().unwrap().store.get("ID_SIDE_FEAT_GIFT").is_none());
}

#[test]
fn secondary_scan_skips_the_app_data_folder_below_a_linked_parent() {
    let parent = user_dir();
    let data = parent.path().join("app-data");
    std::fs::create_dir(&data).unwrap();
    let app = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.clone(),
    })
    .unwrap();
    std::fs::write(data.join("library").join("planted.xml"), XML).unwrap();
    let token = grant(&app, parent.path(), PickPurpose::SecondaryContentDir);
    let r = app.set_secondary_content_dir(&token, &NullSink).unwrap();
    assert!(r.report.installed.is_empty(), "{:?}", r.report.installed);
}

// ---- automatic content updates ----

struct Stub {
    files: HashMap<String, Vec<u8>>,
    calls: AtomicUsize,
    delay: Duration,
}

impl Fetcher for Stub {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| "not found".into())
    }
}

/// An index listing one file with the right hash and one whose hash does not match.
fn stub(delay: Duration) -> Stub {
    let good = XML.as_bytes().to_vec();
    let bad = XML.replace("SIDE", "EVIL").into_bytes();
    let index = serde_json::to_vec(&json!({
        "format": "aurora-index/1",
        "info": { "name": "Side Index" },
        "files": [
            { "name": "good.xml", "url": GOOD_URL, "extra": { "attributes": { "sha256": sha256_hex(&good) } } },
            { "name": "bad.xml", "url": BAD_URL, "extra": { "attributes": { "sha256": sha256_hex(b"other") } } }
        ]
    }))
    .unwrap();
    Stub {
        files: HashMap::from([
            (INDEX_URL.to_string(), index),
            (GOOD_URL.to_string(), good),
            (BAD_URL.to_string(), bad),
        ]),
        calls: AtomicUsize::new(0),
        delay,
    }
}

#[test]
fn auto_update_installs_only_verified_files() {
    let (_d, app) = app();
    assert!(app.get_settings().unwrap().auto_content_update);
    app.add_index(INDEX_URL).unwrap();
    let status = app.run_auto_content_update(&stub(Duration::ZERO), &NullSink);
    assert_eq!(status.state, ContentUpdateState::Done);
    assert_eq!(status.updated, 1);
    assert_eq!(status.failed, vec![BAD_URL.to_string()]);
    assert!(status.message.starts_with(MSG_UPDATED));
    let ids: Vec<String> = app
        .list_packs()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert!(ids.iter().any(|id| id.contains("good")));
    assert!(!ids.iter().any(|id| id.contains("bad")));
    assert_eq!(app.content_update_status().unwrap(), status);
}

#[test]
fn auto_update_off_or_without_indexes_fetches_nothing() {
    let (_d, app) = app();
    let s = stub(Duration::ZERO);
    assert_eq!(
        app.run_auto_content_update(&s, &NullSink).state,
        ContentUpdateState::Done
    );
    assert_eq!(s.calls.load(Ordering::SeqCst), 0);
    app.add_index(INDEX_URL).unwrap();
    app.save_settings(&json!({ "autoContentUpdate": false }))
        .unwrap();
    assert_eq!(
        app.run_auto_content_update(&s, &NullSink).state,
        ContentUpdateState::Disabled
    );
    assert_eq!(s.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn startup_update_never_blocks_startup() {
    let (_d, app) = app();
    let app = Arc::new(app);
    app.add_index(INDEX_URL).unwrap();
    app.mark_content_update_pending();
    let started = Instant::now();
    let worker = Arc::clone(&app);
    let handle = spawn_startup_task(move || {
        worker.startup_content_tasks(
            &stub(Duration::from_millis(100)),
            &NullSink,
            Duration::from_millis(200),
        );
    });
    assert!(
        started.elapsed() < Duration::from_millis(150),
        "spawn waited for the update"
    );
    assert_eq!(
        app.content_update_status().unwrap().state,
        ContentUpdateState::Pending
    );
    // The app stays usable while the update waits and runs.
    assert!(app.get_settings().is_ok());
    handle.join().unwrap();
    let done = app.content_update_status().unwrap();
    assert_eq!((done.state, done.updated), (ContentUpdateState::Done, 1));
}

// ---- settings flow ----

#[test]
fn restore_defaults_resets_general_rows_and_keeps_borealis_ones() {
    let (_d, app) = app();
    app.save_settings(&json!({
        "playerName": "Ada", "rowHeight": "large", "accentColor": "red", "portraitSize": 175,
        "autoContentUpdate": false, "showImportNotice": false, "autosave": false, "ai": { "provider": "codex" },
        "sheet": { "legacySpellcastingPage": true, "cardsNewPage": { "attack": true } }
    }))
    .unwrap();
    let docs = user_dir();
    app.set_documents_dir(Some(&grant(&app, docs.path(), PickPurpose::DocumentsDir)))
        .unwrap();
    let side = user_dir();
    std::fs::write(side.path().join("side.xml"), XML).unwrap();
    app.set_secondary_content_dir(
        &grant(&app, side.path(), PickPurpose::SecondaryContentDir),
        &NullSink,
    )
    .unwrap();

    let s = app.restore_default_settings(&NullSink).unwrap();
    let d = AppSettings::default();
    assert_eq!(
        (
            s.player_name.as_str(),
            s.row_height.as_str(),
            s.accent_color.as_str()
        ),
        ("Player One", "medium", "default")
    );
    assert_eq!((s.portrait_size, s.auto_content_update), (150, true));
    assert!(
        s.show_import_notice,
        "restoring defaults brings the import notice back"
    );
    assert_eq!(s.sheet, d.sheet);
    assert!(s.documents_dir.is_none() && s.secondary_content_dir.is_none());
    assert!(!s.autosave);
    assert_eq!(s.ai.provider, "codex");
    assert!(app.data_dirs().unwrap().is_default);
    assert!(app.hub().unwrap().store.get("ID_SIDE_FEAT_GIFT").is_none());
    // Saved at once.
    assert_eq!(AppSettings::load(&app.paths.settings_file()), s);
}

#[test]
fn new_settings_fields_validate_and_old_files_load() {
    let (_d, app) = app();
    for bad in [
        json!({ "rowHeight": "huge" }),
        json!({ "accentColor": "teal" }),
        json!({ "portraitSize": 200 }),
    ] {
        assert_eq!(code(app.save_settings(&bad)), ErrorCode::InvalidArgument);
    }
    let old: AppSettings =
        serde_json::from_str(r#"{"playerName":"Ada","sheet":{"formFillable":true}}"#).unwrap();
    assert_eq!(
        (old.portrait_size, old.row_height.as_str()),
        (150, "medium")
    );
    assert!(old.auto_content_update && old.skills_panel_expanded && !old.saving_throws_expanded);
    // An explicit formFillable=true is respected; only the default changed.
    assert!(old.sheet.form_fillable);
    assert!(!AppSettings::default().sheet.form_fillable);
    let unset: AppSettings = serde_json::from_str(r#"{"sheet":{}}"#).unwrap();
    assert!(!unset.sheet.form_fillable);
    assert!(old.sheet.cards_new_page.item && !old.sheet.cards_new_page.attack);
}

#[test]
fn clear_caches_resets_index_results_but_keeps_trust() {
    static SWEEPS: AtomicUsize = AtomicUsize::new(0);
    fn fake_sweep() -> usize {
        SWEEPS.fetch_add(1, Ordering::SeqCst);
        2
    }
    let (_d, app) = app();
    app.add_index(INDEX_URL).unwrap();
    app.update_indexes(&stub(Duration::ZERO), &NullSink)
        .unwrap();
    assert!(app.list_indexes()[0].last_checked.is_some());
    let r = app.clear_caches(fake_sweep).unwrap();
    assert_eq!((r.indexes_reset, r.ai_temp_folders_removed), (1, 2));
    assert_eq!(SWEEPS.load(Ordering::SeqCst), 1);
    let idx = &app.list_indexes()[0];
    assert!(idx.last_checked.is_none());
    assert_eq!(idx.url, INDEX_URL);
    // Installed content is untouched.
    assert!(app
        .list_packs()
        .unwrap()
        .iter()
        .any(|p| p.id.contains("good")));
}

#[test]
fn app_folder_enum_maps_to_app_owned_dirs() {
    use borealis_lib::commands_system::AppFolder;
    let (dir, app) = app();
    let data = dir.path();
    assert_eq!(
        app.app_folder(AppFolder::Content),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
    );
    assert_eq!(app.app_folder(AppFolder::UserContent), data.join("library"));
    assert_eq!(
        app.app_folder(AppFolder::Characters),
        data.join("characters")
    );
    assert_eq!(app.app_folder(AppFolder::Logs), data.join("logs"));
}

#[test]
fn app_folder_enum_rejects_unknown_names() {
    use borealis_lib::commands_system::AppFolder;
    assert!(serde_json::from_str::<AppFolder>("\"user-content\"").is_ok());
    assert!(serde_json::from_str::<AppFolder>("\"logs\"").is_ok());
    assert!(serde_json::from_str::<AppFolder>("\"/etc\"").is_err());
}
