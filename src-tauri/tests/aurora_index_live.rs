//! LIVE check of Start -> Additional content -> Index files against a real Aurora index over
//! HTTPS (network). Gated on `BOREALIS_LIVE_INDEX_URL`; skipped when unset. Prints what the
//! update did (per-index record, failures, installed packs, time taken).
//!
//! ```text
//! BOREALIS_LIVE_INDEX_URL=https://raw.githubusercontent.com/AuroraLegacy/elements/master/core.index \
//!     cargo test --release -p borealis --test aurora_index_live -- --nocapture
//! ```
//!
//! `BOREALIS_LIVE_DATA_DIR` keeps the app data in that folder (default: a temp dir).

use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_index::{Fetcher, HttpsFetcher};
use borealis_lib::content_packs::EventSink;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// The real fetcher, counting requests and printing failures.
struct Counting {
    ok: AtomicUsize,
    failed: AtomicUsize,
}

impl Fetcher for Counting {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        let r = HttpsFetcher.get(url, max_bytes);
        match &r {
            Ok(_) => self.ok.fetch_add(1, Ordering::Relaxed),
            Err(why) => {
                println!("  fetch failed: {url}: {why}");
                self.failed.fetch_add(1, Ordering::Relaxed)
            }
        };
        r
    }
}

/// Prints every phase change.
struct Printing;

impl EventSink for Printing {
    fn content_progress(&self, p: &ContentProgress) {
        if p.phase != "downloading" || p.done == 0 {
            println!(
                "  progress {} {}/{} {}",
                p.phase, p.done, p.total, p.message
            );
        }
    }
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, reason: &str) {
        println!("  reloaded: {reason}");
    }
}

#[test]
fn live_aurora_index_update() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_INDEX_URL") else {
        eprintln!("SKIPPED: BOREALIS_LIVE_INDEX_URL is not set");
        return;
    };
    let tmp = tempfile::tempdir().expect("temp dir");
    let data = std::env::var("BOREALIS_LIVE_DATA_DIR")
        .map_or_else(|_| tmp.path().to_path_buf(), PathBuf::from);
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data,
    };
    let state = AppState::new(paths).expect("state");
    let added = state.add_index(&url).expect("add index");
    println!("added: {added:?}");
    let fetcher = Counting {
        ok: AtomicUsize::new(0),
        failed: AtomicUsize::new(0),
    };
    for round in 1..=2 {
        let started = Instant::now();
        let result = state.update_indexes(&fetcher, &Printing).expect("update");
        println!(
            "round {round}: {:.1}s, fetched ok {}, fetch failures {}, result {result:?}",
            started.elapsed().as_secs_f64(),
            fetcher.ok.load(Ordering::Relaxed),
            fetcher.failed.load(Ordering::Relaxed),
        );
        for idx in state.list_indexes() {
            println!("  index record: {idx:?}");
        }
        for p in state.list_packs().expect("packs") {
            if p.kind != PackKind::Bundled {
                println!("  pack {} {:?}: {} elements", p.id, p.kind, p.element_count);
            }
        }
    }
}
