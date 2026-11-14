//! WB-23 (homebrew creator expert features, spec 12) through the real `AppState` on a temporary data folder with the
//! bundled content: SPELL LISTS (own spells by tag, loaded spells by an additive append, HC5), where used, the DATA
//! view and the element list's search fields, and the multi-select edits (tags, edition, move to another pack,
//! duplicate) each undone as one step, deletes too.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::{
    HomebrewBulkPatch, HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest,
    HomebrewPickerFilters, HomebrewPickerTarget,
};
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
    data: tempfile::TempDir,
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
        Self { data, state }
    }

    fn create(&self, name: &str, code: &str, edition: Ruleset) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: name.into(),
                    code: code.into(),
                    edition,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id
    }

    fn read(&self, pack: &str, file: &str) -> Value {
        let path = self.data.path().join("library").join(pack).join(file);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn element(&self, pack: &str, id: &str) -> Option<Value> {
        self.read(pack, "content.json")["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == json!(id))
            .cloned()
    }

    fn edit(&self, v: &HomebrewDraftView, fields: &[(&str, Value)]) -> HomebrewDraftView {
        let mut form: FormState = v.form.clone();
        for (k, value) in fields {
            form.fields.insert((*k).to_string(), value.clone());
        }
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    /// A new element of `kind` saved with these fields; its id.
    fn make(&self, pack: &str, kind: &str, fields: &[(&str, Value)]) -> String {
        let v = self
            .state
            .homebrew_new_draft(pack, kind, None, None, None)
            .unwrap();
        let v = self.edit(&v, fields);
        self.state
            .homebrew_save_draft(&v.draft_id, &Quiet)
            .unwrap()
            .element_ids[0]
            .clone()
    }

    /// A loaded element's id by its exact name and type.
    fn loaded(&self, name: &str, ty: &str) -> String {
        self.state
            .homebrew_reference_options(
                &HomebrewPickerTarget {
                    types: vec![ty.into()],
                    tags: None,
                    multiple: None,
                },
                name,
                &HomebrewPickerFilters {
                    edition: Some(Ruleset::Dnd2024),
                    ..HomebrewPickerFilters::default()
                },
                HomebrewPageRequest {
                    offset: 0,
                    limit: 50,
                },
                None,
            )
            .unwrap()
            .items
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} ({ty}) is loaded"))
            .id
    }

    /// A multi-select edit: the elements it changed and its undo.
    fn bulk(&self, pack: &str, ids: &[&str], patch: HomebrewBulkPatch) -> (Vec<String>, String) {
        let ids: Vec<String> = ids.iter().map(|s| (*s).to_string()).collect();
        let r = self
            .state
            .homebrew_bulk_edit(pack, &ids, &patch, &Quiet)
            .unwrap();
        (r.element_ids, r.undo_id.expect("an undo"))
    }
}

fn spell_fields(name: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("name", json!(name)),
        ("school", json!("Evocation")),
        ("spellLists", json!(["Sorcerer"])),
    ]
}

