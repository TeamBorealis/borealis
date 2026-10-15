//! Headless "Import anything": runs the same public pipeline the desktop `import_start` /
//! `import_choose_parent` / `import_continue` / `import_finalize` commands call, on one local
//! file, with the installed `claude` CLI (default model and settings, untrusted input), and
//! writes the finalized pack.
//!
//! ```sh
//! cargo build -p borealis-mcp   # the profile's MCP server (skip with --no-profile)
//! cargo run -p borealis-assist --example import_file -- <file.pdf|file.txt> <out_dir> \
//!     [--edition 2014|2024] [--max-calls N] [--no-profile] [--mcp-bin <path>]
//! cargo run -p borealis-assist --example import_file -- --resume <out_dir> --parent <CLASS_ID> \
//!     [--max-calls N]
//! ```
//!
//! `<out_dir>/imported/` gets the pack and `report.json` (`ImportedPack::write_to_dir`),
//! `<out_dir>/draft.json` the reviewable draft (also the resume point after a parent choice),
//! `<out_dir>/calls/NN.txt` each model reply. `--max-calls` (default 10) refuses further model calls.
//! `BOREALIS_AI_CALL_TIMEOUT_SECS=<n>` (test only, this runner only) sets the per-call timeout
//! (default 300 s) of every model call from `BOREALIS_AI_CALL_TIMEOUT_FROM` (default 2, after
//! detect) on, e.g. to force timeouts and check that batches split and retry.
//!
//! Like the desktop, the session carries the Borealis AI profile: the profile assets
//! (`crates/borealis-ai/profile-assets`, installed into `<out_dir>/ai-profile`) and the MCP server
//! limited to the enabled packs. The desktop launches its own executable with `--mcp
//! --content-dir <bundled> --content-dir <library> --packs <enabled>`; this runner launches the
//! `borealis-mcp` binary (the same `borealis_mcp` server) with `--content <repo content>
//! --packs core,srd-5.1,srd-5.2.1`. Remaining differences: no user library packs, the pack id/name
//! are fixed, and no UI review between stages. `--no-profile` reproduces the e4f9279 runs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use borealis_ai::provider::detect_all;
use borealis_ai::session::{AiOutcome, AiRequest};
use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{
    AiError, AiEvent, AiSession, Attachment, AttachmentKind, DetectConfig, InputTrust, McpCommand,
    ProfileConfig, SessionOptions,
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
    /// The session with the test-only timeout override and the first call it runs.
    short: Option<(&'a dyn VisionRunner, u32)>,
    calls: AtomicU32,
    cap: u32,
    log: PathBuf,
}

impl<'a> Counted<'a> {
    /// The session call `n` runs on.
    fn session(&self, n: u32) -> &'a dyn VisionRunner {
        match self.short {
            Some((short, from)) if n >= from => short,
            _ => self.inner,
        }
    }

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
            self.record(n, self.session(n).run(req, sink).await)
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
            self.record(n, self.session(n).run_with(req, attachments, sink).await)
        })
    }
}

