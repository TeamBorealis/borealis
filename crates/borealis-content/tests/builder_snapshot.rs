//! PF1 (B-235): the builder snapshot round trip over every bundled pack, and damaged bytes.

use borealis_content::{ContentStoreBuilder, Ruleset};
use std::path::PathBuf;

const BUNDLED: [&str; 5] = [
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

fn bundled() -> ContentStoreBuilder {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content");
    BUNDLED.iter().fold(ContentStoreBuilder::new(), |b, p| {
        b.add_pack_dir(&root.join(p)).unwrap()
    })
}

#[test]
fn a_snapshot_round_trips_every_bundled_pack() {
    let mut editions = std::collections::BTreeMap::new();
    editions.insert(
        "system reference document 5.1".to_string(),
        Ruleset::Dnd2024,
    );
    let builder = bundled().with_source_editions(editions);
    let bytes = builder.to_snapshot().unwrap();
    let back = ContentStoreBuilder::from_snapshot(&bytes).unwrap();
    assert!(back.same_content(&builder));
    assert_eq!(
        back.to_snapshot().unwrap(),
        bytes,
        "encoding is deterministic"
    );
    let (a, b) = (back.build(), builder.build());
    assert!(a.len() > 3_000);
    assert_eq!(a.len(), b.len());
    assert!(a.iter().eq(b.iter()));
    assert_eq!(a.files(), b.files());
    assert_eq!(a.diagnostics(), b.diagnostics());
    assert_eq!(a.indexes(), b.indexes());
}

#[test]
fn a_damaged_snapshot_is_an_error_never_a_partial_builder() {
    let bytes = bundled().to_snapshot().unwrap();
    for cut in [0, 7, 8, 100, bytes.len() / 2, bytes.len() - 1] {
        assert!(
            ContentStoreBuilder::from_snapshot(&bytes[..cut]).is_err(),
            "cut at {cut}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(ContentStoreBuilder::from_snapshot(&longer).is_err());
}
