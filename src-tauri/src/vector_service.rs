//! Semantic half of local search in the app process (local-search-spec sec. 7 and 9, WP S3.3).
//!
//! * **One embedder.** When semantic search is on, the tier's model file is on the device and a backend is linked,
//!   the `semantic-index` thread loads it once (tier dims, threads and chunk cap) and keeps it; a tier, override or
//!   model change replaces it, semantic off or delete drops it.
//! * **Incremental, checkpointed indexing** of every corpus into `<app-data>/search/v1/vectors/<corpus>/`
//!   (`borealis_embed::VectorStore`; outside the Tantivy folders, so a keyword rebuild does not cost a re-embed).
//!   Each run plans against the store: only chunks whose `(doc_key, content_hash)` is new are embedded, rows of
//!   vanished chunks and packs are dropped, and a model / quant / dims / prompt-version change starts a new store
//!   (the store header is the vectors' version record). The scheduler's rules (charging, battery, low power,
//!   thermal, background) are checked before every batch; a pause waits and resumes from the checkpoint.
//! * **Honest status.** Search uses vectors only once every corpus store finished one full run for the current
//!   model and dims (`VectorStore::is_complete`), so "Keyword + semantic" is never shown for half an index. The
//!   query side reads published snapshots, never the store the indexer writes.
//! * **NaN guard.** A degenerate vector (indexing or query) trips the shared `SemanticGuard`: indexing stops,
//!   queries go keyword-only, and Settings shows the report. Turning semantic search off and on again (or a model /
//!   tier change) resets it.
//! * Searching has priority: a query interrupts indexing after the current batch, and indexing waits until no
//!   query came for [`QUIET`].
//!
//! Nothing here touches the network or logs indexed text (C4, BR2).

use borealis_embed::models::ModelFile;
use borealis_embed::scheduler::RunOptions;
use borealis_embed::store::StoreId;
use borealis_embed::{
    Decision, DeviceProfile, DeviceStateProvider, Embedder, GuardedEmbedder, IndexItem, IndexJob,
    PauseReason, RunOutcome, Scheduler, SchedulerSettings, SemanticGuard, TierConfig, VectorStore,
};
use borealis_search::{Corpus, PackInput};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// Vector folders under `search/v1/` (one per corpus inside).
pub const VECTORS_DIR: &str = "vectors";
/// Indexing waits this long after the last query before it takes the embedder again.
pub const QUIET: Duration = Duration::from_secs(3);
/// How often a paused run re-checks the device state.
const PAUSE_POLL: Duration = Duration::from_secs(10);
const WAIT_STEP: Duration = Duration::from_millis(200);
/// Chunks per embedding call: small, so a query waits at most one short batch.
const BATCH: usize = 2;
const CHECKPOINT: Duration = Duration::from_secs(30);
/// Query vectors kept for repeated queries (typing, scope changes).
const QUERY_CACHE: usize = 16;
/// Typical short queries the speed check times (after one warm-up), for tiers with a query budget.
const PROBE_QUERIES: [&str; 5] = [
    "spell that lets you fly",
    "how does grappling work",
    "make an enemy fall asleep",
    "resistance to fire damage",
    "what happens at zero hit points",
];

/// Where semantic indexing stands (Settings > Search and the search footer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexingState {
    /// Semantic search is off, unavailable on this tier, or the model is not on the device.
    Off,
    /// Loading the model.
    Loading,
    /// Embedding chunks.
    Indexing,
    /// Waiting for the device (`message` says why).
    Paused,
    /// Every corpus is indexed: search uses vectors.
    Ready,
    /// The NaN guard turned semantic search off (`message` has the report).
    Disabled,
    /// The model could not be loaded or the vectors could not be written (`message`).
    Failed,
}

/// `borealis://semantic-index` payload, also in `SemanticStatus.indexing`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticIndexing {
    pub state: IndexingState,
    /// Chunks embedded in this run.
    pub done: u64,
    /// Chunks this run had to embed.
    pub total: u64,
    /// Plain sentence: pause reason, guard report or failure (no paths, no indexed text).
    pub message: Option<String>,
    /// Search uses vectors right now.
    pub queryable: bool,
}

