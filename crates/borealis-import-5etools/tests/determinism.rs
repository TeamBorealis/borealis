//! The same input must give byte-identical output, wherever it lives and however often it runs.

mod support;

use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::BTreeMap;
use std::path::Path;
use support::*;

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn go(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                go(base, &p, out);
            } else {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    go(root, root, &mut out);
    out
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let target = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_tree(&p, &target);
        } else {
            std::fs::copy(&p, &target).unwrap();
        }
    }
}

#[test]
fn two_runs_write_identical_bytes() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let first = import_fixtures();
    let second = import_fixtures();
    write_packs(&first, a.path());
    write_packs(&second, b.path());
    let (ta, tb) = (tree(a.path()), tree(b.path()));
    assert!(ta.len() >= 25, "expected many files, got {}", ta.len());
    assert_eq!(ta, tb);
    assert_eq!(
        first.report.to_json_pretty().unwrap(),
        second.report.to_json_pretty().unwrap()
    );
}

#[test]
fn output_does_not_depend_on_where_the_folder_lives() {
    let moved = tempfile::tempdir().unwrap();
    let data = moved.path().join("some").join("other").join("data");
    copy_tree(&fixtures(), &data);
    let from_copy = import_folder(&data, &ImportOptions::default()).unwrap();
    let original = import_fixtures();
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write_packs(&from_copy, a.path());
    write_packs(&original, b.path());
    assert_eq!(tree(a.path()), tree(b.path()));
    assert_eq!(from_copy.report, original.report);
}

#[test]
fn element_ids_are_unique_across_packs() {
    let result = import_fixtures();
    let mut seen = std::collections::BTreeSet::new();
    for e in result.packs.iter().flat_map(|p| p.elements()) {
        assert!(seen.insert(e.id.clone()), "duplicate id {}", e.id);
        assert!(e.id.starts_with("ID_5ET_"));
    }
}
