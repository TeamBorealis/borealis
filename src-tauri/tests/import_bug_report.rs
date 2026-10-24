//! Opt-in import bug reports: the preview is the exact payload, prose never leaves, the source
//! only on request, and Send posts exactly the previewed bytes (BR1-BR3). A fake poster stands
//! in for the network; the fake `claude` CLI is a shell script, so this suite is unix-only.
#![cfg(unix)]

mod contract_support;

use borealis_ai::DetectConfig;
use borealis_lib::ai_service::AiService;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::error::ErrorCode;
use borealis_lib::import_bug_report::{ReportOutbox, ReportPost, ReportRequest, REPORT_ENDPOINT};
use borealis_lib::import_dto::{ImportEvent, ImportSink};
use borealis_lib::import_sources::ImportSourceDto;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FAKE: &str = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-claude 1.0'; exit 0; fi\nD=\"$(dirname \"$0\")\"\ncat > \"$D/stdin.tmp\"\nA=\"$*\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = \"--system-prompt-file\" ]; then A=\"$A $(cat \"$2\")\"; fi; shift; done\nfor t in transcribe detect generate repair revise outline target; do\n case \"$A\" in *\"TASK: import_$t\"*) cat \"$D/r_$t.jsonl\"; exit 0;; esac\ndone\nexit 1\n";
/// Invented description prose that must never reach a payload.
const PROSE: &str =
    "Quillfeather plants both feet and the very ground answers with a rumble of old stone";

struct Quiet;
impl ImportSink for Quiet {
    fn import_event(&self, _: &ImportEvent) {}
}
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

#[derive(Default)]
struct FakePost {
    sent: Mutex<Vec<(String, String)>>,
    refuse: bool,
}
impl ReportPost for FakePost {
    fn post(&self, url: &str, body: &str) -> Result<String, String> {
        if self.refuse {
            return Err("the server answered 429".into());
        }
        self.sent.lock().unwrap().push((url.into(), body.into()));
        Ok("r-1".into())
    }
}

struct H {
    _data: tempfile::TempDir,
    _bin: tempfile::TempDir,
    state: AppState,
    draft: String,
}

fn reply(bin: &std::path::Path, task: &str, v: &Value) {
    let line = json!({"type":"result","subtype":"success","is_error":false,"result":v.to_string(),"structured_output":v});
    std::fs::write(bin.join(format!("r_{task}.jsonl")), format!("{line}\n")).unwrap();
}

fn imported_draft() -> H {
    use std::os::unix::fs::PermissionsExt;
    borealis_engine::invariants::enable_for_thread();
    let data = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let p = bin.path().join("claude");
    std::fs::write(&p, FAKE).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    reply(
        bin.path(),
        "detect",
        &json!({"kind":"feat","edition":"2014","confidence":0.9,
        "items":[{"kind":"feat","name":"Stonewall Stance"}]}),
    );
    reply(
        bin.path(),
        "generate",
        &json!({"element":{"id":"X","name":"Stonewall Stance","type":"Feat",
        "description": format!("<p>{PROSE}</p>")},"notes":[]}),
    );
    let svc = AiService::new(DetectConfig {
        path_env: Some(bin.path().into()),
        skip_known_locations: true,
        home: Some(bin.path().into()),
        ..DetectConfig::default()
    });
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    let source = ImportSourceDto::Text {
        text: format!("Stonewall Stance\nPrerequisite: Strength 13\n{PROSE}."),
    };
    let job = tauri::async_runtime::block_on(state.import_start(&svc, vec![source], None, false))
        .expect("import starts");
    let draft = job.draft_id.clone();
    tauri::async_runtime::block_on(state.run_import_start(&svc, job, Arc::new(Quiet)));
    H {
        _data: data,
        _bin: bin,
        state,
        draft,
    }
}

fn draft_req(h: &H) -> ReportRequest {
    ReportRequest {
        draft_id: Some(h.draft.clone()),
        note: "The feat lost its prerequisite.".into(),
        ..ReportRequest::default()
    }
}