impl SemanticIndexing {
    /// Nothing loaded, nothing indexed.
    #[must_use]
    pub fn off() -> Self {
        Self {
            state: IndexingState::Off,
            done: 0,
            total: 0,
            message: None,
            queryable: false,
        }
    }
}

/// Receives indexing status changes (the Tauri event in the app, a recorder in tests).
pub trait IndexingSink: Send + Sync {
    fn indexing(&self, status: &SemanticIndexing);
}

/// Chunks of every corpus, computed on the indexing thread.
pub type CorpusInputs = Arc<dyn Fn() -> Vec<(Corpus, Vec<PackInput>)> + Send + Sync>;

/// Loads the backend for a tier (`borealis_embed::llama` in the app, a fake in tests).
pub type EmbedLoader =
    Arc<dyn Fn(&Path, &ModelFile, &TierConfig) -> Result<Arc<dyn Embedder>, String> + Send + Sync>;

/// What the indexer should bring about: from settings, the tier and the content.
#[derive(Clone)]
pub struct IndexRequest {
    /// `settings.search.semantic`.
    pub semantic: bool,
    /// `settings.search.indexOnBattery`.
    pub index_on_battery: bool,
    /// The tier in effect.
    pub tier: TierConfig,
    pub inputs: CorpusInputs,
}

/// The query side of one search: the query vector and the published store of each corpus.
pub struct HybridQuery {
    pub vector: Vec<f32>,
    stores: HashMap<Corpus, Arc<VectorStore>>,
}

impl HybridQuery {
    /// The `(doc_key, score)` top `k` of `corpus`.
    #[must_use]
    pub fn search(&self, corpus: Corpus, k: usize) -> Vec<(&str, f32)> {
        self.stores
            .get(&corpus)
            .map(|s| s.search(&self.vector, k))
            .unwrap_or_default()
    }
}

/// What an embedder is loaded for: model SHA-256, dims, threads, chunk cap.
type LoadKey = (String, usize, usize, usize);

/// The loaded embedder and what it was loaded for.
struct Loaded {
    key: LoadKey,
    embedder: GuardedEmbedder,
}

struct Inner {
    root: PathBuf,
    models: PathBuf,
    corpora: Vec<Corpus>,
    backend: bool,
    loader: EmbedLoader,
    guard: SemanticGuard,
    device: RwLock<Arc<dyn DeviceStateProvider>>,
    profile: RwLock<Option<DeviceProfile>>,
    loaded: RwLock<Option<Loaded>>,
    published: RwLock<HashMap<Corpus, Arc<VectorStore>>>,
    queries: Mutex<VecDeque<(String, Vec<f32>)>>,
    status: Mutex<SemanticIndexing>,
    sink: RwLock<Option<Arc<dyn IndexingSink>>>,
    last: Mutex<Option<IndexRequest>>,
    pending: Mutex<Option<IndexRequest>>,
    /// The semantic setting of the last request (queries check it without waiting for the thread).
    semantic: AtomicBool,
    running: AtomicBool,
    /// Set by a new request or a query: the running batch loop stops after its batch.
    interrupt: AtomicBool,
    /// Set by [`VectorService::shutdown`]: waits end and the thread returns.
    stop: AtomicBool,
    last_query: Mutex<Option<Instant>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// A load key whose query embedding missed the tier's budget, with the measured median (checked once per run).
    too_slow: Mutex<Option<(LoadKey, u32)>>,
}

/// The app's semantic index and query side (cheap to clone).
#[derive(Clone)]
pub struct VectorService(Arc<Inner>);

