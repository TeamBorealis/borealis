//! Local-only reproduction of the two ways a user brings a community Aurora repository into the
//! app, run offline against a local corpus checkout. Gated on `BOREALIS_CORPUS_DIR` (the corpus
//! root holding `repos/`, or `repos/` itself); skipped when unset. Prints counts only, and asserts
//! that both paths install the same content (aurora-index-1 acceptance).
//!
//! 1. Start -> Additional content -> Index files: the repo's top-level `.index` URLs are added and
//!    updated through `update_indexes`, with a fetcher that serves
//!    `https://raw.githubusercontent.com/<owner>/<repo>/<branch>/<path>` from
//!    `repos/<owner>__<repo>/<path>` (the same bytes GitHub would serve).
//! 2. Drop the whole repository (folder / zip): every `.xml` and `.index` file, named by its path
//!    under the repository folder, goes through `preview_import_pack` + `install_import_preview`.
//!
//! ```text
//! BOREALIS_CORPUS_DIR=/path/to/corpus cargo test --release -p borealis \
//!     --test aurora_index_path -- --nocapture
//! ```
//!
//! `BOREALIS_INDEX_REPO` picks the repo folder (default `aurorabuilder__elements`).

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::PackKind;
use borealis_lib::content_index::Fetcher;
use borealis_lib::content_packs::NullSink;
use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
use borealis_lib::state::{AppPaths, AppState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const RAW: &str = "https://raw.githubusercontent.com/";

/// Serves raw.githubusercontent.com URLs from the local corpus.
struct LocalGithub {
    repos: PathBuf,
    served: AtomicUsize,
    missing: AtomicUsize,
    /// Repository-relative paths of the files served.
    served_files: Mutex<Vec<String>>,
}

impl Fetcher for LocalGithub {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        let local = url.strip_prefix(RAW).and_then(|rest| {
            let mut parts = rest.splitn(4, '/');
            let (owner, repo, _branch, path) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            Some((
                self.repos.join(format!("{owner}__{repo}")).join(path),
                path.to_string(),
            ))
        });
        match local.and_then(|(p, rel)| Some((std::fs::read(p).ok()?, rel))) {
            Some((b, rel)) if b.len() <= max_bytes => {
                self.served.fetch_add(1, Ordering::Relaxed);
                self.served_files.lock().expect("served").push(rel);
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

/// Every loaded element by id (the store a character is built against). `supports` is compared
/// as a set: `<append>`s from several Aurora groups add tokens in load order, and a dropped repo
/// loads by group (core < supplements < unearthed-arcana < third-party) while the index path
/// loads one pack per top index; the order of supports tokens carries no meaning.
fn store_by_id(state: &AppState) -> BTreeMap<String, String> {
    let hub = state.hub().expect("hub");
    hub.store
        .iter()
        .map(|e| {
            let mut el = e.element.clone();
            el.supports.sort();
            let json = serde_json::to_string(&el).expect("element json");
            (e.element.id.clone(), json)
        })
        .collect()
}

/// Elements in the packs that are not bundled.
fn installed_elements(state: &AppState) -> usize {
    state
        .list_packs()
        .expect("packs")
        .iter()
        .filter(|p| p.kind != PackKind::Bundled)
        .map(|p| p.element_count)
        .sum()
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
            let top = root.file_name().unwrap().to_string_lossy();
            out.push((format!("{top}/{rel}"), std::fs::read(&p).unwrap()));
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
        served_files: Mutex::new(Vec::new()),
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
    let indexes = state.list_indexes();
    for idx in &indexes {
        println!(
            "  index {} checked={} files={} updates_available={} verified={} problem={:?}",
            idx.name,
            idx.last_checked.is_some(),
            idx.file_count,
            idx.updates_available,
            idx.verified,
            idx.problem
        );
    }
    let index_files: usize = indexes.iter().map(|i| i.file_count).sum();
    let index_installed = installed_elements(&state);
    println!("  installed elements via indexes: {index_installed} in {index_files} files");

    // 2. Whole repository dropped as a folder / zip
    let mut files = Vec::new();
    files_under(&root, &root, &mut files);
    let (_tmp2, state2) = drop_folder(&repo, "whole repository", files.clone());
    let folder_installed = installed_elements(&state2);
    println!("  installed elements via folder: {folder_installed}");

    // 3. The same drop restricted to the files the indexes list (Aurora never loads <obsolete> or
    //    unlisted files): this is what the index path must reproduce exactly.
    let served = fetcher.served_files.lock().expect("served").clone();
    let top = root.file_name().unwrap().to_string_lossy().into_owned();
    let listed: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .filter(|(n, _)| {
            let rel = n.strip_prefix(&format!("{top}/")).unwrap_or(n);
            rel.to_lowercase().ends_with(".xml") && served.contains(&rel.to_string())
        })
        .collect();
    let (_tmp3, state3) = drop_folder(&repo, "listed files only", listed);
    let listed_installed = installed_elements(&state3);
    println!(
        "  installed elements: indexes {index_installed}, folder of listed files {listed_installed}, \
         whole folder {folder_installed} ({} more from obsolete/unlisted files)",
        folder_installed.saturating_sub(listed_installed)
    );

    // Acceptance: every top-level index checked, nothing failed, and the same content as the folder.
    assert_eq!(
        indexes.iter().filter(|i| i.last_checked.is_some()).count(),
        tops.len(),
        "every top-level index is checked"
    );
    assert!(result.failed.is_empty(), "failed: {:?}", result.failed);
    if repo == "aurorabuilder__elements" {
        let served = fetcher.served.load(Ordering::Relaxed);
        assert!(
            served >= 650,
            "only {served} files downloaded ({index_files} content files)"
        );
    }
    assert_eq!(index_installed, listed_installed, "installed element count");
    let (by_index, by_folder) = (store_by_id(&state), store_by_id(&state3));
    let differing: Vec<&String> = by_folder
        .iter()
        .filter(|(id, json)| by_index.get(*id) != Some(*json))
        .map(|(id, _)| id)
        .chain(by_index.keys().filter(|id| !by_folder.contains_key(*id)))
        .collect();
    println!(
        "  store: {} elements via indexes, {} via folder, {} differ (first: {:?})",
        by_index.len(),
        by_folder.len(),
        differing.len(),
        differing.iter().take(5).collect::<Vec<_>>()
    );
    assert!(differing.is_empty(), "the engine sees different content");
}

/// Preview and install `files` as one folder drop; prints the preview counts.
fn drop_folder(
    repo: &str,
    label: &str,
    files: Vec<(String, Vec<u8>)>,
) -> (tempfile::TempDir, AppState) {
    let (tmp, state) = app();
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
    let p = state.preview_import_pack(&request).expect("preview");
    println!(
        "[{repo}] folder path ({label}): {count} files, pack {:?}, converted {}, skipped {}, unresolved refs {}, diagnostics {:?}",
        p.pack_id, p.converted, p.skipped, p.unresolved_total, p.summary
    );
    let counts: Vec<String> = p
        .counts
        .iter()
        .map(|c| format!("{}={}", c.ty, c.count))
        .collect();
    println!("  converted by type: {}", counts.join(", "));
    state
        .install_import_preview(&p.preview_id, &NullSink)
        .expect("install");
    (tmp, state)
}
