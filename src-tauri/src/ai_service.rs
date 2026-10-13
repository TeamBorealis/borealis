//! The AI service: provider detection, task launch/streaming/cancellation and the retrieval index cache.

use crate::ai_dto::{
    AiMode, AiSink, AiStatusDto, AiTaskHandle, AiTaskRequest, AiWireEvent, ProviderDto,
};
use crate::ai_jobs::{prepare, run_job, Job};
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::settings::AiSettings;
use crate::state::{AppState, ContentHub};
use borealis_ai::provider::{detect_all, DetectConfig, DetectionSource};
use borealis_ai::retrieval::LazyIndex;
use borealis_ai::task::AiRunner;
use borealis_ai::{AiEvent, AiSession, Backend, InputTrust, ProviderInfo, SessionOptions};
use borealis_assist::VisionRunner;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Notify};

/// Running tasks: cancellation handle and flag.
struct Running {
    wake: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
}

/// Shared AI state (managed next to `AppState`).
pub struct AiService {
    detect: DetectConfig,
    seq: AtomicU64,
    tasks: Mutex<HashMap<String, Running>>,
    index: Mutex<Option<(Arc<ContentHub>, Arc<LazyIndex>)>>,
    pub(crate) host: crate::ai_host::HostSlot,
    pub(crate) setup: crate::ai_setup::SetupState,
    profile: Mutex<Option<crate::ai_profile::ProfileBase>>,
    /// App data dir; an AI program override there is never used.
    data_dir: Mutex<Option<PathBuf>>,
    /// Import drafts the user allowed to run on Codex (in memory only; asked once per import).
    import_consents: Mutex<HashSet<String>>,
}

impl Default for AiService {
    fn default() -> Self {
        Self::new(DetectConfig::default())
    }
}

/// Run the stale session temp dir sweep on a background thread (desktop startup).
pub fn spawn_startup_sweep(sweep: fn() -> usize) -> std::thread::JoinHandle<usize> {
    std::thread::spawn(sweep)
}

fn backend_name(b: Backend) -> &'static str {
    match b {
        Backend::Claude => "claude",
        Backend::Codex => "codex",
    }
}

fn source_name(s: DetectionSource) -> &'static str {
    match s {
        DetectionSource::Override => "override",
        DetectionSource::Path => "path",
        _ => "known_location",
    }
}

impl AiService {
    /// A service; `detect` lets tests point detection at a fake `PATH`.
    #[must_use]
    pub fn new(detect: DetectConfig) -> Self {
        Self {
            detect,
            seq: AtomicU64::new(0),
            tasks: Mutex::new(HashMap::new()),
            index: Mutex::new(None),
            host: crate::ai_host::HostSlot::default(),
            setup: crate::ai_setup::SetupState::default(),
            profile: Mutex::new(None),
            data_dir: Mutex::new(None),
            import_consents: Mutex::new(HashSet::new()),
        }
    }

