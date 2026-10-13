//! Native file drops and File > Load Character: the drop handler turns paths
//! into `dropped-file` tokens, the webview sees tokens and names only, and `take_picked_files`
//! reads each token once. Shapes are checked against the generated UI contract.

mod contract_support;
use borealis_lib::picked_files::{grant_drop, take_picked_files, EVENT_FILE_DROP};
use borealis_lib::picked_paths::{PickPurpose, PickedPaths};
use contract_support::Contract;
use std::path::PathBuf;

fn write(dir: &std::path::Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn the_drop_event_and_the_read_match_the_ui_contract() {
    let c = Contract::load();
    let dir = tempfile::tempdir().unwrap();
    let picks = PickedPaths::default();
    let paths = [
        write(dir.path(), "lantern.xml", "<elements/>"),
        write(dir.path(), "brannoc.dnd5e", "{\"name\":\"Brannoc\"}"),
        write(dir.path(), "notes.txt", "x"),
    ];
    let drop = grant_drop(&picks, &paths).unwrap();
    let event = serde_json::to_value(&drop).unwrap();
    let errs = c.event("files:dropped", &event);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(EVENT_FILE_DROP, "borealis://file-drop");
    assert!(
        !event.to_string().contains(&*dir.path().to_string_lossy()),
        "no paths reach the webview: {event}"
    );
    assert_eq!(drop.rejected, ["notes.txt"]);

    let tokens: Vec<String> = drop.files.iter().map(|f| f.token.clone()).collect();
    let read = take_picked_files(&picks, PickPurpose::DroppedFile, &tokens).unwrap();
    let errs = c.response("take_picked_files", &serde_json::to_value(&read).unwrap());
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(read[1].file_name, "brannoc.dnd5e");
}

#[test]
fn dropped_tokens_are_one_shot_and_scoped_through_app_state() {
    let data = tempfile::tempdir().unwrap();
    let state = borealis_lib::state::AppState::new(borealis_lib::state::AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let xml = write(dir.path(), "pack.xml", "<elements/>");
    let drop = grant_drop(&state.picked, &[xml]).unwrap();
    let token = drop.files[0].token.clone();
    // A dropped file is not a folder or a 5etools pick, and cannot stand in for one.
    assert!(state
        .picked
        .resolve(&token, PickPurpose::ContentFolder)
        .is_err());
    assert!(state
        .picked
        .resolve(&token, PickPurpose::FivetoolsFolder)
        .is_err());
    let once = [token];
    assert!(take_picked_files(&state.picked, PickPurpose::DroppedFile, &once).is_ok());
    assert!(take_picked_files(&state.picked, PickPurpose::DroppedFile, &once).is_err());
}