#[test]
fn draft_preview_is_structure_only_and_send_posts_exactly_it() {
    let h = imported_draft();
    let poster = Arc::new(FakePost::default());
    let outbox = ReportOutbox::new("https://example.test/api/import-report", poster.clone());
    let p = h
        .state
        .import_report_preview(&outbox, draft_req(&h))
        .unwrap();
    let text = p.payload.to_string();
    assert!(!text.contains("Quillfeather"), "description leaked: {text}");
    assert_eq!(p.payload["format"], "borealis-import-bug-report/1");
    assert_eq!(p.payload["origin"], "draft");
    assert_eq!(p.payload["note"], "The feat lost its prerequisite.");
    assert_eq!(p.payload["elements"][0]["name"], "Stonewall Stance");
    assert!(p.payload["source"].is_null());
    assert!(p.source_available && p.source_bytes.unwrap() > 0);
    let errs = contract_support::Contract::load()
        .def("ImportReportPreview", &serde_json::to_value(&p).unwrap());
    assert!(errs.is_empty(), "{errs:?}");

    let sent = outbox.send(&p.preview_id).unwrap();
    assert_eq!(sent.id, "r-1");
    let posts = poster.sent.lock().unwrap().clone();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].0, "https://example.test/api/import-report");
    let body: Value = serde_json::from_str(&posts[0].1).unwrap();
    assert_eq!(body, p.payload, "the posted body is the previewed payload");
    assert_eq!(posts[0].1.len(), p.bytes);
    // Sent previews leave the outbox.
    assert_eq!(
        outbox.send(&p.preview_id).unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn the_source_goes_only_when_ticked_and_failures_are_reported() {
    let h = imported_draft();
    let poster = Arc::new(FakePost {
        refuse: true,
        ..FakePost::default()
    });
    let outbox = ReportOutbox::new(REPORT_ENDPOINT, poster.clone());
    let req = ReportRequest {
        include_source: true,
        ..draft_req(&h)
    };
    let p = h.state.import_report_preview(&outbox, req).unwrap();
    assert!(p.payload["source"]["text"]
        .as_str()
        .unwrap()
        .contains("Quillfeather"));
    assert_eq!(p.endpoint, "https://borealisbuilder.com/api/import-report");
    let err = outbox.send(&p.preview_id).unwrap_err();
    assert_eq!(err.code, ErrorCode::IoError);
    assert!(err.message.contains("429"), "{}", err.message);
    assert_eq!(outbox.send("nope").unwrap_err().code, ErrorCode::NotFound);
    let neither = h
        .state
        .import_report_preview(&outbox, ReportRequest::default());
    assert_eq!(neither.unwrap_err().code, ErrorCode::InvalidArgument);
}

#[test]
fn an_installed_import_pack_can_be_reported_with_flagged_elements() {
    let h = imported_draft();
    let pack = h
        .state
        .import_finalize(&h.draft, "Stone Pack", "stone", &Quiet)
        .unwrap();
    let outbox = ReportOutbox::new(REPORT_ENDPOINT, Arc::new(FakePost::default()));
    let id = h
        .state
        .hub()
        .unwrap()
        .store
        .iter()
        .find(|s| s.element.name == "Stonewall Stance")
        .unwrap()
        .element
        .id
        .clone();
    let req = ReportRequest {
        pack_id: Some(pack.id.clone()),
        flagged: vec![id.clone()],
        ..ReportRequest::default()
    };
    let p = h.state.import_report_preview(&outbox, req).unwrap();
    assert_eq!(p.payload["origin"], "pack");
    assert_eq!(p.payload["flagged"], json!([id]));
    assert!(!p.payload.to_string().contains("Quillfeather"));
    assert!(!p.source_available);
    let codes = p.payload["codes"].as_array().unwrap();
    assert!(
        codes.iter().any(|c| c["code"] == "user_flagged"),
        "{codes:?}"
    );
    let bundled = ReportRequest {
        pack_id: Some("srd".into()),
        ..ReportRequest::default()
    };
    assert_eq!(
        h.state
            .import_report_preview(&outbox, bundled)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn recent_logs_are_in_the_preview_only_when_ticked() {
    let h = imported_draft();
    let logs = h.state.paths.logs();
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("borealis-2026-10-15.log"),
        "INFO borealis_lib: Borealis started\nWARN index https://example.test/a.index: refused\n",
    )
    .unwrap();
    let outbox = ReportOutbox::new(
        "https://example.test/api/import-report",
        Arc::new(FakePost::default()),
    );

    let off = h
        .state
        .import_report_preview(&outbox, draft_req(&h))
        .unwrap();
    assert!(off.logs_available);
    assert!(
        off.payload.get("logs").is_none(),
        "logs stay out unless ticked"
    );
    assert!(!off.payload.to_string().contains("Borealis started"));

    let req = ReportRequest {
        include_logs: true,
        ..draft_req(&h)
    };
    let on = h.state.import_report_preview(&outbox, req).unwrap();
    assert_eq!(on.payload["logs"]["lines"], 2);
    assert!(on.payload["logs"]["text"]
        .as_str()
        .unwrap()
        .contains("refused"));
    let errs = contract_support::Contract::load()
        .def("ImportReportPreview", &serde_json::to_value(&on).unwrap());
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn no_search_index_path_or_indexed_text_enters_a_report() {
    // BR2 (local-search-spec.md section 11): keyword indexes are local derived data; a report built after
    // indexing an imported pack carries no `search/` path, no index file name and none of the indexed text.
    let h = imported_draft();
    let pack = h
        .state
        .import_finalize(&h.draft, "Stone Pack", "stone", &Quiet)
        .unwrap();
    h.state.build_search_index_now().unwrap();
    let search_dir = h.state.paths.data.join("search");
    let manifest = std::fs::read_to_string(search_dir.join("v1/manifest.json")).unwrap();
    assert!(
        manifest.contains(&format!("\"{}\"", pack.id)),
        "the imported pack is indexed locally: {manifest}"
    );
    let logs = h.state.paths.logs();
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("borealis-2026-10-24.log"),
        format!(
            "INFO borealis_lib: Borealis started\nWARN search: index at {}/v1/lore rebuilt\n",
            search_dir.display()
        ),
    )
    .unwrap();
    let outbox = ReportOutbox::new(REPORT_ENDPOINT, Arc::new(FakePost::default()));
    let req = ReportRequest {
        pack_id: Some(pack.id.clone()),
        include_logs: true,
        include_source: true,
        ..ReportRequest::default()
    };

    let p = h.state.import_report_preview(&outbox, req).unwrap();

    let body = p.payload.to_string();
    assert!(body.contains("Borealis started"), "other log lines stay");
    for leak in [
        "search/",
        "search\\\\",
        "manifest.json",
        "meta.json",
        ".tantivy",
        "Quillfeather",
    ] {
        assert!(!body.contains(leak), "{leak} reached the report: {body}");
    }
}
