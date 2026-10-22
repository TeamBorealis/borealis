//! UI-vs-engine (B-22): the real `CharacterSnapshot` DTO built from bundled SRD golden characters
//! must satisfy the UI contract (`ui/src/api/generated/contract.json`, generated from
//! `contract.ts` and checked up to date by vitest): no field the TS type lacks, no required TS
//! field missing, scalar/array/map kinds agree. Adding or renaming a DTO field without the UI
//! contract following fails here. The MockApi side is checked by
//! `ui/src/api/__tests__/snapshot-contract.test.ts`.

mod contract_support;
#[path = "support/golden/mod.rs"]
mod golden;

use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use std::path::PathBuf;

/// A fighter, a full caster, a 2024 caster and two multiclass builds (slots + pact + spell lists).
const SAMPLE: &[&str] = &[
    "srd51-fighter-11",
    "srd51-wizard-8",
    "srd521-cleric-5",
    "srd51-multiclass-paladin2-warlock3",
    "srd521-multiclass-wizard5-cleric3",
];

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

#[test]
fn real_snapshots_satisfy_the_ui_contract() {
    let (_dir, app) = app();
    let contract = Contract::load();
    let mapping = golden::read_json(&golden::goldens_dir().join("mapping.json"));
    let mut problems = Vec::new();
    for id in SAMPLE {
        let spec = golden::read_json(&golden::goldens_dir().join(format!("specs/{id}.json")));
        let built = golden::build::build(&app, &spec, &mapping["goldens"][id], false);
        let value = serde_json::to_value(&built.snap).expect("snapshot json");
        let errs = contract.def("CharacterSnapshot", &value);
        problems.extend(errs.into_iter().map(|e| format!("{id}: {e}")));
        let _ = app.close_character(&built.snap.id, true);
    }
    problems.sort();
    problems.dedup();
    assert!(
        problems.is_empty(),
        "{} contract mismatches (first 20):\n{}",
        problems.len(),
        problems.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_checker_flags_a_missing_and_an_extra_field() {
    let (_dir, app) = app();
    let contract = Contract::load();
    let mapping = golden::read_json(&golden::goldens_dir().join("mapping.json"));
    let id = SAMPLE[0];
    let spec = golden::read_json(&golden::goldens_dir().join(format!("specs/{id}.json")));
    let built = golden::build::build(&app, &spec, &mapping["goldens"][id], false);
    let mut value = serde_json::to_value(&built.snap).expect("snapshot json");
    let map = value.as_object_mut().expect("object");
    map.insert("fieldTheUiNeverHeardOf".into(), serde_json::json!(1));
    let removed = map.remove("name");
    assert!(removed.is_some(), "snapshot has a name");
    let errs = contract.def("CharacterSnapshot", &value).join("\n");
    assert!(errs.contains("fieldTheUiNeverHeardOf"), "{errs}");
    assert!(errs.contains("TS requires `name`"), "{errs}");
}
