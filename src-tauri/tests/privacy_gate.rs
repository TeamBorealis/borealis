//! S3.4 privacy gate (local-search-spec sec. 11, EM3, BR2): with the keyword index built, the semantic vectors
//! written by the app's indexer (`search/v1/vectors/<corpus>/vectors-*.bin` + `keys.json`) and the model file on
//! the device, nothing of `search/` or `models/` reaches any way data leaves the app: the log excerpt (Share logs,
//! Copy logs, the bug report's logs), a character export (`.borealis.json`, Save As, the sheet PDF and HTML), a
//! `.borealis` bundle (Export bundle, Share bundle and Send to device all use the same builder) and the backup
//! rules (Android `data_extraction_rules.xml` / `backup_rules.xml`, iOS `NSURLIsExcludedFromBackupKey`), which
//! are tied here to the folders the search and model code really use. The bug report payload itself is covered in
//! `import_bug_report.rs` (`no_vector_store_keys_or_model_enter_a_report`). Deterministic fake embedder: no model
//! download, no C++ model run.
#![cfg(feature = "device-link")]

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use borealis_embed::models::{ModelFile, Q8_0};
use borealis_embed::{
    DeviceProfile, DeviceState, Embedder, FakeEmbedder, FormFactor, SimulatedDevice, TierConfig,
};
use borealis_lib::content_packs::NullSink;
use borealis_lib::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents};
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::dto_ext::SheetOptionsDto;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::sync_dto::{LinkPackChoice, LinkSelection, LinkStatus};
use borealis_lib::vector_service::{EmbedLoader, IndexingState, VectorService};
use borealis_link::{MemoryStore, SecureStore};

const GIB: u64 = 1024 * 1024 * 1024;
const WAIT: Duration = Duration::from_secs(120);
/// Byte window taken from the middle of every vector and index file: none may appear in any output.
const WINDOW: usize = 48;
/// A window counts only when it is not a trivial pattern (padding, runs).
const MIN_DISTINCT_BYTES: usize = 20;
/// A file name with this many hex digits in a row is unique to this data (segment UUID, model SHA prefix).
const HEX_RUN: usize = 8;

fn has_hex_run(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_hexdigit())
        .any(|run| run.len() >= HEX_RUN)
}

struct Quiet;

impl SyncEvents for Quiet {
    fn status(&self, _: &LinkStatus) {}
    fn content(&self) -> &dyn borealis_lib::content_packs::EventSink {
        &NullSink
    }
}

struct Gate {
    _dir: tempfile::TempDir,
    data: PathBuf,
    state: Arc<AppState>,
    sync: DeviceSync,
    character: String,
}

fn fake_loader() -> EmbedLoader {
    Arc::new(|_: &Path, file: &ModelFile, tier: &TierConfig| {
        let fake = FakeEmbedder::new(tier.dims.expect("dims")).with_sha256(file.sha256);
        Ok(Arc::new(fake) as Arc<dyn Embedder>)
    })
}