#[test]
fn spell_lists_take_loaded_spells_by_append_and_own_spells_by_tag() {
    let app = App::new();
    let pack = app.create("Ember Isles", "EI", Ruleset::Dnd2024);
    let ember = app.make(&pack, "spell", &spell_fields("Ember Lance"));
    let bless = app.loaded("Bless", "Spell");
    let fireball = app.loaded("Fireball", "Spell");

    let before = app.state.homebrew_spell_list(&pack, "Wizard").unwrap();
    assert!(!before.own, "no class of the pack declares the Wizard list");
    let row = |v: &borealis_lib::homebrew_dto::HomebrewSpellListView, id: &str| {
        v.rows.iter().find(|r| r.row.id == id).cloned().expect(id)
    };
    assert!(row(&before, &fireball).on_list && !row(&before, &fireball).removable);
    assert!(!row(&before, &bless).on_list);
    assert!(!row(&before, &ember).on_list);
    assert_eq!(
        row(&before, &ember).row.level,
        Some(1),
        "Magic grid columns"
    );

    let after = app
        .state
        .homebrew_set_spell_list(
            &pack,
            "Wizard",
            &[ember.clone(), bless.clone()],
            &[],
            &Quiet,
        )
        .unwrap();
    assert_eq!(
        after.count,
        before.count + 2,
        "84 spells on this list, plus two"
    );
    assert!(row(&after, &bless).on_list && row(&after, &bless).removable);
    assert!(row(&after, &ember).on_list && row(&after, &ember).removable);
    let content = app.read(&pack, "content.json");
    assert_eq!(
        content["appends"],
        json!([{"id": bless, "supports": ["Wizard"]}]),
        "HC5: an additive append"
    );
    assert_eq!(
        app.element(&pack, &ember).unwrap()["supports"],
        json!(["Sorcerer", "Wizard"]),
        "an own spell changes its own tags"
    );
    assert!(
        app.element(&pack, &bless).is_none(),
        "a loaded spell is never copied in place"
    );

    // where used: the Wizard's list now lists Bless (the append is merged when the pack loads)
    let used = app.state.homebrew_where_used(&bless).unwrap();
    assert!(
        used.iter()
            .any(|r| r.relation == "lists" && r.pack_id.is_none()),
        "a bundled Wizard list lists it: {used:?}"
    );

    // only what the pack added comes off
    let refused = app
        .state
        .homebrew_set_spell_list(
            &pack,
            "Wizard",
            &[],
            std::slice::from_ref(&fireball),
            &Quiet,
        )
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidArgument);
    let back = app
        .state
        .homebrew_set_spell_list(
            &pack,
            "Wizard",
            &[],
            &[bless.clone(), ember.clone()],
            &Quiet,
        )
        .unwrap();
    assert_eq!(back.count, before.count);
    assert!(
        app.read(&pack, "content.json").get("appends").is_none(),
        "the append is gone"
    );
}

#[test]
fn where_used_tells_grants_from_choices_and_the_data_view_shows_the_element() {
    let app = App::new();
    let second_wind = app.loaded("Second Wind", "Class Feature");
    let used = app.state.homebrew_where_used(&second_wind).unwrap();
    assert!(
        used.iter()
            .any(|r| r.relation == "grants" && r.ty == "Class" && r.name == "Fighter"),
        "{used:?}"
    );
    assert!(app.state.homebrew_where_used("ID_NOPE").is_err());

    let pack = app.create("Gloom", "GL", Ruleset::Dnd2024);
    let v = app
        .state
        .homebrew_new_draft(&pack, "language", None, None, None)
        .unwrap();
    let v = app.edit(
        &v,
        &[
            ("name", json!("Gloom \"Speak\"")),
            ("tags", json!(["Exotic"])),
        ],
    );
    let tree = serde_json::to_value(&v.generated.tree).unwrap();
    assert_eq!(
        tree[0]["key"],
        json!("Gloom \"Speak\""),
        "characters, never escaped"
    );
    assert!(tree[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["key"] == json!("supports")));
    let copied: Value = serde_json::from_str(&v.generated.json).unwrap();
    assert_eq!(
        copied["name"],
        json!("Gloom \"Speak\""),
        "Copy as JSON is the element"
    );
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let rows = app.state.homebrew_list_elements(&pack).unwrap();
    assert_eq!(
        rows[0].tags,
        vec!["Exotic".to_string()],
        "the list filter searches tags"
    );
}