    /// Providers installed here (desktop); phones run none.
    pub async fn providers(&self, ai: &AiSettings) -> Vec<ProviderInfo> {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            return Vec::new();
        }
        let mut cfg = self.detect.clone();
        let data_dir = self.data_dir.lock().ok().and_then(|d| d.clone());
        if let Some(p) =
            crate::ai_override::usable(ai.override_path.as_deref(), data_dir.as_deref())
        {
            let backend = if ai.provider == "codex" {
                Backend::Codex
            } else {
                Backend::Claude
            };
            cfg.overrides.insert(backend, p);
        }
        let mut found = detect_all(&cfg).await;
        let preferred = match ai.provider.as_str() {
            "claude" => Some(Backend::Claude),
            "codex" => Some(Backend::Codex),
            _ => None,
        };
        if let Some(pref) = preferred {
            found.sort_by_key(|p| p.backend != pref);
        }
        found
    }

    /// `ai_status`.
    pub async fn status(&self, ai: &AiSettings) -> AiStatusDto {
        let found = self.providers(ai).await;
        let providers = found
            .iter()
            .map(|p| ProviderDto {
                backend: backend_name(p.backend).into(),
                path: p.path.to_string_lossy().into_owned(),
                version: p.version.clone(),
                source: source_name(p.source).into(),
            })
            .collect();
        let (availability, provider, version, host, detail) = match found.first() {
            Some(p) => (
                "ready",
                backend_name(p.backend),
                p.version.clone(),
                Some("local".to_string()),
                format!("Using {} at {}.", backend_name(p.backend), p.path.display()),
            ),
            None if cfg!(any(target_os = "android", target_os = "ios")) => (
                "needs_pairing",
                "none",
                None,
                None,
                "Pair with a desktop Borealis to use AI features.".to_string(),
            ),
            None => (
                "unavailable",
                "none",
                None,
                None,
                "No claude or codex CLI was found. Install one or set its path in Settings."
                    .to_string(),
            ),
        };
        AiStatusDto {
            availability: availability.into(),
            provider: provider.into(),
            version,
            host,
            detail,
            providers,
        }
    }

    /// The `PATH` override used for detection and install (tests), if any.
    pub(crate) fn detect_path_env(&self) -> Option<std::ffi::OsString> {
        self.detect.path_env.clone()
    }

    /// Record the app data dir (startup); an AI program override there is refused.
    pub fn set_data_dir(&self, dir: PathBuf) {
        if let Ok(mut slot) = self.data_dir.lock() {
            *slot = Some(dir);
        }
    }

    /// The app data dir set at startup, if any.
    #[must_use]
    pub fn data_dir(&self) -> Option<PathBuf> {
        self.data_dir.lock().ok().and_then(|d| d.clone())
    }

    /// Enable the Borealis AI profile for every session (desktop startup).
    pub fn set_profile(&self, base: crate::ai_profile::ProfileBase) {
        if let Ok(mut slot) = self.profile.lock() {
            *slot = Some(base);
        }
    }

    /// The runner for the preferred installed provider; sessions carry the Borealis profile when
    /// one is installed, with the MCP server limited to the packs the user has enabled.
    pub async fn runner(&self, state: &AppState) -> Option<Arc<dyn AiRunner>> {
        let session: Arc<dyn AiRunner> = self.session(state).await?;
        Some(session)
    }

    /// The same session, as a runner that can also send image and PDF attachments.
    pub async fn vision_runner(&self, state: &AppState) -> Option<Arc<dyn VisionRunner>> {
        let session: Arc<dyn VisionRunner> = self.session(state).await?;
        Some(session)
    }

    async fn session(&self, state: &AppState) -> Option<Arc<AiSession>> {
        self.build_session(state, false).await
    }

    /// The preferred provider's session; `prefer_claude` puts Claude first (imports: it has no
    /// always-on shell tool).
    async fn build_session(&self, state: &AppState, prefer_claude: bool) -> Option<Arc<AiSession>> {
        let mut found = self.providers(&state.settings().ok()?.ai).await;
        if prefer_claude {
            found.sort_by_key(|p| p.backend != Backend::Claude);
        }
        let p = found.into_iter().next()?;
        let profile = self.profile.lock().ok().and_then(|g| {
            g.as_ref()
                .map(|b| b.config(Some(&state.enabled_pack_ids())))
        });
        let options = SessionOptions {
            profile,
            ..SessionOptions::default()
        };
        Some(Arc::new(AiSession::new(p, options)))
    }

    /// A session for untrusted (imported or attached) content: Claude when installed, else Codex
    /// only with the user's consent (`consent_required` otherwise).
    async fn untrusted_session(
        &self,
        state: &AppState,
        consent: bool,
    ) -> ApiResult<Arc<AiSession>> {
        let base = self.build_session(state, true).await.ok_or_else(|| {
            ApiError::new(
                ErrorCode::AiUnavailable,
                "No AI provider is available. Install the claude or codex CLI.",
            )
        })?;
        if base.provider().backend == Backend::Codex && !consent {
            return Err(ApiError::consent_required("codex"));
        }
        Ok(Arc::new(
            base.with_input_trust(InputTrust::Untrusted, consent),
        ))
    }

    /// The runner for the LAN host: remote prompts may carry imported text, so they run as
    /// untrusted input (Claude preferred; Codex needs the user's consent when enabling).
    pub(crate) async fn host_runner(
        &self,
        state: &AppState,
        consent: bool,
    ) -> ApiResult<Arc<dyn AiRunner>> {
        let session = self.untrusted_session(state, consent).await?;
        Ok(session)
    }

    fn import_consented(&self, draft_id: &str) -> bool {
        self.import_consents
            .lock()
            .is_ok_and(|c| c.contains(draft_id))
    }

    /// Check that an import step may run and remember the user's consent for this draft.
    ///
    /// # Errors
    /// `ai_unavailable` without a provider; `consent_required` when only Codex is installed and
    /// the user has not agreed (`consent` carries this request's flag).
    pub async fn import_authorize(
        &self,
        state: &AppState,
        draft_id: &str,
        consent: bool,
    ) -> ApiResult<()> {
        let allowed = consent || self.import_consented(draft_id);
        self.untrusted_session(state, allowed).await?;
        if consent {
            if let Ok(mut c) = self.import_consents.lock() {
                c.insert(draft_id.to_string());
            }
        }
        Ok(())
    }

    /// Forget an import's consent (draft finished or discarded).
    pub fn import_forget(&self, draft_id: &str) {
        if let Ok(mut c) = self.import_consents.lock() {
            c.remove(draft_id);
        }
    }

    /// The runner for an import step: untrusted input, Claude preferred.
    ///
    /// # Errors
    /// As [`AiService::import_authorize`].
    pub async fn import_runner(
        &self,
        state: &AppState,
        draft_id: &str,
    ) -> ApiResult<Arc<dyn VisionRunner>> {
        let session = self
            .untrusted_session(state, self.import_consented(draft_id))
            .await?;
        Ok(session)
    }

    fn index_for(&self, hub: &Arc<ContentHub>) -> ApiResult<Arc<LazyIndex>> {
        let mut slot = self.index.lock()?;
        if let Some((h, idx)) = slot.as_ref() {
            if Arc::ptr_eq(h, hub) {
                return Ok(Arc::clone(idx));
            }
        }
        let idx = Arc::new(LazyIndex::new(Arc::new(hub.store.clone())));
        *slot = Some((Arc::clone(hub), Arc::clone(&idx)));
        Ok(idx)
    }

    /// Start a task and return at once; events flow to `sink`.
    ///
    /// # Errors
    /// `ai_unavailable` when the task needs a provider and none is installed, `invalid_argument` /
    /// `not_found` for a bad request.
    pub async fn launch(
        self: &Arc<Self>,
        state: &AppState,
        req: AiTaskRequest,
        sink: Arc<dyn AiSink>,
    ) -> ApiResult<AiTaskHandle> {
        let index = self.index_for(&state.hub()?)?;
        let job = prepare(state, &req, &index)?;
        let runner = self.runner(state).await;
        if job.needs_provider() && runner.is_none() {
            return Err(ApiError::new(
                ErrorCode::AiUnavailable,
                "No AI provider is available. Install the claude or codex CLI.",
            ));
        }
        let task_id = format!("task-{}", self.seq.fetch_add(1, Ordering::Relaxed) + 1);
        let running = Running {
            wake: Arc::new(Notify::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let (wake, flag) = (Arc::clone(&running.wake), Arc::clone(&running.cancelled));
        self.tasks.lock()?.insert(task_id.clone(), running);
        let svc = Arc::clone(self);
        let id = task_id.clone();
        tauri::async_runtime::spawn(async move {
            drive(&id, req.mode, job, runner, sink, wake, flag).await;
            if let Ok(mut t) = svc.tasks.lock() {
                t.remove(&id);
            }
        });
        Ok(AiTaskHandle { task_id })
    }

    /// Cancel a running task (unknown or finished ids are ignored).
    pub fn cancel(&self, task_id: &str) {
        if let Ok(tasks) = self.tasks.lock() {
            if let Some(t) = tasks.get(task_id) {
                t.cancelled.store(true, Ordering::SeqCst);
                t.wake.notify_one();
            }
        }
    }
}

async fn forward(
    mut rx: mpsc::UnboundedReceiver<AiEvent>,
    id: String,
    chat: bool,
    sink: Arc<dyn AiSink>,
    cancelled: Arc<AtomicBool>,
) {
    while let Some(ev) = rx.recv().await {
        if cancelled.load(Ordering::SeqCst) {
            continue;
        }
        match ev {
            AiEvent::TextDelta { text } if chat => sink.emit(&AiWireEvent::Delta {
                task_id: id.clone(),
                text,
            }),
            AiEvent::ToolUse(t) => sink.emit(&AiWireEvent::Tool {
                task_id: id.clone(),
                name: t.name,
                summary: t.summary,
            }),
            _ => {}
        }
    }
}

async fn drive(
    id: &str,
    mode: AiMode,
    job: Job,
    runner: Option<Arc<dyn AiRunner>>,
    sink: Arc<dyn AiSink>,
    wake: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
) {
    sink.emit(&AiWireEvent::Started {
        task_id: id.into(),
        mode,
    });
    let chat = matches!(job, Job::Chat { .. });
    let (tx, rx) = mpsc::unbounded_channel();
    let fwd = tauri::async_runtime::spawn(forward(
        rx,
        id.into(),
        chat,
        Arc::clone(&sink),
        Arc::clone(&cancelled),
    ));
    let outcome = tokio::select! {
        r = run_job(job, runner, &tx) => Some(r),
        () = wake.notified() => None,
    };
    drop(tx);
    let _ = fwd.await;
    match outcome {
        None => {}
        Some(Err(e)) => sink.emit(&AiWireEvent::Error {
            task_id: id.into(),
            message: e.message,
        }),
        Some(Ok(out)) => {
            if !chat && !out.text.is_empty() {
                sink.emit(&AiWireEvent::Delta {
                    task_id: id.into(),
                    text: out.text.clone(),
                });
            }
            if let Some(kind) = out.kind {
                let result: Value = json!({ "kind": kind, "data": out.data });
                sink.emit(&AiWireEvent::Result {
                    task_id: id.into(),
                    result,
                });
            }
        }
    }
    sink.emit(&AiWireEvent::Done {
        task_id: id.into(),
        cancelled: cancelled.load(Ordering::SeqCst),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    static CALLS: AtomicU64 = AtomicU64::new(0);

    fn fake_sweep() -> usize {
        CALLS.fetch_add(1, Ordering::SeqCst);
        3
    }

    #[test]
    fn startup_sweep_runs_the_sweep_once() {
        let removed = spawn_startup_sweep(fake_sweep).join().unwrap();
        assert_eq!(removed, 3);
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }
}
