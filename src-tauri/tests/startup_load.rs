//! B-235 (PF1): startup content loading and the content cache, over libraries shaped like a 5etools import.
//!
//! The benchmark is ignored by default; run it in release mode:
//! `cargo test --release -p borealis --test startup_load -- --ignored --nocapture`
//! (`BOREALIS_BENCH_PACKS` and `BOREALIS_BENCH_KEEP_EVERY` size the library).

#[path = "support/synthetic_library.rs"]
mod synthetic_library;

use borealis_content::Ruleset;
use borealis_lib::content_cache;
use borealis_lib::content_prefs::ContentPrefs;
use borealis_lib::settings::AppSettings;
use borealis_lib::state::{AppPaths, ContentHub};
use std::path::Path;
use std::time::{Duration, Instant};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn paths_in(data: &Path) -> AppPaths {
    AppPaths {
        content_root: synthetic_library::content_root(),
        data: data.to_path_buf(),
    }
}

fn load(paths: &AppPaths, prefs: &ContentPrefs) -> ContentHub {
    let hub = ContentHub::load_with(paths, &AppSettings::default(), prefs).expect("load");
    hub.wait_for_cache_write();
    hub
}

fn cache_of(paths: &AppPaths) -> std::path::PathBuf {
    content_cache::cache_file(&paths.data)
}

/// A hub as the files give it, the cache out of the way (removed before and after).
fn fresh(paths: &AppPaths, prefs: &ContentPrefs) -> ContentHub {
    let _ = std::fs::remove_file(cache_of(paths));
    let hub = load(paths, prefs);
    let _ = std::fs::remove_file(cache_of(paths));
    hub
}

/// Same packs, files, elements (deep), diagnostics, warnings and builder as `expected`.
fn assert_same(actual: &ContentHub, expected: &ContentHub) {
    assert_eq!(actual.store.len(), expected.store.len(), "element count");
    assert_eq!(actual.store.files(), expected.store.files());
    assert_eq!(actual.store.packs(), expected.store.packs());
    assert_eq!(actual.store.indexes(), expected.store.indexes());
    assert_eq!(actual.store.diagnostics(), expected.store.diagnostics());
    assert!(
        actual.store.iter().eq(expected.store.iter()),
        "stored elements differ"
    );
    assert!(
        actual.builder.same_content(&expected.builder),
        "builders differ"
    );
    assert_eq!(actual.load_warnings, expected.load_warnings);
    assert_eq!(
        actual.engine.universe(Ruleset::Dnd2024).len(),
        expected.engine.universe(Ruleset::Dnd2024).len()
    );
}

fn has_book(hub: &ContentHub, name: &str) -> bool {
    hub.store.by_type("Source").any(|s| s.element.name == name)
}

