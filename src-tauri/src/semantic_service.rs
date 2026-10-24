//! Semantic search model: consented download, pause / resume / cancel, delete (local-search-spec sec. 8, S3.2).
//!
//! The model lives in `<app-data>/models/` (never under `search/`, so "Clear caches" keeps it). Nothing here makes
//! a request on its own: the HTTP client is built on the first [`SemanticService::start`], which only the
//! `semantic_download_start` command reaches, after the user pressed "Download and turn on" in the consent dialog.
//! A partial left by an earlier run shows as paused; it resumes only when the user presses Resume.
//!
//! No embedding backend is linked yet (B-46): a downloaded model shows "semantic search arrives in a later
//! update" and search stays keyword-only. [`borealis_embed::backend_available`] flips that once it is linked.

use borealis_embed::download::{
    self, Control, DownloadError, DownloadSpec, ModelDir, Outcome, Phase, Transport,
};
use borealis_embed::models::{self, ModelFile};
use borealis_embed::{Lifecycle, Tier, VectorStore};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Folder of downloaded models under the app data folder.
pub const MODELS_DIR: &str = "models";
/// Corpus folders under `search/v1/` that may hold `vectors-*` files (spec sec. 7 vector storage).
pub const VECTOR_CORPORA: [&str; 3] = ["rules", "lore", "sessions"];
/// Shortest gap between two progress events (the last chunk and phase changes always go out).
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const DOWNLOADED_NO_BACKEND: &str =
    "Model downloaded. Semantic search arrives in a later update of Borealis; until then search uses keywords.";
const PAUSED_IN_BACKGROUND: &str = "Paused while Borealis was in the background.";

/// Where the download stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    /// Nothing downloaded and nothing running.
    Idle,
    Downloading,
    /// Checking the SHA-256 of the complete file.
    Verifying,
    /// Stopped with a partial kept (Resume continues with `Range`).
    Paused,
    /// The last attempt failed (`message` says why).
    Failed,
    /// The model file is in place.
    Done,
}

/// `semantic://download` payload, also inside [`SemanticStatus`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticDownload {
    pub state: DownloadState,
    /// Bytes downloaded (or verified, while verifying).
    pub done: u64,
    pub total: u64,
    /// Plain sentence for Settings (failure reason, background pause, no backend yet).
    pub message: Option<String>,
}

/// The file this device downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticFile {
    pub quant: String,
    pub file_name: String,
    pub bytes: u64,
    /// Consent-dialog size text ("about 310 MB", "210 MB").
    pub size_text: String,
}

/// Consent dialog text (spec sec. 8 exact wording, sizes for this device).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticConsent {
    pub title: String,
    pub body: String,
    pub accept: String,
    pub decline: String,
}

/// `semantic_status` result: what Settings > Search shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    /// Tier in effect (probe lowered by the override).
    pub tier: Tier,
    /// Tier the device probe allows (the override can only go below it).
    pub probed_tier: Tier,
    pub mobile: bool,
    /// The file for `tier`; `None` on T0.
    pub file: Option<SemanticFile>,
    /// The file can be downloaded (a URL serves it).
    pub available: bool,
    /// Why semantic search is not offered or not downloadable (T0, mirror not published yet).
    pub unavailable_reason: Option<String>,
    /// The file for `tier` is on the device.
    pub model_present: bool,
    /// An embedding backend is linked into this build.
    pub backend_available: bool,
    /// `settings.search.semantic`.
    pub semantic: bool,
    /// Search really uses embeddings (`semantic`, model present, backend linked).
    pub active: bool,
    pub consent: Option<SemanticConsent>,
    pub download: SemanticDownload,
}

/// What [`SemanticService::status`] needs from the device and the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticContext {
    pub probed: Tier,
    pub tier: Tier,
    pub mobile: bool,
    pub semantic: bool,
    /// Reason the probe gives for T0 (64-bit / memory), if any.
    pub t0_reason: Option<&'static str>,
}

/// Receives download progress (the Tauri event in the app, a recorder in tests).
pub trait SemanticSink: Send + Sync {
    fn download(&self, status: &SemanticDownload);
}

/// The model file per tier and the full list delete removes.
#[derive(Debug, Clone)]
pub struct Catalog {
    t1: Option<DownloadSpec>,
    t2_t3: Option<DownloadSpec>,
    meta: Vec<(String, SemanticFile)>,
}

