#![cfg(unix)]
//! Integration tests for the process runner using the fake CLI script.

mod common;

use std::time::Duration;

use borealis_ai::error::TimeoutKind;
use borealis_ai::{AiError, AiEvent, AiRequest, AiSession, Backend};
use common::*;
use serde_json::json;

fn req(prompt: &str) -> AiRequest {
    AiRequest {
        prompt: prompt.into(),
        system: None,
        schema: None,
    }
}

fn session_with(
    mode: &str,
    stream: Option<&str>,
    extra: &[(&str, String)],
    f: impl FnOnce(&mut borealis_ai::SessionOptions),
) -> AiSession {
    let mut opts = borealis_ai::SessionOptions::default();
    opts.env.push(("FAKE_MODE".into(), mode.into()));
    if let Some(s) = stream {
        opts.env.push((
            "FAKE_STREAM".into(),
            fixture(s).to_string_lossy().into_owned(),
        ));
    }
    for (k, v) in extra {
        opts.env.push(((*k).into(), v.clone()));
    }
    f(&mut opts);
    AiSession::new(fake_provider(Backend::Claude), opts)
}

#[tokio::test]
async fn claude_stream_runs_to_final_and_records_prompt_on_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let stdin_out = dir.path().join("stdin.txt");
    let args_out = dir.path().join("args.txt");
    let s = fake_session(
        Backend::Claude,
        "replay",
        Some("claude_text_ok.jsonl"),
        &[
            ("FAKE_STDIN_OUT", stdin_out.to_string_lossy().into()),
            ("FAKE_ARGS_OUT", args_out.to_string_lossy().into()),
        ],
    );
    let secret = "PROMPT-SECRET; $(rm -rf /) `x`";
    let out = s.run(&req(secret), None).await.unwrap();
    assert_eq!(out.text, "OK");
    assert_eq!(out.exit_code, Some(0));
    assert!(out.usage.is_some());
    let stdin = std::fs::read_to_string(stdin_out).unwrap();
    assert!(
        stdin.contains(secret),
        "prompt delivered via stdin verbatim"
    );
    let args = std::fs::read_to_string(args_out).unwrap();
    assert!(
        !args.contains("PROMPT-SECRET"),
        "prompt must never be in argv"
    );
    assert!(args.contains("--no-session-persistence"));
}

#[tokio::test]
async fn events_are_streamed_in_order() {
    let s = fake_session(Backend::Claude, "replay", Some("claude_text_ok.jsonl"), &[]);
    let mut running = s.start(&req("hi")).await.unwrap();
    let mut seen = Vec::new();
    while let Some(ev) = running.next_event().await {
        seen.push(ev);
    }
    assert!(matches!(seen.first(), Some(AiEvent::Started { .. })));
    assert!(matches!(seen.last(), Some(AiEvent::Final { .. })));
    assert!(running.wait().await.is_ok());
}

#[tokio::test]
async fn codex_stream_with_json_final() {
    let s = fake_session(
        Backend::Codex,
        "replay",
        Some("codex_ok_synthetic.jsonl"),
        &[],
    );
    let schema = json!({"type":"object","properties":{"answer":{"type":"string"}}});
    let out = s
        .run(
            &AiRequest {
                prompt: "x".into(),
                system: Some("sys".into()),
                schema: Some(schema),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.structured.unwrap()["answer"], "OK");
    assert_eq!(out.tool_uses.len(), 1);
}

#[tokio::test]
async fn cli_failure_surfaces_redacted_stderr() {
    let s = fake_session(Backend::Claude, "fail", None, &[]);
    let err = s.run(&req("x"), None).await.unwrap_err();
    match err {
        AiError::Cli {
            stderr_tail,
            exit_code,
            ..
        } => {
            assert_eq!(exit_code, Some(3));
            assert!(stderr_tail.contains("boom"));
            assert!(
                !stderr_tail.contains("someone"),
                "home path must be redacted: {stderr_tail}"
            );
        }
        other => panic!("expected Cli error, got {other:?}"),
    }
}

#[tokio::test]
async fn backend_error_event_becomes_cli_error() {
    let s = fake_session(
        Backend::Codex,
        "replay",
        Some("codex_auth_error.jsonl"),
        &[],
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let err = s.run(&req("x"), Some(&tx)).await.unwrap_err();
    assert!(matches!(err, AiError::Cli { .. }), "{err:?}");
    drop(tx);
    let mut errors = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, AiEvent::Error { .. }) {
            errors += 1;
        }
    }
    assert_eq!(errors, 1, "exactly one Error event reaches the consumer");
}

#[tokio::test]
async fn empty_output_is_no_result() {
    let s = fake_session(Backend::Claude, "empty", None, &[]);
    assert!(matches!(
        s.run(&req("x"), None).await.unwrap_err(),
        AiError::NoResult
    ));
}

