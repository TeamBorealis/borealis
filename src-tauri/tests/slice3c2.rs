//! "Import anything" and homebrew commands end to end, with a fake `claude` script that
//! answers by the `TASK:` line of the system prompt.

// The fake `claude`/`codex` CLIs here are `#!/bin/sh` scripts, so this suite is unix-only.
#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_ai::DetectConfig;
use borealis_lib::ai_service::AiService;
use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_packs::EventSink;
use borealis_lib::error::ErrorCode;
use borealis_lib::import_dto::{ImportEvent, ImportSink};
use borealis_lib::import_sources::ImportSourceDto;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
const FAKE: &str = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-claude 1.0'; exit 0; fi\nD=\"$(dirname \"$0\")\"\ncat > \"$D/stdin.tmp\"\nA=\"$*\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = \"--system-prompt-file\" ]; then A=\"$A $(cat \"$2\")\"; fi; shift; done\nfor t in detect generate repair revise outline target; do\n case \"$A\" in *\"TASK: import_$t\"*) cp \"$D/stdin.tmp\" \"$D/stdin_$t.log\"; cat \"$D/r_$t.jsonl\"; exit 0;; esac\ndone\nexit 1\n";

#[derive(Default)]
struct Rec(Mutex<Vec<ImportEvent>>);
impl ImportSink for Rec {
    fn import_event(&self, e: &ImportEvent) {
        self.0.lock().unwrap().push(e.clone());
    }
}
#[derive(Default)]
struct Reloads(Mutex<Vec<String>>);
impl EventSink for Reloads {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, r: &str) {
        self.0.lock().unwrap().push(r.to_string());
    }
}

struct H {
    data: tempfile::TempDir,
    bin: tempfile::TempDir,
    state: AppState,
    svc: Arc<AiService>,
}

fn paths(data: &Path) -> AppPaths {
    AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    }
}

fn harness() -> H {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let p = bin.path().join("claude");
    std::fs::write(&p, FAKE).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cfg = DetectConfig {
        path_env: Some(bin.path().into()),
        skip_known_locations: true,
        home: Some(bin.path().into()),
        ..DetectConfig::default()
    };
    H {
        state: AppState::new(paths(data.path())).unwrap(),
        data,
        bin,
        svc: Arc::new(AiService::new(cfg)),
    }
}

fn structured(v: &Value) -> String {
    json!({"type":"result","subtype":"success","is_error":false,"result":v.to_string(),"structured_output":v}).to_string()
}

impl H {
    fn reply(&self, task: &str, v: &Value) {
        std::fs::write(
            self.bin.path().join(format!("r_{task}.jsonl")),
            structured(v) + "\n",
        )
        .unwrap();
    }
    fn script_feat(&self, transcript: &str) {
        self.reply(
            "detect",
            &json!({"kind":"feat","edition":"2014","confidence":0.9,
            "items":[{"kind":"feat","name":"Stonewall Stance"}],"transcript":transcript}),
        );
        self.reply(
            "generate",
            &json!({"element":{"id":"X","name":"Stonewall Stance","type":"Feat",
            "description":"<p>You plant your feet and cannot be moved.</p>"},"notes":[]}),
        );
    }
    fn run(&self, sources: Vec<ImportSourceDto>) -> (Result<String, ErrorCode>, Arc<Rec>) {
        let rec = Arc::new(Rec::default());
        let job = match tauri::async_runtime::block_on(
            self.state.import_start(&self.svc, sources, None, false),
        ) {
            Ok(j) => j,
            Err(e) => return (Err(e.code), rec),
        };
        let id = job.draft_id.clone();
        tauri::async_runtime::block_on(self.state.run_import_start(&self.svc, job, rec.clone()));
        (Ok(id), rec)
    }
}

fn text() -> ImportSourceDto {
    ImportSourceDto::Text {
        text: "Stonewall Stance\nPrerequisite: Strength 13\nYou plant your feet.".into(),
    }
}

fn rank(stage: &str) -> usize {
    ["detect", "plan", "generate", "validate", "repair", "ready"]
        .iter()
        .position(|s| *s == stage)
        .unwrap()
}