impl Catalog {
    /// The compiled table (`borealis_embed::models`).
    #[must_use]
    pub fn compiled() -> Self {
        let file = |f: &ModelFile| SemanticFile {
            quant: f.quant.into(),
            file_name: f.file_name.into(),
            bytes: f.bytes,
            size_text: f.size_text.into(),
        };
        Self {
            t1: Some(DownloadSpec::for_file(&models::Q6_K_TE_Q4_K)),
            t2_t3: Some(DownloadSpec::for_file(&models::Q8_0)),
            meta: vec![
                (
                    models::Q6_K_TE_Q4_K.file_name.into(),
                    file(&models::Q6_K_TE_Q4_K),
                ),
                (models::Q8_0.file_name.into(), file(&models::Q8_0)),
            ],
        }
    }

    /// A catalog of test files (one spec for T1, one for T2/T3), for network-mocked tests.
    #[must_use]
    pub fn for_tests(t1: Option<DownloadSpec>, t2_t3: Option<DownloadSpec>) -> Self {
        let meta = t1
            .iter()
            .chain(t2_t3.iter())
            .map(|s| {
                (
                    s.file_name.clone(),
                    SemanticFile {
                        quant: "TEST".into(),
                        file_name: s.file_name.clone(),
                        bytes: s.bytes,
                        size_text: format!("{} bytes", s.bytes),
                    },
                )
            })
            .collect();
        Self { t1, t2_t3, meta }
    }

    fn spec(&self, tier: Tier) -> Option<&DownloadSpec> {
        match tier {
            Tier::T0 => None,
            Tier::T1 => self.t1.as_ref(),
            Tier::T2 | Tier::T3 => self.t2_t3.as_ref(),
        }
    }

    fn file(&self, spec: &DownloadSpec) -> Option<&SemanticFile> {
        self.meta
            .iter()
            .find(|(n, _)| *n == spec.file_name)
            .map(|(_, f)| f)
    }

    fn all(&self) -> Vec<DownloadSpec> {
        self.t1.iter().chain(self.t2_t3.iter()).cloned().collect()
    }
}

type TransportFactory = Box<dyn Fn() -> Result<Arc<dyn Transport>, String> + Send + Sync>;

struct Job {
    /// The spec of the file the progress belongs to.
    file_name: Option<String>,
    progress: SemanticDownload,
    control: Option<Control>,
    handle: Option<JoinHandle<()>>,
}

struct Inner {
    models: ModelDir,
    search_root: PathBuf,
    catalog: Catalog,
    make_transport: TransportFactory,
    transport: Mutex<Option<Arc<dyn Transport>>>,
    job: Mutex<Job>,
    sink: RwLock<Option<Arc<dyn SemanticSink>>>,
}

/// The app's model download service (cheap to clone).
#[derive(Clone)]
pub struct SemanticService(Arc<Inner>);

impl std::fmt::Debug for SemanticService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticService").finish_non_exhaustive()
    }
}

fn idle() -> SemanticDownload {
    SemanticDownload {
        state: DownloadState::Idle,
        done: 0,
        total: 0,
        message: None,
    }
}

/// The real HTTPS client: the platform TLS of `content_fetch_tls`, locked down by `HttpTransport`.
fn https_transport() -> Result<Arc<dyn Transport>, String> {
    let builder = crate::content_fetch_tls::client_builder()?;
    download::http::HttpTransport::new(builder)
        .map(|t| Arc::new(t) as Arc<dyn Transport>)
        .map_err(|e| e.0)
}

impl SemanticService {
    /// The service for the app data folder `data` with the compiled model table and the real HTTPS client.
    #[must_use]
    pub fn new(data: &Path) -> Self {
        Self::with_backend(data, Catalog::compiled(), Box::new(https_transport))
    }

    /// Same, with another catalog and transport (network-mocked tests).
    #[must_use]
    pub fn with_backend(data: &Path, catalog: Catalog, make_transport: TransportFactory) -> Self {
        Self(Arc::new(Inner {
            models: ModelDir::new(data.join(MODELS_DIR)),
            search_root: data
                .join(crate::search_service::SEARCH_DIR)
                .join(crate::search_service::LAYOUT_DIR),
            catalog,
            make_transport,
            transport: Mutex::new(None),
            job: Mutex::new(Job {
                file_name: None,
                progress: idle(),
                control: None,
                handle: None,
            }),
            sink: RwLock::new(None),
        }))
    }

    pub fn set_sink(&self, sink: Arc<dyn SemanticSink>) {
        if let Ok(mut s) = self.0.sink.write() {
            *s = Some(sink);
        }
    }