/// A load that read the cache leaves it untouched; a full load rewrites it.
fn modified(file: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

struct Fixture {
    _dir: tempfile::TempDir,
    paths: AppPaths,
}

fn fixture(packs: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    synthetic_library::write_library(&paths.library(), packs, 9);
    Fixture { _dir: dir, paths }
}

/// The cache file after one full load, and the hub that load gave.
fn primed(f: &Fixture, prefs: &ContentPrefs) -> ContentHub {
    let _ = std::fs::remove_file(cache_of(&f.paths));
    let hub = load(&f.paths, prefs);
    assert!(cache_of(&f.paths).is_file(), "a full load writes the cache");
    hub
}

/// True when the next load used the cache (it does not rewrite the file).
fn next_load_hits(f: &Fixture, prefs: &ContentPrefs) -> (bool, ContentHub) {
    let file = cache_of(&f.paths);
    let before = std::fs::read(&file).ok();
    let stamp = modified(&file);
    // Make a rewrite visible even on coarse clocks.
    std::thread::sleep(Duration::from_millis(20));
    let hub = load(&f.paths, prefs);
    let after = std::fs::read(&file).ok();
    (
        before.is_some() && before == after && stamp == modified(&file),
        hub,
    )
}

#[test]
fn a_cache_hit_gives_the_same_content_as_a_fresh_load() {
    let f = fixture(6);
    let prefs = ContentPrefs::default();
    let cold = primed(&f, &prefs);
    let (hit, warm) = next_load_hits(&f, &prefs);
    assert!(hit, "the second load reads the cache");
    assert_same(&warm, &cold);
    assert_same(&warm, &fresh(&f.paths, &prefs));
    assert!(warm.store.len() > 4_000, "{}", warm.store.len());
}

#[test]
fn a_changed_content_file_forces_a_fresh_load() {
    let f = fixture(3);
    let prefs = ContentPrefs::default();
    primed(&f, &prefs);
    let pack = f.paths.library().join("5etools-syn001");
    let file = pack.join("source.json");
    let text = std::fs::read_to_string(&file).unwrap();
    let renamed = text.replace("Synthetic Book 001", "Synthetic Book 1b");
    std::fs::write(&file, renamed).unwrap();
    let (hit, hub) = next_load_hits(&f, &prefs);
    assert!(!hit, "an edited file is not read from the cache");
    assert!(has_book(&hub, "Synthetic Book 1b"));
    assert_same(&hub, &fresh(&f.paths, &prefs));
}

#[test]
fn a_same_size_rewrite_forces_a_fresh_load() {
    let f = fixture(3);
    let prefs = ContentPrefs::default();
    primed(&f, &prefs);
    let file = f.paths.library().join("5etools-syn002").join("source.json");
    let text = std::fs::read_to_string(&file).unwrap();
    let same_len = text.replace("Synthetic Book 002", "Synthetic Book 00X");
    assert_eq!(same_len.len(), text.len());
    std::fs::write(&file, same_len).unwrap();
    let (hit, hub) = next_load_hits(&f, &prefs);
    assert!(!hit);
    assert!(has_book(&hub, "Synthetic Book 00X"));
}

/// SY7a: a backup restore writes the files anew (and the repair then sets their mtimes back), so the
/// same bytes and the same mtime still miss on Unix (change time and inode differ).
#[cfg(unix)]
#[test]
fn a_restored_file_with_its_old_mtime_forces_a_fresh_load() {
    let f = fixture(2);
    let prefs = ContentPrefs::default();
    primed(&f, &prefs);
    let file = f.paths.library().join("5etools-syn000").join("spells.json");
    let bytes = std::fs::read(&file).unwrap();
    let mtime = modified(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, &bytes).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(modified(&file), Some(mtime));
    let (hit, _) = next_load_hits(&f, &prefs);
    assert!(!hit, "a rewritten file is never taken from the cache");
}

#[test]
fn an_added_or_removed_pack_forces_a_fresh_load() {
    let f = fixture(3);
    let prefs = ContentPrefs::default();
    let three = primed(&f, &prefs);
    std::fs::remove_dir_all(f.paths.library().join("5etools-syn002")).unwrap();
    let (hit, two) = next_load_hits(&f, &prefs);
    assert!(!hit, "a deleted pack is not read from the cache");
    assert!(two.store.len() < three.store.len());
    synthetic_library::write_library(&f.paths.library(), 3, 9);
    let (hit, again) = next_load_hits(&f, &prefs);
    assert!(!hit, "an imported pack is not missed");
    assert_same(&again, &three);
}

#[test]
fn disabling_a_pack_or_setting_an_edition_forces_a_fresh_load() {
    let f = fixture(3);
    let all = ContentPrefs::default();
    primed(&f, &all);
    let mut off = ContentPrefs::default();
    off.disabled_packs.insert("5etools-syn001".into());
    let (hit, hub) = next_load_hits(&f, &off);
    assert!(!hit, "disabled pack");
    assert_same(&hub, &fresh(&f.paths, &off));
    primed(&f, &all);
    let mut edition = ContentPrefs::default();
    edition.set_source_edition("Synthetic Book 000", Ruleset::Dnd2024);
    let (hit, hub) = next_load_hits(&f, &edition);
    assert!(!hit, "edition set");
    let (hit, again) = next_load_hits(&f, &edition);
    assert!(hit, "the edition load wrote a cache for itself");
    assert_same(&again, &hub);
    assert_same(&hub, &fresh(&f.paths, &edition));
}

#[test]
fn a_corrupt_or_truncated_cache_falls_back_to_a_full_load() {
    let f = fixture(2);
    let prefs = ContentPrefs::default();
    let expected = primed(&f, &prefs);
    let file = cache_of(&f.paths);
    let good = std::fs::read(&file).unwrap();
    let mut flipped = good.clone();
    let mid = flipped.len() / 2;
    flipped[mid] ^= 0x5A;
    let damaged: [Vec<u8>; 5] = [
        good[..good.len() / 3].to_vec(),
        good[..good.len() - 1].to_vec(),
        flipped,
        Vec::new(),
        b"not a cache at all".to_vec(),
    ];
    for (i, bytes) in damaged.iter().enumerate() {
        std::fs::write(&file, bytes).unwrap();
        let hub = load(&f.paths, &prefs);
        assert_same(&hub, &expected);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            good,
            "case {i}: the full load rewrote a good cache"
        );
    }
}

