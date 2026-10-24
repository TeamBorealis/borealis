//! `--mcp` mode, AI profile startup and first-run setup.

use borealis_ai::provider::DetectConfig;
use borealis_lib::ai_profile::{ensure_profile, mcp_command, mcp_command_scoped, ProfileOutcome};
use borealis_lib::ai_service::AiService;
#[cfg(unix)]
use borealis_lib::ai_setup::{SetupEvent, SetupSink};
use borealis_lib::error::ErrorCode;
use borealis_lib::mcp_mode;
use borealis_lib::state::{AppPaths, AppState};
use borealis_mcp::config::SessionScope;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use std::sync::Mutex;

fn content_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn discover_request() -> String {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "t", "version": "0"}}}});
    format!("{req}\n")
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn mcp_flag_is_detected_and_args_parse() {
    assert!(mcp_mode::requested(&strs(&["--mcp"])));
    assert!(!mcp_mode::requested(&strs(&["--other"])));
    let cfg = mcp_mode::parse_args(
        &strs(&[
            "--mcp",
            "--content-dir",
            "/a",
            "--content-dir",
            "/b",
            "--packs",
            "core,x",
        ]),
        Path::new("/nowhere/borealis"),
    )
    .unwrap();
    assert_eq!(
        cfg.content_dirs,
        vec![PathBuf::from("/a"), PathBuf::from("/b")]
    );
    assert_eq!(cfg.enabled_packs.unwrap().len(), 2);
    assert!(mcp_mode::parse_args(&strs(&["--mcp", "--bogus"]), Path::new("/x/y")).is_err());
}

#[test]
fn mcp_mode_finds_bundled_content_next_to_an_app_bundle_executable() {
    let app = tempfile::tempdir().unwrap();
    let macos = app.path().join("Borealis.app/Contents/MacOS");
    let core = app
        .path()
        .join("Borealis.app/Contents/Resources/content/core");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(core.join("pack.json"), "{}").unwrap();
    let exe = macos.join("borealis");
    let cfg = mcp_mode::parse_args_with(&strs(&["--mcp"]), &exe, None).unwrap();
    assert_eq!(cfg.content_dirs.len(), 1);
    assert!(cfg.content_dirs[0].ends_with("Resources/content"));
    // the user's imported library follows the bundled content, never replaces it
    let library = app.path().join("data/app.borealis/library");
    let cfg = mcp_mode::parse_args_with(&strs(&["--mcp"]), &exe, Some(library.clone())).unwrap();
    assert_eq!(cfg.content_dirs.len(), 2);
    assert!(cfg.content_dirs[0].ends_with("Resources/content"));
    assert_eq!(cfg.content_dirs[1], library);
    // an explicit --content-dir means exactly that
    let explicit = strs(&["--mcp", "--content-dir", "/a"]);
    let cfg = mcp_mode::parse_args_with(&explicit, &exe, Some(library)).unwrap();
    assert_eq!(cfg.content_dirs, vec![PathBuf::from("/a")]);
}

