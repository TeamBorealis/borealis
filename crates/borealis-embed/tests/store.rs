//! Vector store: round trip, incremental add/remove per pack, corruption and model-change rebuilds.

use std::fs;

use borealis_embed::store::{HEADER_BYTES, KEYS_FILE};
use borealis_embed::{DocText, Embedder, FakeEmbedder, OpenStatus, StoreId, VectorStore};

fn id(dims: usize) -> StoreId {
    StoreId::for_embedder(&FakeEmbedder::new(dims)).expect("id")
}

fn vec_of(e: &FakeEmbedder, text: &str) -> Vec<f32> {
    e.embed_docs(&[DocText { title: None, text }])
        .expect("embed")
        .remove(0)
}

fn filled(dir: &std::path::Path) -> VectorStore {
    let e = FakeEmbedder::new(256);
    let (mut store, status) = VectorStore::open(dir, id(256)).expect("open");
    assert_eq!(status, OpenStatus::Created);
    for (pack, key, text) in [
        ("srd-5.1", "ID_FIREBALL", "fireball fire explosion damage"),
        ("srd-5.1", "ID_GRAPPLED", "grappled creature speed zero"),
        (
            "srd-5.2.1",
            "ID_FIREBALL_2024",
            "fireball fire sphere damage",
        ),
        ("homebrew", "ID_HB_SWORD", "a sword of sharpness"),
    ] {
        store
            .upsert(pack, key, &format!("h-{key}"), &vec_of(&e, text))
            .expect("upsert");
    }
    store
}

#[test]
fn round_trip_keeps_rows_and_scores() {
    let tmp = tempfile::tempdir().expect("tmp");
    let e = FakeEmbedder::new(256);
    let mut store = filled(tmp.path());
    let query = e.embed_query("fire damage").expect("q");
    let before: Vec<(String, f32)> = store
        .search(&query, 3)
        .into_iter()
        .map(|(k, s)| (k.to_string(), s))
        .collect();
    store.save().expect("save");
    assert!(tmp.path().join("vectors-fa4efa4e-256.bin").exists());
    assert!(tmp.path().join(KEYS_FILE).exists());

    let (reopened, status) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    assert_eq!(status, OpenStatus::Loaded);
    assert_eq!(reopened.len(), 4);
    assert!(reopened.contains("ID_GRAPPLED", "h-ID_GRAPPLED"));
    assert!(!reopened.contains("ID_GRAPPLED", "other-hash"));
    let after: Vec<(String, f32)> = reopened
        .search(&query, 3)
        .into_iter()
        .map(|(k, s)| (k.to_string(), s))
        .collect();
    assert_eq!(before, after);
    assert!(after[0].0.starts_with("ID_FIREBALL"), "{after:?}");
    // int8 scores stay close to the exact f32 cosine.
    let exact = borealis_embed::vector::dot(&query, &vec_of(&e, "fireball fire explosion damage"));
    let stored = after
        .iter()
        .find(|(k, _)| k == "ID_FIREBALL")
        .expect("fireball")
        .1;
    assert!((exact - stored).abs() < 0.02, "{exact} vs {stored}");
}

#[test]
fn incremental_add_replace_remove_per_pack() {
    let tmp = tempfile::tempdir().expect("tmp");
    let e = FakeEmbedder::new(256);
    let mut store = filled(tmp.path());
    // Replace one row (new hash), add one, drop a pack.
    store
        .upsert(
            "srd-5.1",
            "ID_GRAPPLED",
            "h2",
            &vec_of(&e, "grappled restrained"),
        )
        .expect("replace");
    store
        .upsert(
            "srd-5.1",
            "ID_DASH",
            "h-dash",
            &vec_of(&e, "dash action movement"),
        )
        .expect("add");
    assert_eq!(store.remove_pack("homebrew"), 1);
    assert!(!store.remove("ID_NOPE"));
    assert!(store.remove("ID_FIREBALL_2024"));
    store.save().expect("save");

    let (reopened, _) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    let rows: Vec<(String, String, String)> = reopened
        .rows()
        .map(|(k, h, p)| (k.into(), h.into(), p.into()))
        .collect();
    assert_eq!(reopened.len(), 3, "{rows:?}");
    assert!(reopened.contains("ID_GRAPPLED", "h2"));
    assert!(reopened.contains("ID_DASH", "h-dash"));
    assert!(reopened.contains("ID_FIREBALL", "h-ID_FIREBALL"));
    assert_eq!(
        reopened.packs().into_iter().collect::<Vec<_>>(),
        ["srd-5.1"]
    );
    // Removal by swap kept every remaining row searchable under its own key.
    let q = e.embed_query("dash action movement").expect("q");
    assert_eq!(reopened.search(&q, 1)[0].0, "ID_DASH");
}

#[test]
fn wrong_length_vectors_are_refused() {
    let tmp = tempfile::tempdir().expect("tmp");
    let (mut store, _) = VectorStore::open(tmp.path(), id(128)).expect("open");
    assert!(store.upsert("p", "k", "h", &[0.5; 256]).is_err());
    assert!(store.search(&[0.5; 256], 5).is_empty());
}

