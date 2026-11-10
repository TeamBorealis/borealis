//! WB-06 (homebrew creator, HC7, spec 10.1 and 14.4): a homebrew pack travels with its `creator.json`; the creator's
//! drafts (`<data>/homebrew-drafts/`), the pack's `.trash` copies of deleted elements and the temporary files of an
//! interrupted save never do.

use borealis_bundle::{is_excluded, read_pack_dir, Limits};

#[test]
fn a_creator_pack_travels_with_creator_json_and_without_its_trash() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("homebrew-gh");
    std::fs::create_dir_all(pack.join(".trash")).unwrap();
    for f in [
        "pack.json",
        "content.json",
        "pack-meta.json",
        "creator.json",
    ] {
        std::fs::write(pack.join(f), "{}").unwrap();
    }
    std::fs::write(
        pack.join(".trash/1731230000000-ID_GH_LANGUAGE_A.json"),
        "{}",
    )
    .unwrap();
    // a write the app was interrupted in leaves a dot-named temporary file: it never travels either
    std::fs::write(pack.join(".content.json.tmp"), "{}").unwrap();
    let files = read_pack_dir(&pack, &Limits::default()).unwrap();
    let names: Vec<&str> = files.files.iter().map(|(rel, _)| rel.as_str()).collect();
    assert!(names.contains(&"creator.json"), "{names:?}");
    assert!(
        names
            .iter()
            .all(|n| !n.contains("trash") && !n.contains("tmp")),
        "{names:?}"
    );
    assert!(files.skipped.iter().any(|s| s == ".trash"));
}

#[test]
fn creator_drafts_never_travel() {
    assert!(is_excluded("homebrew-drafts/hb-0123.json"));
    assert!(is_excluded("Homebrew-Drafts/x.json"));
    assert!(is_excluded("data/homebrew-drafts"));
    assert!(!is_excluded("packs/homebrew-gh/creator.json"));
}
