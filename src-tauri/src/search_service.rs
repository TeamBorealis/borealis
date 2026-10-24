//! Local keyword search in the app process (local-search-spec.md sections 9 and 11, WP S1.5).
//!
//! The app is the single writer of `<app-data>/search/v1/` (Tantivy dirs + `manifest.json`). The first sync starts in
//! a background thread once content has loaded (startup) or on first use; every content snapshot swap
//! (`AppState::set_hub`: pack add/remove/update/enable/disable, settings rebuilds) schedules an incremental sync that
//! only rewrites changed packs. Until the first sync finishes, callers fall back to the in-memory compendium scan.
//! User and 5etools packs are indexed here like bundled ones and never leave the device (C4). Nothing in this module
//! logs paths or indexed text, so no `search/` data can reach a bug report's log section (BR2).

use crate::state::ContentHub;
use borealis_search::{corpus, Corpus, DocsPack, SearchEngine, WriterBudget};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

/// Folder of all search data under the app data folder (derived, cleared by "Clear caches").
pub const SEARCH_DIR: &str = "search";
/// Layout version folder inside [`SEARCH_DIR`].
pub const LAYOUT_DIR: &str = "v1";
/// Bundled rules-text packs (`borealis-docs/1`, loaded only by search; never in the content store).
pub const RULES_TEXT_PACKS: [&str; 2] = ["srd-5.1-rules-text", "srd-5.2.1-rules-text"];
const DOCS_FILE: &str = "docs.json";
/// Steps of one sync, reported as `done / total`: read rules text, rules corpus, lore corpus.
const SYNC_STEPS: u32 = 3;
/// Longest a query waits for a running incremental sync before falling back.
const READ_WAIT: Duration = Duration::from_millis(1500);
const READ_POLL: Duration = Duration::from_millis(5);

#[cfg(mobile)]
const BUDGET: WriterBudget = WriterBudget::MOBILE;
#[cfg(not(mobile))]
const BUDGET: WriterBudget = WriterBudget::DESKTOP;

/// Index state shown in the search footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexState {
    /// Not built in this run yet (search falls back to the compendium scan).
    Idle,
    /// A sync is running.
    Building,
    /// The keyword indexes answer queries.
    Ready,
    /// The last sync failed; the fallback stays in use.
    Failed,
}

/// `search://index-status` payload (and `search_index_status` result).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    /// Current state.
    pub state: IndexState,
    /// Finished steps of the running sync.
    pub done: u32,
    /// Steps of a sync.
    pub total: u32,
    /// Plain-language problem when `state` is `failed` (no paths, no indexed text).
    pub message: Option<String>,
}

impl IndexStatus {
    fn idle() -> Self {
        Self {
            state: IndexState::Idle,
            done: 0,
            total: SYNC_STEPS,
            message: None,
        }
    }
}

/// Receives index status changes (the Tauri event in the app, a recorder in tests).
pub trait StatusSink: Send + Sync {
    /// The status changed.
    fn index_status(&self, status: &IndexStatus);
}

struct Inner {
    root: PathBuf,
    content_root: PathBuf,
    engine: RwLock<Option<SearchEngine>>,
    status: Mutex<IndexStatus>,
    pending: Mutex<Option<Arc<ContentHub>>>,
    running: AtomicBool,
    started: AtomicBool,
    /// A sync completed since the engine was opened: the indexes can answer (also during a later re-sync).
    usable: AtomicBool,
    sink: RwLock<Option<Arc<dyn StatusSink>>>,
    docs: OnceLock<Vec<DocsPack>>,
}

/// The app's search service (cheap to clone; all clones share one writer).
#[derive(Clone)]
pub struct SearchService(Arc<Inner>);

impl std::fmt::Debug for SearchService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchService")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

/// Plain error text for the footer (never a path).
fn plain(e: &borealis_search::SearchError) -> String {
    use borealis_search::SearchError as E;
    match e {
        E::Io(_) => "The search index could not be written.".into(),
        _ => "The search index could not be built.".into(),
    }
}

