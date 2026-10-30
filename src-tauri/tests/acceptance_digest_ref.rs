//! S5 macOS reference (specs/acceptance-imports-1.md, S5): installs the phone inputs through the
//! same commands the phone UI calls, in the same order (Update indexes with an index URL, an
//! Aurora `.zip` via preview + install, a 5etools `.zip` via stage + scan + import of every book,
//! fluff off), restarts, and writes the debug-only `acceptance_digest` the phones must equal.
//!
//! Env (skip-pass when any is unset; inputs stay outside the repo): `BOREALIS_S5_INDEX_URL`,
//! `BOREALIS_S5_A_USER_ZIP`, `BOREALIS_S5_F_FULL_ZIP`, `BOREALIS_S5_OUT` (result JSON path);
//! optional `BOREALIS_S5_DATA` installs into that (new) folder and keeps it.
//! The zips are sent under their own file names, as a phone's file chooser does.

/// The command and its code exist only in debug builds: each item in lib.rs sits under
/// `#[cfg(debug_assertions)]`, so a release build neither compiles nor registers it.
#[test]
fn the_acceptance_digest_is_debug_only() {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("lib.rs");
    let lines: Vec<&str> = lib.lines().map(str::trim).collect();
    for item in [
        "pub mod acceptance_digest;",
        "pub mod commands_acceptance;",
        "commands_acceptance::acceptance_digest,",
    ] {
        let at = lines
            .iter()
            .position(|l| *l == item)
            .unwrap_or_else(|| panic!("{item} missing"));
        assert_eq!(
            lines[at - 1],
            "#[cfg(debug_assertions)]",
            "{item} must be debug-only"
        );
    }
    let boot = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/startup_boot.rs"))
        .expect("startup_boot.rs");
    let hook = boot
        .find("acceptance_digest::answer_request")
        .expect("startup hook");
    assert!(boot[..hook]
        .rfind("#[cfg(debug_assertions)]")
        .is_some_and(|c| hook - c < 200));
}

/// The reference itself needs the debug-only command.
#[cfg(debug_assertions)]
mod reference {
    use base64::Engine as _;
    use borealis_lib::acceptance_digest;
    use borealis_lib::content_dto::FiveToolsImportRequest;
    use borealis_lib::content_index::HttpsFetcher;
    use borealis_lib::content_packs::NullSink;
    use borealis_lib::content_preview::{ImportFileInput, PreviewImportRequest};
    use borealis_lib::state::{AppPaths, AppState};
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    fn env(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }

    fn file_input(path: &Path) -> ImportFileInput {
        let bytes = std::fs::read(path).expect("read input zip");
        ImportFileInput {
            file_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    fn app_at(data: &Path) -> AppState {
        AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.to_path_buf(),
        })
        .expect("state")
    }

    fn ms(t: Instant) -> u64 {
        u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Install the three inputs; returns the time of each step in ms.
    fn install(app: &AppState, url: &str, a_user: &Path, f_full: &Path) -> Value {
        let t = Instant::now();
        app.add_index(url).expect("add index");
        let r = app
            .update_indexes(&HttpsFetcher, &NullSink)
            .expect("update indexes");
        let index_ms = ms(t);
        eprintln!("index: {} pack(s) updated in {index_ms} ms", r.updated);

        let t = Instant::now();
        let request = PreviewImportRequest {
            files: vec![file_input(a_user)],
            folder: None,
        };
        let preview = app.preview_import_pack(&request).expect("preview a-user");
        app.install_import_preview(&preview.preview_id, None, &NullSink)
            .expect("install a-user");
        let a_user_ms = ms(t);

        let t = Instant::now();
        let pick = app
            .stage_5etools_zip(&file_input(f_full))
            .expect("stage f-full");
        let scan = app.scan_5etools_folder(&pick.token).expect("scan f-full");
        let request = FiveToolsImportRequest {
            folder: pick.token,
            sources: scan.sources.iter().map(|s| s.abbr.clone()).collect(),
            include_fluff: false,
        };
        let report = app
            .import_5etools(&request, &NullSink)
            .expect("import f-full");
        let f_full_ms = ms(t);
        eprintln!("f-full: {} source(s) in {f_full_ms} ms", scan.sources.len());
        let _ = report;
        json!({ "indexMs": index_ms, "aUserMs": a_user_ms, "fFullMs": f_full_ms })
    }

    #[test]
    fn s5_reference_digest() {
        let (Some(url), Some(a_user), Some(f_full), Some(out)) = (
            env("BOREALIS_S5_INDEX_URL"),
            env("BOREALIS_S5_A_USER_ZIP"),
            env("BOREALIS_S5_F_FULL_ZIP"),
            env("BOREALIS_S5_OUT"),
        ) else {
            return eprintln!("SKIPPED s5 reference: BOREALIS_S5_* not set");
        };
        let tmp = tempfile::tempdir().expect("temp dir");
        // BOREALIS_S5_DATA keeps the installed data folder for inspection (must not exist yet)
        let data = env("BOREALIS_S5_DATA").map_or_else(|| tmp.path().join("data"), PathBuf::from);
        assert!(!data.exists(), "the data folder must not exist yet");
        let app = app_at(&data);
        let times = install(&app, &url, Path::new(&a_user), Path::new(&f_full));
        let before = acceptance_digest::compute(&app).expect("digest before restart");
        drop(app);
        // the phones read the digest after a restart: a new state over the same data folder
        let t = Instant::now();
        let app = app_at(&data);
        let restart_ms = ms(t);
        let mut after = acceptance_digest::compute(&app).expect("digest after restart");
        assert_eq!(
            before["digest"], after["digest"],
            "the digest changed across a restart"
        );
        after["mac"] = json!({ "times": times, "restartMs": restart_ms });
        std::fs::write(&out, serde_json::to_string_pretty(&after).unwrap()).expect("write digest");
        eprintln!("s5 reference digest {} -> {out}", after["digest"]);
    }
}
