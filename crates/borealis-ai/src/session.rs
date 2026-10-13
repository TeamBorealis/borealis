//! Spawning a CLI, streaming its output as [`AiEvent`]s and enforcing safety limits.
//!
//! * The prompt is written to stdin; argv never contains user content and no shell is involved.
//! * The agent runs in an isolated, empty temp directory by default with no tools.
//! * An overall timeout, an idle timeout, a total output cap and a per-line cap are enforced.
//! * Cancellation (explicit, or by dropping [`RunningSession`] or its event receiver) kills the
//!   whole process group, not just the direct child.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::attachment::{self, Attachment, PreparedItem};
use crate::child_env;
use crate::command::{CommandInputs, ToolPolicy};
use crate::error::TimeoutKind;
use crate::event::{AiEvent, ToolUseInfo, Usage};
use crate::parse::{parser_for, StreamParser};
use crate::profile::{prepare_profile_args, ProfileConfig};
use crate::provider::ProviderInfo;
use crate::task::prompt;
use crate::trust::{self, InputTrust};
use crate::{redact_paths, AiError, Backend};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_MAX_OUTPUT: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_LINE: usize = 4 * 1024 * 1024;
const EXIT_GRACE: Duration = Duration::from_secs(5);
const STDERR_JOIN_GRACE: Duration = Duration::from_secs(2);
const STDERR_TAIL_BYTES: usize = 16 * 1024;

/// Tunables for one session.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Wall-clock budget for the whole run.
    pub timeout: Duration,
    /// Maximum silence between output lines.
    pub idle_timeout: Duration,
    /// Cap on total stdout bytes.
    pub max_output_bytes: usize,
    /// Cap on a single stdout line.
    pub max_line_bytes: usize,
    /// Model alias or name.
    pub model: Option<String>,
    /// Reasoning effort.
    pub effort: Option<String>,
    /// Tool access (default none).
    pub tools: ToolPolicy,
    /// Working directory; `None` = fresh empty temp dir removed afterwards.
    pub workdir: Option<PathBuf>,
    /// Extra environment variables for the child.
    pub env: Vec<(String, String)>,
    /// Use the CLI's native schema flag. `None` = Claude yes, Codex no (Codex requires OpenAI
    /// strict-mode schemas, which our content schema is not; the schema is still in the prompt).
    pub native_schema: Option<bool>,
    /// Let Codex load the user's `config.toml`.
    pub use_user_config: bool,
    /// Borealis AI profile (MCP server, locked settings, skills). `None` = no profile. When set
    /// and `enabled`, the profile is verified before every run and its args are added.
    pub profile: Option<ProfileConfig>,
    /// Trust of the prompt content. Attachments always make a run [`InputTrust::Untrusted`].
    pub input_trust: InputTrust,
    /// Explicit user consent to send untrusted input to Codex anyway (see [`crate::trust`]).
    pub allow_untrusted_with_codex: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT,
            max_line_bytes: DEFAULT_MAX_LINE,
            model: None,
            effort: None,
            tools: ToolPolicy::None,
            workdir: None,
            env: Vec::new(),
            native_schema: None,
            use_user_config: false,
            profile: None,
            input_trust: InputTrust::Trusted,
            allow_untrusted_with_codex: false,
        }
    }
}

/// What to ask.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiRequest {
    /// User prompt (delivered over stdin).
    pub prompt: String,
    /// System instructions.
    pub system: Option<String>,
    /// JSON Schema the final answer must satisfy.
    pub schema: Option<Value>,
}

/// Result of a successful run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AiOutcome {
    /// Final text.
    pub text: String,
    /// Parsed structured output when available.
    pub structured: Option<Value>,
    /// Token/cost accounting.
    pub usage: Option<Usage>,
    /// Tool calls made.
    pub tool_uses: Vec<ToolUseInfo>,
    /// Non-fatal CLI warnings.
    pub notices: Vec<String>,
    /// Redacted stderr tail.
    pub stderr_tail: String,
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Backend session id.
    pub session_id: Option<String>,
}

