//! P3 bug (homebrew creator, 1.5.0, HC11) through the real `AppState`: deleting a 2024 class's Spellcasting feature
//! (instead of turning "Gives spellcasting" off) takes its spellcasting columns off the class table, as turning it off
//! does. The engine reads `progression.prepared` and the pact columns, so columns left behind kept the class preparing.

use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const CLASS_ID: &str = "ID_EI_CLASS_WARDEN";
const PREPARED: [u32; 20] = [
    2, 3, 4, 5, 6, 6, 7, 7, 9, 9, 10, 10, 11, 11, 12, 12, 14, 14, 15, 15,
];

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn app() -> (tempfile::TempDir, AppState, String) {
    let data = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
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
    (data, state, pack)
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn saved(data: &tempfile::TempDir, pack: &str, id: &str) -> Option<Value> {
    let path = data.path().join("library").join(pack).join("content.json");
    let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    content["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
}

/// The class's spellcasting columns (`cantrips`, `prepared`, `slot_N`, `pact_*`).
fn casting_columns(class: &Value) -> Vec<String> {
    class["data"]["progression"]
        .as_object()
        .map(|p| {
            p.keys()
                .filter(|k| {
                    ["cantrips", "prepared"].contains(&k.as_str())
                        || k.starts_with("slot_")
                        || k.starts_with("pact_")
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// A saved 2024 half caster "Warden" whose Spellcasting feature is made inside it.
fn warden(state: &AppState, pack: &str) {
    let v = state
        .homebrew_new_draft(pack, "class", None, None, None)
        .unwrap();
    let v = state
        .homebrew_new_draft(pack, "classFeature", None, Some(&v.draft_id), None)
        .unwrap();
    let key = v.form.children.last().unwrap().key.clone();
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Warden"));
    form.fields.insert("hitDie".into(), json!("d10"));
    form.fields.insert(
        "skills".into(),
        json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_STEALTH", "name": "", "type": "Proficiency"}]}),
    );
    let sc = form.children.iter_mut().find(|c| c.key == key).unwrap();
    for (k, x) in [
        ("name", json!("Spellcasting")),
        ("casts", json!(true)),
        ("casterType", json!("halfUp")),
        ("ability", json!("Wisdom")),
        ("preparation", json!("prepared")),
        ("spellList", json!("Ranger")),
        ("prepared", json!(PREPARED)),
    ] {
        sc.fields.insert(k.into(), x);
    }
    let v = state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
}

#[test]
fn deleting_a_2024_class_s_spellcasting_feature_takes_its_columns_off_the_class_table() {
    // Arrange: the class saved with its Spellcasting feature and the columns it writes
    let (data, state, pack) = app();
    warden(&state, &pack);
    let class = saved(&data, &pack, CLASS_ID).unwrap();
    assert_eq!(class["data"]["progression"]["prepared"], json!(PREPARED));
    assert!(casting_columns(&class).contains(&"slot_1".to_string()));
    let feature_id = "ID_EI_CLASS_FEATURE_WARDEN_SPELLCASTING";
    assert!(saved(&data, &pack, feature_id).is_some());

    // Act: the class opened again, its Spellcasting feature removed (the child and its entries, as the UI does), saved
    let v = state
        .homebrew_open_draft(&pack, Some(CLASS_ID), None)
        .unwrap();
    let key = v
        .form
        .children
        .iter()
        .find(|c| c.fields.get("casts") == Some(&json!(true)))
        .expect("the Spellcasting feature opens inside the class")
        .key
        .clone();
    let mut form = v.form.clone();
    form.children.retain(|c| c.key != key);
    for value in form.fields.values_mut() {
        if let Value::Array(entries) = value {
            entries.retain(|e| e.get("child").and_then(Value::as_str) != Some(key.as_str()));
        }
    }
    let v = state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();

    // Assert: the feature is gone, and so are the columns it wrote
    assert!(saved(&data, &pack, feature_id).is_none());
    let class = saved(&data, &pack, CLASS_ID).unwrap();
    assert_eq!(casting_columns(&class), Vec::<String>::new());

    // an unchanged save of the class keeps it that way
    let again = state
        .homebrew_open_draft(&pack, Some(CLASS_ID), None)
        .unwrap();
    let again = state
        .homebrew_update_draft(&again.draft_id, &again.form)
        .unwrap();
    state.homebrew_save_draft(&again.draft_id, &Quiet).unwrap();
    assert_eq!(
        casting_columns(&saved(&data, &pack, CLASS_ID).unwrap()),
        Vec::<String>::new()
    );
}
