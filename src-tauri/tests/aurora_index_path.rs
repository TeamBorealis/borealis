//! Local-only reproduction of the two ways a user brings a community Aurora repository into the
//! app, run offline against a local corpus checkout. Gated on `BOREALIS_CORPUS_DIR` (the corpus
//! root holding `repos/`, or `repos/` itself); skipped when unset. Prints counts only.
//!
//! 1. Start -> Additional content -> Index files: the repo's top-level `.index` URLs are added and
//!    updated through `update_indexes`, with a fetcher that serves
//!    `https://raw.githubusercontent.com/<owner>/<repo>/<branch>/<path>` from
//!    `repos/<owner>__<repo>/<path>` (the same bytes GitHub would serve).
//! 2. Drop the whole repository (folder / zip): every `.xml` and `.index` file goes through
//!    `preview_import_pack` + `install_import_preview`.
//!
//! ```text
//! BOREALIS_CORPUS_DIR=/path/to/corpus cargo test --release -p borealis \
//!     --test aurora_index_path -- --nocapture
//! ```
//!
//! `BOREALIS_INDEX_REPO` picks the repo folder (default `aurorabuilder__elements`).

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_index::Fetcher;
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::state::{AppPaths, AppState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const RAW: &str = "https://raw.githubusercontent.com/";

/// Serves raw.githubusercontent.com URLs from the local corpus.
struct LocalGithub {
    repos: PathBuf,
    served: AtomicUsize,
    missing: AtomicUsize,
}

impl Fetcher for LocalGithub {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        let local = url.strip_prefix(RAW).and_then(|rest| {
            let mut parts = rest.splitn(4, '/');
            let (owner, repo, _branch, path) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            Some(self.repos.join(format!("{owner}__{repo}")).join(path))
        });
        match local.and_then(|p| std::fs::read(p).ok()) {
            Some(b) if b.len() <= max_bytes => {
                self.served.fetch_add(1, Ordering::Relaxed);
                Ok(b)
            }
            Some(_) => Err("the file is too large".into()),
            None => {
                self.missing.fetch_add(1, Ordering::Relaxed);
                Err("the server answered 404 Not Found".into())
            }
        }
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn files_under(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if !p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                files_under(root, &p, out);
            }
            continue;
        }
        let lower = p.to_string_lossy().to_lowercase();
        if lower.ends_with(".xml") || lower.ends_with(".index") {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, std::fs::read(&p).unwrap()));
        }
    }
}

#[test]
fn aurora_repo_through_the_app_paths() {
    let Ok(dir) = std::env::var("BOREALIS_CORPUS_DIR") else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set");
        return;
    };
    let dir = PathBuf::from(dir);
    let repos = if dir.join("repos").is_dir() {
        dir.join("repos")
    } else {
        dir
    };
    let repo =
        std::env::var("BOREALIS_INDEX_REPO").unwrap_or_else(|_| "aurorabuilder__elements".into());
    let root = repos.join(&repo);
    let (owner, name) = repo.split_once("__").expect("owner__repo folder name");

    // 1. Index files
    let (_tmp, state) = app();
    let mut tops: Vec<String> = std::fs::read_dir(&root)
        .expect("repo dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".index"))
        .collect();
    tops.sort();
    for top in &tops {
        state
            .add_index(&format!("{RAW}{owner}/{name}/master/{top}"))
            .expect("add index");
    }
    let fetcher = LocalGithub {
        repos: repos.clone(),
        served: AtomicUsize::new(0),
        missing: AtomicUsize::new(0),
    };
    let result = state.update_indexes(&fetcher, &NullSink).expect("update");
    println!(
        "[{repo}] index path: {} top-level indexes, {} files served, {} 404s, updated packs {}, failed {}, reloaded {}",
        tops.len(),
        fetcher.served.load(Ordering::Relaxed),
        fetcher.missing.load(Ordering::Relaxed),
        result.updated,
        result.failed.len(),
        result.reloaded
    );
    for idx in state.list_indexes() {
        println!(
            "  index checked={} files={} updates_available={} verified={}",
            idx.last_checked.is_some(),
            idx.file_count,
            idx.updates_available,
            idx.verified
        );
    }

    // 2. Whole repository dropped as a folder / zip
    let (_tmp2, state2) = app();
    let mut files = Vec::new();
    files_under(&root, &root, &mut files);
    let count = files.len();
    let request = PreviewImportRequest {
        files: files
            .into_iter()
            .map(|(file_name, bytes)| ImportFileInput {
                file_name,
                content_base64: STANDARD.encode(bytes),
            })
            .collect(),
        folder: None,
    };
    match state2.preview_import_pack(&request) {
        Err(e) => println!("[{repo}] folder path: preview refused: {}", e.message),
        Ok(p) => {
            println!(
                "[{repo}] folder path: {count} files, converted {}, skipped {}, unresolved refs {}, diagnostics {:?}",
                p.converted, p.skipped, p.unresolved_total, p.summary
            );
            let counts: Vec<String> = p
                .counts
                .iter()
                .map(|c| format!("{}={}", c.ty, c.count))
                .collect();
            println!("  converted by type: {}", counts.join(", "));
            match state2.install_import_preview(&p.preview_id, &NullSink) {
                Ok(r) => println!(
                    "  installed: pack {:?}, report converted {}",
                    r.pack.map(|x| x.id),
                    r.report.converted
                ),
                Err(e) => println!("  install refused: {}", e.message),
            }
        }
    }
}