/// Clonable cancellation switch.
#[derive(Debug, Clone)]
pub struct CancelToken {
    tx: Arc<watch::Sender<bool>>,
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelToken {
    /// New, not-cancelled token.
    pub fn new() -> Self {
        Self {
            tx: Arc::new(watch::channel(false).0),
        }
    }

    /// Request cancellation (idempotent).
    pub fn cancel(&self) {
        self.tx.send_replace(true);
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }

    fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }
}

/// A configured CLI plus options; start one run per [`AiRequest`].
#[derive(Debug, Clone)]
pub struct AiSession {
    provider: ProviderInfo,
    options: SessionOptions,
}

impl AiSession {
    /// Create a session factory for `provider`.
    pub fn new(provider: ProviderInfo, options: SessionOptions) -> Self {
        Self { provider, options }
    }

    /// The provider in use.
    pub fn provider(&self) -> &ProviderInfo {
        &self.provider
    }

    /// The options in use.
    pub fn options(&self) -> &SessionOptions {
        &self.options
    }

    /// A copy of this session for content of the given trust. `allow_untrusted_with_codex` is the
    /// user's explicit consent to run untrusted content on Codex (ignored for Claude).
    pub fn with_input_trust(&self, trust: InputTrust, allow_untrusted_with_codex: bool) -> Self {
        let mut options = self.options.clone();
        options.input_trust = trust;
        options.allow_untrusted_with_codex = allow_untrusted_with_codex;
        Self::new(self.provider.clone(), options)
    }

    /// Spawn the CLI and return a handle that streams events.
    pub async fn start(&self, req: &AiRequest) -> Result<RunningSession, AiError> {
        self.start_with(req, &[]).await
    }