#[test]
fn mcp_mode_answers_server_discover_in_process() {
    let args = strs(&["--mcp", "--content-dir", content_dir().to_str().unwrap()]);
    let mut out = Vec::new();
    mcp_mode::serve_with(
        &args,
        Path::new("/x/borealis"),
        std::io::Cursor::new(discover_request()),
        &mut out,
    )
    .unwrap();
    let v: Value = serde_json::from_slice(out.split(|b| *b == b'\n').next().unwrap()).unwrap();
    assert_eq!(v["result"]["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(
        v["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "borealis"
    );
}

#[test]
fn built_binary_in_mcp_mode_answers_discover_without_a_gui() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_borealis"))
        .args(["--mcp", "--content-dir"])
        .arg(content_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(discover_request().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    let line = String::from_utf8(out.stdout).unwrap();
    let v: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
    assert_eq!(v["result"]["resultType"], "complete");
}

fn assets() -> PathBuf {
    borealis_ai::profile::default_assets_dir()
}

#[test]
fn profile_install_update_verify_tamper_and_repair() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("ai-profile");
    assert_eq!(
        ensure_profile(&assets(), &target, "1.0.0").unwrap(),
        ProfileOutcome::Installed
    );
    assert_eq!(
        ensure_profile(&assets(), &target, "1.0.0").unwrap(),
        ProfileOutcome::Verified
    );
    assert_eq!(
        ensure_profile(&assets(), &target, "2.0.1").unwrap(),
        ProfileOutcome::Updated
    );
    assert_eq!(
        ensure_profile(&assets(), &target, "2.0.1").unwrap(),
        ProfileOutcome::Verified
    );

    // Tamper: replace a settings file (files are read-only, so remove it first).
    let settings = target.join("profile/claude/settings.json");
    let dir = settings.parent().unwrap();
    set_writable(dir);
    set_writable(&settings); // Windows refuses to delete a read-only file
    std::fs::remove_file(&settings).unwrap();
    std::fs::write(&settings, "{\"permissions\":{\"allow\":[\"Bash\"]}}").unwrap();
    match ensure_profile(&assets(), &target, "2.0.1").unwrap() {
        ProfileOutcome::Repaired(files) => {
            assert!(files.iter().any(|f| f.contains("settings.json")))
        }
        other => panic!("expected repair, got {other:?}"),
    }
    assert_eq!(
        ensure_profile(&assets(), &target, "2.0.1").unwrap(),
        ProfileOutcome::Verified
    );
    let text = std::fs::read_to_string(&settings).unwrap();
    assert!(!text.contains("\"Bash\"]"), "{text}");
}

fn set_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let mut perm = std::fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(path, perm).unwrap();
    }
}

#[test]
fn mcp_command_carries_content_dirs_and_packs() {
    let packs = ["core".to_string(), "srd-5.1".to_string()]
        .into_iter()
        .collect();
    let c = mcp_command(
        Path::new("/App/borealis"),
        &[PathBuf::from("/c"), PathBuf::from("/lib")],
        Some(&packs),
    );
    assert_eq!(c.program, PathBuf::from("/App/borealis"));
    assert_eq!(
        c.args,
        strs(&[
            "--mcp",
            "--content-dir",
            "/c",
            "--content-dir",
            "/lib",
            "--packs",
            "core,srd-5.1"
        ])
    );
}

#[test]
fn the_session_scope_is_only_in_the_launch_of_the_ask_that_got_it() {
    let dirs = [PathBuf::from("/c")];
    let exe = Path::new("/App/borealis");
    let plain = mcp_command_scoped(exe, &dirs, None, None);
    assert_eq!(plain.args, mcp_command(exe, &dirs, None).args);
    assert!(!plain.args.iter().any(|a| a == "--session-scope"));
    let scoped = mcp_command_scoped(exe, &dirs, None, Some("camp-1"));
    assert_eq!(
        scoped.args,
        strs(&["--mcp", "--content-dir", "/c", "--session-scope", "camp-1"])
    );
}

#[test]
fn mcp_mode_parses_search_dir_and_session_scope() {
    let app = tempfile::tempdir().unwrap();
    let exe = app.path().join("borealis");
    let base = ["--mcp", "--content-dir", "/a"];
    let none = mcp_mode::parse_args_with(&strs(&base), &exe, None).unwrap();
    assert_eq!(
        none.session_scope,
        SessionScope::None,
        "default is no scope"
    );
    assert!(none.search_dir.is_none());
    let mut args = strs(&base);
    args.extend(strs(&["--search-dir", "/idx", "--session-scope", "camp-1"]));
    let cfg = mcp_mode::parse_args_with(&args, &exe, None).unwrap();
    assert_eq!(cfg.search_dir, Some(PathBuf::from("/idx")));
    assert_eq!(cfg.session_scope, SessionScope::Campaign("camp-1".into()));
    for bad in [
        strs(&["--mcp", "--content-dir", "/a", "--session-scope"]),
        strs(&["--mcp", "--content-dir", "/a", "--session-scope", " "]),
        strs(&["--mcp", "--content-dir", "/a", "--search-dir"]),
    ] {
        assert!(
            mcp_mode::parse_args_with(&bad, &exe, None).is_err(),
            "{bad:?}"
        );
    }
}