    /// The models folder.
    #[must_use]
    pub fn models_dir(&self) -> &Path {
        self.0.models.root()
    }

    fn job(&self) -> MutexGuard<'_, Job> {
        self.0
            .job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn emit(&self, progress: &SemanticDownload) {
        if let Ok(sink) = self.0.sink.read() {
            if let Some(sink) = sink.as_ref() {
                sink.download(progress);
            }
        }
    }

    /// What Settings > Search shows; reads the folder, never the network.
    #[must_use]
    pub fn status(&self, ctx: &SemanticContext) -> SemanticStatus {
        let spec = self.0.catalog.spec(ctx.tier);
        let present = spec.is_some_and(|s| self.0.models.is_installed(s));
        let available = spec.is_some_and(|s| !s.sources.is_empty());
        let backend = borealis_embed::backend_available();
        let unavailable_reason = match spec {
            None => Some(
                ctx.t0_reason
                    .unwrap_or("Semantic search is not available on this device. Keyword search works fully.")
                    .to_owned(),
            ),
            Some(_) if !available && !present => Some(DownloadError::NotAvailable.to_string()),
            Some(_) => None,
        };
        let file = spec.and_then(|s| self.0.catalog.file(s)).cloned();
        let consent = file.as_ref().map(|f| consent_for(f, ctx.mobile));
        SemanticStatus {
            tier: ctx.tier,
            probed_tier: ctx.probed,
            mobile: ctx.mobile,
            file,
            available,
            unavailable_reason,
            model_present: present,
            backend_available: backend,
            semantic: ctx.semantic,
            active: ctx.semantic && present && backend,
            consent,
            download: self.download_status(spec, present, backend),
        }
    }

    fn download_status(
        &self,
        spec: Option<&DownloadSpec>,
        present: bool,
        backend: bool,
    ) -> SemanticDownload {
        let job = self.job();
        let running = job.handle.as_ref().is_some_and(|h| !h.is_finished());
        let same_file = spec.is_some_and(|s| job.file_name.as_deref() == Some(&s.file_name));
        if running && same_file {
            return job.progress.clone();
        }
        let Some(spec) = spec else { return idle() };
        if present {
            return SemanticDownload {
                state: DownloadState::Done,
                done: spec.bytes,
                total: spec.bytes,
                message: (!backend).then(|| DOWNLOADED_NO_BACKEND.to_owned()),
            };
        }
        if same_file && job.progress.state == DownloadState::Failed {
            return job.progress.clone();
        }
        let have = self.0.models.partial_bytes(spec);
        if have > 0 {
            let message = (same_file && job.progress.state == DownloadState::Paused)
                .then(|| job.progress.message.clone())
                .flatten();
            return SemanticDownload {
                state: DownloadState::Paused,
                done: have,
                total: spec.bytes,
                message,
            };
        }
        idle()
    }

    /// Start or resume the download of the file for `tier` on a background thread. The user consented
    /// (consent dialog, or Resume of a download they started). A running download is left alone.
    ///
    /// # Errors
    /// The plain reason when the tier has no file or no URL serves it, or the HTTPS client cannot be built.
    pub fn start(&self, tier: Tier, lifecycle: Option<Lifecycle>) -> Result<(), String> {
        let spec = self
            .0
            .catalog
            .spec(tier)
            .cloned()
            .ok_or("Semantic search is not available on this device.")?;
        let mut job = self.job();
        if job.handle.as_ref().is_some_and(|h| !h.is_finished()) {
            return Ok(());
        }
        if self.0.models.is_installed(&spec) {
            job.file_name = Some(spec.file_name.clone());
            return Ok(());
        }
        if spec.sources.is_empty() {
            return Err(DownloadError::NotAvailable.to_string());
        }
        let transport = self.transport()?;
        let control = Control::new();
        let total = spec.bytes;
        job.file_name = Some(spec.file_name.clone());
        job.progress = SemanticDownload {
            state: DownloadState::Downloading,
            done: self.0.models.partial_bytes(&spec),
            total,
            message: None,
        };
        job.control = Some(control.clone());
        self.emit(&job.progress);
        let me = self.clone();
        job.handle = Some(std::thread::spawn(move || {
            me.run(&spec, transport.as_ref(), &control, lifecycle.as_ref());
        }));
        Ok(())
    }

    fn transport(&self) -> Result<Arc<dyn Transport>, String> {
        let mut slot = self
            .0
            .transport
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(t) = slot.as_ref() {
            return Ok(Arc::clone(t));
        }
        let made = (self.0.make_transport)()?;
        *slot = Some(Arc::clone(&made));
        Ok(made)
    }