impl SearchService {
    /// A service for the app data folder `data`; bundled rules text is read from `content_root`.
    #[must_use]
    pub fn new(data: &Path, content_root: &Path) -> Self {
        Self(Arc::new(Inner {
            root: data.join(SEARCH_DIR).join(LAYOUT_DIR),
            content_root: content_root.to_path_buf(),
            engine: RwLock::new(None),
            status: Mutex::new(IndexStatus::idle()),
            pending: Mutex::new(None),
            running: AtomicBool::new(false),
            started: AtomicBool::new(false),
            usable: AtomicBool::new(false),
            sink: RwLock::new(None),
            docs: OnceLock::new(),
        }))
    }

    /// Where status changes go.
    pub fn set_sink(&self, sink: Arc<dyn StatusSink>) {
        if let Ok(mut s) = self.0.sink.write() {
            *s = Some(sink);
        }
    }

    /// The current status.
    #[must_use]
    pub fn status(&self) -> IndexStatus {
        self.0
            .status
            .lock()
            .map_or_else(|_| IndexStatus::idle(), |s| s.clone())
    }

    fn set_status(&self, next: IndexStatus) {
        if let Ok(mut s) = self.0.status.lock() {
            *s = next.clone();
        }
        let sink = self.0.sink.read().ok().and_then(|s| s.clone());
        if let Some(sink) = sink {
            sink.index_status(&next);
        }
    }

    /// True once a sync was requested in this run (startup or first use).
    #[must_use]
    pub fn is_started(&self) -> bool {
        self.0.started.load(Ordering::SeqCst)
    }

    /// Start (or queue) a background sync to `hub`. Requests made while a sync runs collapse into one.
    pub fn request_sync(&self, hub: Arc<ContentHub>) {
        self.0.started.store(true, Ordering::SeqCst);
        if let Ok(mut p) = self.0.pending.lock() {
            *p = Some(hub);
        }
        if self.0.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        let spawned = std::thread::Builder::new()
            .name("search-index".into())
            .spawn(move || me.drain());
        if spawned.is_err() {
            self.0.running.store(false, Ordering::SeqCst);
            tracing::warn!("search: could not start the indexing thread");
        }
    }

    /// The content snapshot changed: re-sync when the index is in use (incremental, changed packs only).
    pub fn content_changed(&self, hub: Arc<ContentHub>) {
        if self.is_started() {
            self.request_sync(hub);
        }
    }

    fn take_pending(&self) -> Option<Arc<ContentHub>> {
        self.0.pending.lock().ok().and_then(|mut p| p.take())
    }

    fn drain(&self) {
        loop {
            while let Some(hub) = self.take_pending() {
                self.sync_now(&hub);
            }
            self.0.running.store(false, Ordering::SeqCst);
            let more = self.0.pending.lock().is_ok_and(|p| p.is_some());
            if !more || self.0.running.swap(true, Ordering::SeqCst) {
                return;
            }
        }
    }

    fn docs(&self) -> &[DocsPack] {
        self.0.docs.get_or_init(|| {
            RULES_TEXT_PACKS
                .iter()
                .filter_map(|id| {
                    let path = self.0.content_root.join(id).join(DOCS_FILE);
                    let text = std::fs::read_to_string(path).ok()?;
                    DocsPack::parse(&text)
                        .map_err(|e| tracing::warn!("search: rules text {id} unreadable: {e}"))
                        .ok()
                })
                .collect()
        })
    }

    /// The bundled rules-text pack with this id.
    #[must_use]
    pub fn docs_pack(&self, id: &str) -> Option<&DocsPack> {
        self.docs().iter().find(|d| d.id() == id)
    }

