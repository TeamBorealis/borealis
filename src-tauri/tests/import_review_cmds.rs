//! Review commands of "Import anything": accept an element, override text only, re-locate one
//! element on user-selected source lines (offline, fake model).

//! "Import anything" and homebrew commands end to end, with a fake `claude` script that
//! answers by the `TASK:` line of the system prompt.

// The fake `claude`/`codex` CLIs here are `#!/bin/sh` scripts, so this suite is unix-only.
#![cfg(unix)]

use borealis_ai::DetectConfig;
use borealis_lib::ai_service::AiService;
use borealis_lib::error::ErrorCode;
use borealis_lib::import_dto::{ImportEvent, ImportSink};
use borealis_lib::import_sources::ImportSourceDto;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FAKE: &str = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-claude 1.0'; exit 0; fi\nD=\"$(dirname \"$0\")\"\ncat > \"$D/stdin.tmp\"\nA=\"$*\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = \"--system-prompt-file\" ]; then A=\"$A $(cat \"$2\")\"; fi; shift; done\nfor t in transcribe detect generate repair revise outline target; do\n case \"$A\" in *\"TASK: import_$t\"*) cp \"$D/stdin.tmp\" \"$D/stdin_$t.log\"; cat \"$D/r_$t.jsonl\"; exit 0;; esac\ndone\nexit 1\n";

#[derive(Default)]
struct Rec(Mutex<Vec<ImportEvent>>);
impl ImportSink for Rec {
    fn import_event(&self, e: &ImportEvent) {
        self.0.lock().unwrap().push(e.clone());
    }
}

struct H {
    _data: tempfile::TempDir,
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
    borealis_engine::invariants::enable_for_thread();
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
        _data: data,
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
        // Attachments are transcribed verbatim first; detection then reads that text.
        self.reply("transcribe", &json!({"text": transcript, "headings": []}));
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

fn ready_draft() -> (H, String) {
    let h = harness();
    h.script_feat("");
    let (id, _) = h.run(vec![text()]);
    (h, id.unwrap())
}

#[test]
fn the_draft_carries_review_states_source_lines_and_passages() {
    let (h, id) = ready_draft();
    let dto = h.state.import_get_draft(&id).unwrap();
    assert_eq!(dto.source_lines.len(), 3);
    let c = &dto.review_counts;
    assert_eq!(c.ok + c.check + c.not_imported, dto.elements.len());
    let v = serde_json::to_value(&dto.elements[0]).unwrap();
    assert!(v["review"]["state"].is_string(), "{v}");
    assert!(v.get("sourcePassage").is_some(), "{v}");
}

#[test]
fn accept_and_text_only_are_remembered_and_can_be_cleared() {
    let (h, id) = ready_draft();
    let el = h.state.import_get_draft(&id).unwrap().elements[0]
        .id
        .clone();
    let dto = h.state.import_accept_element(&id, &el, true).unwrap();
    assert!(dto.elements[0].review.accepted || dto.elements[0].review.reason.is_empty());
    let dto = h.state.import_set_text_only(&id, &el, Some(true)).unwrap();
    let r = &dto.elements[0].review;
    assert!(r.text_only && r.text_only_override == Some(true));
    // Remembered in the stored draft.
    let again = h.state.import_get_draft(&id).unwrap();
    assert_eq!(again.elements[0].review.text_only_override, Some(true));
    let dto = h.state.import_set_text_only(&id, &el, None).unwrap();
    assert_eq!(dto.elements[0].review.text_only_override, None);
    assert_eq!(
        h.state
            .import_accept_element(&id, "ID_NOPE", true)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn relocate_checks_the_range_then_rebuilds_the_element_on_those_lines() {
    let (h, id) = ready_draft();
    let el = h.state.import_get_draft(&id).unwrap().elements[0]
        .id
        .clone();
    for (s, e) in [(0, 1), (3, 2), (1, 4)] {
        assert_eq!(
            h.state
                .import_begin_relocate(&id, &el, s, e)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument,
            "{s}-{e}"
        );
    }
    h.reply(
        "revise",
        &json!({"element":{"id":"X","name":"Stonewall Stance","type":"Feat",
        "description":"<p>You plant your feet.</p>"},"notes":[]}),
    );
    h.state.import_begin_relocate(&id, &el, 3, 3).unwrap();
    let rec = Arc::new(Rec::default());
    tauri::async_runtime::block_on(h.state.run_import_relocate(
        &h.svc,
        &id,
        &el,
        (3, 3),
        rec.clone(),
    ));
    assert_eq!(rec.0.lock().unwrap().last().unwrap().stage, "ready");
    let dto = h.state.import_get_draft(&id).unwrap();
    let p = dto.elements[0].source_passage.as_ref().unwrap();
    assert_eq!(
        (p.start_line, p.end_line, p.text.as_str()),
        (3, 3, "You plant your feet.")
    );
}
