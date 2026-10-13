//! Security wiring: imports and the LAN host treat outside content as untrusted. Codex needs the
//! user's consent (typed `consent_required` error); Claude is preferred when both are installed.

// The fake `claude`/`codex` CLIs here are `#!/bin/sh` scripts, so this suite is unix-only.
#![cfg(unix)]

use borealis_ai::DetectConfig;
use borealis_lib::ai_service::AiService;
use borealis_lib::error::ErrorCode;
use borealis_lib::import_sources::ImportSourceDto;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const VERSION_ONLY: &str =
    "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake 1.0.0'; exit 0; fi\nexit 1\n";

struct H {
    _data: tempfile::TempDir,
    _bin: tempfile::TempDir,
    state: AppState,
    svc: AiService,
}

fn harness(programs: &[&str]) -> H {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    for name in programs {
        let p = bin.path().join(name);
        std::fs::write(&p, VERSION_ONLY).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let cfg = DetectConfig {
        path_env: Some(bin.path().into()),
        skip_known_locations: true,
        home: Some(bin.path().into()),
        ..DetectConfig::default()
    };
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    H {
        state: AppState::new(paths).unwrap(),
        svc: AiService::new(cfg),
        _data: data,
        _bin: bin,
    }
}

fn text() -> Vec<ImportSourceDto> {
    vec![ImportSourceDto::Text {
        text: "Stonewall Stance\nYou plant your feet.".into(),
    }]
}

fn start(h: &H, consent: bool) -> Result<String, borealis_lib::error::ApiError> {
    tauri::async_runtime::block_on(h.state.import_start(&h.svc, text(), None, consent))
        .map(|j| j.draft_id)
}

#[test]
fn codex_only_import_asks_for_consent_then_proceeds() {
    let h = harness(&["codex"]);
    let e = start(&h, false).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConsentRequired);
    assert_eq!(e.details.as_ref().unwrap()["backend"], "codex");
    assert!(start(&h, true).is_ok());
}

#[test]
fn claude_is_preferred_for_imports_even_when_codex_is_the_chosen_provider() {
    let h = harness(&["codex", "claude"]);
    assert!(start(&h, false).is_ok());
}

#[test]
fn import_without_any_provider_stays_ai_unavailable() {
    let h = harness(&[]);
    assert_eq!(start(&h, false).unwrap_err().code, ErrorCode::AiUnavailable);
}

#[test]
fn consent_is_remembered_for_that_draft_only() {
    let h = harness(&["codex"]);
    let id = start(&h, true).unwrap();
    // Later steps of the same import do not ask again; another draft does.
    assert!(tauri::async_runtime::block_on(h.svc.import_authorize(&h.state, &id, false)).is_ok());
    let other =
        tauri::async_runtime::block_on(h.svc.import_authorize(&h.state, "imp_other", false));
    assert_eq!(other.unwrap_err().code, ErrorCode::ConsentRequired);
    h.svc.import_forget(&id);
    assert!(tauri::async_runtime::block_on(h.svc.import_authorize(&h.state, &id, false)).is_err());
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn lan_host_on_codex_needs_consent() {
    let h = harness(&["codex"]);
    let e = tauri::async_runtime::block_on(h.svc.host_enable(&h.state, false)).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConsentRequired);
}

#[test]
fn old_settings_files_with_auto_content_update_still_load() {
    // the flag was re-added ("Automatic Updates"): an old file's value is kept.
    let old = r#"{"playerName":"Ada","autoContentUpdate":false,"autosave":false}"#;
    let s: borealis_lib::settings::AppSettings = serde_json::from_str(old).unwrap();
    assert_eq!(s.player_name, "Ada");
    assert!(!s.autosave);
    assert!(!s.auto_content_update);
    let out = serde_json::to_value(&s).unwrap();
    assert_eq!(out["autoContentUpdate"], false);
}
