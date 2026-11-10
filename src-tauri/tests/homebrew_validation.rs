//! WB-07 (homebrew creator validation, spec 8) through the real `AppState` on a temporary data folder with the bundled
//! content: the live problems of every autosave (plain sentences at form paths), Save refused on errors only, and the
//! engine checks of `homebrew_check_draft`, which never block.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest, HomebrewProblem};
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
    _data: tempfile::TempDir,
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
                    name: "Gloom Hollow".into(),
                    code: "GH".into(),
                    edition: Ruleset::Dnd2024,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id;
        Self {
            _data: data,
            state,
            pack,
        }
    }

    /// A new element of the generic form, of `ty`, named `name`, with `blocks` (as the UI sends them).
    fn draft(&self, ty: &str, name: &str, blocks: Value) -> HomebrewDraftView {
        self.tagged(ty, name, &[], blocks)
    }

    /// [`App::draft`] with tags.
    fn tagged(&self, ty: &str, name: &str, tags: &[&str], blocks: Value) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(&self.pack, "other", None, None, None)
            .unwrap();
        let mut form: FormState = v.form.clone();
        form.fields.insert("name".into(), json!(name));
        form.fields.insert("type".into(), json!(ty));
        form.fields.insert("tags".into(), json!(tags));
        form.blocks = serde_json::from_value(blocks).unwrap();
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }
}

/// The problems that count (info hints, such as the empty description, left out).
fn told(problems: &[HomebrewProblem]) -> Vec<(&str, &str, &str, &str)> {
    problems
        .iter()
        .filter(|p| p.severity != "info")
        .map(|p| {
            (
                p.severity.as_str(),
                p.code.as_str(),
                p.path.as_deref().unwrap_or(""),
                p.message.as_str(),
            )
        })
        .collect()
}

#[test]
fn every_autosave_tells_the_live_problems_in_plain_words_and_errors_stop_the_save() {
    let app = App::new();
    let v = app.draft(
        "Feat",
        "Howl",
        json!([{"key": "b1", "kind": "grant", "fields": {}}]),
    );
    assert_eq!(
        told(&v.problems),
        vec![(
            "error",
            "block.required",
            "blocks.0.fields.id",
            "Gives: fill in what it gives."
        )],
        "{:?}",
        v.problems
    );
    assert!(
        v.problems.iter().all(|p| p.element_id.is_none()),
        "not saved yet"
    );
    let e = app
        .state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap_err();
    assert_eq!(e.details.unwrap()[0]["code"], "block.required");
}

#[test]
fn warnings_do_not_stop_the_save() {
    let app = App::new();
    let v = app.draft(
        "Feat",
        "Howl",
        json!([{"key": "b1", "kind": "resource_cost", "fields": {"resource": "ember points", "cost": 1}}]),
    );
    assert_eq!(
        told(&v.problems),
        vec![(
            "warning",
            "lint.unknown_pool",
            "blocks.0.fields.resource",
            "Nothing defines the points \"ember points\" this costs. Add the pool, or pick one that exists."
        )]
    );
    let saved = app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    assert_eq!(saved.element_ids, vec!["ID_GH_FEAT_HOWL"]);
}

#[test]
fn the_engine_checks_tell_a_choice_with_nothing_to_offer_and_never_fail() {
    let app = App::new();
    let v = app.tagged(
        "Feat",
        "Odd Tongue",
        &["General Feat", "Feat"],
        json!([{"key": "b1", "kind": "select",
            "fields": {"type": "Language", "name": "Odd Tongue (Feat)", "supports": {"value": {"shape": "kept", "text": "Nobody Speaks This", "reason": "typed"}}}}]),
    );
    assert!(told(&v.problems).is_empty(), "{:?}", v.problems);
    let deep = app.state.homebrew_check_draft(&v.draft_id).unwrap();
    assert_eq!(
        told(&deep),
        vec![(
            "warning",
            "lint.select_empty",
            "blocks.0",
            "Nothing loaded matches what this choice offers. Check what it lets the player choose from."
        )],
        "{deep:?}"
    );
    // a draft that does not build has no engine problems: the live ones say why
    let broken = app.draft(
        "Feat",
        "Broken",
        json!([{"key": "b1", "kind": "grant", "fields": {}}]),
    );
    assert!(app
        .state
        .homebrew_check_draft(&broken.draft_id)
        .unwrap()
        .is_empty());
    assert!(app.state.homebrew_check_draft("no-such-draft").is_err());
}

#[test]
fn a_saved_element_that_lost_a_reference_names_it() {
    let app = App::new();
    let v = app.draft(
        "Feat",
        "Howl",
        json!([{"key": "b1", "kind": "grant",
            "fields": {"id": {"id": "ID_GONE_FEAT_SHIELD_MASTER", "name": "Shield Master", "type": "Feat"}}}]),
    );
    let gone: Vec<&HomebrewProblem> = v
        .problems
        .iter()
        .filter(|p| p.code == "dangling_reference")
        .collect();
    assert_eq!(gone.len(), 1, "{:?}", v.problems);
    assert_eq!(gone[0].severity, "warning");
    assert_eq!(gone[0].path.as_deref(), Some("blocks.0"));
    assert_eq!(
        gone[0].message,
        "\"Shield Master\" isn't loaded any more (its pack was removed or turned off). Pick it again."
    );
}

#[test]
fn a_feat_no_feat_choice_offers_is_told_by_the_test_character() {
    let app = App::new();
    let v = app.draft("Feat", "Loner", json!([]));
    let deep = app.state.homebrew_check_draft(&v.draft_id).unwrap();
    assert_eq!(
        told(&deep),
        vec![(
            "warning",
            "engine.refused",
            "",
            "A test character couldn't take this feat: no feat choice of a level 4 fighter offers it (check its tags and prerequisite)."
        )]
    );
}
