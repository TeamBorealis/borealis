//! AI commands end to end through the command layer, with a fake `claude` script on a fake PATH.

// The fake `claude`/`codex` CLIs here are `#!/bin/sh` scripts, so this suite is unix-only.
#![cfg(unix)]

mod contract_support;

use borealis_ai::DetectConfig;
use borealis_lib::ai_dto::{AiMode, AiSink, AiTaskRequest, AiWireEvent};
use borealis_lib::ai_service::AiService;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const GRAPPLED: &str = "ID_2024_CONDITION_GRAPPLED";
const FAKE_CLAUDE: &str = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-claude 1.0'; exit 0; fi\ncat > /dev/null\nD=\"$(dirname \"$0\")\"\ncat \"$D/response.jsonl\"\n[ -f \"$D/hang\" ] && sleep 30\nexit 0\n";

struct Rec(Mutex<Sender<Value>>);

impl AiSink for Rec {
    fn emit(&self, event: &AiWireEvent) {
        let _ = self
            .0
            .lock()
            .unwrap()
            .send(serde_json::to_value(event).unwrap());
    }
}

struct Harness {
    _data: tempfile::TempDir,
    bin: tempfile::TempDir,
    state: AppState,
    svc: Arc<AiService>,
}

fn harness(with_cli: bool) -> Harness {
    borealis_engine::invariants::enable_for_thread();
    let data = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    let state = AppState::new(paths).unwrap();
    if with_cli {
        install_fake(bin.path());
    }
    let cfg = DetectConfig {
        path_env: Some(bin.path().into()),
        skip_known_locations: true,
        home: Some(bin.path().into()),
        ..DetectConfig::default()
    };
    Harness {
        _data: data,
        bin,
        state,
        svc: Arc::new(AiService::new(cfg)),
    }
}

fn install_fake(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("claude");
    std::fs::write(&p, FAKE_CLAUDE).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

impl Harness {
    fn respond(&self, lines: &[Value], hang: bool) {
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        std::fs::write(
            self.bin.path().join("response.jsonl"),
            text.join("\n") + "\n",
        )
        .unwrap();
        let flag = self.bin.path().join("hang");
        if hang {
            std::fs::write(flag, "").unwrap();
        } else {
            let _ = std::fs::remove_file(flag);
        }
    }

    fn hero(&self, ruleset: Ruleset, class: bool) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Mira".into(),
            ruleset,
            starting_level: 1,
            gender: "f".into(),
            ability_method: AbilityMethodDto::PointBuy,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let snap = self.state.create_character(ruleset, &request).unwrap();
        if !class {
            return snap;
        }
        let slot = snap
            .slots
            .iter()
            .find(|s| s.title == "Class")
            .unwrap()
            .id
            .clone();
        self.state
            .apply_selection(&snap.id, &slot, FIGHTER)
            .unwrap()
    }

    fn start(&self, req: AiTaskRequest) -> (Result<String, ErrorCode>, Receiver<Value>) {
        let (tx, rx) = channel();
        let r = tauri::async_runtime::block_on(self.svc.launch(
            &self.state,
            req,
            Arc::new(Rec(Mutex::new(tx))),
        ));
        (r.map(|h| h.task_id).map_err(|e| e.code), rx)
    }
}

fn request(mode: AiMode, prompt: &str, id: Option<&str>) -> AiTaskRequest {
    AiTaskRequest {
        mode,
        prompt: prompt.into(),
        character_id: id.map(str::to_string),
        history: vec![],
        params: None,
    }
}

fn delta(text: &str) -> Value {
    json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}})
}

fn structured(v: &Value) -> Value {
    json!({"type":"result","subtype":"success","is_error":false,"result":v.to_string(),"structured_output":v})
}

fn until_done(rx: &Receiver<Value>) -> Vec<Value> {
    let mut out = Vec::new();
    loop {
        let ev = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("event before timeout");
        let done = ev["kind"] == "done";
        out.push(ev);
        if done {
            return out;
        }
    }
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["kind"].as_str().unwrap()).collect()
}

