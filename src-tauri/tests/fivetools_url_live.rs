//! Live 5etools import from a real address over HTTPS (env-gated; never runs in CI).
//!
//! `BOREALIS_LIVE_5ETOOLS_URL=<GitHub repository or data/ folder address> cargo test -p borealis
//! --release --test fivetools_url_live -- --nocapture`. No address is kept in the repository.
//! Prints download time, size and counts; a second Update must say "up to date".

use borealis_lib::content_dto::FiveToolsImportRequest;
use borealis_lib::content_index::HttpsFetcher;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn tree_size(dir: &Path) -> (usize, u64) {
    let mut out = (0, 0);
    for e in std::fs::read_dir(dir).expect("dir").map(Result::unwrap) {
        let p = e.path();
        if p.is_dir() {
            let (n, b) = tree_size(&p);
            out = (out.0 + n, out.1 + b);
        } else {
            out = (
                out.0 + 1,
                out.1 + p.metadata().map(|m| m.len()).unwrap_or(0),
            );
        }
    }
    out
}

#[test]
fn live_5etools_url_import_and_update() {
    let Ok(url) = std::env::var("BOREALIS_LIVE_5ETOOLS_URL") else {
        eprintln!("SKIP: BOREALIS_LIVE_5ETOOLS_URL is not set");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");
    let app = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    })
    .expect("state");

    let t = Instant::now();
    let picked = app
        .stage_5etools_url(&HttpsFetcher, &url, &NullSink)
        .expect("download");
    let download = t.elapsed();
    let staged = app
        .paths
        .data
        .join(borealis_lib::content_fivetools_url::STAGE_DIR);
    let (files, bytes) = tree_size(&staged);

    let t = Instant::now();
    let scan = app.scan_5etools_folder(&picked.token).expect("scan");
    let sources: Vec<String> = scan.sources.iter().map(|s| s.abbr.clone()).collect();
    let request = FiveToolsImportRequest {
        folder: picked.token,
        sources: sources.clone(),
        include_fluff: false,
    };
    let report = app.import_5etools(&request, &NullSink).expect("import");
    let import = t.elapsed();

    let t = Instant::now();
    let update = app
        .update_5etools_url(&HttpsFetcher, &url, &NullSink)
        .expect("update");
    let check = t.elapsed();
    eprintln!(
        "LIVE 5etools url: download {:.1}s, {files} files, {:.1} MB; scan+import {:.1}s: {} sources, {} elements, {} packs, {} partial, {} skipped; update {:.1}s up_to_date={}",
        download.as_secs_f64(),
        bytes as f64 / 1e6,
        import.as_secs_f64(),
        sources.len(),
        report.converted,
        report.packs.len(),
        report.partial.len(),
        report.skipped.len(),
        check.as_secs_f64(),
        update.up_to_date,
    );
    assert!(
        report.converted > 1_000,
        "a full data folder converts thousands of elements"
    );
    assert!(update.up_to_date, "an unchanged address must not re-import");
    assert_eq!(app.list_5etools_urls()[0].sources, sources);
}
