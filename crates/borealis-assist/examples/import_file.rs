//! Headless "Import anything": runs the same public pipeline the desktop `import_start` /
//! `import_choose_parent` / `import_continue` / `import_finalize` commands call, on one local
//! file, with the installed `claude` CLI (default model and settings, untrusted input), and
//! writes the finalized pack.
//!
//! ```sh
//! cargo run -p borealis-assist --example import_file -- <file.pdf|file.txt> <out_dir> \
//!     [--edition 2014|2024] [--max-calls N]
//! cargo run -p borealis-assist --example import_file -- --resume <out_dir> --parent <CLASS_ID> \
//!     [--max-calls N]
//! ```
//!
//! `<out_dir>/imported/` gets the pack (`ImportedPack::write_to_dir`), `<out_dir>/draft.json` the
//! reviewable draft (also the resume point after a parent choice), `<out_dir>/calls/NN.txt` each
//! model reply. `--max-calls` (default 10) refuses further model calls. Differences from the desktop:
//! no Borealis AI profile (MCP server), base packs are the bundled `core`, `srd-5.1`, `srd-5.2.1`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use borealis_ai::provider::detect_all;
use borealis_ai::session::{AiOutcome, AiRequest};
use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{
    AiError, AiEvent, AiSession, Attachment, AttachmentKind, DetectConfig, InputTrust,
    SessionOptions,
};
use borealis_assist::import::anything::{ImportProgress, ImportSource};
use borealis_assist::{
    choose_parent, continue_draft, finalize_draft, import_anything, AnythingRequest, Edition,
    ImportDraft, ImportStep, VisionRunner,
};
use borealis_content::store::ContentStoreBuilder;
use tokio::sync::mpsc::UnboundedSender;

/// Repair rounds per element, as the desktop app (`MAX_REPAIRS`).
const MAX_REPAIRS: u32 = 2;
/// Id prefix until the user picks one, as the desktop app.
const DEFAULT_PREFIX: &str = "imp";
/// Packs enabled by default on the desktop.
const BASE_PACKS: [&str; 3] = ["core", "srd-5.1", "srd-5.2.1"];

/// Counts model calls, refuses any beyond `cap`, and logs each outcome to `log/NN.txt`.
struct Counted<'a> {
    inner: &'a dyn VisionRunner,
    calls: AtomicU32,
    cap: u32,
    log: PathBuf,
}

impl Counted<'_> {
    fn admit(&self) -> Result<u32, AiError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        eprintln!("[model call {n}/{}]", self.cap);
        if n > self.cap {
            return Err(AiError::Invalid(format!("call cap {} reached", self.cap)));
        }
        Ok(n)
    }

    fn record(&self, n: u32, r: Result<AiOutcome, AiError>) -> Result<AiOutcome, AiError> {
        let body = match &r {
            Ok(o) => o.text.clone(),
            Err(e) => format!("ERROR: {e}"),
        };
        let _ = std::fs::create_dir_all(&self.log);
        let _ = std::fs::write(self.log.join(format!("{n:02}.txt")), body);
        r
    }
}

impl AiRunner for Counted<'_> {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        Box::pin(async move {
            let n = self.admit()?;
            self.record(n, self.inner.run(req, sink).await)
        })
    }
}

impl VisionRunner for Counted<'_> {
    fn run_with<'a>(
        &'a self,
        req: &'a AiRequest,
        attachments: &'a [Attachment],
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        Box::pin(async move {
            let n = self.admit()?;
            self.record(n, self.inner.run_with(req, attachments, sink).await)
        })
    }
}

struct Args {
    source: Option<PathBuf>,
    out: PathBuf,
    edition: Edition,
    parent: Option<String>,
    resume: bool,
    max_calls: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut pos = Vec::new();
    let (mut edition, mut parent, mut resume, mut max_calls) = (Edition::Y2024, None, false, 10);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--edition" => {
                edition = match it.next().as_deref() {
                    Some("2014" | "dnd2014") => Edition::Y2014,
                    Some("2024" | "dnd2024") => Edition::Y2024,
                    other => return Err(format!("bad edition {other:?}")),
                }
            }
            "--parent" => parent = it.next(),
            "--resume" => resume = true,
            "--max-calls" => {
                max_calls = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("bad --max-calls")?;
            }
            _ => pos.push(PathBuf::from(a)),
        }
    }
    let (source, out) = match (resume, pos.as_slice()) {
        (true, [out]) => (None, out.clone()),
        (false, [src, out]) => (Some(src.clone()), out.clone()),
        _ => return Err("usage: import_file <file> <out_dir> | --resume <out_dir>".into()),
    };
    Ok(Args {
        source,
        out,
        edition,
        parent,
        resume,
        max_calls,
    })
}