#[test]
fn rules_qa_streams_in_order_and_strips_unknown_citations() {
    let h = harness(true);
    let hero = h.hero(Ruleset::Dnd2024, false);
    let answer = json!({
        "answer_markdown": format!("A grappled creature has Speed 0 [[{GRAPPLED}]] and also [[ID_MADE_UP]]."),
        "citations": [GRAPPLED, "ID_MADE_UP"], "confidence": "high", "not_covered": false
    });
    h.respond(&[structured(&answer)], false);
    let (id, rx) = h.start(request(
        AiMode::RulesQa,
        "What does the grappled condition do?",
        Some(&hero.id),
    ));
    let id = id.unwrap();
    let events = until_done(&rx);
    assert_eq!(kinds(&events), ["started", "delta", "result", "done"]);
    assert!(events.iter().all(|e| e["taskId"] == id.as_str()));
    let result = &events[2]["result"];
    assert_eq!(result["kind"], "rules_qa");
    assert_eq!(result["data"]["citations"], json!([GRAPPLED]));
    assert_eq!(result["data"]["strippedCitations"], json!(["ID_MADE_UP"]));
    assert_eq!(events[3]["cancelled"], false);
}

/// The UI renders citation chips from `result.data` (fix-ai-qa): the streamed text is the validated
/// answer with its `[[id]]` markers, every surviving marker resolves to a source, and the payload
/// matches the TS `CitedAnswerData` type.
#[test]
fn rules_qa_result_carries_resolved_sources_for_the_citation_chips() {
    let h = harness(true);
    let hero = h.hero(Ruleset::Dnd2024, false);
    let answer = json!({
        "answer_markdown": format!("Speed 0 [[{GRAPPLED}]], not [[ID_MADE_UP]]."),
        "citations": [GRAPPLED], "confidence": "high", "not_covered": false
    });
    h.respond(&[structured(&answer)], false);
    let (id, rx) = h.start(request(
        AiMode::RulesQa,
        "What does the grappled condition do?",
        Some(&hero.id),
    ));
    id.unwrap();
    let events = until_done(&rx);
    assert_eq!(kinds(&events), ["started", "delta", "result", "done"]);
    let data = &events[2]["result"]["data"];
    let text = data["answerMarkdown"].as_str().unwrap();
    assert_eq!(events[1]["text"].as_str(), Some(text));
    assert!(text.contains(&format!("[[{GRAPPLED}]]")) && !text.contains("ID_MADE_UP"));
    let sources = data["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["id"], GRAPPLED);
    assert!(!sources[0]["name"].as_str().unwrap().is_empty());
    assert!(!sources[0]["type"].as_str().unwrap().is_empty());
    let errs = contract_support::Contract::load().def("CitedAnswerData", data);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn level_up_advisor_returns_engine_validated_picks() {
    let h = harness(true);
    let hero = h.hero(Ruleset::Dnd2014, true);
    let first = h
        .state
        .read(&hero.id, |ctx, s| {
            Ok(
                borealis_assist::legal_options(&ctx.hub.engine, &s.character.build).unwrap()[0]
                    .id
                    .clone(),
            )
        })
        .unwrap();
    let advice = json!({"picks": [{"option_id": first, "rationale": "Take the level."}, {"option_id": "made:up", "rationale": "x"}], "summary": "Level up."});
    h.respond(&[structured(&advice)], false);
    let (_, rx) = h.start(request(AiMode::BuildAdvisor, "tanky", Some(&hero.id)));
    let events = until_done(&rx);
    assert_eq!(
        kinds(&events),
        ["started", "delta", "result", "done"],
        "{events:?}"
    );
    let data = &events[2]["result"]["data"];
    assert_eq!(events[2]["result"]["kind"], "level_up");
    assert_eq!(data["picks"][0]["option"]["id"], first.as_str());
    assert_eq!(data["dropped"][0]["optionId"], "made:up");
    assert_eq!(
        data["picks"].as_array().unwrap().len(),
        1,
        "made-up id dropped"
    );
}

#[test]
fn cancel_stops_the_stream() {
    let h = harness(true);
    h.respond(&[delta("Thinking")], true);
    let (id, rx) = h.start(request(AiMode::Chat, "hello", None));
    let id = id.unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(30)).unwrap()["kind"],
        "started"
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(30)).unwrap()["kind"],
        "delta"
    );
    h.svc.cancel(&id);
    let rest = until_done(&rx);
    assert_eq!(kinds(&rest), ["done"]);
    assert_eq!(rest[0]["cancelled"], true);
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "nothing after done"
    );
}

#[test]
fn no_provider_gives_offline_explain_and_unavailable_elsewhere() {
    let h = harness(false);
    let hero = h.hero(Ruleset::Dnd2014, true);
    let status = tauri::async_runtime::block_on(h.svc.status(&h.state.settings().unwrap().ai));
    assert_eq!(status.availability, "unavailable");
    let mut req = request(AiMode::Explain, "", Some(&hero.id));
    req.params = Some(json!({"stat": "ac"}));
    let (id, rx) = h.start(req);
    assert!(id.is_ok());
    let events = until_done(&rx);
    assert_eq!(kinds(&events), ["started", "delta", "result", "done"]);
    assert_eq!(events[2]["result"]["kind"], "explain");
    assert_eq!(events[2]["result"]["data"]["source"], "template");
    let (qa, _) = h.start(request(AiMode::RulesQa, "grappled?", None));
    assert_eq!(qa.unwrap_err(), ErrorCode::AiUnavailable);
}