/// An app with a saved character, the keyword index, the model "on the device" (a sparse file of the compiled
/// size; the fake loader never reads it) and complete vector stores for every corpus.
fn gate() -> Gate {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.clone(),
    })
    .unwrap();
    state.vectors = VectorService::with_loader(&data, true, fake_loader());
    let desktop = DeviceProfile {
        form: FormFactor::Desktop,
        total_ram_bytes: Some(16 * GIB),
        cores: 8,
        pointer_bits: 64,
    };
    state.vectors.set_device(
        desktop,
        Arc::new(SimulatedDevice::new(DeviceState::DESKTOP)),
    );
    state.build_search_index_now().unwrap();
    let models = data.join("models");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::File::create(models.join(Q8_0.file_name))
        .unwrap()
        .set_len(Q8_0.bytes)
        .unwrap();
    state
        .save_settings(&serde_json::json!({"search": {"semantic": true, "tierOverride": "t3"}}))
        .unwrap();
    assert!(state.vectors.wait_idle(WAIT), "indexer finished");
    assert_eq!(state.vectors.status().state, IndexingState::Ready);

    let request = NewCharacterRequest {
        name: "Mira".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let character = state
        .create_character(Ruleset::Dnd2014, &request)
        .unwrap()
        .id;
    state.save_character(&character).unwrap();

    let state = Arc::new(state);
    let store = Arc::new(MemoryStore::default());
    let sync = DeviceSync::new(
        Arc::clone(&state) as Arc<dyn StateRef>,
        Arc::new(Quiet),
        SyncConfig {
            bind_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            device_name: "Mac".into(),
            discovery: false,
        },
        move || Ok(Arc::clone(&store) as Arc<dyn SecureStore>),
    );
    Gate {
        _dir: dir,
        data,
        state,
        sync,
        character,
    }
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// What must never leave: every file name under `search/` and `models/`, the content hashes and doc keys of
/// every `keys.json`, and a byte window of every vector and index file.
struct Markers {
    names: Vec<String>,
    texts: Vec<String>,
    windows: Vec<Vec<u8>>,
}

impl Gate {
    fn markers(&self) -> Markers {
        let mut m = Markers {
            names: Vec::new(),
            texts: Vec::new(),
            windows: Vec::new(),
        };
        let search = files_under(&self.data.join("search"));
        let vectors: Vec<&PathBuf> = search
            .iter()
            .filter(|p| {
                p.to_string_lossy()
                    .replace('\\', "/")
                    .contains("/v1/vectors/")
            })
            .collect();
        assert!(
            vectors
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "keys.json")),
            "the indexer wrote keys.json: {vectors:?}"
        );
        assert!(
            vectors.iter().any(|p| p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("vectors-"))),
            "the indexer wrote vector files: {vectors:?}"
        );
        for p in search.iter().chain(&files_under(&self.data.join("models"))) {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            // Names only this data has: Tantivy segment files (a UUID), vector stores (the model SHA), the model.
            if has_hex_run(&name) || name.ends_with(".gguf") {
                m.names.push(name.clone());
            }
            if name == "keys.json" {
                let keys: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
                for row in keys["rows"].as_array().unwrap().iter().take(50) {
                    m.texts.push(row["hash"].as_str().unwrap().to_string());
                }
                continue;
            }
            let bytes = std::fs::read(p).unwrap();
            if bytes.len() >= 4 * WINDOW {
                let mid = bytes.len() / 2;
                let window = &bytes[mid..mid + WINDOW];
                let distinct: std::collections::HashSet<u8> = window.iter().copied().collect();
                if distinct.len() >= MIN_DISTINCT_BYTES {
                    m.windows.push(window.to_vec());
                }
            }
        }
        m.names.push("search/v1".into());
        m.names.push("vectors/".into());
        m.names.push(".gguf".into());
        assert!(
            m.texts.len() > 10 && m.windows.len() > 2 && m.names.len() > 5,
            "markers found"
        );
        m
    }
}

impl Markers {
    fn assert_absent(&self, what: &str, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes).replace('\\', "/");
        for n in &self.names {
            assert!(!text.contains(n.as_str()), "{n} reached {what}");
        }
        for t in &self.texts {
            assert!(
                !text.contains(t.as_str()),
                "a keys.json hash reached {what}"
            );
        }
        for w in &self.windows {
            assert!(
                !bytes.windows(w.len()).any(|x| x == w.as_slice()),
                "vector or index bytes reached {what}"
            );
        }
    }
}

#[test]
fn the_log_excerpt_of_share_and_copy_logs_names_no_search_data() {
    let g = gate();
    let m = g.markers();
    let logs = g.state.paths.logs();
    std::fs::create_dir_all(&logs).unwrap();
    let vectors = g.state.vectors.root().join("rules");
    let model = g.data.join("models").join(Q8_0.file_name);
    std::fs::write(
        logs.join("borealis-2026-10-25.log"),
        format!(
            "INFO borealis_lib: Borealis started\n\
             WARN semantic: indexing stopped: {}\n\
             WARN vector store unusable, rebuilding reason={}\n\
             INFO semantic model {}\n\
             INFO search: rebuilding corpus=lore\n",
            vectors.join("keys.json").display(),
            vectors
                .join(format!("vectors-{}-256.bin", &Q8_0.sha256[..8]))
                .display(),
            model.display(),
        ),
    )
    .unwrap();

    // `get_recent_logs` and `share_logs` (Share logs / Copy logs in Settings) both send exactly this excerpt.
    let excerpt = borealis_lib::app_logs::recent_logs(&logs, borealis_lib::app_logs::EXCERPT_LINES);

    assert!(excerpt.contains("Borealis started") && excerpt.contains("corpus=lore"));
    m.assert_absent("the log excerpt", excerpt.as_bytes());
    for leak in ["keys.json", "vectors-", "/models/", "search/"] {
        assert!(
            !excerpt.replace('\\', "/").contains(leak),
            "{leak}: {excerpt}"
        );
    }
}

