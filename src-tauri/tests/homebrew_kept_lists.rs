//! P3 INW5c (homebrew creator, 1.5.0, HC11): unsaved SPELL LISTS checks kept when the workspace is left are kept on the
//! device, like drafts (spec 10.1, HC7), so a restart does not lose them. Through the real `AppState`; a "restart" is a
//! new `AppState` over the same data folder.

use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::{HomebrewKeptCheck, HomebrewKeptChecks, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use std::path::{Path, PathBuf};

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn state(data: &Path) -> AppState {
    AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    })
    .unwrap()
}

fn pack(s: &AppState) -> String {
    s.homebrew_create_pack(
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
    .id
}

fn check(id: &str, on: bool) -> HomebrewKeptCheck {
    HomebrewKeptCheck { id: id.into(), on }
}

#[test]
fn kept_spell_list_checks_survive_a_restart_and_are_forgotten_when_emptied() {
    // Arrange
    let data = tempfile::tempdir().unwrap();
    let first = state(data.path());
    let pack = pack(&first);
    let kept = HomebrewKeptChecks {
        list: "Wizard".into(),
        pending: vec![
            check("ID_2024_SPELL_FIREBALL", true),
            check("ID_EI_SPELL_EMBER", false),
        ],
    };

    // Act
    first.homebrew_keep_spell_list_checks(&pack, &kept).unwrap();
    drop(first);
    let again = state(data.path());

    // Assert: the new run reads them back; drafts' folder, so sync never carries them
    assert_eq!(
        again.homebrew_kept_spell_list_checks(&pack).unwrap(),
        Some(kept)
    );
    assert!(data
        .path()
        .join("homebrew-drafts")
        .join("spell-lists")
        .join(format!("{pack}.json"))
        .is_file());
    // nothing pending: forgotten, the file gone
    again
        .homebrew_keep_spell_list_checks(
            &pack,
            &HomebrewKeptChecks {
                list: "Wizard".into(),
                pending: Vec::new(),
            },
        )
        .unwrap();
    assert_eq!(again.homebrew_kept_spell_list_checks(&pack).unwrap(), None);
    // drafts are listed from the same folder: the kept checks are no draft
    assert!(again.homebrew_list_drafts().unwrap().is_empty());
}

#[test]
fn kept_checks_need_a_file_safe_pack_id() {
    let data = tempfile::tempdir().unwrap();
    let s = state(data.path());
    let kept = HomebrewKeptChecks {
        list: "Wizard".into(),
        pending: vec![check("ID_X", true)],
    };
    assert_eq!(
        s.homebrew_keep_spell_list_checks("../evil", &kept)
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        s.homebrew_kept_spell_list_checks("../evil")
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
}