#[test]
fn a_bad_user_pack_is_still_skipped_with_a_warning_on_a_cache_hit() {
    let f = fixture(3);
    let prefs = ContentPrefs::default();
    std::fs::write(
        f.paths.library().join("5etools-syn001").join("spells.json"),
        "{ not json",
    )
    .unwrap();
    let cold = primed(&f, &prefs);
    assert_eq!(cold.load_warnings.len(), 1, "{:?}", cold.load_warnings);
    assert!(cold.load_warnings[0].starts_with("A user content pack could not be loaded"));
    assert!(cold.store.packs().iter().all(|p| p.id != "5etools-syn001"));
    assert!(cold.store.packs().iter().any(|p| p.id == "5etools-syn002"));
    assert!(cold
        .store
        .files()
        .iter()
        .all(|f| !f.starts_with("5etools-syn001/")));
    assert!(cold.store.element("ID_SOURCE_SYN002").is_some());
    let (hit, warm) = next_load_hits(&f, &prefs);
    assert!(hit);
    assert_same(&warm, &cold);
}

#[test]
fn clear_caches_removes_the_content_cache() {
    let f = fixture(1);
    let state = borealis_lib::state::AppState::new(f.paths.clone()).unwrap();
    state.hub().unwrap().wait_for_cache_write();
    assert!(cache_of(&f.paths).is_file());
    state.clear_caches(|| 0).unwrap();
    assert!(!cache_of(&f.paths).exists());
}

#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn bench_startup_load_of_a_large_library() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();
    let packs = env_usize("BOREALIS_BENCH_PACKS", 120);
    let keep_every = env_usize("BOREALIS_BENCH_KEEP_EVERY", 3);
    let data = tempfile::tempdir().unwrap();
    let paths = paths_in(data.path());
    let written = synthetic_library::write_library(&paths.library(), packs, keep_every);
    eprintln!(
        "library: {packs} packs, {written} elements, {:.1} MB of JSON",
        walk_size(&paths.library()) as f64 / 1e6
    );
    let prefs = ContentPrefs::default();
    for run in 1..=3 {
        let _ = std::fs::remove_file(cache_of(&paths));
        let began = Instant::now();
        let hub = ContentHub::load_with(&paths, &AppSettings::default(), &prefs).unwrap();
        let took = began.elapsed();
        hub.wait_for_cache_write();
        eprintln!(
            "cold {run}: {} ms, {} elements",
            took.as_millis(),
            hub.store.len()
        );
    }
    let size = std::fs::metadata(cache_of(&paths)).map_or(0, |m| m.len());
    eprintln!("cache file: {:.1} MB", size as f64 / 1e6);
    for run in 1..=3 {
        let began = Instant::now();
        let hub = ContentHub::load_with(&paths, &AppSettings::default(), &prefs).unwrap();
        eprintln!(
            "warm {run}: {} ms, {} elements",
            began.elapsed().as_millis(),
            hub.store.len()
        );
    }
}

fn walk_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                walk_size(&p)
            } else {
                p.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}
