//! B-222 (decision U17b): a character linked to a note of the Obsidian vault. The link is a vault-relative path in the
//! character file; a note created from the character is filled from it, never replaces a file, and is linked at once.
//! Links are refused outside the vault, to hidden places, to missing files and to files that are not notes.

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::{ApiError, ErrorCode};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use std::path::{Path, PathBuf};

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const HUMAN: &str = "ID_RACE_HUMAN";
const ACOLYTE: &str = "ID_BACKGROUND_ACOLYTE";

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    })
    .unwrap();
    (dir, state)
}

/// A vault folder outside the system temp folder (user folders may not live there), with one note and one image.
fn vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
    std::fs::write(dir.path().join(".obsidian/app.json"), "{}").unwrap();
    std::fs::write(dir.path().join("Home.md"), "# Home\n").unwrap();
    std::fs::write(dir.path().join("map.png"), [0x89, b'P', b'N', b'G']).unwrap();
    dir
}

fn open_vault(state: &AppState, dir: &Path) {
    let token = state
        .picked
        .grant(dir.to_path_buf(), PickPurpose::VaultDir)
        .unwrap()
        .token;
    state.set_vault_dir(Some(&token)).unwrap();
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

/// A level-2 human fighter acolyte named `name`.
fn hero(state: &AppState, name: &str) -> String {
    let request = NewCharacterRequest {
        name: name.into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = state.create_character(Ruleset::Dnd2014, &request).unwrap();
    let id = snap.id.clone();
    let s = state
        .apply_selection(&id, &slot_id(&snap, "Race"), HUMAN)
        .unwrap();
    let s = state
        .apply_selection(&id, &slot_id(&s, "Class"), FIGHTER)
        .unwrap();
    state
        .apply_selection(&id, &slot_id(&s, "Background"), ACOLYTE)
        .unwrap();
    state.level_up(&id, "main").unwrap();
    id
}

fn code<T: std::fmt::Debug>(r: Result<T, ApiError>) -> ErrorCode {
    r.expect_err("expected an error").code
}

#[test]
fn a_created_note_is_filled_from_the_character_and_linked() {
    let (_d, state) = app();
    let dir = vault();
    open_vault(&state, dir.path());
    let id = hero(&state, "Aria Vale");
    let snap = state.vault_create_character_note(&id).unwrap();
    assert_eq!(snap.vault_note.as_deref(), Some("Characters/Aria Vale.md"));
    assert!(snap.dirty, "the link is a change to save");
    let text = std::fs::read_to_string(dir.path().join("Characters/Aria Vale.md")).unwrap();
    assert_eq!(
        text,
        "---\nname: \"Aria Vale\"\nspecies: \"Human\"\nclasses:\n  - \"Fighter 2\"\nbackground: \"Acolyte\"\nlevel: 2\n\
         tags: [character]\n---\n\n## Backstory\n\n## Notes\n\n## Sessions\n"
    );
    // The vault sees it as a note tagged #character.
    let tags = state.vault_tags().unwrap();
    assert!(tags.iter().any(|t| t.tag == "character"), "{tags:?}");
}

#[test]
fn a_taken_name_is_numbered_from_two_and_nothing_is_replaced() {
    let (_d, state) = app();
    let dir = vault();
    std::fs::create_dir_all(dir.path().join("Characters")).unwrap();
    std::fs::write(dir.path().join("Characters/Brann.md"), "mine\n").unwrap();
    open_vault(&state, dir.path());
    let id = hero(&state, "Brann");
    let first = state.vault_create_character_note(&id).unwrap();
    assert_eq!(first.vault_note.as_deref(), Some("Characters/Brann 2.md"));
    let second = state.vault_create_character_note(&id).unwrap();
    assert_eq!(second.vault_note.as_deref(), Some("Characters/Brann 3.md"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Characters/Brann.md")).unwrap(),
        "mine\n"
    );
}

#[test]
fn a_name_with_link_characters_becomes_a_safe_file_name() {
    let (_d, state) = app();
    let dir = vault();
    open_vault(&state, dir.path());
    let id = hero(&state, "Ser [Bold]: the #1/2");
    let snap = state.vault_create_character_note(&id).unwrap();
    assert_eq!(
        snap.vault_note.as_deref(),
        Some("Characters/Ser -Bold-- the -1-2.md")
    );
}

#[test]
fn the_link_is_saved_in_the_character_file_and_read_back() {
    let (_d, state) = app();
    let dir = vault();
    open_vault(&state, dir.path());
    let id = hero(&state, "Cora");
    let snap = state.set_character_note(&id, Some("Home.md")).unwrap();
    assert_eq!(snap.vault_note.as_deref(), Some("Home.md"));
    state.save_character(&id).unwrap();
    state.close_character(&id, false).unwrap();
    let back = state.load_character(&id).unwrap();
    assert_eq!(back.vault_note.as_deref(), Some("Home.md"));
    let unlinked = state.set_character_note(&id, None).unwrap();
    assert_eq!(unlinked.vault_note, None);
}

#[test]
fn links_outside_the_vault_to_hidden_or_missing_files_and_to_non_notes_are_refused() {
    let (_d, state) = app();
    let dir = vault();
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::write(outside.path().join("secret.md"), "x").unwrap();
    let id = hero(&state, "Dax");
    // No vault yet.
    assert_eq!(
        code(state.set_character_note(&id, Some("Home.md"))),
        ErrorCode::InvalidArgument
    );
    open_vault(&state, dir.path());
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        dir.path().join("Escape.md"),
    )
    .unwrap();
    let escapes = [
        "../secret.md",
        "/etc/hosts",
        "Notes/../../secret.md",
        ".obsidian/app.json",
        "C:\\secret.md",
        "Escape.md",
    ];
    for path in escapes {
        if cfg!(not(unix)) && path == "Escape.md" {
            continue;
        }
        assert_eq!(
            code(state.set_character_note(&id, Some(path))),
            ErrorCode::InvalidArgument,
            "{path}"
        );
    }
    assert_eq!(
        code(state.set_character_note(&id, Some("Gone.md"))),
        ErrorCode::NotFound
    );
    assert_eq!(
        code(state.set_character_note(&id, Some("map.png"))),
        ErrorCode::InvalidArgument
    );
    assert_eq!(state.load_character(&id).unwrap().vault_note, None);
}

#[test]
fn creating_without_a_vault_or_with_an_unavailable_one_writes_nothing() {
    let (_d, state) = app();
    let id = hero(&state, "Eli");
    assert_eq!(
        code(state.vault_create_character_note(&id)),
        ErrorCode::InvalidArgument
    );
    let dir = vault();
    open_vault(&state, dir.path());
    let gone = dir.path().to_path_buf();
    drop(dir);
    assert!(!gone.exists());
    assert_eq!(
        code(state.vault_create_character_note(&id)),
        ErrorCode::InvalidArgument
    );
    assert_eq!(state.load_character(&id).unwrap().vault_note, None);
}
