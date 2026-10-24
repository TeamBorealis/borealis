mod common;

use borealis_bundle::{decide, plan_receive, Action, Item, ItemKind, LocalItem, Manifest, PackRef};
use common::{character_item, manifest_of};
use std::collections::BTreeSet;

fn local(key: &str, sha: &str, at: u64) -> LocalItem {
    LocalItem {
        kind: ItemKind::Character,
        key: key.into(),
        sha256: sha.into(),
        edited_at_ms: at,
    }
}

fn incoming(key: &str, content: &[u8], at: u64) -> Item {
    character_item(key, content, at)
}

fn one(m: &Manifest, l: &[LocalItem]) -> Action {
    plan_receive(m, l, &BTreeSet::new()).entries[0].action
}

#[test]
fn absent_locally_is_new() {
    let m = manifest_of(vec![incoming("c_a", b"{}", 10)]);
    assert_eq!(one(&m, &[]), Action::New);
}

#[test]
fn identical_hash_is_a_no_op_whatever_the_times() {
    let it = incoming("c_a", b"{}", 10);
    let m = manifest_of(vec![it.clone()]);
    assert_eq!(one(&m, &[local("c_a", &it.sha256, 10)]), Action::Identical);
    assert_eq!(one(&m, &[local("c_a", &it.sha256, 999)]), Action::Identical);
    assert_eq!(one(&m, &[local("c_a", &it.sha256, 1)]), Action::Identical);
}

#[test]
fn incoming_newer_replaces_and_trashes_the_local_copy() {
    let m = manifest_of(vec![incoming("c_a", b"{}", 20)]);
    let a = one(&m, &[local("c_a", "other", 10)]);
    assert_eq!(a, Action::ReplaceWithIncoming);
    assert!(a.writes() && a.trashes_local() && !a.needs_report());
}

#[test]
fn local_newer_is_kept_and_reported() {
    let m = manifest_of(vec![incoming("c_a", b"{}", 10)]);
    let a = one(&m, &[local("c_a", "other", 20)]);
    assert_eq!(a, Action::KeepLocalNewer);
    assert!(!a.writes() && !a.trashes_local() && a.needs_report());
}

#[test]
fn equal_time_different_hash_keeps_local_and_reports() {
    let m = manifest_of(vec![incoming("c_a", b"{}", 10)]);
    let p = plan_receive(&m, &[local("c_a", "other", 10)], &BTreeSet::new());
    assert_eq!(p.entries[0].action, Action::KeepLocalTie);
    assert_eq!(p.reported().count(), 1);
}

#[test]
fn decide_is_total_over_the_three_orderings() {
    let l = local("k", "h", 50);
    assert_eq!(decide("x", 51, Some(&l)), Action::ReplaceWithIncoming);
    assert_eq!(decide("x", 49, Some(&l)), Action::KeepLocalNewer);
    assert_eq!(decide("x", 50, Some(&l)), Action::KeepLocalTie);
    assert_eq!(decide("h", 0, Some(&l)), Action::Identical);
    assert_eq!(decide("x", 0, None), Action::New);
}

#[test]
fn kinds_do_not_collide_on_the_same_key() {
    let m = manifest_of(vec![incoming("same", b"{}", 5)]);
    let pack_local = LocalItem {
        kind: ItemKind::Pack,
        ..local("same", "zzz", 100)
    };
    assert_eq!(one(&m, &[pack_local]), Action::New);
}

#[test]
fn missing_packs_are_reported_per_character() {
    let mut it = incoming("c_a", b"{}", 5);
    it.required_packs = vec![
        PackRef {
            id: "in-bundle".into(),
            bundled: false,
        },
        PackRef {
            id: "installed".into(),
            bundled: false,
        },
        PackRef {
            id: "gone".into(),
            bundled: false,
        },
        PackRef {
            id: "srd-5.1".into(),
            bundled: true,
        },
    ];
    let pack = common::file_entry("packs/in-bundle/pack.json", b"{}");
    let pack_item = Item {
        kind: ItemKind::Pack,
        key: "in-bundle".into(),
        sha256: pack.sha256.clone(),
        size: pack.size,
        edited_at_ms: 1,
        files: vec![pack],
        required_packs: Vec::new(),
    };
    let m = manifest_of(vec![it, pack_item]);
    let installed: BTreeSet<String> = ["installed", "srd-5.1"].map(String::from).into();
    let p = plan_receive(&m, &[], &installed);
    let ch = p.entries.iter().find(|e| e.key == "c_a").unwrap();
    assert_eq!(
        ch.missing_packs
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["gone"]
    );
}
