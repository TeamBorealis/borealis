//! B-235 (PF1): startup content loading over a library shaped like a full 5etools import.
//!
//! The benchmark is ignored by default; run it in release mode:
//! `cargo test --release -p borealis --test startup_load -- --ignored --nocapture`
//! (`BOREALIS_BENCH_PACKS` and `BOREALIS_BENCH_KEEP_EVERY` size the library).

#[path = "support/synthetic_library.rs"]
mod synthetic_library;

use borealis_lib::content_prefs::ContentPrefs;
use borealis_lib::settings::AppSettings;
use borealis_lib::state::{AppPaths, ContentHub};
use std::time::{Duration, Instant};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn time_load(paths: &AppPaths) -> (Duration, usize) {
    let began = Instant::now();
    let hub = ContentHub::load_with(paths, &AppSettings::default(), &ContentPrefs::default())
        .expect("load");
    (began.elapsed(), hub.store.len())
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
    let paths = AppPaths {
        content_root: synthetic_library::content_root(),
        data: data.path().to_path_buf(),
    };
    let written = synthetic_library::write_library(&paths.library(), packs, keep_every);
    let bytes: u64 = walk_size(&paths.library());
    eprintln!(
        "library: {packs} packs, {written} elements, {:.1} MB of JSON",
        bytes as f64 / 1e6
    );
    for run in 1..=3 {
        let (took, elements) = time_load(&paths);
        eprintln!("run {run}: {} ms, {elements} elements", took.as_millis());
    }
}

fn walk_size(dir: &std::path::Path) -> u64 {
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