impl std::fmt::Debug for VectorService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorService")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The real backend: llama.cpp with the tier's dims, threads and chunk cap.
#[cfg(feature = "llama")]
fn llama_loader() -> EmbedLoader {
    Arc::new(|path, file, tier| {
        let dims = tier.dims.ok_or("this device tier has no semantic search")?;
        borealis_embed::llama::LlamaEmbedder::load_with(
            path,
            file.info(),
            dims,
            tier.threads,
            borealis_embed::llama::LlamaOptions::for_max_tokens(tier.max_tokens),
        )
        .map(|e| Arc::new(e) as Arc<dyn Embedder>)
        .map_err(|e| e.to_string())
    })
}

#[cfg(not(feature = "llama"))]
fn llama_loader() -> EmbedLoader {
    Arc::new(|_, _, _| Err("no embedding backend in this build".to_string()))
}

/// Plain sentence for a pause.
#[must_use]
pub fn pause_message(reason: PauseReason) -> &'static str {
    match reason {
        PauseReason::Unsupported => "Semantic search is not available on this device.",
        PauseReason::SemanticOff => "Semantic search is off.",
        PauseReason::GuardTripped => "Semantic search was turned off after an unusable vector.",
        PauseReason::Backgrounded => "Indexing continues when Borealis is back in the foreground.",
        PauseReason::LowPowerMode => "Indexing waits while Low Power Mode or Battery Saver is on.",
        PauseReason::Thermal => "Indexing waits while the device is hot.",
        PauseReason::NotCharging => "Indexing continues while the device is charging.",
        PauseReason::BatteryLow => {
            "Indexing continues while charging, or on battery at 50% or more."
        }
    }
}

impl VectorService {
    /// The service for the app data folder `data`, with this build's backend.
    #[must_use]
    pub fn new(data: &Path) -> Self {
        Self::with_loader(data, borealis_embed::backend_available(), llama_loader())
    }

    /// Same, with another backend (tests load a `FakeEmbedder`).
    #[must_use]
    pub fn with_loader(data: &Path, backend: bool, loader: EmbedLoader) -> Self {
        let lifecycle = borealis_embed::Lifecycle::new();
        Self(Arc::new(Inner {
            root: data
                .join(crate::search_service::SEARCH_DIR)
                .join(crate::search_service::LAYOUT_DIR)
                .join(VECTORS_DIR),
            models: data.join(crate::semantic_service::MODELS_DIR),
            corpora: Corpus::ALL.to_vec(),
            backend,
            loader,
            guard: SemanticGuard::new(),
            device: RwLock::new(Arc::new(borealis_embed::device::DesktopDevice {
                lifecycle,
            })),
            profile: RwLock::new(None),
            loaded: RwLock::new(None),
            published: RwLock::new(HashMap::new()),
            queries: Mutex::new(VecDeque::new()),
            status: Mutex::new(SemanticIndexing::off()),
            sink: RwLock::new(None),
            last: Mutex::new(None),
            pending: Mutex::new(None),
            semantic: AtomicBool::new(false),
            running: AtomicBool::new(false),
            interrupt: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            last_query: Mutex::new(None),
            worker: Mutex::new(None),
            too_slow: Mutex::new(None),
        }))
    }

    /// The device profile (tier probe) and the battery / thermal / background provider the scheduler reads.
    /// Startup installs the platform ones (`semantic_device`); until then a desktop probe and provider apply.
    pub fn set_device(&self, profile: DeviceProfile, provider: Arc<dyn DeviceStateProvider>) {
        if let Ok(mut p) = self.0.profile.write() {
            *p = Some(profile);
        }
        if let Ok(mut d) = self.0.device.write() {
            *d = provider;
        }
    }

    /// The device profile the tier comes from.
    #[must_use]
    pub fn profile(&self) -> DeviceProfile {
        if let Some(p) = self.0.profile.read().ok().and_then(|p| *p) {
            return p;
        }
        let probed = DeviceProfile::probe();
        if let Ok(mut p) = self.0.profile.write() {
            *p = Some(probed);
        }
        probed
    }

    pub fn set_sink(&self, sink: Arc<dyn IndexingSink>) {
        if let Ok(mut s) = self.0.sink.write() {
            *s = Some(sink);
        }
    }