#[tokio::test]
async fn overall_timeout_kills_hung_cli() {
    let s = session_with("hang", Some("claude_text_ok.jsonl"), &[], |o| {
        o.timeout = Duration::from_millis(400);
    });
    let started = std::time::Instant::now();
    let err = s.run(&req("x"), None).await.unwrap_err();
    assert!(
        matches!(
            err,
            AiError::Timeout {
                kind: TimeoutKind::Overall,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn idle_timeout_fires_on_silence() {
    let s = session_with("silent", None, &[], |o| {
        o.idle_timeout = Duration::from_millis(300);
    });
    let err = s.run(&req("x"), None).await.unwrap_err();
    assert!(
        matches!(
            err,
            AiError::Timeout {
                kind: TimeoutKind::Idle,
                ..
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn total_output_cap_is_enforced() {
    let s = session_with("big", None, &[], |o| o.max_output_bytes = 10_000);
    let err = s.run(&req("x"), None).await.unwrap_err();
    assert!(
        matches!(err, AiError::OutputTooLarge { limit: 10_000 }),
        "{err:?}"
    );
}

#[tokio::test]
async fn single_line_cap_is_enforced() {
    let s = session_with("longline", None, &[], |o| o.max_line_bytes = 50_000);
    let err = s.run(&req("x"), None).await.unwrap_err();
    assert!(
        matches!(err, AiError::OutputTooLarge { limit: 50_000 }),
        "{err:?}"
    );
}

fn pid_alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn cancel_kills_the_whole_process_tree() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let s = fake_session(
        Backend::Claude,
        "child",
        None,
        &[("FAKE_PID_FILE", pid_file.to_string_lossy().into())],
    );
    let mut running = s.start(&req("x")).await.unwrap();
    // Wait for the grandchild to exist.
    let first = tokio::time::timeout(Duration::from_secs(5), running.next_event())
        .await
        .unwrap();
    assert!(matches!(first, Some(AiEvent::Started { .. })));
    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .to_string();
    assert!(pid_alive(&pid), "grandchild should be running");
    running.cancel();
    let err = running.wait().await.unwrap_err();
    assert!(matches!(err, AiError::Cancelled), "{err:?}");
    for _ in 0..50 {
        if !pid_alive(&pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("grandchild {pid} survived cancellation");
}

#[tokio::test]
async fn dropping_the_event_sink_cancels() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let s = fake_session(
        Backend::Claude,
        "child",
        None,
        &[("FAKE_PID_FILE", pid_file.to_string_lossy().into())],
    );
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    drop(rx);
    let err = tokio::time::timeout(Duration::from_secs(10), s.run(&req("x"), Some(&tx)))
        .await
        .unwrap()
        .unwrap_err();
    assert!(matches!(err, AiError::Cancelled), "{err:?}");
}

#[tokio::test]
async fn missing_binary_is_a_spawn_error() {
    let mut p = fake_provider(Backend::Claude);
    p.path = "/definitely/not/here/claude".into();
    let s = AiSession::new(p, Default::default());
    assert!(matches!(
        s.run(&req("x"), None).await.unwrap_err(),
        AiError::Spawn(_)
    ));
}

fn png_attachment() -> borealis_ai::Attachment {
    let img = image::RgbImage::from_pixel(16, 8, image::Rgb([0, 0, 0]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    borealis_ai::Attachment {
        kind: borealis_ai::AttachmentKind::Image {
            mime: "image/png".into(),
        },
        bytes,
        name: "hi.png".into(),
    }
}

#[tokio::test]
async fn fake_claude_receives_image_and_document_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let stdin_out = dir.path().join("stdin.json");
    let args_out = dir.path().join("args.txt");
    let session = session_with(
        "echo_blocks",
        None,
        &[
            ("FAKE_STDIN_OUT", stdin_out.to_string_lossy().into_owned()),
            ("FAKE_ARGS_OUT", args_out.to_string_lossy().into_owned()),
        ],
        |_| {},
    );
    let pdf = borealis_ai::Attachment {
        kind: borealis_ai::AttachmentKind::Pdf,
        bytes: b"%PDF-1.4 tiny".to_vec(),
        name: "s.pdf".into(),
    };
    let out = session
        .run_with(&req("what do you see"), &[png_attachment(), pdf], None)
        .await
        .unwrap();
    assert_eq!(
        out.text, "blocks=text,image,text,document,text, images=0",
        "types in order: label, image, label, document, prompt"
    );
    let args = std::fs::read_to_string(args_out).unwrap();
    assert!(args.contains("--input-format\nstream-json"));
    assert!(args.contains("--tools\n\n"), "tools stay disabled: {args}");
}

#[tokio::test]
async fn fake_codex_gets_image_files_and_they_are_deleted_afterwards() {
    let dir = tempfile::tempdir().unwrap();
    let args_out = dir.path().join("args.txt");
    let stdin_out = dir.path().join("stdin.txt");
    let mut opts = borealis_ai::SessionOptions::default();
    opts.env.push(("FAKE_MODE".into(), "echo_blocks".into()));
    opts.env.push((
        "FAKE_ARGS_OUT".into(),
        args_out.to_string_lossy().into_owned(),
    ));
    opts.env.push((
        "FAKE_STDIN_OUT".into(),
        stdin_out.to_string_lossy().into_owned(),
    ));
    // Attachments are untrusted input; Codex needs explicit consent.
    opts.allow_untrusted_with_codex = true;
    let session = AiSession::new(fake_provider(Backend::Codex), opts);
    // Codex fake reply is a Claude-shaped result line, so only the argv is asserted.
    let _ = session
        .run_with(&req("look"), &[png_attachment()], None)
        .await;
    let args = std::fs::read_to_string(args_out).unwrap();
    let lines: Vec<&str> = args.lines().collect();
    assert_eq!(lines[0], "exec");
    assert_eq!(lines[1], "-i");
    let file = std::path::Path::new(lines[2]);
    assert!(file
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("attachment-0"));
    assert!(
        !file.exists(),
        "temp image must be deleted after the session"
    );
    assert_eq!(lines.last().unwrap(), &"-");
}

#[tokio::test]
async fn bad_attachment_fails_before_spawning() {
    let session = session_with("fail", None, &[], |_| {});
    let bad = borealis_ai::Attachment {
        kind: borealis_ai::AttachmentKind::Pdf,
        bytes: b"nope".to_vec(),
        name: "x.pdf".into(),
    };
    let err = session.run_with(&req("x"), &[bad], None).await.unwrap_err();
    assert!(matches!(err, AiError::Invalid(_)), "{err:?}");
}