/// H0eFo: where used matches a choice the way the engine does: the tags a spell has from its level and school, a
/// class's `$(spellcasting:list)` and `$(spellcasting:slots)` terms, and (IM16) the tags of the 2014 element a 2024
/// one replaces; and it lists the pack's unsaved new drafts.
#[test]
fn where_used_counts_derived_and_dynamic_tags_and_unsaved_drafts() {
    let app = App::new();
    let fireball = app.loaded("Fireball", "Spell");
    let used = app.state.homebrew_where_used(&fireball).unwrap();
    let uses = |id: &str, relation: &str| used.iter().any(|r| r.id == id && r.relation == relation);
    assert!(
        uses(
            "ID_2024_SUBCLASS_FEATURE_WIZARD_EVOKER_EVOCATION_SAVANT",
            "selects"
        ),
        "a choice of 'Wizard, Evocation, (1||2||3)' offers a 3rd-level evocation: {used:?}"
    );
    assert!(
        uses("ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING", "selects"),
        "the Wizard's spellbook choice ($(spellcasting:list), $(spellcasting:slots)) offers it: {used:?}"
    );
    assert!(
        !used
            .iter()
            .any(|r| r.id == "ID_2024_CLASS_FEATURE_CLERIC_SPELLCASTING"),
        "the Cleric's list does not hold Fireball: {used:?}"
    );

    // IM16: the 2014 Rogue's archetype choice names a tag only the 2014 Thief carries
    let used = app
        .state
        .homebrew_where_used("ID_2024_SUBCLASS_ROGUE_THIEF")
        .unwrap();
    assert!(
        used.iter()
            .any(|r| r.relation == "selects" && r.name == "Roguish Archetype"),
        "the 2014 choice offers the 2024 Thief that replaces its own: {used:?}"
    );

    // an unsaved new draft of a pack that gives the element
    let pack = app.create("Ember Isles", "EI", Ruleset::Dnd2024);
    let alert = app.loaded("Alert", "Feat");
    let v = app
        .state
        .homebrew_new_draft(&pack, "background", None, None, None)
        .unwrap();
    let v = app.edit(
        &v,
        &[
            ("name", json!("Ember Warden")),
            (
                "originFeat",
                json!({"feat": {"id": alert, "name": "", "type": "Feat"}, "params": {}}),
            ),
            (
                "abilityScores",
                json!(["Wisdom", "Strength", "Constitution"]),
            ),
            (
                "skills",
                json!([
                    {"id": "ID_PROFICIENCY_SKILL_NATURE", "name": "", "type": "Proficiency"},
                    {"id": "ID_PROFICIENCY_SKILL_SURVIVAL", "name": "", "type": "Proficiency"}
                ]),
            ),
        ],
    );
    let used = app.state.homebrew_where_used(&alert).unwrap();
    let draft = used
        .iter()
        .find(|r| r.name == "Ember Warden")
        .unwrap_or_else(|| panic!("the unsaved background is listed: {used:?}"));
    assert_eq!(draft.relation, "grants");
    assert_eq!(draft.pack_id.as_deref(), Some(pack.as_str()));
    assert_eq!(
        draft.draft_id.as_deref(),
        Some(v.draft_id.as_str()),
        "it opens as its draft"
    );
    app.state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
    let used = app.state.homebrew_where_used(&alert).unwrap();
    let saved: Vec<_> = used.iter().filter(|r| r.name == "Ember Warden").collect();
    assert_eq!(saved.len(), 1, "saved, it is listed once: {used:?}");
    assert_eq!(saved[0].draft_id, None);
}

#[test]
fn tags_and_edition_change_together_as_one_undoable_step() {
    let app = App::new();
    let pack = app.create("Gloom", "GL", Ruleset::Dnd2024);
    let a = app.make(&pack, "language", &[("name", json!("Gloomspeak"))]);
    let b = app.make(
        &pack,
        "language",
        &[("name", json!("Brightspeak")), ("tags", json!(["Planar"]))],
    );
    let version = app.read(&pack, "pack.json")["version"].clone();
    let (changed, undo) = app.bulk(
        &pack,
        &[&a, &b],
        HomebrewBulkPatch {
            add_tags: Some(vec!["Secret".into()]),
            remove_tags: Some(vec!["planar".into()]),
            edition: Some(Some(Ruleset::Dnd2014)),
            ..HomebrewBulkPatch::default()
        },
    );
    assert_eq!(changed.len(), 2);
    assert_eq!(
        app.element(&pack, &a).unwrap()["supports"],
        json!(["Secret"])
    );
    assert_eq!(
        app.element(&pack, &b).unwrap()["supports"],
        json!(["Secret"])
    );
    assert_eq!(app.element(&pack, &b).unwrap()["ruleset"], json!("dnd2014"));
    assert_ne!(app.read(&pack, "pack.json")["version"], version, "one save");

    app.state.homebrew_undo(&undo, &Quiet).unwrap();
    assert!(app.element(&pack, &a).unwrap().get("supports").is_none());
    assert_eq!(
        app.element(&pack, &b).unwrap()["supports"],
        json!(["Planar"])
    );
    assert!(app.element(&pack, &b).unwrap().get("ruleset").is_none());
    assert!(
        app.state.homebrew_undo(&undo, &Quiet).is_err(),
        "an undo is used once"
    );

    // one change at a time, and nothing to change is no write
    let none = app
        .state
        .homebrew_bulk_edit(
            &pack,
            std::slice::from_ref(&a),
            &HomebrewBulkPatch::default(),
            &Quiet,
        )
        .unwrap_err();
    assert_eq!(none.code, ErrorCode::InvalidArgument);
    let same = app
        .state
        .homebrew_bulk_edit(
            &pack,
            std::slice::from_ref(&a),
            &HomebrewBulkPatch {
                remove_tags: Some(vec!["Nothing".into()]),
                ..HomebrewBulkPatch::default()
            },
            &Quiet,
        )
        .unwrap();
    assert!(same.element_ids.is_empty() && same.undo_id.is_none());
}