    fn run(
        &self,
        spec: &DownloadSpec,
        transport: &dyn Transport,
        control: &Control,
        lifecycle: Option<&Lifecycle>,
    ) {
        let mut last_emit: Option<(Instant, Phase)> = None;
        let mut on_progress = |p: download::Progress| {
            if lifecycle.is_some_and(Lifecycle::is_backgrounded) {
                control.pause();
            }
            let state = match p.phase {
                Phase::Downloading => DownloadState::Downloading,
                Phase::Verifying => DownloadState::Verifying,
            };
            let progress = SemanticDownload {
                state,
                done: p.done,
                total: p.total,
                message: None,
            };
            self.job().progress = progress.clone();
            let due = last_emit.map_or(true, |(at, phase)| {
                phase != p.phase || at.elapsed() >= PROGRESS_EVERY || p.done == p.total
            });
            if due {
                last_emit = Some((Instant::now(), p.phase));
                self.emit(&progress);
            }
        };
        let result = download::download(&self.0.models, spec, transport, control, &mut on_progress);
        let backgrounded = lifecycle.is_some_and(Lifecycle::is_backgrounded);
        let end = match result {
            Ok(Outcome::Installed(_)) => SemanticDownload {
                state: DownloadState::Done,
                done: spec.bytes,
                total: spec.bytes,
                message: (!borealis_embed::backend_available())
                    .then(|| DOWNLOADED_NO_BACKEND.to_owned()),
            },
            Ok(Outcome::Paused { have }) => SemanticDownload {
                state: DownloadState::Paused,
                done: have,
                total: spec.bytes,
                message: backgrounded.then(|| PAUSED_IN_BACKGROUND.to_owned()),
            },
            Ok(Outcome::Cancelled) => idle(),
            Err(e) => {
                tracing::warn!("semantic model download failed: {e}");
                SemanticDownload {
                    state: DownloadState::Failed,
                    done: self.0.models.partial_bytes(spec),
                    total: spec.bytes,
                    message: Some(e.to_string()),
                }
            }
        };
        self.job().progress = end.clone();
        self.emit(&end);
    }

    /// Pause a running download (the partial is kept).
    pub fn pause(&self) {
        if let Some(c) = self.job().control.as_ref() {
            c.pause();
        }
    }

    /// Cancel a running download and delete every partial; waits for the download thread to stop.
    ///
    /// # Errors
    /// The partial could not be deleted.
    pub fn cancel(&self) -> std::io::Result<()> {
        self.stop_job();
        for spec in self.0.catalog.all() {
            let part = self.0.models.partial_path(&spec);
            match std::fs::remove_file(&part) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        let progress = idle();
        self.job().progress = progress.clone();
        self.emit(&progress);
        Ok(())
    }

    fn stop_job(&self) {
        let handle = {
            let mut job = self.job();
            if let Some(c) = job.control.as_ref() {
                c.cancel();
            }
            job.handle.take()
        };
        if let Some(h) = handle {
            let _ = h.join();
        }
    }

    /// "Delete model and semantic indexes": stop any download, delete every model file, partial and licence
    /// file, and every `vectors-*` file and sidecar under `search/v1/<corpus>/`. Keyword indexes stay.
    ///
    /// # Errors
    /// A file could not be deleted.
    pub fn delete_model_and_vectors(&self) -> std::io::Result<()> {
        self.stop_job();
        self.0.models.delete(&self.0.catalog.all())?;
        for corpus in VECTOR_CORPORA {
            let dir = self.0.search_root.join(corpus);
            if dir.is_dir() {
                VectorStore::delete_all(&dir).map_err(|e| std::io::Error::other(e.to_string()))?;
            }
        }
        let progress = idle();
        self.job().progress = progress.clone();
        self.emit(&progress);
        Ok(())
    }

    /// Wait for a running download to end (tests).
    pub fn wait(&self) {
        let handle = self.job().handle.take();
        if let Some(h) = handle {
            let _ = h.join();
        }
    }
}

fn consent_for(file: &SemanticFile, mobile: bool) -> SemanticConsent {
    let compiled = models::ALL_FILES
        .into_iter()
        .find(|f| f.file_name == file.file_name)
        .unwrap_or(models::Q8_0);
    SemanticConsent {
        title: models::CONSENT_TITLE.into(),
        body: models::consent_body(&compiled, mobile),
        accept: models::CONSENT_ACCEPT.into(),
        decline: models::CONSENT_DECLINE.into(),
    }
}