#[test]
fn text_import_streams_in_order_and_finalizes_into_the_store() {
    let h = harness();
    h.script_feat("");
    let (id, rec) = h.run(vec![text()]);
    let id = id.unwrap();
    let ev = rec.0.lock().unwrap().clone();
    assert_eq!(ev.first().unwrap().stage, "detect");
    assert_eq!(ev.last().unwrap().stage, "ready");
    assert!(ev
        .windows(2)
        .all(|w| rank(w[0].stage) <= rank(w[1].stage) && w[0].progress <= w[1].progress));
    assert!(ev.iter().any(|e| e.stage == "generate") && ev.iter().all(|e| e.draft_id == id));
    let dto = h.state.import_get_draft(&id).unwrap();
    assert_eq!(dto.elements.len(), 1);
    assert_ne!(dto.elements[0].status, "failed");
    let reloads = Reloads::default();
    assert_eq!(
        h.state
            .import_finalize(&id, "Stone Pack", "!!", &reloads)
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    let pack = h
        .state
        .import_finalize(&id, "Stone Pack", "stone", &reloads)
        .unwrap();
    assert_eq!(pack.kind, PackKind::Imported);
    assert_eq!(pack.redistributable, Some(false));
    assert_eq!(reloads.0.lock().unwrap().as_slice(), ["pack-imported"]);
    let hub = h.state.hub().unwrap();
    let el = hub
        .store
        .iter()
        .find(|s| s.element.name == "Stonewall Stance")
        .expect("loaded");
    assert!(el.element.id.starts_with("ID_STONE_"), "{}", el.element.id);
    assert_eq!(
        h.state.import_get_draft(&id).unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn a_bytes_png_source_reaches_the_pipeline_as_an_image() {
    let h = harness();
    h.script_feat("Stonewall Stance. Prerequisite: Strength 13. You plant your feet.");
    let src = ImportSourceDto::Bytes {
        name: "page.png".into(),
        mime: "image/png".into(),
        base64: PNG.into(),
    };
    let (id, rec) = h.run(vec![src]);
    assert_eq!(rec.0.lock().unwrap().last().unwrap().stage, "ready");
    let stdin = std::fs::read_to_string(h.bin.path().join("stdin_detect.log")).unwrap();
    assert!(stdin.contains("\"type\":\"image\""), "{stdin}");
    assert_eq!(
        h.state
            .import_get_draft(&id.unwrap())
            .unwrap()
            .elements
            .len(),
        1
    );
}

fn png() -> ImportSourceDto {
    ImportSourceDto::Bytes {
        name: "page.png".into(),
        mime: "image/png".into(),
        base64: PNG.into(),
    }
}

#[test]
fn text_notes_travel_with_attachments_in_one_import() {
    let h = harness();
    h.script_feat("Stonewall Stance. You plant your feet.");
    let notes = ImportSourceDto::Text {
        text: "this is a 2024 Fighter subclass".into(),
    };
    let (id, rec) = h.run(vec![notes, png()]);
    let id = id.expect("mixed text and image is accepted");
    assert_eq!(rec.0.lock().unwrap().last().unwrap().stage, "ready");
    for task in ["detect", "generate"] {
        let stdin =
            std::fs::read_to_string(h.bin.path().join(format!("stdin_{task}.log"))).unwrap();
        assert!(stdin.contains("2024 Fighter subclass"), "{task}: {stdin}");
    }
    let stdin = std::fs::read_to_string(h.bin.path().join("stdin_detect.log")).unwrap();
    assert!(stdin.contains("\"type\":\"image\""), "{stdin}");
    assert_eq!(h.state.import_get_draft(&id).unwrap().elements.len(), 1);
}

#[test]
fn oversized_unsupported_and_mixed_sources_are_rejected_up_front() {
    let h = harness();
    h.script_feat("");
    let pdf = STANDARD.encode(vec![b'%'; 21 * 1024 * 1024]);
    let big = ImportSourceDto::Bytes {
        name: "big.pdf".into(),
        mime: "application/pdf".into(),
        base64: pdf,
    };
    let video = ImportSourceDto::Bytes {
        name: "a.mp4".into(),
        mime: "video/mp4".into(),
        base64: "AAAA".into(),
    };
    let not_png = ImportSourceDto::Bytes {
        name: "p.png".into(),
        mime: "image/png".into(),
        base64: STANDARD.encode(b"not an image"),
    };
    for bad in [
        vec![big],
        vec![video],
        vec![not_png],
        vec![],
        vec![ImportSourceDto::Text {
            text: "x".repeat(200_001),
        }],
        vec![
            ImportSourceDto::Text {
                text: "n".repeat(4001),
            },
            png(),
        ],
    ] {
        assert_eq!(h.run(bad).0.unwrap_err(), ErrorCode::InvalidArgument);
    }
    assert!(!h.data.path().join("import-drafts").exists());
}

#[test]
fn a_draft_survives_an_app_restart_and_an_interrupted_one_reports_it() {
    let h = harness();
    h.script_feat("");
    let (id, _) = h.run(vec![text()]);
    let id = id.unwrap();
    let before = h.state.import_get_draft(&id).unwrap();
    let job =
        tauri::async_runtime::block_on(h.state.import_start(&h.svc, vec![text()], None, false))
            .unwrap();
    let fresh = AppState::new(paths(h.data.path())).unwrap();
    let after = fresh.import_get_draft(&id).unwrap();
    assert_eq!(after.elements.len(), 1);
    assert_eq!(after.elements[0].json, before.elements[0].json);
    assert_eq!(after.suggested_pack_name, "Stonewall Stance");
    assert!(fresh
        .import_get_draft(&job.draft_id)
        .unwrap_err()
        .message
        .contains("interrupted"));
    assert!(fresh
        .import_finalize(&id, "Restarted", "rs", &Reloads::default())
        .is_ok());
}

fn spell(id: &str, name: &str, dice: &str) -> Value {
    json!({"id": id, "name": name, "type":"Spell","source":"Homebrew","supports":["Wizard"],
        "description": format!("<p>Each creature in a 30-foot-radius sphere takes {dice} fire damage.</p>"),
        "setters":[{"name":"level","value":"1"},{"name":"school","value":"Evocation"},
            {"name":"time","value":"1 action"},{"name":"duration","value":"Instantaneous"},
            {"name":"range","value":"120 feet"}]})
}

#[test]
fn homebrew_validate_returns_balance_flags_and_add_files_it_in_a_pack() {
    let h = harness();
    let doom = spell("ID_HB_SPELL_DOOM_NOVA", "Doom Nova", "10d10");
    let report = h.state.ai_validate_homebrew(&doom).unwrap();
    assert!(report.valid, "{:?}", report.errors);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("spell.damage_avg") && w.contains("High")),
        "{:?}",
        report.warnings
    );
    let broken = h
        .state
        .ai_validate_homebrew(&json!({"id":"ID_HB_X","name":"X"}))
        .unwrap();
    assert!(!broken.valid);
    let reloads = Reloads::default();
    let pack = h
        .state
        .ai_add_homebrew_to_pack(&doom, None, &reloads)
        .unwrap();
    assert_eq!(
        (pack.id.as_str(), pack.kind, pack.element_count),
        ("homebrew", PackKind::Homebrew, 1)
    );
    let again = h
        .state
        .ai_add_homebrew_to_pack(
            &spell("ID_HB_SPELL_DOOM_NOVA", "Doom Nova", "2d6"),
            None,
            &reloads,
        )
        .unwrap();
    assert_eq!(again.element_count, 1);
    let other = h
        .state
        .ai_add_homebrew_to_pack(
            &spell("ID_HB_SPELL_SPARK", "Spark", "1d6"),
            Some("homebrew"),
            &reloads,
        )
        .unwrap();
    assert_eq!(other.element_count, 2);
    assert!(h
        .state
        .hub()
        .unwrap()
        .store
        .contains_id("ID_HB_SPELL_SPARK"));
    assert_eq!(reloads.0.lock().unwrap().len(), 3);
    let bad = h
        .state
        .ai_add_homebrew_to_pack(&json!({"id":"ID_HB_X"}), None, &reloads);
    assert_eq!(bad.unwrap_err().code, ErrorCode::InvalidArgument);
}