    /// Like [`AiSession::start`], with image/PDF attachments the model can see.
    ///
    /// Attachments are validated and downscaled first (see [`crate::attachment`]); a bad one
    /// fails here before any process is spawned. Codex image files live in the session's temp
    /// dir and are deleted when the process is gone.
    pub async fn start_with(
        &self,
        req: &AiRequest,
        attachments: &[Attachment],
    ) -> Result<RunningSession, AiError> {
        let backend = self.provider.backend;
        trust::check_backend(
            backend,
            self.options
                .input_trust
                .and(InputTrust::from_attachments(attachments.len())),
            self.options.allow_untrusted_with_codex,
        )?;
        let support = tempfile::Builder::new()
            .prefix("borealis-ai-support-")
            .tempdir()
            .map_err(AiError::io)?;
        let (workdir, work_guard) = match &self.options.workdir {
            Some(p) => (p.clone(), None),
            None => {
                let d = tempfile::Builder::new()
                    .prefix("borealis-ai-work-")
                    .tempdir()
                    .map_err(AiError::io)?;
                (d.path().to_path_buf(), Some(d))
            }
        };
        let native = self
            .options
            .native_schema
            .unwrap_or(backend == Backend::Claude);
        let items = attachment::prepare(backend, attachments)?;
        let mut image_paths = Vec::new();
        if backend == Backend::Codex {
            for item in &items {
                if let PreparedItem::Image { mime, bytes, .. } = item {
                    let p = support
                        .path()
                        .join(attachment::codex_file_name(image_paths.len(), mime));
                    std::fs::write(&p, bytes).map_err(AiError::io)?;
                    image_paths.push(p);
                }
            }
        }
        let schema_file = match (&req.schema, backend, native) {
            (Some(schema), Backend::Codex, true) => {
                let p = support.path().join("schema.json");
                std::fs::write(&p, schema.to_string()).map_err(AiError::io)?;
                Some(p)
            }
            _ => None,
        };
        let profile_args = match self.options.profile.as_ref().filter(|p| p.enabled) {
            Some(p) => {
                prepare_profile_args(backend, &p.dir, &p.mcp, support.path()).map_err(|files| {
                    AiError::Invalid(redact_paths(&format!(
                        "Borealis AI profile failed verification: {}",
                        files.join(", ")
                    )))
                })?
            }
            None => Vec::new(),
        };
        let system = system_with_note(
            req.system.as_deref(),
            prompt::capability_note(
                backend,
                !profile_args.is_empty(),
                self.options.tools == ToolPolicy::ReadOnly,
            ),
        );
        // Claude: the system prompt goes to a file in the private support dir, not argv.
        let system_file = if backend == Backend::Claude {
            let p = support.path().join("system.txt");
            std::fs::write(&p, &system).map_err(AiError::io)?;
            Some(p)
        } else {
            None
        };
        let plan = attachment::build_with_attachments(
            &CommandInputs {
                backend,
                program: &self.provider.path,
                workdir: &workdir,
                prompt: &req.prompt,
                system: Some(&system),
                system_file: system_file.as_deref(),
                model: self.options.model.as_deref(),
                effort: self.options.effort.as_deref(),
                tools: self.options.tools,
                schema: req.schema.as_ref(),
                native_schema: native,
                schema_file: schema_file.as_deref(),
                use_user_config: self.options.use_user_config,
                profile_args: &profile_args,
            },
            &items,
            &image_paths,
        );

        let mut cmd = Command::new(&plan.program);
        cmd.args(&plan.args)
            .current_dir(&workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear()
            .envs(child_env::child_env(backend, std::env::vars_os()));
        for (k, v) in &self.options.env {
            cmd.env(k, v);
        }
        #[cfg(unix)]
        cmd.process_group(0);
        crate::spawn::hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(AiError::spawn)?;
        tracing::info!(backend = ?backend, "ai session started");

        let stdin = child.stdin.take();
        let stdin_task = tokio::spawn(async move {
            if let Some(mut stdin) = stdin {
                // A child that exits before reading everything yields BrokenPipe; not an error.
                let _ = stdin.write_all(&plan.stdin).await;
                let _ = stdin.shutdown().await;
            }
        });
        let stderr_task = child
            .stderr
            .take()
            .map(|s| tokio::spawn(read_tail(s, STDERR_TAIL_BYTES)));

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AiError::Spawn("no stdout pipe".into()))?;
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = CancelToken::new();
        let driver = Driver {
            child,
            stdout: BufReader::new(stdout),
            parser: parser_for(backend),
            options: self.options.clone(),
            tx,
            cancel_rx: cancel.subscribe(),
            stderr_task,
            state: RunState::default(),
        };
        let join = tokio::spawn(async move {
            // Keep temp dirs alive until the process is gone.
            let _guards = (support, work_guard);
            let result = driver.run().await;
            stdin_task.abort();
            result
        });
        Ok(RunningSession {
            events: rx,
            cancel,
            join: Some(join),
        })
    }

    /// Run to completion, forwarding every event to `sink` if given.
    pub async fn run(
        &self,
        req: &AiRequest,
        sink: Option<&mpsc::UnboundedSender<AiEvent>>,
    ) -> Result<AiOutcome, AiError> {
        self.run_with(req, &[], sink).await
    }

    /// [`AiSession::run`] with attachments.
    pub async fn run_with(
        &self,
        req: &AiRequest,
        attachments: &[Attachment],
        sink: Option<&mpsc::UnboundedSender<AiEvent>>,
    ) -> Result<AiOutcome, AiError> {
        let mut running = self.start_with(req, attachments).await?;
        while let Some(ev) = running.next_event().await {
            if let Some(s) = sink {
                if s.send(ev).is_err() {
                    running.cancel();
                }
            }
        }
        running.wait().await
    }
}

/// The request's system prompt followed by the provider capability note.
fn system_with_note(system: Option<&str>, note: String) -> String {
    match system {
        Some(s) if !s.trim().is_empty() => format!("{s}\n\n{note}"),
        _ => note,
    }
}

