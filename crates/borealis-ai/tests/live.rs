//! Live smoke tests against the real CLIs. Ignored by default (they cost tokens and need a
//! logged-in CLI): `cargo test -p borealis-ai --test live -- --ignored --nocapture`.

use borealis_ai::provider::{detect_one, DetectConfig};
use borealis_ai::task::qa::{QaInput, RulesQa};
use borealis_ai::task::{run_task, Ruleset, RunOptions};
use borealis_ai::{AiRequest, AiSession, Backend, SessionOptions};

async fn session(backend: Backend, model: Option<&str>) -> Option<AiSession> {
    let provider = detect_one(backend, &DetectConfig::default()).await?;
    println!(
        "{backend:?}: {} ({:?})",
        provider.path.display(),
        provider.version
    );
    let options = SessionOptions {
        model: model.map(str::to_string),
        ..Default::default()
    };
    Some(AiSession::new(provider, options))
}

#[tokio::test]
#[ignore = "calls the real claude CLI"]
async fn claude_plain_and_task() {
    let Some(s) = session(Backend::Claude, Some("haiku")).await else {
        return;
    };
    let out = s
        .run(
            &AiRequest {
                prompt: "Reply with the single word OK".into(),
                system: Some("Be terse.".into()),
                schema: None,
            },
            None,
        )
        .await
        .unwrap();
    println!("text={:?} usage={:?}", out.text, out.usage);
    assert!(out.text.contains("OK"));

    let task = RulesQa {
        input: QaInput {
            question:
                "How many hit points does a level 1 wizard with 10 CON have, using the fixed value?"
                    .into(),
            ruleset: Ruleset::Dnd2014,
            context: vec![],
            character: None,
        },
    };
    let r = run_task(&s, &task, &RunOptions::default()).await.unwrap();
    println!("{:#?}", r);
}

#[tokio::test]
#[ignore = "calls the real codex CLI"]
async fn codex_plain() {
    let Some(s) = session(Backend::Codex, None).await else {
        return;
    };
    let out = s
        .run(
            &AiRequest {
                prompt: "Reply with the single word OK".into(),
                system: None,
                schema: None,
            },
            None,
        )
        .await;
    println!("{out:?}");
}

#[tokio::test]
#[ignore = "calls the real claude CLI"]
async fn claude_sees_image_and_pdf() {
    use borealis_ai::{Attachment, AttachmentKind};
    let Some(s) = session(Backend::Claude, Some("haiku")).await else {
        return;
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let attachments = [
        Attachment {
            kind: AttachmentKind::Image {
                mime: "image/png".into(),
            },
            bytes: std::fs::read(dir.join("hi.png")).unwrap(),
            name: "hi.png".into(),
        },
        Attachment {
            kind: AttachmentKind::Pdf,
            bytes: std::fs::read(dir.join("zebra.pdf")).unwrap(),
            name: "zebra.pdf".into(),
        },
    ];
    let req = AiRequest {
        prompt: "Image: which two capital letters are drawn? PDF: which word is written? Answer as `LETTERS WORD`.".into(),
        system: None,
        schema: None,
    };
    let out = s.run_with(&req, &attachments, None).await.unwrap();
    println!("text={:?}", out.text);
    assert!(out.text.contains("ZEBRA42"), "{}", out.text);
    assert!(out.text.replace(' ', "").contains("HI"), "{}", out.text);
}