fn base_builder() -> Result<ContentStoreBuilder, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    BASE_PACKS
        .iter()
        .try_fold(ContentStoreBuilder::new(), |b, p| {
            b.add_pack_dir(&root.join(p))
                .map_err(|e| format!("pack {p}: {e}"))
        })
}

/// The source as `prepare_sources` builds it for one picked file.
fn source_of(path: &Path) -> Result<(ImportSource, String), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_pdf = name.to_ascii_lowercase().ends_with(".pdf");
    let source = if is_pdf {
        ImportSource::Attachments(vec![Attachment {
            kind: AttachmentKind::Pdf,
            bytes,
            name: name.clone(),
        }])
    } else {
        ImportSource::Text(String::from_utf8_lossy(&bytes).trim().to_string())
    };
    Ok((source, name))
}

async fn claude_session() -> Result<AiSession, String> {
    let found = detect_all(&DetectConfig::default()).await;
    let p = found
        .into_iter()
        .find(|p| p.backend == borealis_ai::Backend::Claude)
        .ok_or("the claude CLI is not installed")?;
    Ok(AiSession::new(p, SessionOptions::default()).with_input_trust(InputTrust::Untrusted, false))
}

fn save_draft(out: &Path, draft: &ImportDraft) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(draft).map_err(|e| e.to_string())?;
    std::fs::write(out.join("draft.json"), json).map_err(|e| e.to_string())
}

fn load_draft(out: &Path) -> Result<ImportDraft, String> {
    let text = std::fs::read_to_string(out.join("draft.json")).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn summarize(draft: &ImportDraft) {
    for e in &draft.elements {
        eprintln!("  {:?} {} [{}] {:?}", e.status, e.id, e.ty, e.issues);
    }
    for c in draft.plan.open_choices() {
        eprintln!("  open choice: {c:?}");
    }
}

async fn run(args: Args) -> Result<(), String> {
    let base = base_builder()?;
    let session = claude_session().await?;
    let runner = Counted {
        inner: &session,
        calls: AtomicU32::new(0),
        cap: args.max_calls,
        log: args.out.join("calls"),
    };
    let opts = RunOptions {
        max_repairs: MAX_REPAIRS,
        sink: None,
    };
    let cb = |p: ImportProgress| eprintln!("[{:?} {}/{}] {}", p.stage, p.done, p.total, p.message);
    let step = if args.resume {
        let draft = load_draft(&args.out)?;
        let parent = args.parent.as_deref().ok_or("--resume needs --parent")?;
        let group = draft
            .plan
            .open_choices()
            .first()
            .map(|c| c.group.clone())
            .ok_or("the draft has no open parent choice")?;
        let next = choose_parent(&base, &draft, &group, parent).map_err(|e| e.to_string())?;
        continue_draft(&runner, &base, next, &opts, Some(&cb))
            .await
            .map_err(|e| e.to_string())?
    } else {
        let src = args.source.as_deref().ok_or("no source")?;
        let (source, source_name) = source_of(src)?;
        let req = AnythingRequest {
            source,
            id_prefix: DEFAULT_PREFIX.to_string(),
            source_name,
            pack_id: "import-headless".to_string(),
            pack_name: "Imported content".to_string(),
            edition_hint: args.edition,
        };
        import_anything(&runner, &base, &req, &opts, Some(&cb))
            .await
            .map_err(|e| e.to_string())?
    };
    let calls = runner.calls.load(Ordering::SeqCst);
    eprintln!("model calls this run: {calls}");
    let draft = step.draft().clone();
    save_draft(&args.out, &draft)?;
    summarize(&draft);
    if let ImportStep::NeedsChoice(_) = step {
        return Err(
            "needs a parent choice: rerun with --resume <out_dir> --parent <CLASS_ID>".into(),
        );
    }
    let finalized = finalize_draft(&base, &draft).map_err(|e| format!("finalize: {e}"))?;
    finalized
        .pack
        .write_to_dir(&args.out.join("imported"))
        .map_err(|e| e.to_string())?;
    eprintln!("renamed: {:?}", finalized.renamed);
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let result = match parse_args() {
        Ok(args) => run(args).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
