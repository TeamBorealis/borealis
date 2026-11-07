//! GitHub #32: the portrait library folder (Settings > CONTENT) and the gallery's library images. Token-only
//! folders, the unavailable folder (kept, like B-188), image reads, and settings files from before the key.

use borealis_lib::content_packs::NullSink;
use borealis_lib::error::ErrorCode;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::settings::AppSettings;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::{Path, PathBuf};

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = AppState::new(paths(dir.path())).expect("state");
    (dir, state)
}

/// A folder outside the system temp dir (which user folders may not live in), with two images and a text file.
fn library() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("library dir");
    std::fs::write(dir.path().join("elf-female.png"), PNG).unwrap();
    std::fs::write(dir.path().join("readme.txt"), b"not an image").unwrap();
    std::fs::create_dir(dir.path().join("Dwarves")).unwrap();
    std::fs::write(dir.path().join("Dwarves/shield.png"), PNG).unwrap();
    dir
}

fn grant(app: &AppState, path: &Path, purpose: PickPurpose) -> String {
    app.picked.grant(path.to_path_buf(), purpose).unwrap().token
}

fn code<T: std::fmt::Debug>(r: Result<T, borealis_lib::error::ApiError>) -> ErrorCode {
    r.expect_err("should fail").code
}

fn linked(lib: &Path) -> (tempfile::TempDir, AppState) {
    let (d, app) = app();
    let token = grant(&app, lib, PickPurpose::PortraitLibraryDir);
    app.set_portrait_library_dir(Some(&token)).unwrap();
    (d, app)
}

#[test]
fn no_library_by_default_and_old_settings_files_load_without_the_key() {
    let (d, app) = app();
    assert_eq!(app.settings().unwrap().portrait_library_dir, None);
    let listing = app.portrait_library().unwrap();
    assert_eq!(listing.dir, None);
    assert!(!listing.unavailable);
    assert!(listing.images.is_empty());
    assert!(!app.data_dirs().unwrap().portrait_library_unavailable);
    assert_eq!(
        code(app.portrait_library_image("a.png")),
        ErrorCode::InvalidArgument
    );

    let file = d.path().join("old-settings.json");
    std::fs::write(&file, r#"{"playerName":"Old","portraitSize":175}"#).unwrap();
    let old = AppSettings::load(&file);
    assert_eq!(old.player_name, "Old");
    assert_eq!(old.portrait_library_dir, None);
}

#[test]
fn the_setting_round_trips_through_the_settings_file() {
    let lib = library();
    let (d, app) = linked(lib.path());
    let chosen = app.settings().unwrap().portrait_library_dir.expect("set");
    let canonical = std::fs::canonicalize(lib.path()).unwrap();
    assert_eq!(PathBuf::from(&chosen), canonical);
    assert_eq!(
        app.data_dirs().unwrap().portrait_library_dir.as_deref(),
        Some(chosen.as_str())
    );

    let text = std::fs::read_to_string(d.path().join("settings.json")).unwrap();
    let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(raw["portraitLibraryDir"], json!(chosen));
    drop(app);
    let again = AppState::new(paths(d.path())).unwrap();
    assert_eq!(
        again.settings().unwrap().portrait_library_dir,
        Some(chosen.clone())
    );
    // A full save from the settings window resends it unchanged.
    let full = serde_json::to_value(again.settings().unwrap()).unwrap();
    again.save_settings(&full).unwrap();
    assert_eq!(again.settings().unwrap().portrait_library_dir, Some(chosen));
}

#[test]
fn the_folder_needs_a_portrait_library_token() {
    let lib = library();
    let (_d, app) = app();
    let raw = lib.path().to_string_lossy().into_owned();
    assert_eq!(
        code(app.set_portrait_library_dir(Some(&raw))),
        ErrorCode::InvalidArgument
    );
    let wrong = grant(&app, lib.path(), PickPurpose::SecondaryContentDir);
    assert_eq!(
        code(app.set_portrait_library_dir(Some(&wrong))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.save_settings(&json!({ "portraitLibraryDir": raw }))),
        ErrorCode::InvalidArgument
    );
    let system = grant(&app, Path::new("/usr"), PickPurpose::PortraitLibraryDir);
    assert_eq!(
        code(app.set_portrait_library_dir(Some(&system))),
        ErrorCode::InvalidArgument
    );
    let temp = tempfile::tempdir().unwrap();
    let in_temp = grant(&app, temp.path(), PickPurpose::PortraitLibraryDir);
    assert_eq!(
        code(app.set_portrait_library_dir(Some(&in_temp))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(app.settings().unwrap().portrait_library_dir, None);
}

#[test]
fn the_gallery_lists_the_library_and_reads_one_image_at_a_time() {
    let lib = library();
    let (_d, app) = linked(lib.path());
    let listing = app.portrait_library().unwrap();
    assert!(!listing.unavailable);
    let ids: Vec<_> = listing.images.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["elf-female.png", "Dwarves/shield.png"]);
    let url = app.portrait_library_image("Dwarves/shield.png").unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
    assert_eq!(
        code(app.portrait_library_image("readme.txt")),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.portrait_library_image("../settings.json")),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn clearing_the_folder_turns_the_library_off() {
    let lib = library();
    let (_d, app) = linked(lib.path());
    let dirs = app.set_portrait_library_dir(None).unwrap();
    assert_eq!(dirs.portrait_library_dir, None);
    assert!(app.portrait_library().unwrap().images.is_empty());
}

#[test]
fn an_unavailable_folder_is_reported_and_kept() {
    let lib = library();
    let (d, app) = linked(lib.path());
    let chosen = app.settings().unwrap().portrait_library_dir.unwrap();
    drop(app);
    let parked = lib.path().with_extension("unplugged");
    std::fs::rename(lib.path(), &parked).unwrap();

    let app = AppState::new(paths(d.path())).unwrap();
    let listing = app.portrait_library().unwrap();
    assert!(listing.unavailable);
    assert!(listing.images.is_empty());
    assert_eq!(listing.dir.as_deref(), Some(chosen.as_str()));
    assert!(app.data_dirs().unwrap().portrait_library_unavailable);
    assert_eq!(
        code(app.portrait_library_image("elf-female.png")),
        ErrorCode::InvalidArgument
    );
    // Saving every other setting keeps it.
    let full = serde_json::to_value(app.settings().unwrap()).unwrap();
    app.save_settings(&full).unwrap();
    assert_eq!(
        app.settings().unwrap().portrait_library_dir,
        Some(chosen.clone())
    );

    std::fs::rename(&parked, lib.path()).unwrap();
    let back = app.portrait_library().unwrap();
    assert!(!back.unavailable);
    assert_eq!(back.images.len(), 2);
}

#[test]
fn restore_defaults_clears_the_library_like_the_other_folders() {
    let lib = library();
    let (_d, app) = linked(lib.path());
    let restored = app.restore_default_settings(&NullSink).unwrap();
    assert_eq!(restored.portrait_library_dir, None);
    assert!(!app.data_dirs().unwrap().portrait_library_unavailable);
}
