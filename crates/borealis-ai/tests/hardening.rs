#![cfg(unix)]
//! Security hardening of the CLI child: environment scrub, untrusted-input policy.

mod common;

use borealis_ai::{AiError, AiRequest, Backend, InputTrust};
use common::*;

fn req(prompt: &str) -> AiRequest {
    AiRequest {
        prompt: prompt.into(),
        system: None,
        schema: None,
    }
}

#[tokio::test]
async fn parent_secrets_do_not_reach_the_child() {
    // Edition 2021: `set_var` is safe; the name is unique to this test.
    std::env::set_var("BOREALIS_TEST_SECRET", "sentinel-value-9431");
    std::env::set_var("TERM", "xterm-256color");
    for backend in [Backend::Claude, Backend::Codex] {
        let dir = tempfile::tempdir().unwrap();
        let env_out = dir.path().join("env.txt");
        let stream = match backend {
            Backend::Claude => "claude_text_ok.jsonl",
            Backend::Codex => "codex_ok_synthetic.jsonl",
        };
        let s = fake_session(
            backend,
            "replay",
            Some(stream),
            &[("FAKE_ENV_OUT", env_out.to_string_lossy().into())],
        );
        s.run(&req("hi"), None).await.unwrap();
        let env = std::fs::read_to_string(&env_out).unwrap();
        assert!(!env.contains("BOREALIS_TEST_SECRET"), "{backend:?}: {env}");
        assert!(!env.contains("sentinel-value-9431"), "{backend:?}");
        assert!(env.lines().any(|l| l == "TERM=dumb"), "{backend:?}: {env}");
        assert!(env.lines().any(|l| l == "NO_COLOR=1"));
        assert!(env.lines().any(|l| l.starts_with("PATH=")));
        // Explicit per-session extras still pass.
        assert!(env.lines().any(|l| l == "FAKE_MODE=replay"));
    }
}

fn png() -> borealis_ai::Attachment {
    borealis_ai::Attachment {
        kind: borealis_ai::AttachmentKind::Image {
            mime: "image/png".into(),
        },
        bytes: std::fs::read(fixture("hi.png")).unwrap(),
        name: "hi.png".into(),
    }
}

#[tokio::test]
async fn codex_refuses_untrusted_input_without_consent() {
    let dir = tempfile::tempdir().unwrap();
    let args_out = dir.path().join("args.txt");
    let s = fake_session(
        Backend::Codex,
        "replay",
        Some("codex_ok_synthetic.jsonl"),
        &[("FAKE_ARGS_OUT", args_out.to_string_lossy().into())],
    );
    // Attachments make the input untrusted.
    let e = s.run_with(&req("hi"), &[png()], None).await.unwrap_err();
    assert!(
        matches!(e, AiError::UntrustedInputNeedsConsent { .. }),
        "{e}"
    );
    // So does an explicit Untrusted session.
    let untrusted = s.with_input_trust(InputTrust::Untrusted, false);
    let e = untrusted.run(&req("hi"), None).await.unwrap_err();
    assert!(
        matches!(e, AiError::UntrustedInputNeedsConsent { .. }),
        "{e}"
    );
    assert!(!args_out.exists(), "nothing may be spawned without consent");
    // Trusted plain chat still runs, and consent unlocks untrusted input.
    s.run(&req("hi"), None).await.unwrap();
    s.with_input_trust(InputTrust::Untrusted, true)
        .run(&req("hi"), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn claude_accepts_untrusted_input() {
    let s = fake_session(Backend::Claude, "replay", Some("claude_text_ok.jsonl"), &[])
        .with_input_trust(InputTrust::Untrusted, false);
    s.run(&req("hi"), None).await.unwrap();
}

#[tokio::test]
async fn claude_system_prompt_travels_in_a_private_file_with_the_tool_note() {
    let dir = tempfile::tempdir().unwrap();
    let args_out = dir.path().join("args.txt");
    let sys_out = dir.path().join("system.txt");
    let s = fake_session(
        Backend::Claude,
        "replay",
        Some("claude_text_ok.jsonl"),
        &[
            ("FAKE_ARGS_OUT", args_out.to_string_lossy().into()),
            ("FAKE_SYSFILE_OUT", sys_out.to_string_lossy().into()),
        ],
    );
    let r = AiRequest {
        system: Some("SYSTEM-SENTINEL rules".into()),
        ..req("hi")
    };
    s.run(&r, None).await.unwrap();
    let args = std::fs::read_to_string(&args_out).unwrap();
    assert!(args.contains("--system-prompt-file"), "{args}");
    assert!(
        !args.contains("SYSTEM-SENTINEL"),
        "system text must not be in argv"
    );
    let system = std::fs::read_to_string(&sys_out).unwrap();
    assert!(system.starts_with("SYSTEM-SENTINEL rules"), "{system}");
    assert!(
        system.contains("Environment: You have no tools"),
        "{system}"
    );
}

#[tokio::test]
async fn codex_gets_the_shell_warning_in_its_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let stdin_out = dir.path().join("stdin.txt");
    let s = fake_session(
        Backend::Codex,
        "replay",
        Some("codex_ok_synthetic.jsonl"),
        &[("FAKE_STDIN_OUT", stdin_out.to_string_lossy().into())],
    );
    s.run(&req("hi"), None).await.unwrap();
    let stdin = std::fs::read_to_string(&stdin_out).unwrap();
    assert!(stdin.contains("must not use it"), "{stdin}");
    assert!(!stdin.contains("no tools"), "{stdin}");
}
