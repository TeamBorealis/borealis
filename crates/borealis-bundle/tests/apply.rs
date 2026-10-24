//! Applying a receive plan: new / replace (old copy to the trash) / keep-local, edit stamps,
//! and refusal of the whole bundle when any item is invalid or has a non-plain key.

mod common;

use borealis_bundle::{
    apply_plan, mtime_ms, plan_receive, scan_characters, scan_packs, ApplyTargets, Bundle,
    BundleError, BundleWriter, ItemKind, ItemValidator, Limits,
};
use common::{character_json, meta, pack_json, write};
use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;

struct Accept;
impl ItemValidator for Accept {
    fn character(&self, _: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(bytes.to_vec())
    }
    fn pack(&self, _: &str, dir: &Path) -> Result<(), String> {
        dir.join("pack.json")
            .is_file()
            .then_some(())
            .ok_or_else(|| "no pack.json".into())
    }
}

struct RefuseNamed(&'static str);
impl ItemValidator for RefuseNamed {
    fn character(&self, key: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        (key != self.0)
            .then(|| bytes.to_vec())
            .ok_or_else(|| "not a character".into())
    }
    fn pack(&self, _: &str, _: &Path) -> Result<(), String> {
        Ok(())
    }
}

fn targets(root: &Path) -> ApplyTargets {
    ApplyTargets {
        characters_dir: root.join("characters"),
        library_dir: root.join("library"),
        trash_dir: root.join("sync/trash"),
        staging_dir: root.join("sync/staging"),
    }
}

const T0: u64 = 1_700_000_000_000;

fn bundle(chars: &[(&str, &[u8], u64)], packs: &[(&str, u64)]) -> Bundle<Cursor<Vec<u8>>> {
    let mut w = BundleWriter::new(Cursor::new(Vec::new()), meta());
    for (key, bytes, ms) in chars {
        w.add_character(key, bytes, *ms, Vec::new()).unwrap();
    }
    for (key, ms) in packs {
        let files = vec![
            ("pack.json".to_string(), br#"{"id":"x"}"#.to_vec()),
            ("data/content.json".to_string(), pack_json("ID_X")),
        ];
        w.add_pack(key, &files, *ms).unwrap();
    }
    let (out, _) = w.finish().unwrap();
    Bundle::open(Cursor::new(out.into_inner())).unwrap()
}

fn local_state(root: &Path) -> Vec<borealis_bundle::LocalItem> {
    let t = targets(root);
    let mut all = scan_characters(&t.characters_dir).unwrap_or_default();
    all.extend(scan_packs(&t.library_dir, &Limits::default()).unwrap_or_default());
    all
}

fn set_mtime(path: &Path, ms: u64) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms))
        .unwrap();
}

#[test]
fn new_replace_and_keep_local_follow_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let t = targets(root);
    let older = t.characters_dir.join("c_old.json");
    let newer = t.characters_dir.join("c_new.json");
    write(&older, &character_json("Old local", "A"));
    set_mtime(&older, T0);
    write(&newer, &character_json("New local", "A"));
    set_mtime(&newer, T0 + 10_000);

    let mut b = bundle(
        &[
            ("c_fresh", &character_json("Fresh", "A"), T0 + 1),
            ("c_old", &character_json("Old incoming", "A"), T0 + 5_000),
            ("c_new", &character_json("New incoming", "A"), T0 + 5_000),
        ],
        &[("homebrew", T0 + 7)],
    );
    let plan = plan_receive(b.manifest(), &local_state(root), &BTreeSet::new());
    let report = apply_plan(&mut b, &plan, &t, &Accept).unwrap();

    assert_eq!(report.replaced, vec![(ItemKind::Character, "c_old".into())]);
    assert_eq!(
        report.kept_local,
        vec![(ItemKind::Character, "c_new".into())]
    );
    assert_eq!(report.added.len(), 2);
    let text = std::fs::read_to_string(&older).unwrap();
    assert!(text.contains("Old incoming"));
    assert_eq!(
        mtime_ms(&older),
        T0 + 5_000,
        "stamped with the incoming edit time"
    );
    assert!(std::fs::read_to_string(&newer)
        .unwrap()
        .contains("New local"));
    assert!(t.library_dir.join("homebrew/data/content.json").is_file());
    let trash: Vec<String> = std::fs::read_dir(&t.trash_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(trash.len(), 1);
    assert!(trash[0].ends_with("-c_old.json"));

    // A second receive of the same bundle is a no-op.
    let plan = plan_receive(b.manifest(), &local_state(root), &BTreeSet::new());
    let again = apply_plan(&mut b, &plan, &t, &Accept).unwrap();
    assert!(again.added.is_empty() && again.replaced.is_empty());
    assert_eq!(again.identical.len(), 3);
}

fn files_under(p: &Path) -> Vec<String> {
    walk(p, p)
}

fn walk(root: &Path, p: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(p) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() {
            out.extend(walk(root, &path));
        } else {
            out.push(path.strip_prefix(root).unwrap().display().to_string());
        }
    }
    out.sort();
    out
}

#[test]
fn one_invalid_item_refuses_the_whole_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let t = targets(dir.path());
    let mut b = bundle(
        &[
            ("c_good", &character_json("Good", "A"), T0),
            ("c_bad", &character_json("Bad", "A"), T0),
        ],
        &[("homebrew", T0)],
    );
    let plan = plan_receive(b.manifest(), &[], &BTreeSet::new());
    let err = apply_plan(&mut b, &plan, &t, &RefuseNamed("c_bad")).unwrap_err();
    assert!(matches!(err, BundleError::Rejected(_)), "{err}");
    assert_eq!(
        files_under(dir.path()),
        Vec::<String>::new(),
        "nothing written"
    );
}

#[test]
fn keys_that_are_not_plain_ids_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let t = targets(dir.path());
    // The reader accepts `c (copy)` as a safe segment; the receive side is stricter.
    let mut b = bundle(&[("c (copy)", &character_json("Copy", "A"), T0)], &[]);
    let plan = plan_receive(b.manifest(), &[], &BTreeSet::new());
    let err = apply_plan(&mut b, &plan, &t, &Accept).unwrap_err();
    assert!(matches!(err, BundleError::Rejected(_)), "{err}");
    assert!(files_under(dir.path()).is_empty());
}
