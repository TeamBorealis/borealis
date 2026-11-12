//! H8mmg (HC11): before its first save, a deep copy's live preview, its DATA view and the deep check write it as a save
//! does (`write_copy`): the copy grants its own copied features, not the originals (P2b fixed save and Try it).

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const CHAMPION: &str = "ID_2024_SUBCLASS_FIGHTER_CHAMPION";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    _data: tempfile::TempDir,
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
        Self { _data: data, state }
    }

    fn pack(&self) -> String {
        let request = HomebrewPackRequest {
            name: "Gloom Hollow".into(),
            code: "GH".into(),
            edition: Ruleset::Dnd2024,
            author: None,
            description: None,
            license: None,
        };
        self.state
            .homebrew_create_pack(&request, &Quiet)
            .unwrap()
            .id
    }
}

/// The child form named `name`.
fn child_named<'f>(form: &'f mut FormState, name: &str) -> &'f mut FormState {
    form.children
        .iter_mut()
        .find(|c| c.fields.get("name") == Some(&json!(name)))
        .unwrap_or_else(|| panic!("a child named {name}"))
}

/// Every element id in the text that ends with `suffix`.
fn ids_ending(text: &str, suffix: &str) -> Vec<String> {
    text.split('"')
        .filter(|t| t.starts_with("ID_") && t.ends_with(suffix))
        .map(str::to_string)
        .collect()
}

#[test]
fn an_unsaved_deep_copy_s_preview_data_and_deep_check_name_its_own_features() {
    let app = App::new();
    let pack = app.pack();
    let copy: HomebrewDraftView = app
        .state
        .homebrew_new_draft(&pack, "other", Some(CHAMPION), None, Some(true))
        .unwrap();
    let mut form = copy.form.clone();
    // the copied level-3 feature, renamed, gets a choice that offers nothing
    let athlete = child_named(&mut form, "Remarkable Athlete");
    athlete
        .fields
        .insert("name".into(), json!("Moonlit Athlete"));
    athlete.blocks.push(
        serde_json::from_value(json!({"key": "b-new", "kind": "select",
            "fields": {"type": "Proficiency", "name": "Moonlit Lore", "level": 3, "number": 1,
                "supports": {"value": {"shape": "kept", "text": "ID_GH_NO_SUCH_SUPPORT", "reason": "test"}}}}))
        .unwrap(),
    );
    let view = app
        .state
        .homebrew_update_draft(&copy.draft_id, &form)
        .unwrap();
    assert!(view.element_id.is_none(), "never saved");

    // the live preview grants the copy's feature under its new name
    let grants: Vec<&str> = view
        .preview
        .as_ref()
        .expect("a preview")
        .grants
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert!(grants.contains(&"Moonlit Athlete"), "{grants:?}");
    assert!(!grants.contains(&"Remarkable Athlete"), "{grants:?}");

    // the DATA view names the copy's (renamed) feature, never the original
    let json = &view.generated.json;
    assert_eq!(
        ids_ending(json, "_REMARKABLE_ATHLETE"),
        Vec::<String>::new(),
        "the original's id: {json}"
    );
    let athletes = ids_ending(json, "_MOONLIT_ATHLETE");
    assert!(
        athletes.len() > 1 && athletes.iter().all(|id| id.starts_with("ID_GH_")),
        "the copy's feature and the grant naming it: {athletes:?}"
    );

    // the deep check builds a character with the copy's feature, so it finds the choice that offers nothing
    let problems = app.state.homebrew_check_draft(&view.draft_id).unwrap();
    assert!(
        problems.iter().any(|p| p.message.contains("Moonlit Lore")),
        "{:?}",
        problems.iter().map(|p| &p.message).collect::<Vec<_>>()
    );
}