/// Handle to a live session.
#[derive(Debug)]
pub struct RunningSession {
    events: mpsc::UnboundedReceiver<AiEvent>,
    cancel: CancelToken,
    join: Option<JoinHandle<Result<AiOutcome, AiError>>>,
}

impl RunningSession {
    /// Next event, or `None` once the session has ended.
    pub async fn next_event(&mut self) -> Option<AiEvent> {
        self.events.recv().await
    }

    /// A clonable token that cancels this session from anywhere.
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Kill the process tree and end the session.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Drain remaining events and return the outcome.
    pub async fn wait(mut self) -> Result<AiOutcome, AiError> {
        while self.events.recv().await.is_some() {}
        match self.join.take() {
            Some(j) => j
                .await
                .map_err(|e| AiError::Io(format!("session task failed: {e}")))?,
            None => Err(AiError::Cancelled),
        }
    }
}

impl Drop for RunningSession {
    fn drop(&mut self) {
        // Dropping an unfinished session must not leave an orphaned CLI running.
        self.cancel.cancel();
    }
}

#[derive(Default)]
struct RunState {
    outcome: AiOutcome,
    final_seen: bool,
    cli_error: Option<String>,
    streamed: String,
    total_bytes: usize,
}

struct Driver {
    child: Child,
    stdout: BufReader<tokio::process::ChildStdout>,
    parser: Box<dyn StreamParser>,
    options: SessionOptions,
    tx: mpsc::UnboundedSender<AiEvent>,
    cancel_rx: watch::Receiver<bool>,
    stderr_task: Option<JoinHandle<String>>,
    state: RunState,
}

enum LineRead {
    Line,
    Eof,
    TooLong,
}

impl Driver {
    async fn run(mut self) -> Result<AiOutcome, AiError> {
        let result = self.pump().await;
        if let Err(e) = &result {
            // Parser-originated errors were already streamed; surface the rest.
            if self.state.cli_error.is_none() {
                let _ = self.tx.send(AiEvent::Error {
                    message: redact_paths(&e.to_string()),
                });
            }
        }
        result
    }

    async fn pump(&mut self) -> Result<AiOutcome, AiError> {
        let overall = tokio::time::sleep(self.options.timeout);
        tokio::pin!(overall);
        let mut idle_deadline = Instant::now() + self.options.idle_timeout;
        let mut buf = Vec::new();
        loop {
            let read = read_line_limited(&mut self.stdout, &mut buf, self.options.max_line_bytes);
            let step = tokio::select! {
                biased;
                _ = self.cancel_rx.wait_for(|c| *c) => Err(AiError::Cancelled),
                _ = &mut overall => Err(AiError::Timeout { kind: TimeoutKind::Overall, after: self.options.timeout }),
                _ = tokio::time::sleep_until(idle_deadline) => Err(AiError::Timeout { kind: TimeoutKind::Idle, after: self.options.idle_timeout }),
                r = read => r.map_err(AiError::io),
            };
            match step {
                Err(e) => {
                    kill_tree(&mut self.child).await;
                    return Err(e);
                }
                Ok(LineRead::Eof) => break,
                Ok(LineRead::TooLong) => {
                    kill_tree(&mut self.child).await;
                    return Err(AiError::OutputTooLarge {
                        limit: self.options.max_line_bytes,
                    });
                }
                Ok(LineRead::Line) => {
                    idle_deadline = Instant::now() + self.options.idle_timeout;
                    self.state.total_bytes += buf.len();
                    if self.state.total_bytes > self.options.max_output_bytes {
                        kill_tree(&mut self.child).await;
                        return Err(AiError::OutputTooLarge {
                            limit: self.options.max_output_bytes,
                        });
                    }
                    let line = String::from_utf8_lossy(&buf);
                    let events = self.parser.feed_line(line.trim_end_matches(['\r', '\n']));
                    if !self.dispatch(events) {
                        kill_tree(&mut self.child).await;
                        return Err(AiError::Cancelled);
                    }
                }
            }
        }
        let tail = self.parser.finish();
        self.dispatch(tail);
        self.finish().await
    }

