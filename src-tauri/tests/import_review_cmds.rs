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
        "generate",
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
    // Rebuilt by the generation task from the passage, not revised from the current JSON.
    let sent = std::fs::read_to_string(h.bin.path().join("stdin_generate.log")).unwrap();
    assert!(sent.contains("You plant your feet."), "{sent}");
    assert!(!sent.contains("previous_output"), "{sent}");
    assert!(!h.bin.path().join("stdin_revise.log").exists());
}

#[test]
fn regenerate_rebuilds_one_element_from_its_current_passage() {
    let (h, id) = ready_draft();
    let el = h.state.import_get_draft(&id).unwrap().elements[0]
        .id
        .clone();
    assert_eq!(
        h.state
            .import_begin_regenerate(&id, "ID_NOPE")
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    h.reply(
        "generate",
        &json!({"element":{"id":"X","name":"Stonewall Stance","type":"Feat",
        "description":"<p>You plant your feet and hold the line.</p>"},"notes":[]}),
    );
    h.state.import_begin_regenerate(&id, &el).unwrap();
    // The draft is claimed while the run is going.
    assert!(h.state.import_begin_regenerate(&id, &el).is_err());
    let rec = Arc::new(Rec::default());
    tauri::async_runtime::block_on(h.state.run_import_regenerate(&h.svc, &id, &el, rec.clone()));
    assert_eq!(rec.0.lock().unwrap().last().unwrap().stage, "ready");
    let dto = h.state.import_get_draft(&id).unwrap();
    assert!(
        serde_json::to_string(&dto.elements[0].json)
            .unwrap()
            .contains("hold the line"),
        "{:?}",
        dto.elements[0].json
    );
    assert!(!h.bin.path().join("stdin_revise.log").exists());
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

#[test]
fn finalize_leaves_out_not_imported_elements_and_lists_them_in_the_report() {
    // Arrange: a second element that was never generated (failed, no output), stored by hand.
    let (h, id) = ready_draft();
    let file = h
        ._data
        .path()
        .join("import-drafts")
        .join(format!("{id}.json"));
    let mut stored: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let mut lost = stored["draft"]["elements"][0].clone();
    lost["id"] = json!("ID_IMP_FEAT_LOST_THING");
    lost["name"] = json!("Lost Thing");
    lost["status"] = json!("failed");
    lost["json"] = Value::Null;
    lost["issues"] = json!(["generation failed: the model gave up"]);
    let mut planned = stored["draft"]["plan"]["elements"][0].clone();
    planned["id"] = json!("ID_IMP_FEAT_LOST_THING");
    planned["name"] = json!("Lost Thing");
    stored["draft"]["elements"]
        .as_array_mut()
        .unwrap()
        .push(lost);
    stored["draft"]["plan"]["elements"]
        .as_array_mut()
        .unwrap()
        .push(planned);
    std::fs::write(&file, stored.to_string()).unwrap();
    let state = AppState::new(paths(h._data.path())).unwrap();
    let dto = state.import_get_draft(&id).unwrap();
    assert_eq!(dto.review_counts.not_imported, 1);
    // Act
    let pack = state
        .import_finalize(
            &id,
            "Lost Pack",
            "lost",
            &borealis_lib::content_packs::NullSink,
        )
        .expect("a not-imported element does not block the pack");
    // Assert: only the generated element is in the pack, and the report says what was left out.
    assert_eq!(pack.element_count, 1);
    let report = find_file(h._data.path(), "report.json").expect("the pack has a report");
    let report: Value = serde_json::from_str(&std::fs::read_to_string(report).unwrap()).unwrap();
    let dropped: Vec<&Value> = report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["outcome"] == "dropped")
        .collect();
    assert_eq!(dropped.len(), 1, "{report}");
    assert_eq!(dropped[0]["source"], "Lost Thing");
    assert_eq!(report["review"]["not_imported"], 1, "{report}");
}

#[test]
fn failed_import_level_checks_reach_the_draft_as_notes() {
    let (h, id) = ready_draft();
    let file = h
        ._data
        .path()
        .join("import-drafts")
        .join(format!("{id}.json"));
    let mut stored: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    stored["draft"]["source_checks"] = json!([
        {"code": "heading_uncovered", "message": "The section \"Camp Rules\" was not imported", "failed": true},
        {"code": "heading_uncovered", "message": "The section \"Fine\" was imported", "failed": false}
    ]);
    std::fs::write(&file, stored.to_string()).unwrap();
    let state = AppState::new(paths(h._data.path())).unwrap();
    let dto = serde_json::to_value(state.import_get_draft(&id).unwrap()).unwrap();
    assert_eq!(
        dto["importNotes"],
        json!([{"code": "heading_uncovered", "summary": "These headings weren't imported",
                "items": ["Camp Rules"]}])
    );
}