    /// Bring both corpora in line with `hub`, blocking (the background thread and tests call this).
    pub fn sync_now(&self, hub: &ContentHub) -> bool {
        self.0.started.store(true, Ordering::SeqCst);
        let began = Instant::now();
        let step = |done| IndexStatus {
            state: IndexState::Building,
            done,
            total: SYNC_STEPS,
            message: None,
        };
        self.set_status(step(0));
        let rules = self.rules_inputs(hub);
        self.set_status(step(1));
        let lore = corpus::lore_packs(&hub.store);
        let outcome = self.write(Corpus::Rules, &rules).and_then(|r| {
            self.set_status(step(2));
            self.write(Corpus::Lore, &lore).map(|l| (r, l))
        });
        match outcome {
            Ok((r, l)) => {
                self.0.usable.store(true, Ordering::SeqCst);
                tracing::info!(
                    rules_chunks = r,
                    lore_chunks = l,
                    ms = began.elapsed().as_millis(),
                    "search: keyword indexes in sync"
                );
                self.set_status(IndexStatus {
                    state: IndexState::Ready,
                    done: SYNC_STEPS,
                    total: SYNC_STEPS,
                    message: None,
                });
                true
            }
            Err(message) => {
                self.set_status(IndexStatus {
                    state: IndexState::Failed,
                    done: 0,
                    total: SYNC_STEPS,
                    message: Some(message),
                });
                false
            }
        }
    }

    fn rules_inputs(&self, hub: &ContentHub) -> Vec<borealis_search::PackInput> {
        let texts: Vec<String> = RULES_TEXT_PACKS
            .iter()
            .filter_map(|id| {
                std::fs::read_to_string(self.0.content_root.join(id).join(DOCS_FILE)).ok()
            })
            .collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        corpus::rules_packs(&refs, &hub.store).unwrap_or_else(|e| {
            tracing::warn!("search: rules text skipped: {e}");
            corpus::rules_packs(&[], &hub.store).unwrap_or_default()
        })
    }

    /// Sync one corpus under the writer lock; opens (and on damage rebuilds) the engine first.
    fn write(&self, corpus: Corpus, packs: &[borealis_search::PackInput]) -> Result<usize, String> {
        let mut guard = self
            .0
            .engine
            .write()
            .map_err(|_| "The search index is busy.".to_string())?;
        if guard.is_none() {
            let (engine, report) =
                SearchEngine::open(&self.0.root, BUDGET).map_err(|e| plain(&e))?;
            for (c, why) in &report.rebuilt {
                tracing::info!(corpus = c.name(), reason = %why, "search: rebuilding");
            }
            *guard = Some(engine);
        }
        let engine = guard.as_mut().ok_or("The search index is not open.")?;
        engine
            .sync(corpus, packs)
            .map(|r| r.chunks_written)
            .map_err(|e| {
                tracing::warn!(corpus = corpus.name(), "search: sync failed: {e}");
                plain(&e)
            })
    }

    /// The engine for reading once a sync has completed; `None` means "use the fallback". While a later
    /// incremental sync holds the writer this waits up to [`READ_WAIT`] (a sync of changed packs takes well under
    /// a second) rather than flickering to the fallback.
    #[must_use]
    pub fn reader(&self) -> Option<RwLockReadGuard<'_, Option<SearchEngine>>> {
        let until = Instant::now() + READ_WAIT;
        loop {
            if !self.0.usable.load(Ordering::SeqCst) {
                return None;
            }
            match self.0.engine.try_read() {
                Ok(guard) => return guard.is_some().then_some(guard),
                Err(std::sync::TryLockError::Poisoned(_)) => return None,
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() >= until => return None,
                Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(READ_POLL),
            }
        }
    }

    /// Wait until no sync is running or queued (tests).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let until = Instant::now() + timeout;
        while Instant::now() < until {
            let queued = self.0.pending.lock().is_ok_and(|p| p.is_some());
            if !self.0.running.load(Ordering::SeqCst) && !queued {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// "Clear caches": drop the indexes and delete every keyword index file. The next search rebuilds them.
    ///
    /// # Errors
    /// The folder could not be removed.
    pub fn clear(&self) -> std::io::Result<bool> {
        let mut guard = self
            .0
            .engine
            .write()
            .map_err(|_| std::io::Error::other("search index lock poisoned"))?;
        *guard = None;
        self.0.started.store(false, Ordering::SeqCst);
        self.0.usable.store(false, Ordering::SeqCst);
        let dir = self
            .0
            .root
            .parent()
            .map_or(self.0.root.clone(), Path::to_path_buf);
        let existed = dir.exists();
        if existed {
            std::fs::remove_dir_all(&dir)?;
        }
        drop(guard);
        self.set_status(IndexStatus::idle());
        Ok(existed)
    }
}