    /// Record events and forward them. Returns `false` when the consumer is gone.
    fn dispatch(&mut self, events: Vec<AiEvent>) -> bool {
        for ev in events.into_iter().map(AiEvent::redacted) {
            let st = &mut self.state;
            match &ev {
                AiEvent::Started { session_id, .. } => st.outcome.session_id.clone_from(session_id),
                AiEvent::TextDelta { text } => st.streamed.push_str(text),
                AiEvent::ToolUse(t) => st.outcome.tool_uses.push(t.clone()),
                AiEvent::Usage(u) => st.outcome.usage = Some(u.clone()),
                AiEvent::Notice { message } => st.outcome.notices.push(message.clone()),
                AiEvent::Final { text, structured } => {
                    st.final_seen = true;
                    st.outcome.text.clone_from(text);
                    st.outcome.structured.clone_from(structured);
                }
                AiEvent::Error { message } => st.cli_error = Some(message.clone()),
            }
            if self.tx.send(ev).is_err() {
                return false;
            }
        }
        true
    }

    async fn finish(&mut self) -> Result<AiOutcome, AiError> {
        let status = match tokio::time::timeout(EXIT_GRACE, self.child.wait()).await {
            Ok(r) => r.map_err(AiError::io)?,
            Err(_) => {
                kill_tree(&mut self.child).await;
                self.child.wait().await.map_err(AiError::io)?
            }
        };
        // Reap stragglers (grandchildren holding the pipes open) once the leader is gone.
        #[cfg(unix)]
        kill_group(self.child.id()).await;
        let stderr = match self.stderr_task.take() {
            Some(t) => tokio::time::timeout(STDERR_JOIN_GRACE, t)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
            None => String::new(),
        };
        let stderr_tail = redact_paths(stderr.trim());
        let code = status.code();
        let st = &mut self.state;
        st.outcome.stderr_tail.clone_from(&stderr_tail);
        st.outcome.exit_code = code;
        if st.final_seen {
            return Ok(std::mem::take(&mut st.outcome));
        }
        if let Some(msg) = st.cli_error.take() {
            st.cli_error = Some(msg.clone());
            return Err(AiError::cli(&msg, &stderr_tail, code));
        }
        if !status.success() {
            let msg = format!(
                "the CLI exited with status {}",
                code.map_or("unknown".to_string(), |c| c.to_string())
            );
            return Err(AiError::cli(&msg, &stderr_tail, code));
        }
        Err(AiError::NoResult)
    }
}

async fn read_line_limited<R: AsyncBufRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<LineRead> {
    buf.clear();
    loop {
        let avail = r.fill_buf().await?;
        if avail.is_empty() {
            return Ok(if buf.is_empty() {
                LineRead::Eof
            } else {
                LineRead::Line
            });
        }
        let newline = avail.iter().position(|&b| b == b'\n');
        let take = newline.map_or(avail.len(), |p| p + 1);
        if buf.len() + take > max {
            return Ok(LineRead::TooLong);
        }
        buf.extend_from_slice(&avail[..take]);
        r.consume(take);
        if newline.is_some() {
            return Ok(LineRead::Line);
        }
    }
}

/// Read a stream to its end, keeping only the last `keep` bytes.
async fn read_tail<R: AsyncReadExt + Unpin>(mut r: R, keep: usize) -> String {
    let mut tail: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&chunk[..n]);
                if tail.len() > keep {
                    let cut = tail.len() - keep;
                    tail.drain(..cut);
                }
            }
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

#[cfg(unix)]
async fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = Command::new("/bin/kill")
            .args(["-s", "KILL", "--"])
            .arg(format!("-{pid}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

/// Kill the child and everything it spawned.
async fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    kill_group(child.id()).await;
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let _ = crate::spawn::hide_console(
            Command::new("taskkill")
                .args(["/T", "/F", "/PID"])
                .arg(pid.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .status()
        .await;
    }
    let _ = child.kill().await;
}