#[test]
fn status_reports_the_detected_provider_and_honours_the_override_path() {
    let h = harness(true);
    let status = tauri::async_runtime::block_on(h.svc.status(&h.state.settings().unwrap().ai));
    assert_eq!(
        (status.availability.as_str(), status.provider.as_str()),
        ("ready", "claude")
    );
    assert_eq!(status.version.as_deref(), Some("fake-claude 1.0"));
    let mut ai = h.state.settings().unwrap().ai;
    ai.provider = "codex".into();
    ai.override_path = Some(h.bin.path().join("claude").to_string_lossy().into_owned());
    let status = tauri::async_runtime::block_on(h.svc.status(&ai));
    assert_eq!(status.providers[0].backend, "codex");
    assert_eq!(status.providers[0].source, "override");
}

#[test]
fn apply_plan_returns_a_snapshot_and_refuses_bad_plans() {
    let h = harness(false);
    let hero = h.hero(Ruleset::Dnd2014, true);
    let plan = json!({"steps": [{"level": 2, "path": {"kind": "main"}, "choices": [], "rationale": "Action Surge"}]});
    let snap = h.state.apply_plan(&hero.id, plan.clone(), 2).unwrap();
    assert_eq!(snap.level.total, 2);
    let bad = json!({"steps": [{"level": 3, "path": {"kind": "new_multiclass", "class": "ID_NOPE"}, "choices": []}]});
    assert!(h.state.apply_plan(&hero.id, bad, 3).is_err());
    assert_eq!(
        h.state.get_snapshot(&hero.id).unwrap().level.total,
        2,
        "unchanged after a refused plan"
    );
    assert_eq!(
        h.state
            .apply_plan(&hero.id, json!({"nonsense": 1}), 2)
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
}

#[test]
fn deterministic_encounter_commands() {
    let h = harness(false);
    let s = h
        .state
        .encounter_suggest(
            &json!({"partyLevels": [3, 3, 3, 3], "band": "moderate", "ruleset": "dnd2024"}),
        )
        .unwrap();
    assert_eq!(s["target"], "Moderate");
    assert!(!s["monsters"].as_array().unwrap().is_empty());
    let first = s["monsters"][0]["id"].as_str().unwrap().to_string();
    let e = h.state.encounter_evaluate(&json!({"partyLevels": [3, 3, 3, 3], "ruleset": "dnd2024", "monsters": [{"id": first, "count": 1}]})).unwrap();
    assert_eq!(e["report"]["partySize"], 4);
    assert!(h
        .state
        .encounter_evaluate(&json!({"partyLevels": [3], "monsters": [{"id": "nope", "count": 1}]}))
        .is_err());
    assert!(h
        .state
        .encounter_suggest(&json!({"partyLevels": []}))
        .is_err());
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn host_pairing_code_and_revocation() {
    let h = harness(true);
    let status = tauri::async_runtime::block_on(h.svc.host_enable(&h.state, false)).unwrap();
    assert!(status.enabled && status.supported);
    let code = status.pairing_code.clone().expect("code");
    assert_eq!(code.len(), 8);
    let uri = status
        .pairing_uri
        .clone()
        .expect("pairing string while open");
    assert!(uri.starts_with("borealis-pair:"), "{uri}");
    let fp = status
        .fingerprint
        .clone()
        .expect("fingerprint while hosting");
    assert_eq!(fp.split(' ').count(), 4, "{fp}");
    assert!(code.chars().all(|c| c.is_ascii_digit()));
    let again = tauri::async_runtime::block_on(h.svc.host_status(&h.state)).unwrap();
    assert_eq!(again.pairing_code, Some(code));
    assert_eq!(
        tauri::async_runtime::block_on(h.svc.host_revoke(&h.state, "nobody"))
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let off = tauri::async_runtime::block_on(h.svc.host_disable(&h.state)).unwrap();
    assert!(!off.enabled);
}

#[test]
fn host_needs_a_provider() {
    let h = harness(false);
    let r = tauri::async_runtime::block_on(h.svc.host_enable(&h.state, false));
    assert_eq!(r.unwrap_err().code, ErrorCode::AiUnavailable);
}
