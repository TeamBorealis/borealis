//! A large imported library for the creator's device suite (spec 18.5 phone budget, item IDpwE): the local corpora
//! imported through the real `AppState`, as a user imports them, into a data folder whose `library/` and
//! `content-prefs.json` the device suite copies into the phone's app data (`run.mjs --library <dir>`).
//!
//! Not a check: it only writes the folder, so it is `#[ignore]` and runs on demand:
//!
//! ```sh
//! BOREALIS_DEVICE_LIBRARY_OUT=../target/device-library \
//! BOREALIS_5ETOOLS_DIR=../../aurora-re/corpus/5etools/data BOREALIS_CORPUS_DIR=../../aurora-re/corpus \
//! cargo test -p borealis --test device_library -- --ignored --nocapture
//! ```
//!
//! * `BOREALIS_5ETOOLS_DIR`: a 5etools `data` folder, every source imported (Import 5etools, all books).
//! * `BOREALIS_CORPUS_DIR`: Aurora element repositories (`<dir>/repos/*` or `<dir>/*`), each zipped (its `.xml` and
//!   `.index` files) and imported as one content file (Import content with a `.zip`).
//!
//! Either may be left out. A repository the import refuses is skipped and named in the output.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::{FiveToolsImportRequest, ImportPackRequest};
use borealis_lib::content_packs::NullSink;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The import's own cap (`MAX_PACK_BYTES`): a larger zip is refused by the app, so it is skipped here.
const MAX_ZIP_BYTES: usize = 64 * 1024 * 1024;

fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Every `.xml` and `.index` file under `dir` (not `.git`), sorted, relative to `dir`.
fn content_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("xml") || x.eq_ignore_ascii_case("index"))
            {
                out.push(p.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

/// A repository as the zip a user would pick.
fn zip_repo(dir: &Path) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for rel in content_files(dir) {
        zip.start_file(rel.to_string_lossy().replace('\\', "/"), options)
            .unwrap();
        zip.write_all(&std::fs::read(dir.join(&rel)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn import_5etools(app: &AppState, data: &Path) {
    let t0 = Instant::now();
    let folder = app
        .picked
        .grant(data.to_path_buf(), PickPurpose::FivetoolsFolder)
        .unwrap()
        .token;
    let sources: Vec<String> = app
        .scan_5etools_folder(&folder)
        .expect("scan the 5etools folder")
        .sources
        .into_iter()
        .map(|s| s.abbr)
        .collect();
    let report = app
        .import_5etools(
            &FiveToolsImportRequest {
                folder,
                sources,
                include_fluff: false,
            },
            &NullSink,
        )
        .expect("import 5etools");
    println!(
        "5etools: {} converted, {} packs, {:.1} s",
        report.converted,
        report.packs.len(),
        t0.elapsed().as_secs_f64()
    );
}

fn import_aurora(app: &AppState, corpus: &Path) {
    let repos = if corpus.join("repos").is_dir() {
        corpus.join("repos")
    } else {
        corpus.to_path_buf()
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&repos)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let (mut ok, mut skipped) = (0, Vec::new());
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let bytes = zip_repo(&dir);
        if bytes.len() > MAX_ZIP_BYTES {
            skipped.push(format!("{name} (zip of {} MB)", bytes.len() >> 20));
            continue;
        }
        let request = ImportPackRequest {
            file_name: format!("{name}.zip"),
            content_base64: STANDARD.encode(&bytes),
            format: None,
        };
        match app.import_pack(&request, &NullSink) {
            Ok(_) => ok += 1,
            Err(e) => skipped.push(format!("{name} ({})", e.message)),
        }
    }
    println!(
        "aurora: {ok} repositories imported, {} skipped",
        skipped.len()
    );
    for s in skipped {
        println!("  skipped {s}");
    }
}

#[test]
#[ignore = "writes a device library on demand (BOREALIS_DEVICE_LIBRARY_OUT)"]
fn write_the_device_library() {
    let out = PathBuf::from(
        std::env::var_os("BOREALIS_DEVICE_LIBRARY_OUT").expect("BOREALIS_DEVICE_LIBRARY_OUT"),
    );
    assert!(
        !out.exists() || std::fs::read_dir(&out).unwrap().next().is_none(),
        "{} must be empty or absent",
        out.display()
    );
    std::fs::create_dir_all(&out).unwrap();
    let app = AppState::new(AppPaths {
        content_root: content_root(),
        data: out.clone(),
    })
    .unwrap();
    let bundled = app.hub().unwrap().store.len();
    if let Some(dir) = env_dir("BOREALIS_5ETOOLS_DIR") {
        import_5etools(&app, &dir);
    }
    if let Some(dir) = env_dir("BOREALIS_CORPUS_DIR") {
        import_aurora(&app, &dir);
    }
    let hub = app.hub().unwrap();
    println!(
        "library: {} elements in the store ({} bundled), {} packs, in {}",
        hub.store.len(),
        bundled,
        hub.store.packs().len(),
        out.display()
    );
}
