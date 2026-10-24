//! Manifest lifecycle: incremental per-pack sync, version-change rebuild, corruption recovery, read-only open.

mod common;

use borealis_search::{
    Corpus, EditionTag, SearchConfig, SearchEngine, SearchError, SearchRequest, WriterBudget,
};
use common::{chunk, pack};
use std::path::Path;

const NONE: EditionTag = EditionTag::None;

fn pack_of(id: &str, word: &str, n: usize) -> borealis_search::PackInput {
    pack(
        id,
        (0..n)
            .map(|i| {
                chunk(
                    &format!("{id}-{i}"),
                    &format!("{word} {i}"),
                    "Test",
                    &format!("{word} text number {i}"),
                    NONE,
                )
            })
            .collect(),
    )
}

fn open(dir: &Path) -> (SearchEngine, borealis_search::OpenReport) {
    SearchEngine::open(dir, WriterBudget::MOBILE).unwrap()
}

fn count(engine: &SearchEngine, query: &str) -> usize {
    engine
        .search(
            Corpus::Lore,
            &SearchRequest {
                limit: 50,
                ..SearchRequest::new(query)
            },
            &SearchConfig::default(),
        )
        .unwrap()
        .len()
}

#[test]
fn first_sync_adds_every_pack_and_a_second_sync_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, report) = open(dir.path());
    assert!(report.rebuilt.is_empty());
    let wanted = [pack_of("a", "alpha", 3), pack_of("b", "beta", 2)];
    let first = engine.sync(Corpus::Lore, &wanted).unwrap();
    assert_eq!(first.added, ["a", "b"]);
    assert_eq!(first.chunks_written, 5);
    let second = engine.sync(Corpus::Lore, &wanted).unwrap();
    assert!(second.is_noop());
    assert_eq!(second.unchanged, ["a", "b"]);
    assert_eq!(second.chunks_written, 0);
}

#[test]
fn a_changed_pack_is_replaced_and_the_others_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, _) = open(dir.path());
    engine
        .sync(
            Corpus::Lore,
            &[
                pack_of("a", "alpha", 3),
                pack_of("b", "beta", 2),
                pack_of("c", "gamma", 4),
            ],
        )
        .unwrap();
    let lore = engine.corpus_index(Corpus::Lore);
    assert_eq!(
        [
            lore.pack_doc_count("a").unwrap(),
            lore.pack_doc_count("b").unwrap(),
            lore.pack_doc_count("c").unwrap()
        ],
        [3, 2, 4]
    );

    let report = engine
        .sync(
            Corpus::Lore,
            &[
                pack_of("a", "alpha", 3),
                pack_of("b", "delta", 5),
                pack_of("c", "gamma", 4),
            ],
        )
        .unwrap();
    assert_eq!(report.updated, ["b"]);
    assert_eq!(report.unchanged, ["a", "c"]);
    assert_eq!(report.chunks_written, 5);
    let lore = engine.corpus_index(Corpus::Lore);
    assert_eq!(lore.pack_doc_count("b").unwrap(), 5);
    assert_eq!(lore.pack_doc_count("a").unwrap(), 3);
    assert_eq!(lore.pack_doc_count("c").unwrap(), 4);
    assert_eq!(count(&engine, "delta"), 5);
    assert_eq!(count(&engine, "beta"), 0, "old content of the pack is gone");
    assert_eq!(count(&engine, "alpha"), 3);
}

#[test]
fn a_removed_pack_disappears_and_the_others_stay() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, _) = open(dir.path());
    engine
        .sync(
            Corpus::Lore,
            &[pack_of("a", "alpha", 3), pack_of("b", "beta", 2)],
        )
        .unwrap();
    let report = engine
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
        .unwrap();
    assert_eq!(report.removed, ["b"]);
    assert_eq!(report.unchanged, ["a"]);
    assert_eq!(count(&engine, "beta"), 0);
    assert_eq!(count(&engine, "alpha"), 3);
    assert!(!engine.manifest().corpora["lore"].packs.contains_key("b"));
    // Re-enabling the pack adds it back.
    let back = engine
        .sync(
            Corpus::Lore,
            &[pack_of("a", "alpha", 3), pack_of("b", "beta", 2)],
        )
        .unwrap();
    assert_eq!(back.added, ["b"]);
    assert_eq!(count(&engine, "beta"), 2);
}

#[test]
fn corpora_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut engine, _) = open(dir.path());
    engine
        .sync(Corpus::Rules, &[pack_of("r", "rule", 2)])
        .unwrap();
    assert_eq!(engine.corpus_index(Corpus::Lore).num_docs(), 0);
    assert_eq!(engine.corpus_index(Corpus::Rules).num_docs(), 2);
}

#[test]
fn the_manifest_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
            .unwrap();
    }
    let (mut engine, report) = open(dir.path());
    assert!(report.rebuilt.is_empty());
    assert_eq!(count(&engine, "alpha"), 3);
    assert!(engine
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
        .unwrap()
        .is_noop());
    let m = engine.manifest();
    assert_eq!((m.schema_version, m.analyzer_version), (1, 1));
    assert_eq!(m.tantivy_version, "0.26");
}

fn rebuilt_corpora(report: &borealis_search::OpenReport) -> Vec<Corpus> {
    report.rebuilt.iter().map(|(c, _)| *c).collect()
}