#[test]
fn character_exports_carry_no_search_data() {
    let g = gate();
    let m = g.markers();

    let json = g.state.export_character(&g.character).unwrap();
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        json.data_base64.as_deref().unwrap(),
    )
    .unwrap();
    assert!(json.file_name.ends_with(".borealis.json"));
    m.assert_absent("the character export", &decoded);

    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("Mira copy.json");
    g.state
        .save_character_as(&g.character, Some("Mira copy"), Some(&path))
        .unwrap();
    m.assert_absent("Save As", &std::fs::read(&path).unwrap());

    let pdf = g
        .state
        .sheet_file(&g.character, &SheetOptionsDto::default())
        .unwrap();
    assert!(pdf.bytes.starts_with(b"%PDF-"));
    m.assert_absent("the sheet PDF", &pdf.bytes);
    let html = g
        .state
        .sheet_file(
            &g.character,
            &SheetOptionsDto {
                format: Some("html".into()),
            },
        )
        .unwrap();
    m.assert_absent("the sheet HTML", &html.bytes);
}

#[test]
fn a_bundle_with_every_pack_carries_no_search_data_even_when_planted_in_a_pack() {
    let g = gate();
    // A user pack that holds copies of the vector store, the keyword index and the model in its own `search/`
    // and `models/` folders (someone copied app data into it): the bundle leaves them out too.
    let pack = g.state.paths.library().join("homebrew");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(
        pack.join("pack.json"),
        r#"{"format":"borealis-pack/1","id":"homebrew","name":"Homebrew","version":"1.0.0","origin":"custom","rulesets":["dnd2024"],"files":[]}"#,
    )
    .unwrap();
    for src in files_under(&g.data.join("search"))
        .into_iter()
        .chain(files_under(&g.data.join("models")))
    {
        let rel = src.strip_prefix(&g.data).unwrap();
        let dst = pack.join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        if src.extension().is_some_and(|e| e == "gguf") {
            std::fs::write(&dst, b"GGUF model copy").unwrap();
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
    let m = g.markers();

    // Export bundle, Share bundle (mobile) and Send to device all build through `build_outgoing`.
    let built = g
        .sync
        .build_bundle_file(&LinkSelection {
            character_ids: vec![g.character.clone()],
            packs: LinkPackChoice::All,
        })
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("all.borealis");
    DeviceSync::save_bundle(&built, &dest).unwrap();

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        m.assert_absent(&format!("bundle entry {name}"), &bytes);
        names.push(name);
    }
    assert!(
        names.iter().any(|n| n == "packs/homebrew/pack.json"),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("characters/")),
        "{names:?}"
    );
    for n in &names {
        let lower = n.to_lowercase();
        for leak in ["search/", "models/", "vectors", "keys.json", ".gguf"] {
            assert!(!lower.contains(leak), "{n} reached the bundle");
        }
    }
    m.assert_absent("the bundle file", &std::fs::read(&dest).unwrap());
}

/// The folders the search, vector and model code write are the ones the backup rules and the bundle exclusions
/// name: moving one (say the vectors out of `search/`) fails here before it can reach a backup or a bundle.
#[test]
fn every_search_and_model_folder_is_kept_out_of_backups_and_bundles() {
    let g = gate();
    let rel = |p: &Path| {
        p.strip_prefix(&g.data)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    };
    let folders = [
        rel(&g.data.join(borealis_lib::search_service::SEARCH_DIR)),
        rel(g.state.vectors.root()),
        rel(&g.data.join(borealis_lib::semantic_service::MODELS_DIR)),
    ];
    let android = [
        include_str!("../gen/android/app/src/main/res/xml/data_extraction_rules.xml"),
        include_str!("../gen/android/app/src/main/res/xml/backup_rules.xml"),
    ];
    for folder in &folders {
        let top = folder.split('/').next().unwrap();
        assert!(
            borealis_lib::no_backup::NO_BACKUP_DIRS.contains(&top),
            "{folder}: not in NO_BACKUP_DIRS (iOS backup exclusion)"
        );
        for rules in android {
            assert!(
                rules.contains(&format!(r#"<exclude domain="root" path="{top}" />"#)),
                "{folder}: an Android backup rule set does not exclude {top}/"
            );
        }
        assert!(borealis_bundle::is_excluded(&format!("{folder}/keys.json")));
    }
    // Every file the indexer and the download wrote sits in one of those folders.
    for p in files_under(&g.data.join("search"))
        .iter()
        .chain(&files_under(&g.data.join("models")))
    {
        let r = rel(p);
        assert!(
            borealis_bundle::is_excluded(&r),
            "{r} would travel in a bundle"
        );
        assert!(
            folders.iter().any(|f| r.starts_with(f.as_str())),
            "{r} outside the excluded folders"
        );
    }
}