    /// Folder of one corpus's vectors.
    #[must_use]
    pub fn corpus_dir(&self, corpus: Corpus) -> PathBuf {
        self.0.root.join(corpus.name())
    }

    /// The vectors root (`search/v1/vectors`).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.0.root
    }

    /// The current indexing status.
    #[must_use]
    pub fn status(&self) -> SemanticIndexing {
        lock(&self.0.status).clone()
    }

    /// The shared NaN guard.
    #[must_use]
    pub fn guard(&self) -> &SemanticGuard {
        &self.0.guard
    }

    fn set_status(&self, next: SemanticIndexing) {
        *lock(&self.0.status) = next.clone();
        let sink = self.0.sink.read().ok().and_then(|s| s.clone());
        if let Some(sink) = sink {
            sink.indexing(&next);
        }
    }

    fn status_with(&self, state: IndexingState, done: u64, total: u64, message: Option<String>) {
        self.set_status(SemanticIndexing {
            state,
            done,
            total,
            message,
            queryable: self.queryable(),
        });
    }

    /// Search can use vectors: semantic on, guard intact, an embedder loaded and every corpus published.
    #[must_use]
    pub fn queryable(&self) -> bool {
        self.0.semantic.load(Ordering::SeqCst)
            && self.0.guard.is_enabled()
            && self.0.loaded.read().is_ok_and(|l| l.is_some())
            && self
                .0
                .published
                .read()
                .is_ok_and(|p| self.0.corpora.iter().all(|c| p.contains_key(c)))
    }

    /// Bring the index in line with `request` on the background thread. Requests made while one runs replace
    /// each other; the running one stops after its current batch.
    pub fn refresh(&self, request: IndexRequest) {
        self.0.semantic.store(request.semantic, Ordering::SeqCst);
        if !request.semantic {
            // Queries stop using vectors at once, also before the thread unloads.
            self.0.guard.reset();
        }
        let idle = !self.0.running.load(Ordering::SeqCst)
            && self.0.loaded.read().is_ok_and(|l| l.is_none())
            && self.status().state == IndexingState::Off;
        *lock(&self.0.last) = Some(request.clone());
        if !request.semantic && idle {
            return; // nothing loaded, nothing to stop: no thread for a switched-off feature
        }
        *lock(&self.0.pending) = Some(request);
        self.0.interrupt.store(true, Ordering::SeqCst);
        if self.0.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        match std::thread::Builder::new()
            .name("semantic-index".into())
            .spawn(move || me.drain())
        {
            Ok(handle) => *lock(&self.0.worker) = Some(handle),
            Err(_) => {
                self.0.running.store(false, Ordering::SeqCst);
                tracing::warn!("semantic: could not start the indexing thread");
            }
        }
    }

    /// Run the last request again (the model download finished, the guard was reset).
    pub fn retry(&self) {
        let last = lock(&self.0.last).clone();
        if let Some(request) = last {
            self.refresh(request);
        }
    }

    /// Stop indexing, drop the embedder and every snapshot, and wait for the thread (before the vector files are
    /// deleted: nothing may write them back).
    pub fn shutdown(&self) {
        self.0.stop.store(true, Ordering::SeqCst);
        lock(&self.0.pending).take();
        self.0.interrupt.store(true, Ordering::SeqCst);
        let handle = lock(&self.0.worker).take();
        if let Some(h) = handle {
            let _ = h.join();
        }
        self.unload();
        self.0.stop.store(false, Ordering::SeqCst);
        self.set_status(SemanticIndexing::off());
    }

    /// A new request is waiting or the service is shutting down: the current run should end.
    fn superseded(&self) -> bool {
        self.0.stop.load(Ordering::SeqCst) || lock(&self.0.pending).is_some()
    }

    fn device(&self) -> Arc<dyn DeviceStateProvider> {
        match self.0.device.read() {
            Ok(d) => Arc::clone(&d),
            Err(p) => Arc::clone(&p.into_inner()),
        }
    }

    /// Wait until no run is going or queued (tests).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let until = Instant::now() + timeout;
        while Instant::now() < until {
            let queued = lock(&self.0.pending).is_some();
            if !self.0.running.load(Ordering::SeqCst) && !queued {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn take_pending(&self) -> Option<IndexRequest> {
        lock(&self.0.pending).take()
    }

    fn drain(&self) {
        loop {
            while let Some(request) = self.take_pending() {
                if self.0.stop.load(Ordering::SeqCst) {
                    break;
                }
                self.0.interrupt.store(false, Ordering::SeqCst);
                self.process(&request);
            }
            self.0.running.store(false, Ordering::SeqCst);
            let more = lock(&self.0.pending).is_some();
            if !more || self.0.running.swap(true, Ordering::SeqCst) {
                return;
            }
        }
    }

    fn unload(&self) {
        if let Ok(mut p) = self.0.published.write() {
            p.clear();
        }
        if let Ok(mut l) = self.0.loaded.write() {
            *l = None;
        }
        lock(&self.0.queries).clear();
    }

    /// The model file of `tier` when it is on the device (size check; the loader verifies the SHA-256).
    fn model_path(&self, tier: &TierConfig) -> Option<(PathBuf, &'static ModelFile)> {
        let file = tier.model?;
        let path = self.0.models.join(file.file_name);
        let size = std::fs::metadata(&path).ok()?.len();
        (size == file.bytes).then_some((path, file))
    }

    fn process(&self, request: &IndexRequest) {
        let ready = request.semantic && self.0.backend && request.tier.dims.is_some();
        let Some((path, file)) = ready.then(|| self.model_path(&request.tier)).flatten() else {
            self.unload();
            self.set_status(SemanticIndexing::off());
            return;
        };
        let Some(embedder) = self.ensure_loaded(&path, file, &request.tier) else {
            return;
        };
        if let Some(report) = self.0.guard.report() {
            self.status_with(IndexingState::Disabled, 0, 0, Some(report.message()));
            return;
        }
        if let Err(e) = self.index(request, &embedder) {
            tracing::warn!("semantic: indexing stopped: {e}");
            self.status_with(
                IndexingState::Failed,
                0,
                0,
                Some("The semantic index could not be written.".into()),
            );
        }
    }

    fn ensure_loaded(
        &self,
        path: &Path,
        file: &'static ModelFile,
        tier: &TierConfig,
    ) -> Option<GuardedEmbedder> {
        let dims = tier.dims?;
        let key = (file.sha256.to_string(), dims, tier.threads, tier.max_tokens);
        if let Ok(l) = self.0.loaded.read() {
            if let Some(l) = l.as_ref().filter(|l| l.key == key) {
                return Some(l.embedder.clone());
            }
        }
        // Another model, tier or override: the old vectors belong to another store.
        self.unload();
        self.0.guard.reset();
        let slow = lock(&self.0.too_slow).as_ref().and_then(|(k, ms)| {
            (*k == key && tier.query_budget_ms.is_some_and(|b| *ms > b)).then_some(*ms)
        });
        if let Some(ms) = slow {
            self.too_slow_status(ms, tier);
            return None;
        }
        self.status_with(IndexingState::Loading, 0, 0, None);
        let started = Instant::now();
        match (self.0.loader)(path, file, tier) {
            Ok(inner) => {
                tracing::info!(
                    quant = file.quant,
                    dims,
                    threads = tier.threads,
                    ms = started.elapsed().as_millis(),
                    "semantic: model loaded"
                );
                let embedder = GuardedEmbedder::new(inner, self.0.guard.clone());
                if let Some(ms) = tier
                    .query_budget_ms
                    .and_then(|budget| probe_query_ms(&embedder).filter(|ms| *ms > budget))
                {
                    tracing::warn!(ms, quant = file.quant, "semantic: query embedding too slow");
                    *lock(&self.0.too_slow) = Some((key, ms));
                    self.too_slow_status(ms, tier);
                    return None;
                }
                if let Ok(mut l) = self.0.loaded.write() {
                    *l = Some(Loaded {
                        key,
                        embedder: embedder.clone(),
                    });
                }
                Some(embedder)
            }
            Err(message) => {
                tracing::warn!("semantic: model not loaded: {message}");
                self.status_with(
                    IndexingState::Failed,
                    0,
                    0,
                    Some(format!(
                        "The semantic search model could not be loaded: {message}"
                    )),
                );
                None
            }
        }
    }

    fn too_slow_status(&self, ms: u32, tier: &TierConfig) {
        let budget = tier.query_budget_ms.unwrap_or_default();
        self.status_with(
            IndexingState::Failed,
            0,
            0,
            Some(format!(
                "Semantic search is too slow on this computer: one search takes {ms} ms to embed (the budget is \
                 {budget} ms), so search uses keywords. You can delete the model in Settings."
            )),
        );
    }

    fn publish(&self, corpus: Corpus, store: &VectorStore) {
        if !store.is_complete() {
            return;
        }
        if let Ok(mut p) = self.0.published.write() {
            p.insert(corpus, Arc::new(store.clone()));
        }
    }

    /// Plan and run every corpus. Returns early (Ok) when a new request arrived.
    fn index(
        &self,
        request: &IndexRequest,
        embedder: &GuardedEmbedder,
    ) -> borealis_embed::Result<()> {
        let id = StoreId::for_embedder(embedder.embedder())?;
        let inputs = (request.inputs)();
        let mut runs = Vec::new();
        for corpus in &self.0.corpora {
            let (mut store, _) = VectorStore::open(&self.corpus_dir(*corpus), id.clone())?;
            let items = inputs
                .iter()
                .filter(|(c, _)| c == corpus)
                .flat_map(|(_, packs)| packs.iter().flat_map(index_items))
                .collect();
            let job = IndexJob::plan(&mut store, items);
            self.publish(*corpus, &store);
            runs.push((*corpus, store, job));
        }
        let total: usize = runs.iter().map(|(_, _, j)| j.remaining()).sum();
        let total = total as u64;
        let mut done = 0u64;
        let scheduler = Scheduler::new(
            request.tier.policy,
            SchedulerSettings {
                semantic: request.semantic,
                index_on_battery: request.index_on_battery,
            },
            self.device(),
            self.0.guard.clone(),
        );
        let options = RunOptions {
            batch: BATCH,
            checkpoint_every: CHECKPOINT,
        };
        for (corpus, mut store, mut job) in runs {
            loop {
                // Checked here too, so a paused device shows "paused" without a flash of "indexing".
                if let Decision::Pause(reason) = scheduler.decide() {
                    if reason == PauseReason::GuardTripped {
                        break;
                    }
                    self.status_with(
                        IndexingState::Paused,
                        done,
                        total,
                        Some(pause_message(reason).into()),
                    );
                    if self.wait(PAUSE_POLL) {
                        return Ok(());
                    }
                    continue;
                }
                self.status_with(IndexingState::Indexing, done, total, None);
                let before = store.len();
                let mut checkpoint = |s: &VectorStore| {
                    self.publish(corpus, s);
                    let now = done + s.len().saturating_sub(before) as u64;
                    if s.len() > before && !s.is_complete() {
                        self.status_with(IndexingState::Indexing, now, total, None);
                    }
                };
                let outcome = job.run_observed(
                    &mut store,
                    embedder,
                    &scheduler,
                    options,
                    &self.0.interrupt,
                    &mut checkpoint,
                )?;
                done += store.len().saturating_sub(before) as u64;
                match outcome {
                    RunOutcome::Completed { .. } => break,
                    RunOutcome::Disabled { report } => {
                        lock(&self.0.queries).clear();
                        self.status_with(
                            IndexingState::Disabled,
                            done,
                            total,
                            Some(report.message()),
                        );
                        return Ok(());
                    }
                    RunOutcome::Paused { reason, .. } => {
                        self.status_with(
                            IndexingState::Paused,
                            done,
                            total,
                            Some(pause_message(reason).into()),
                        );
                        if self.wait(PAUSE_POLL) {
                            return Ok(());
                        }
                    }
                    RunOutcome::Cancelled { .. } => {
                        if self.yield_to_queries() {
                            return Ok(());
                        }
                    }
                }
            }
        }
        if let Some(report) = self.0.guard.report() {
            self.status_with(IndexingState::Disabled, done, total, Some(report.message()));
            return Ok(());
        }
        self.status_with(IndexingState::Ready, done, total, None);
        Ok(())
    }

    /// Sleep up to `max`; true when a new request is waiting.
    fn wait(&self, max: Duration) -> bool {
        let until = Instant::now() + max;
        while Instant::now() < until {
            if self.superseded() {
                return true;
            }
            std::thread::sleep(WAIT_STEP);
        }
        self.0.interrupt.store(false, Ordering::SeqCst);
        false
    }

    /// After an interrupt: true when it was a new request; otherwise wait until queries have been quiet for
    /// [`QUIET`] and carry on.
    fn yield_to_queries(&self) -> bool {
        loop {
            if self.superseded() {
                return true;
            }
            let since = lock(&self.0.last_query).map(|t| t.elapsed());
            if since.is_none_or(|s| s >= QUIET) {
                self.0.interrupt.store(false, Ordering::SeqCst);
                return false;
            }
            std::thread::sleep(WAIT_STEP);
        }
    }

    /// The query side for `query`, when search can use vectors. Interrupts indexing so the query gets the
    /// embedder after at most one short batch. A degenerate query vector trips the guard (keyword only from then).
    #[must_use]
    pub fn query(&self, query: &str) -> Option<HybridQuery> {
        if !self.queryable() {
            return None;
        }
        *lock(&self.0.last_query) = Some(Instant::now());
        self.0.interrupt.store(true, Ordering::SeqCst);
        let stores = self.0.published.read().ok()?.clone();
        let cached = lock(&self.0.queries)
            .iter()
            .find(|(q, _)| q == query)
            .map(|(_, v)| v.clone());
        let vector = match cached {
            Some(v) => v,
            None => {
                let embedder = self.0.loaded.read().ok()?.as_ref()?.embedder.clone();
                match embedder.embed_query(query) {
                    Ok(v) => {
                        let mut cache = lock(&self.0.queries);
                        if cache.len() >= QUERY_CACHE {
                            cache.pop_front();
                        }
                        cache.push_back((query.to_string(), v.clone()));
                        v
                    }
                    Err(e) => {
                        tracing::warn!("semantic: query not embedded: {e}");
                        if let Some(report) = self.0.guard.report() {
                            self.status_with(IndexingState::Disabled, 0, 0, Some(report.message()));
                        }
                        return None;
                    }
                }
            }
        };
        Some(HybridQuery { vector, stores })
    }
}

/// Median query-embedding time in ms over [`PROBE_QUERIES`] after one warm-up; `None` when a probe fails (the
/// guard then reports the reason).
fn probe_query_ms(embedder: &GuardedEmbedder) -> Option<u32> {
    embedder.embed_query(PROBE_QUERIES[0]).ok()?;
    let mut ms = Vec::with_capacity(PROBE_QUERIES.len());
    for q in PROBE_QUERIES {
        let started = Instant::now();
        embedder.embed_query(q).ok()?;
        ms.push(started.elapsed().as_millis());
    }
    ms.sort_unstable();
    u32::try_from(ms[ms.len() / 2]).ok()
}

/// The chunks of one pack as indexing items: title = element or section name, text = chunk body (the eval's V4
/// prompt, `borealis-search/tests/harness/semantic.rs`).
fn index_items(pack: &PackInput) -> impl Iterator<Item = IndexItem> + '_ {
    pack.chunks.iter().map(move |c| IndexItem {
        pack: pack.pack.clone(),
        doc_key: c.doc_key.clone(),
        content_hash: c.content_hash(),
        title: Some(c.name.clone()),
        text: c.body.clone(),
    })
}