#[test]
fn a_move_keeps_ids_and_provenance_and_undo_moves_it_back() {
    let app = App::new();
    let from = app.create("Gloom", "GL", Ruleset::Dnd2024);
    let to = app.create("Old Ways", "OW", Ruleset::Dnd2014);
    let a = app.make(&from, "language", &[("name", json!("Gloomspeak"))]);
    let (moved, undo) = app.bulk(
        &from,
        &[&a],
        HomebrewBulkPatch {
            move_to_pack_id: Some(to.clone()),
            ..HomebrewBulkPatch::default()
        },
    );
    assert_eq!(moved, vec![a.clone()], "ids never change (spec 5.3)");
    assert!(app.element(&from, &a).is_none());
    let there = app.element(&to, &a).expect("moved");
    assert_eq!(there["source"], json!("Old Ways"));
    assert_eq!(
        there["ruleset"],
        json!("dnd2024"),
        "it keeps its edition in a 2014 pack"
    );
    assert_eq!(
        app.read(&to, "creator.json")["elements"][&a]["kind"],
        json!("language")
    );
    assert!(app.read(&from, "creator.json")["retiredIds"]
        .as_array()
        .unwrap()
        .contains(&json!(a)));

    app.state.homebrew_undo(&undo, &Quiet).unwrap();
    assert!(app.element(&to, &a).is_none());
    assert_eq!(app.element(&from, &a).unwrap()["source"], json!("Gloom"));
    assert!(!app.read(&from, "creator.json")["retiredIds"]
        .as_array()
        .unwrap()
        .contains(&json!(a)));
}

#[test]
fn duplicates_and_deletes_are_undone_as_one_step() {
    let app = App::new();
    let pack = app.create("Gloom", "GL", Ruleset::Dnd2024);
    let a = app.make(&pack, "language", &[("name", json!("Gloomspeak"))]);
    let b = app.make(&pack, "language", &[("name", json!("Brightspeak"))]);
    let (copies, undo) = app.bulk(
        &pack,
        &[&a, &b],
        HomebrewBulkPatch {
            duplicate: Some(true),
            ..HomebrewBulkPatch::default()
        },
    );
    assert_eq!(copies.len(), 2);
    let names: Vec<Value> = copies
        .iter()
        .map(|id| app.element(&pack, id).unwrap()["name"].clone())
        .collect();
    assert_eq!(
        names,
        [json!("Gloomspeak (copy)"), json!("Brightspeak (copy)")]
    );
    app.state.homebrew_undo(&undo, &Quiet).unwrap();
    assert!(copies.iter().all(|id| app.element(&pack, id).is_none()));
    let retired = app.read(&pack, "creator.json")["retiredIds"].clone();
    assert!(
        copies
            .iter()
            .all(|id| retired.as_array().unwrap().contains(&json!(id))),
        "never given out again"
    );

    let deleted = app
        .state
        .homebrew_delete_elements(&pack, std::slice::from_ref(&a), false, &Quiet)
        .unwrap();
    assert!(deleted.deleted);
    assert!(app.element(&pack, &a).is_none());
    app.state
        .homebrew_undo(deleted.undo_id.as_deref().unwrap(), &Quiet)
        .unwrap();
    assert_eq!(app.element(&pack, &a).unwrap()["name"], json!("Gloomspeak"));
    assert!(app.read(&pack, "creator.json")["elements"]
        .get(&a)
        .is_some());
    assert!(
        app.state.hub().unwrap().store.contains_id(&a),
        "loaded again"
    );

    // an undo after another change to the pack does nothing
    let again = app
        .state
        .homebrew_delete_elements(&pack, std::slice::from_ref(&b), false, &Quiet)
        .unwrap();
    app.make(&pack, "language", &[("name", json!("Thirdspeak"))]);
    let stale = app
        .state
        .homebrew_undo(again.undo_id.as_deref().unwrap(), &Quiet)
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::RequirementFailed);
    assert!(app.element(&pack, &b).is_none());
}
