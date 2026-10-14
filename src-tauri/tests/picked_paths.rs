//! Backend-owned path picking: commands that touch the filesystem accept
//! only tokens minted by a backend dialog, for the right purpose, and the AI program override
//! validates before it is stored.

use borealis_ai::DetectConfig;
use borealis_lib::ai_service::AiService;
use borealis_lib::commands::with_json_extension;
use borealis_lib::error::ErrorCode;
use borealis_lib::import_sources::ImportSourceDto;
use borealis_lib::picked_paths::{PickPurpose, PickedPaths};
use borealis_lib::state::{AppPaths, AppState};
#[cfg(unix)]
use serde_json::json;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

struct H {
    state: AppState,
    svc: AiService,
    _data: tempfile::TempDir,
    _empty: tempfile::TempDir,
}

fn harness() -> H {
    borealis_engine::invariants::enable_for_thread();
    let data = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    let svc = AiService::new(DetectConfig {
        path_env: Some(empty.path().into()),
        skip_known_locations: true,
        home: Some(empty.path().into()),
        ..DetectConfig::default()
    });
    H {
        state,
        svc,
        _data: data,
        _empty: empty,
    }
}

fn import_code(h: &H, sources: Vec<ImportSourceDto>) -> ErrorCode {
    tauri::async_runtime::block_on(h.state.import_start(&h.svc, sources, None, false))
        .map(|_| ())
        .unwrap_err()
        .code
}

fn file(token: &str) -> ImportSourceDto {
    ImportSourceDto::File {
        token: token.into(),
    }
}

#[test]
fn import_file_sources_need_a_live_import_token() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "Fire Bolt, a cantrip").unwrap();
    // A raw path, an unknown token and a token minted for another purpose are refused.
    let raw = notes.to_string_lossy().into_owned();
    assert_eq!(
        import_code(&h, vec![file(&raw)]),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        import_code(&h, vec![file("pick_nope")]),
        ErrorCode::InvalidArgument
    );
    let folder = h
        .state
        .picked
        .grant(notes.clone(), PickPurpose::FivetoolsFolder)
        .unwrap();
    assert_eq!(
        import_code(&h, vec![file(&folder.token)]),
        ErrorCode::InvalidArgument
    );
    // The file type comes from the picked path, not from the webview: a key file is refused.
    let key = dir.path().join("id_rsa");
    std::fs::write(&key, "-----BEGIN KEY-----").unwrap();
    let key = h.state.picked.grant(key, PickPurpose::ImportFile).unwrap();
    assert_eq!(
        import_code(&h, vec![file(&key.token)]),
        ErrorCode::InvalidArgument
    );
    // A good pick passes source validation and only stops at "no AI installed".
    let ok = h
        .state
        .picked
        .grant(notes, PickPurpose::ImportFile)
        .unwrap();
    assert_eq!(ok.name, "notes.txt");
    assert_eq!(
        import_code(&h, vec![file(&ok.token)]),
        ErrorCode::AiUnavailable
    );
}

#[test]
fn expired_picks_are_refused() {
    let picks = PickedPaths::with_ttl(Duration::ZERO);
    let t = picks
        .grant(PathBuf::from("/x/out.json"), PickPurpose::SaveCharacter)
        .unwrap();
    assert!(picks.resolve(&t.token, PickPurpose::SaveCharacter).is_err());
}

#[test]
fn save_targets_always_end_in_json() {
    assert_eq!(
        with_json_extension("/x/hero".into()),
        PathBuf::from("/x/hero.json")
    );
    assert_eq!(
        with_json_extension("/x/.bashrc".into()),
        PathBuf::from("/x/.bashrc.json")
    );
    assert_eq!(
        with_json_extension("/x/Hero.JSON".into()),
        PathBuf::from("/x/Hero.JSON")
    );
}

/// A fake CLI with unix permission bits, so the override tests are unix-only.
#[cfg(unix)]
fn exe(dir: &Path, name: &str, mode: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, "#!/bin/sh\necho fake 1.0\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    p
}

#[cfg(unix)]
fn override_code(h: &H, path: PathBuf) -> ErrorCode {
    let t = h.state.picked.grant(path, PickPurpose::AiOverride).unwrap();
    h.state
        .set_ai_override(&t.token, None, |_| true)
        .unwrap_err()
        .code
}

#[cfg(unix)]
#[test]
fn ai_override_validation_rejects_shells_temp_binaries_and_non_executables() {
    let h = harness();
    assert_eq!(
        override_code(&h, "/bin/sh".into()),
        ErrorCode::InvalidArgument
    );
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        override_code(&h, exe(tmp.path(), "claude", 0o755)),
        ErrorCode::InvalidArgument
    );
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    assert_eq!(
        override_code(&h, exe(outside.path(), "codex", 0o644)),
        ErrorCode::InvalidArgument
    );
    // Wrong-purpose and unknown tokens never reach validation.
    let good = exe(outside.path(), "claude", 0o755);
    let wrong = h
        .state
        .picked
        .grant(good.clone(), PickPurpose::ImportFile)
        .unwrap();
    assert!(h
        .state
        .set_ai_override(&wrong.token, None, |_| true)
        .is_err());
    assert!(h
        .state
        .set_ai_override("pick_nope", None, |_| true)
        .is_err());
    assert_eq!(h.state.settings().unwrap().ai.override_path, None);
}

#[cfg(unix)]
#[test]
fn ai_override_is_set_only_after_the_native_confirmation() {
    let h = harness();
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let good = exe(outside.path(), "claude", 0o755);
    let t = h
        .state
        .picked
        .grant(good.clone(), PickPurpose::AiOverride)
        .unwrap();
    let mut shown = None;
    let s = h
        .state
        .set_ai_override(&t.token, None, |p| {
            shown = Some(p.to_path_buf());
            false
        })
        .unwrap();
    assert_eq!(shown.as_deref(), Some(good.as_path()));
    assert_eq!(s.ai.override_path, None);
    // The token was spent by the declined attempt.
    assert!(h.state.set_ai_override(&t.token, None, |_| true).is_err());
    let t = h
        .state
        .picked
        .grant(good.clone(), PickPurpose::AiOverride)
        .unwrap();
    let s = h.state.set_ai_override(&t.token, None, |_| true).unwrap();
    let stored = good.to_string_lossy().into_owned();
    assert_eq!(s.ai.override_path.as_deref(), Some(stored.as_str()));
    // save_settings may resend the same value or clear it, but never set a new one.
    let same = json!({"ai": {"provider": "claude", "overridePath": stored}});
    assert!(h.state.save_settings(&same).is_ok());
    let other = json!({"ai": {"overridePath": "/bin/sh"}});
    assert_eq!(
        h.state.save_settings(&other).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    let cleared = h
        .state
        .save_settings(&json!({"ai": {"overridePath": null}}))
        .unwrap();
    assert_eq!(cleared.ai.override_path, None);
}
