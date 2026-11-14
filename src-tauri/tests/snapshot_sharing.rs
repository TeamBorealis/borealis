//! A save in the homebrew creator, and New pack, never hold two full content snapshots at once (item `Isq-Y`, spec
//! 18.5 phone budget on a large imported library): the next snapshot shares every element that did not change with the
//! one it replaces (the store's elements, the builder's files and the engine's prepared elements are the same
//! allocations, not copies), so only what changed is new. And nothing keeps the replaced snapshot alive once it is
//! swapped out (the background search index included).
//!
//! The library here is the bundled one; `tests/library_profile.rs` measures the same steps on a large imported one.

use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::HomebrewPackRequest;
use borealis_lib::state::{AppPaths, AppState, ContentHub};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

/// An element no step here changes.
const UNCHANGED: &str = "ID_2024_SPELL_BLESS";
const SEARCH_WAIT: Duration = Duration::from_secs(120);

fn app() -> (tempfile::TempDir, AppState) {
    let data = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    (data, state)
}

fn new_pack(state: &AppState) -> String {
    new_pack_named(state, "Ember Isles", "EI")
}

fn new_pack_named(state: &AppState, name: &str, code: &str) -> String {
    state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: name.into(),
                code: code.into(),
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

/// The elements of `after` that are the very allocation `before` holds for the same id.
fn shared(before: &ContentHub, after: &ContentHub) -> usize {
    after
        .store
        .iter()
        .filter(|s| {
            before
                .store
                .get(&s.element.id)
                .is_some_and(|b| Arc::ptr_eq(&b.element, &s.element))
        })
        .count()
}

fn save_a_spell(state: &AppState, pack: &str, name: &str) {
    let v = state
        .homebrew_new_draft(pack, "spell", Some(UNCHANGED), None, None)
        .unwrap();
    let form = v.form.with_field("name", json!(name));
    state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    state.homebrew_save_draft(&v.draft_id, &Quiet).unwrap();
}

#[test]
fn a_save_shares_every_unchanged_element_with_the_snapshot_it_replaces() {
    let (_data, state) = app();
    let pack = new_pack(&state);
    let before = state.hub().unwrap();
    save_a_spell(&state, &pack, "Ember Lance");
    let after = state.hub().unwrap();
    assert!(
        !Arc::ptr_eq(&before, &after),
        "the save swapped the snapshot"
    );
    // the spell is new; everything else, the saved pack's book included (only its manifest's version changes), is the
    // same allocation
    assert_eq!(after.store.len(), before.store.len() + 1);
    assert_eq!(
        shared(&before, &after),
        before.store.len(),
        "only the saved element is new"
    );
    assert!(after.store.get_exact(UNCHANGED).is_some());
}

#[test]
fn new_pack_shares_every_loaded_element_with_the_snapshot_it_replaces() {
    let (_data, state) = app();
    let before = state.hub().unwrap();
    new_pack(&state);
    let after = state.hub().unwrap();
    assert_eq!(after.store.len(), before.store.len() + 1, "the new book");
    assert_eq!(shared(&before, &after), before.store.len());
}

#[test]
fn the_engine_shares_the_store_elements_it_uses_as_they_are() {
    let (_data, state) = app();
    let hub = state.hub().unwrap();
    let stored = hub.store.get(UNCHANGED).unwrap();
    for ruleset in [Ruleset::Dnd2024, Ruleset::Dnd2014] {
        let prepared = hub.engine.universe(ruleset).get(UNCHANGED).unwrap();
        assert!(
            Arc::ptr_eq(&prepared.el, &stored.element),
            "{ruleset:?}: the engine holds its own copy of an element it uses unchanged"
        );
    }
}

#[test]
fn a_replaced_snapshot_is_freed_once_the_search_index_has_caught_up() {
    let (_data, state) = app();
    state.start_search_index().unwrap();
    assert!(state.search.wait_idle(SEARCH_WAIT));
    let pack = new_pack(&state);
    let before = Arc::downgrade(&state.hub().unwrap());
    save_a_spell(&state, &pack, "Ember Lance");
    assert!(state.search.wait_idle(SEARCH_WAIT));
    assert!(
        before.upgrade().is_none(),
        "something still holds the snapshot the save replaced"
    );
}

/// The index holds, for every pack of `hub`, what a sync of everything from scratch would write.
fn index_matches(state: &AppState, hub: &ContentHub) {
    let reader = state.search.reader().expect("the index is usable");
    let engine = reader.as_ref().expect("the index is open");
    for (corpus, inputs) in state.search.corpus_inputs(hub) {
        let want: BTreeMap<String, String> = inputs
            .into_iter()
            .map(|p| (p.pack, p.fingerprint))
            .collect();
        let held = engine
            .manifest()
            .corpora
            .get(corpus.name())
            .map(|c| c.packs.clone())
            .unwrap_or_default();
        assert_eq!(held, want, "{corpus:?}");
    }
}

#[test]
fn a_search_resync_after_a_save_makes_only_the_saved_pack_again() {
    let (_data, state) = app();
    state.start_search_index().unwrap();
    assert!(state.search.wait_idle(SEARCH_WAIT));
    let pack = new_pack(&state);
    assert!(state.search.wait_idle(SEARCH_WAIT));
    assert_eq!(
        state.search.inputs_remade(),
        1,
        "New pack: its own Lore input"
    );
    index_matches(&state, &state.hub().unwrap());
    save_a_spell(&state, &pack, "Ember Lance");
    assert!(state.search.wait_idle(SEARCH_WAIT));
    assert_eq!(
        state.search.inputs_remade(),
        1,
        "a save: the saved pack's Lore input"
    );
    index_matches(&state, &state.hub().unwrap());
}

#[test]
fn new_pack_loads_the_content_a_full_reload_loads() {
    let (_data, state) = app();
    // the new pack's folder sorts between the other two
    new_pack_named(&state, "Alpha Isles", "AI");
    new_pack_named(&state, "Zeta Isles", "ZI");
    new_pack_named(&state, "Mid Isles", "MI");
    let fast = state.hub().unwrap();
    let full = ContentHub::load_with(
        &state.paths,
        &state.settings().unwrap(),
        &state.content_prefs(),
    )
    .unwrap();
    assert!(fast.builder.same_content(&full.builder));
    assert_eq!(fast.store.len(), full.store.len());
    for (a, b) in fast.store.iter().zip(full.store.iter()) {
        assert_eq!(a, b);
    }
    assert_eq!(fast.store.packs(), full.store.packs());
    assert_eq!(fast.store.files(), full.store.files());
    assert_eq!(fast.load_warnings, full.load_warnings);
}