#[test]
fn corrupt_rows_rebuild_empty() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    store.save().expect("save");
    let bin = store.bin_path();
    let mut bytes = fs::read(&bin).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x55;
    fs::write(&bin, &bytes).expect("write");

    let (reopened, status) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    assert!(
        matches!(&status, OpenStatus::Rebuilt { reason } if reason.contains("row digest")),
        "{status:?}"
    );
    assert!(reopened.is_empty());
    assert!(!bin.exists(), "corrupt file wiped");
}

#[test]
fn truncated_file_bad_magic_and_bad_sidecar_rebuild() {
    for damage in ["truncate", "magic", "sidecar", "no-sidecar"] {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut store = filled(tmp.path());
        store.save().expect("save");
        let bin = store.bin_path();
        let keys = tmp.path().join(KEYS_FILE);
        match damage {
            "truncate" => {
                let bytes = fs::read(&bin).expect("read");
                fs::write(&bin, &bytes[..HEADER_BYTES + 10]).expect("write");
            }
            "magic" => {
                let mut bytes = fs::read(&bin).expect("read");
                bytes[0] = b'X';
                fs::write(&bin, &bytes).expect("write");
            }
            "sidecar" => fs::write(
                &keys,
                b"{\"format\":\"borealis-vector-keys/1\",\"rows\":[]}",
            )
            .expect("write"),
            _ => fs::remove_file(&keys).expect("rm"),
        }
        let (reopened, status) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
        assert!(
            matches!(status, OpenStatus::Rebuilt { .. }),
            "{damage}: {status:?}"
        );
        assert!(reopened.is_empty(), "{damage}");
    }
}

#[test]
fn crash_between_renames_recovers_from_the_tmp_sidecar() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    store.save().expect("save");
    let keys = tmp.path().join(KEYS_FILE);
    let good = fs::read(&keys).expect("read");
    // Simulate: new binary renamed, new keys still at keys.json.tmp, old keys.json in place.
    fs::write(tmp.path().join(format!("{KEYS_FILE}.tmp")), &good).expect("tmp");
    fs::write(
        &keys,
        b"{\"format\":\"borealis-vector-keys/1\",\"rows\":[]}",
    )
    .expect("old");
    let (reopened, status) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    assert_eq!(status, OpenStatus::Loaded);
    assert_eq!(reopened.len(), 4);
}

#[test]
fn model_dims_quant_or_prompt_change_rebuilds() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    store.save().expect("save");
    let old_bin = store.bin_path();

    // Other dims: a different file; the old one is deleted.
    let (s128, status) = VectorStore::open(tmp.path(), id(128)).expect("open 128");
    assert_eq!(status, OpenStatus::Created);
    assert!(s128.is_empty() && !old_bin.exists());

    // Same file name, other prompt version or quant: the header check rebuilds.
    let mut store = filled(tmp.path());
    store.save().expect("save");
    let sha = FakeEmbedder::new(256).model().sha256.clone();
    let newer_prompt = StoreId::new(&sha, "FAKE", 256, 2).expect("id");
    let (s, status) = VectorStore::open(tmp.path(), newer_prompt).expect("open");
    assert!(matches!(&status, OpenStatus::Rebuilt { reason } if reason.contains("prompt version")));
    assert!(s.is_empty());

    let mut store = filled(tmp.path());
    store.save().expect("save");
    let other_quant = StoreId::new(&sha, "Q8_0", 256, 1).expect("id");
    let (_, status) = VectorStore::open(tmp.path(), other_quant).expect("open");
    assert!(matches!(&status, OpenStatus::Rebuilt { reason } if reason.contains("quant")));
}

#[test]
fn delete_all_removes_vectors_and_keys_only() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    store.save().expect("save");
    fs::write(tmp.path().join("meta.json"), b"{}").expect("tantivy file");
    VectorStore::delete_all(tmp.path()).expect("delete");
    let left: Vec<String> = fs::read_dir(tmp.path())
        .expect("ls")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["meta.json"]);
}

#[test]
fn completion_is_saved_and_a_new_model_or_dims_store_starts_incomplete() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    assert!(!store.is_complete());
    store.mark_complete();
    store.save().expect("save");
    let snapshot = store.clone();
    assert!(
        snapshot.is_complete() && snapshot.len() == 4,
        "a clone is a full snapshot"
    );
    let (reopened, status) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    assert_eq!(status, OpenStatus::Loaded);
    assert!(reopened.is_complete());
    // Other dims: a new, empty and incomplete store.
    let (other, status) = VectorStore::open(tmp.path(), id(128)).expect("open 128");
    assert_eq!(status, OpenStatus::Created);
    assert!(!other.is_complete() && other.is_empty());
}

#[test]
fn a_keys_file_without_the_completion_field_reads_as_incomplete() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = filled(tmp.path());
    store.save().expect("save");
    let keys = fs::read_to_string(tmp.path().join(KEYS_FILE)).expect("keys");
    assert!(
        !keys.contains("complete"),
        "the field is left out while false (S3.1 files)"
    );
    let (reopened, _) = VectorStore::open(tmp.path(), id(256)).expect("reopen");
    assert!(!reopened.is_complete());
}