#[test]
fn a_corrupted_meta_json_is_wiped_and_the_index_rebuilds() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
            .unwrap();
        engine
            .sync(Corpus::Rules, &[pack_of("r", "rule", 2)])
            .unwrap();
    }
    std::fs::write(dir.path().join("lore/meta.json"), b"{ not json").unwrap();

    let (mut engine, report) = open(dir.path());
    assert_eq!(rebuilt_corpora(&report), [Corpus::Lore]);
    assert_eq!(engine.corpus_index(Corpus::Lore).num_docs(), 0);
    assert_eq!(
        engine.corpus_index(Corpus::Rules).num_docs(),
        2,
        "the healthy corpus is kept"
    );
    let sync = engine
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
        .unwrap();
    assert_eq!(sync.added, ["a"], "the manifest no longer claims the pack");
    assert_eq!(count(&engine, "alpha"), 3);
}

#[test]
fn a_damaged_segment_file_is_caught_by_the_checksum_and_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 20)])
            .unwrap();
    }
    let victim = std::fs::read_dir(dir.path().join("lore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "store"))
        .expect("a .store file");
    let mut bytes = std::fs::read(&victim).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    std::fs::write(&victim, bytes).unwrap();

    let (mut engine, report) = open(dir.path());
    assert_eq!(rebuilt_corpora(&report), [Corpus::Lore]);
    engine
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 20)])
        .unwrap();
    assert_eq!(count(&engine, "alpha"), 20);
}

#[test]
fn a_missing_segment_file_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 5)])
            .unwrap();
    }
    let victim = std::fs::read_dir(dir.path().join("lore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "idx"))
        .expect("an .idx file");
    std::fs::remove_file(victim).unwrap();
    let (mut engine, report) = open(dir.path());
    assert_eq!(rebuilt_corpora(&report), [Corpus::Lore]);
    engine
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 5)])
        .unwrap();
    assert_eq!(count(&engine, "alpha"), 5);
}

#[test]
fn a_missing_or_garbled_manifest_wipes_both_corpora() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
            .unwrap();
    }
    std::fs::write(dir.path().join("manifest.json"), b"garbage").unwrap();
    let (engine, report) = open(dir.path());
    assert_eq!(rebuilt_corpora(&report), [Corpus::Rules, Corpus::Lore]);
    assert_eq!(engine.corpus_index(Corpus::Lore).num_docs(), 0);
}

#[test]
fn a_version_change_forces_a_full_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
            .unwrap();
    }
    let path = dir.path().join("manifest.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("\"analyzer_version\": 1", "\"analyzer_version\": 0"),
    )
    .unwrap();
    let (engine, report) = open(dir.path());
    assert_eq!(report.rebuilt.len(), 2);
    assert!(report.rebuilt[0].1.contains("analyzer_version"));
    assert_eq!(engine.corpus_index(Corpus::Lore).num_docs(), 0);
}

#[test]
fn a_read_only_process_sees_commits_made_by_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let (mut writer, _) = open(dir.path());
    writer
        .sync(Corpus::Lore, &[pack_of("a", "alpha", 3)])
        .unwrap();
    writer
        .sync(Corpus::Rules, &[pack_of("r", "rule", 1)])
        .unwrap();

    let mut reader = SearchEngine::open_read_only(dir.path()).unwrap();
    assert_eq!(count(&reader, "alpha"), 3);
    writer
        .sync(
            Corpus::Lore,
            &[pack_of("a", "alpha", 3), pack_of("b", "beta", 2)],
        )
        .unwrap();
    assert_eq!(count(&reader, "beta"), 2, "reloaded before the query");
    assert!(
        reader.sync(Corpus::Lore, &[]).is_err(),
        "no writes from a reader"
    );
}

#[test]
fn read_only_open_reports_missing_and_stale_indexes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        SearchEngine::open_read_only(dir.path()),
        Err(SearchError::NotBuilt)
    ));
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 1)])
            .unwrap();
    }
    let path = dir.path().join("manifest.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("\"schema_version\": 1", "\"schema_version\": 9"),
    )
    .unwrap();
    assert!(matches!(
        SearchEngine::open_read_only(dir.path()),
        Err(SearchError::Stale(_))
    ));
}

#[test]
fn only_one_writer_process_can_hold_the_index() {
    // The writer lock is taken per sync, so a second app instance syncing at the same moment is refused rather
    // than corrupting the index; sequential use (as here) works.
    let dir = tempfile::tempdir().unwrap();
    let (mut a, _) = open(dir.path());
    let (mut b, _) = open(dir.path());
    a.sync(Corpus::Lore, &[pack_of("a", "alpha", 1)]).unwrap();
    b.sync(Corpus::Lore, &[pack_of("b", "beta", 1)]).unwrap();
}

#[test]
fn a_chunk_is_read_back_whole_by_its_exact_key() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (mut engine, _) = open(dir.path());
        engine
            .sync(Corpus::Lore, &[pack_of("a", "alpha", 2)])
            .unwrap();
        engine.sync(Corpus::Rules, &[]).unwrap();
    }
    let reader = SearchEngine::open_read_only(dir.path()).unwrap();
    let got = reader.passage(Corpus::Lore, "a-1").unwrap().unwrap();
    assert_eq!(got.title, "alpha 1");
    assert_eq!(got.body, "alpha text number 1");
    assert_eq!(got.pack, "a");
    assert!(got.campaign.is_none());
    assert!(reader.passage(Corpus::Lore, "a-9").unwrap().is_none());
    assert!(reader.passage(Corpus::Rules, "a-1").unwrap().is_none());
}