struct Args {
    source: Vec<PathBuf>,
    out: PathBuf,
    edition: Edition,
    parent: Option<String>,
    resume: bool,
    max_calls: u32,
    profile: bool,
    mcp_bin: Option<PathBuf>,
    /// `--revise <ELEMENT_ID> <instruction>` with `--resume`: the desktop's Revise on one element.
    revise: Option<(String, String)>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut pos = Vec::new();
    let (mut edition, mut parent, mut resume, mut max_calls) = (Edition::Y2024, None, false, 10);
    let (mut profile, mut mcp_bin) = (true, None);
    let mut revise = None;
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
            "--no-profile" => profile = false,
            "--mcp-bin" => mcp_bin = it.next().map(PathBuf::from),
            "--resume" => resume = true,
            "--revise" => {
                let id = it
                    .next()
                    .ok_or("--revise needs <ELEMENT_ID> <instruction>")?;
                let text = it
                    .next()
                    .ok_or("--revise needs <ELEMENT_ID> <instruction>")?;
                revise = Some((id, text));
            }
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
        (true, [out]) => (Vec::new(), out.clone()),
        (false, [srcs @ .., out]) if !srcs.is_empty() => (srcs.to_vec(), out.clone()),
        _ => return Err("usage: import_file <file> <out_dir> | --resume <out_dir>".into()),
    };
    Ok(Args {
        source,
        out,
        edition,
        parent,
        resume,
        max_calls,
        profile,
        mcp_bin,
        revise,
    })
}

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn base_builder() -> Result<ContentStoreBuilder, String> {
    let root = content_root();
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
    let office = borealis_assist::import::OfficeFormat::of_name(&name);
    let source = if let Some(format) = office {
        // As the desktop does: a Word / OpenDocument file is imported as its extracted text.
        let text = borealis_assist::import::office_text(format, &bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        ImportSource::Text(text)
    } else if is_pdf {
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

/// Several picked files as one import, as `prepare_sources` joins them: every PDF one attachment,
/// the source name the file names joined by ", ". One file is exactly [`source_of`].
fn sources_of(paths: &[PathBuf]) -> Result<(ImportSource, String), String> {
    let [first, rest @ ..] = paths else {
        return Err("no source".into());
    };
    if rest.is_empty() {
        return source_of(first);
    }
    let mut attachments = Vec::new();
    let mut names = Vec::new();
    for p in paths {
        match source_of(p)? {
            (ImportSource::Attachments(a), name) => {
                attachments.extend(a);
                names.push(name);
            }
            _ => return Err(format!("{}: only PDFs can be combined", p.display())),
        }
    }
    Ok((ImportSource::Attachments(attachments), names.join(", ")))
}

/// The desktop's profile: assets installed under `<out>/ai-profile`, MCP server on the base packs.
fn profile_config(out: &Path, mcp_bin: Option<PathBuf>) -> Result<ProfileConfig, String> {
    let program = match mcp_bin {
        Some(p) => p,
        None => std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .and_then(Path::parent)
            .map(|d| d.join("borealis-mcp"))
            .ok_or("cannot locate the target directory")?,
    };
    if !program.is_file() {
        return Err(format!(
            "{} not found: run `cargo build -p borealis-mcp`, pass --mcp-bin, or --no-profile",
            program.display()
        ));
    }
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../borealis-ai/profile-assets");
    let dir = out.join("ai-profile");
    borealis_ai::profile::install_profile(&assets, &dir).map_err(|e| e.to_string())?;
    let args = vec![
        "--content".to_string(),
        content_root().to_string_lossy().into_owned(),
        "--packs".to_string(),
        BASE_PACKS.join(","),
    ];
    Ok(ProfileConfig::new(dir, McpCommand { program, args }))
}

/// Test-only override of the per-call timeout, to force timeouts in a live run and exercise the
/// import's split/retry path. Read by this runner only; the desktop app always uses the default.
const CALL_TIMEOUT_ENV: &str = "BOREALIS_AI_CALL_TIMEOUT_SECS";
/// The first model call (1-based) the override applies to. Default 2: the detect call (which is
/// not retried and reads the whole source) keeps the default timeout.
const CALL_TIMEOUT_FROM_ENV: &str = "BOREALIS_AI_CALL_TIMEOUT_FROM";
/// Default of [`CALL_TIMEOUT_FROM_ENV`].
const DEFAULT_TIMEOUT_FROM: u32 = 2;

/// A whole number > 0 from `var`, `None` when unset.
fn positive_env(var: &str) -> Result<Option<u64>, String> {
    let Ok(raw) = std::env::var(var) else {
        return Ok(None);
    };
    raw.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .map(Some)
        .ok_or_else(|| format!("{var} must be a whole number > 0, got {raw:?}"))
}

/// `(timeout, first call)` of the test-only per-call timeout override, `None` when unset.
fn call_timeout_override() -> Result<Option<(Duration, u32)>, String> {
    let Some(secs) = positive_env(CALL_TIMEOUT_ENV)? else {
        return Ok(None);
    };
    let from = match positive_env(CALL_TIMEOUT_FROM_ENV)? {
        Some(n) => u32::try_from(n).map_err(|e| e.to_string())?,
        None => DEFAULT_TIMEOUT_FROM,
    };
    eprintln!("per-call timeout overridden: {secs} s from model call {from} ({CALL_TIMEOUT_ENV})");
    Ok(Some((Duration::from_secs(secs), from)))
}

async fn claude_session(
    profile: Option<ProfileConfig>,
    timeout: Option<Duration>,
) -> Result<AiSession, String> {
    let found = detect_all(&DetectConfig::default()).await;
    let p = found
        .into_iter()
        .find(|p| p.backend == borealis_ai::Backend::Claude)
        .ok_or("the claude CLI is not installed")?;
    let defaults = SessionOptions::default();
    let options = SessionOptions {
        profile,
        timeout: timeout.unwrap_or(defaults.timeout),
        ..defaults
    };
    Ok(AiSession::new(p, options).with_input_trust(InputTrust::Untrusted, false))
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
    let profile = if args.profile {
        Some(profile_config(&args.out, args.mcp_bin.clone())?)
    } else {
        None
    };
    let session = claude_session(profile.clone(), None).await?;
    let short = match call_timeout_override()? {
        Some((timeout, from)) => Some((claude_session(profile, Some(timeout)).await?, from)),
        None => None,
    };
    let runner = Counted {
        inner: &session,
        short: short
            .as_ref()
            .map(|(s, from)| (s as &dyn VisionRunner, *from)),
        calls: AtomicU32::new(0),
        cap: args.max_calls,
        log: args.out.join("calls"),
    };
    let opts = RunOptions {
        max_repairs: MAX_REPAIRS,
        sink: None,
    };
    let cb = |p: ImportProgress| eprintln!("[{:?} {}/{}] {}", p.stage, p.done, p.total, p.message);
    let step = if let (true, Some((id, text))) = (args.resume, args.revise.as_ref()) {
        let draft = load_draft(&args.out)?;
        let next =
            borealis_assist::revise_draft(&runner, &base, &draft, text, Some(id), &opts, Some(&cb))
                .await
                .map_err(|e| e.to_string())?;
        ImportStep::Draft(Box::new(next))
    } else if args.resume {
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
        let (source, source_name) = sources_of(&args.source)?;
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