#[cfg(unix)]
struct Collect(Mutex<Vec<SetupEvent>>);
#[cfg(unix)]
impl SetupSink for Collect {
    fn emit(&self, event: &SetupEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

struct Rig {
    _data: tempfile::TempDir,
    // Read only by the fake-CLI tests, which are unix-only (shell scripts).
    #[cfg_attr(not(unix), allow(dead_code))]
    bin: tempfile::TempDir,
    state: AppState,
    svc: Arc<AiService>,
}

/// A fake CLI as a `#!/bin/sh` script, so the tests that use it are unix-only.
#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn rig(setup: impl Fn(&Path)) -> Rig {
    borealis_engine::invariants::enable_for_thread();
    let data = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    setup(bin.path());
    let state = AppState::new(AppPaths {
        content_root: content_dir(),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    let cfg = DetectConfig {
        path_env: Some(bin.path().into()),
        skip_known_locations: true,
        home: Some(bin.path().into()),
        ..DetectConfig::default()
    };
    Rig {
        _data: data,
        bin,
        state,
        svc: Arc::new(AiService::new(cfg)),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn setup_status_reports_install_and_login_with_a_fake_path() {
    let r = rig(|d| {
        script(d, "claude", "case \"$1\" in --version) echo '9.9.9 (Claude Code)';; auth) echo '{\"loggedIn\": true}';; esac");
        script(
            d,
            "codex",
            "case \"$1\" in --version) echo 'codex-cli 1.0';; login) exit 1;; esac",
        );
        script(d, "npm", "exit 0");
    });
    let s = r.svc.setup_status(&r.state).await.unwrap();
    let claude = s.providers.iter().find(|p| p.backend == "claude").unwrap();
    assert!(claude.installed);
    assert_eq!(claude.logged_in, Some(true));
    assert_eq!(
        claude.install_command,
        "npm install -g @anthropic-ai/claude-code@2.1.293"
    );
    let codex = s.providers.iter().find(|p| p.backend == "codex").unwrap();
    assert!(codex.installed);
    assert_eq!(codex.logged_in, Some(false));
    assert!(s.npm_path.is_some());
    assert!(!s.completed);
    r.svc.setup_complete(&r.state, true).unwrap();
    assert!(r.svc.setup_status(&r.state).await.unwrap().completed);
}

#[tokio::test]
async fn setup_status_with_nothing_installed() {
    let r = rig(|_| {});
    let s = r.svc.setup_status(&r.state).await.unwrap();
    assert!(s
        .providers
        .iter()
        .all(|p| !p.installed && p.logged_in.is_none()));
    assert!(s.npm_path.is_none());
    let err = r.svc.setup_confirm_install("claude").unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
}

#[cfg(unix)]
#[tokio::test]
async fn install_refuses_without_a_valid_one_time_consent_token() {
    let r = rig(|d| script(d, "npm", "echo \"npm $@\"; echo ok > \"$0.installed\""));
    let sink = Arc::new(Collect(Mutex::new(Vec::new())));
    let err = r
        .svc
        .setup_install("claude", "forged", sink.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
    assert!(!r.bin.path().join("npm.installed").exists());

    let consent = r.svc.setup_confirm_install("claude").unwrap();
    assert_eq!(
        consent.command,
        "npm install -g @anthropic-ai/claude-code@2.1.293"
    );
    // A token for claude cannot install codex.
    let err = r
        .svc
        .setup_install("codex", &consent.token, sink.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
    // The failed attempt consumed it.
    let err = r
        .svc
        .setup_install("claude", &consent.token, sink.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);

    let consent = r.svc.setup_confirm_install("claude").unwrap();
    r.svc
        .setup_install("claude", &consent.token, sink.clone())
        .await
        .unwrap();
    assert!(r.bin.path().join("npm.installed").exists());
    let events = sink.0.lock().unwrap().clone();
    assert!(events.iter().any(|e| matches!(e, SetupEvent::Output { line, .. } if line == "npm install -g @anthropic-ai/claude-code@2.1.293")));
    assert!(matches!(
        events.last(),
        Some(SetupEvent::Done { ok: true, .. })
    ));
    // Reuse fails.
    let err = r
        .svc
        .setup_install("claude", &consent.token, sink)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RequirementFailed);
}

#[cfg(unix)]
#[tokio::test]
async fn failed_install_reports_done_not_ok() {
    let r = rig(|d| script(d, "npm", "echo boom >&2; exit 3"));
    let sink = Arc::new(Collect(Mutex::new(Vec::new())));
    let consent = r.svc.setup_confirm_install("codex").unwrap();
    let err = r
        .svc
        .setup_install("codex", &consent.token, sink.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::IoError);
    let events = sink.0.lock().unwrap().clone();
    assert!(events
        .iter()
        .any(|e| matches!(e, SetupEvent::Output { line, .. } if line == "boom")));
    assert!(matches!(
        events.last(),
        Some(SetupEvent::Done { ok: false, .. })
    ));
}

#[test]
fn login_help_returns_the_vendor_command_and_rejects_unknown_providers() {
    let r = rig(|_| {});
    assert_eq!(
        r.svc.setup_login("claude", false).unwrap().command,
        "claude auth login"
    );
    assert_eq!(
        r.svc.setup_login("codex", false).unwrap().command,
        "codex login"
    );
    assert!(!r.svc.setup_login("codex", false).unwrap().opened);
    assert_eq!(
        r.svc.setup_login("rm -rf", false).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
}

#[cfg(unix)]
struct Rec(Mutex<std::sync::mpsc::Sender<Value>>);
#[cfg(unix)]
impl borealis_lib::ai_dto::AiSink for Rec {
    fn emit(&self, event: &borealis_lib::ai_dto::AiWireEvent) {
        let _ = self
            .0
            .lock()
            .unwrap()
            .send(serde_json::to_value(event).unwrap());
    }
}

#[cfg(unix)]
#[test]
fn sessions_carry_the_profile_and_mcp_command_when_enabled() {
    use borealis_lib::ai_dto::{AiMode, AiTaskRequest};
    use borealis_lib::ai_profile::ProfileBase;
    let r = rig(|d| {
        script(
            d,
            "claude",
            "if [ \"$1\" = \"--version\" ]; then echo 'fake 1.0'; exit 0; fi\ncat > /dev/null\necho \"$@\" > \"$0.args\"\nwhile [ $# -gt 0 ]; do case \"$1\" in --mcp-config|--system-prompt-file) cat \"$2\" >> \"$0.args\";; esac; shift; done\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"hi\"}'",
        );
    });
    let profile_dir = tempfile::tempdir().unwrap();
    let target = profile_dir.path().join("p");
    ensure_profile(&assets(), &target, "1.0.0").unwrap();
    r.svc.set_profile(ProfileBase {
        dir: target,
        exe: PathBuf::from("/Applications/Borealis.app/Contents/MacOS/borealis"),
        content_dirs: vec![content_dir()],
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let req = AiTaskRequest {
        mode: AiMode::Chat,
        prompt: "hello".into(),
        character_id: None,
        history: vec![],
        params: None,
    };
    tauri::async_runtime::block_on(r.svc.launch(&r.state, req, Arc::new(Rec(Mutex::new(tx)))))
        .unwrap();
    loop {
        let ev = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("done");
        if ev["kind"] == "done" {
            break;
        }
    }
    let args = std::fs::read_to_string(r.bin.path().join("claude.args")).unwrap();
    assert!(args.contains("--mcp-config"), "{args}");
    assert!(
        args.contains("Borealis.app/Contents/MacOS/borealis"),
        "{args}"
    );
    assert!(args.contains("--mcp"), "{args}");
    assert!(args.contains("srd-5.2.1"), "{args}");
}
