//! Profile args reach the CLI (fake CLI records argv) and tampering blocks the run.
mod common;

#[cfg(unix)]
use borealis_ai::profile::{default_assets_dir, install_profile};
use borealis_ai::{AiRequest, AiSession, Backend, McpCommand, ProfileConfig};
use common::{fake_provider, fixture};

fn session(
    backend: Backend,
    dir: &std::path::Path,
    args_out: &std::path::Path,
    enabled: bool,
) -> AiSession {
    let mut o = borealis_ai::SessionOptions::default();
    o.env.push(("FAKE_MODE".into(), "replay".into()));
    let stream = match backend {
        Backend::Claude => "claude_text_ok.jsonl",
        Backend::Codex => "codex_ok_synthetic.jsonl",
    };
    o.env.push((
        "FAKE_STREAM".into(),
        fixture(stream).to_string_lossy().into_owned(),
    ));
    o.env.push((
        "FAKE_ARGS_OUT".into(),
        args_out.to_string_lossy().into_owned(),
    ));
    let mut cfg = ProfileConfig::new(
        dir,
        McpCommand {
            program: "/app/borealis".into(),
            args: vec!["--mcp".into()],
        },
    );
    cfg.enabled = enabled;
    o.profile = Some(cfg);
    AiSession::new(fake_provider(backend), o)
}

fn req() -> AiRequest {
    AiRequest {
        prompt: "hi".into(),
        ..Default::default()
    }
}

// The fake CLI is a `#!/bin/sh` script, which Windows cannot spawn.
#[cfg(unix)]
#[tokio::test]
async fn claude_run_receives_profile_args() {
    let t = tempfile::tempdir().unwrap();
    let prof = t.path().join("p");
    install_profile(&default_assets_dir(), &prof).unwrap();
    let out = t.path().join("args.txt");
    session(Backend::Claude, &prof, &out, true)
        .run(&req(), None)
        .await
        .unwrap();
    let args = std::fs::read_to_string(&out).unwrap();
    for want in [
        "--settings",
        "--plugin-dir",
        "--mcp-config",
        "dontAsk",
        "--strict-mcp-config",
        "Skill",
    ] {
        assert!(args.contains(want), "missing {want} in {args}");
    }
    assert!(!args.contains("--disable-slash-commands"));
    // The MCP command (it contains the install path) travels in a private file, not argv.
    assert!(!args.contains("/app/borealis"), "{args}");
}

// The fake CLI is a `#!/bin/sh` script, which Windows cannot spawn.
#[cfg(unix)]
#[tokio::test]
async fn codex_run_receives_profile_args() {
    let t = tempfile::tempdir().unwrap();
    let prof = t.path().join("p");
    install_profile(&default_assets_dir(), &prof).unwrap();
    let out = t.path().join("args.txt");
    session(Backend::Codex, &prof, &out, true)
        .run(&req(), None)
        .await
        .unwrap();
    let args = std::fs::read_to_string(&out).unwrap();
    assert!(args.contains("mcp_servers.borealis.command="), "{args}");
    assert!(args.contains("--ignore-user-config"));
}

// The fake CLI is a `#!/bin/sh` script, which Windows cannot spawn.
#[cfg(unix)]
#[tokio::test]
async fn disabled_profile_keeps_plain_isolation() {
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("args.txt");
    // not installed at all, but disabled: must not even verify
    session(Backend::Claude, &t.path().join("none"), &out, false)
        .run(&req(), None)
        .await
        .unwrap();
    let args = std::fs::read_to_string(&out).unwrap();
    assert!(args.contains("--disable-slash-commands") && !args.contains("--settings"));
}

#[tokio::test]
async fn tampered_profile_refuses_to_run() {
    let t = tempfile::tempdir().unwrap();
    let out = t.path().join("args.txt");
    let err = session(Backend::Claude, &t.path().join("missing"), &out, true)
        .run(&req(), None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("failed verification"), "{err}");
    assert!(!out.exists());
}
