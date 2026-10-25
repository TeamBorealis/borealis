//! S3.3 semantic index in the app with a deterministic fake backend (no model download, no C++ model run): the
//! indexer loads one embedder for the tier, embeds every corpus incrementally into `search/v1/vectors/<corpus>/`,
//! search fuses vectors only once everything is indexed (`active` / "Keyword + semantic" is honest), the
//! scheduler pauses on device state, the NaN guard turns semantic search off, and delete stops the indexer before
//! removing the vectors. Statuses and events match the UI contract.

mod contract_support;

use borealis_embed::models::{ModelFile, Q6_K_TE_Q4_K, Q8_0};
use borealis_embed::{
    DeviceProfile, DeviceState, Embedder, FakeEmbedder, FormFactor, Power, SimulatedDevice,
    TierConfig,
};
use borealis_embed::{ModelInfo, Tier};
use borealis_lib::search_api::{SearchQuery, SearchScope};
use borealis_lib::semantic_service::SemanticContext;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::vector_service::{
    EmbedLoader, IndexRequest, IndexingSink, IndexingState, SemanticIndexing, VectorService,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GIB: u64 = 1024 * 1024 * 1024;
const WAIT: Duration = Duration::from_secs(120);

#[derive(Default)]
struct Events(Mutex<Vec<SemanticIndexing>>);

impl IndexingSink for Events {
    fn indexing(&self, status: &SemanticIndexing) {
        self.0.lock().unwrap().push(status.clone());
    }
}

struct App {
    _dir: tempfile::TempDir,
    data: PathBuf,
    state: AppState,
    device: Arc<SimulatedDevice>,
    events: Arc<Events>,
    /// Every embedder the loader made (to count prompts and loads).
    loads: Arc<Mutex<Vec<Arc<FakeEmbedder>>>>,
}

fn desktop() -> DeviceProfile {
    DeviceProfile {
        form: FormFactor::Desktop,
        total_ram_bytes: Some(16 * GIB),
        cores: 8,
        pointer_bits: 64,
    }
}

/// A fake for the tier's dims that reports the model file's SHA-256; `poison` makes matching prompts NaN.
fn loader(loads: Arc<Mutex<Vec<Arc<FakeEmbedder>>>>, poison: Option<&'static str>) -> EmbedLoader {
    Arc::new(move |_path: &Path, file: &ModelFile, tier: &TierConfig| {
        let mut fake = FakeEmbedder::new(tier.dims.expect("tier dims")).with_sha256(file.sha256);
        if let Some(p) = poison {
            fake = fake.with_poison(p);
        }
        let fake = Arc::new(fake);
        loads.lock().unwrap().push(Arc::clone(&fake));
        Ok(fake as Arc<dyn Embedder>)
    })
}

fn app_with(poison: Option<&'static str>) -> App {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    let data = paths.data.clone();
    let mut state = AppState::new(paths).unwrap();
    let loads = Arc::new(Mutex::new(Vec::new()));
    state.vectors = VectorService::with_loader(&data, true, loader(Arc::clone(&loads), poison));
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    state
        .vectors
        .set_device(desktop(), Arc::clone(&device) as _);
    let events = Arc::new(Events::default());
    state.vectors.set_sink(Arc::clone(&events) as _);
    state.build_search_index_now().unwrap();
    App {
        _dir: dir,
        data,
        state,
        device,
        events,
        loads,
    }
}

impl App {
    /// The T2 / T3 model "on the device": a sparse file of the compiled size (the fake loader never reads it).
    fn install_model(&self) {
        self.install(&Q8_0);
    }

    fn install(&self, file: &ModelFile) {
        let dir = self.data.join("models");
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::File::create(dir.join(file.file_name)).unwrap();
        f.set_len(file.bytes).unwrap();
    }

    fn settings(&self, patch: serde_json::Value) {
        self.state
            .save_settings(&serde_json::json!({ "search": patch }))
            .unwrap();
        assert!(self.state.vectors.wait_idle(WAIT), "indexer finished");
    }

    fn ctx(&self) -> SemanticContext {
        self.state.semantic_context(&desktop(), false).unwrap()
    }

    fn prompts(&self) -> usize {
        self.loads
            .lock()
            .unwrap()
            .iter()
            .map(|f| f.prompts_embedded())
            .sum()
    }

    fn vector_file(&self, corpus: &str, dims: usize) -> PathBuf {
        self.vector_file_of(&Q8_0, corpus, dims)
    }

    fn vector_file_of(&self, file: &ModelFile, corpus: &str, dims: usize) -> PathBuf {
        self.data
            .join("search/v1/vectors")
            .join(corpus)
            .join(format!("vectors-{}-{dims}.bin", &file.sha256[..8]))
    }

    fn search(&self, query: &str) -> Vec<borealis_lib::search_api::SearchHit> {
        self.state
            .search(&SearchQuery {
                scope: SearchScope::All,
                query: query.into(),
                limit: 50,
                ..SearchQuery::default()
            })
            .unwrap()
            .items
    }
}

#[test]
fn every_corpus_is_indexed_before_search_claims_semantic_then_vectors_fuse() {
    let a = app_with(None);
    // Semantic on but no model on the device: nothing loads, nothing claims semantic.
    a.settings(serde_json::json!({"semantic": true, "tierOverride": "t2"}));
    assert_eq!(a.state.vectors.status().state, IndexingState::Off);
    assert!(a.loads.lock().unwrap().is_empty());
    assert!(!a.state.semantic_status(&a.ctx()).active);

    a.install_model();
    a.state.vectors.retry();
    assert!(a.state.vectors.wait_idle(WAIT));
    let st = a.state.semantic_status(&a.ctx());
    assert_eq!(st.indexing.state, IndexingState::Ready, "{:?}", st.indexing);
    assert!(
        st.active && st.indexing.queryable,
        "search uses vectors now"
    );
    assert!(st.indexing.total > 1000 && st.indexing.done == st.indexing.total);
    assert_eq!(a.loads.lock().unwrap().len(), 1, "one embedder");
    assert!(a.vector_file("rules", 256).is_file());
    assert!(a.vector_file("lore", 256).is_file());
    // Progress went out before Ready, and nothing claimed queryable while indexing the first time.
    let events = a.events.0.lock().unwrap().clone();
    assert!(events.iter().any(|e| e.state == IndexingState::Indexing));
    assert!(events
        .iter()
        .filter(|e| e.state == IndexingState::Indexing)
        .all(|e| !e.queryable));

    // An unknown word finds nothing by keyword; the vector side still answers (vector-only rows, no highlights).
    let hits = a.search("zzqxv");
    assert!(!hits.is_empty(), "vector-only hits");
    assert!(hits.iter().all(|h| h.highlights.is_empty()));
    // A name still comes first (exact names stay pinned).
    assert_eq!(a.search("Fireball")[0].title, "Fireball");
}

#[test]
fn indexing_is_incremental_and_a_tier_change_starts_a_new_store() {
    let a = app_with(None);
    a.install_model();
    a.settings(serde_json::json!({"semantic": true, "tierOverride": "t2"}));
    let first = a.prompts();
    assert!(first > 1000);
    // Same model, dims and content: a refresh embeds nothing.
    a.state.refresh_semantic_index();
    assert!(a.state.vectors.wait_idle(WAIT));
    assert_eq!(a.prompts(), first, "nothing re-embedded");
    assert_eq!(a.state.vectors.status().state, IndexingState::Ready);
    // T3 has the same file, dims and (on this 8-core profile) threads as T2: the same embedder and store.
    a.settings(serde_json::json!({"tierOverride": "t3"}));
    assert_eq!(a.loads.lock().unwrap().len(), 1);
    assert_eq!(a.prompts(), first, "nothing re-embedded");
    // T1 (its own file): a new embedder and store, everything embedded again; the Q8_0 store is replaced.
    a.install(&Q6_K_TE_Q4_K);
    a.settings(serde_json::json!({"tierOverride": "t1"}));
    assert_eq!(
        a.loads.lock().unwrap().len(),
        2,
        "the embedder was replaced"
    );
    assert!(a.prompts() >= 2 * first);
    assert!(a.vector_file_of(&Q6_K_TE_Q4_K, "lore", 256).is_file());
    assert!(!a.vector_file("lore", 256).exists());
    // A restart (new service over the same folder) resumes from the stores: nothing to embed.
    let loads = Arc::new(Mutex::new(Vec::new()));
    let again = VectorService::with_loader(&a.data, true, loader(Arc::clone(&loads), None));
    let mut state = a.state;
    state.vectors = again;
    state
        .vectors
        .set_device(desktop(), Arc::clone(&a.device) as _);
    state.refresh_semantic_index();
    assert!(state.vectors.wait_idle(WAIT));
    assert_eq!(state.vectors.status().state, IndexingState::Ready);
    // Only the speed probe's queries (one warm-up and five timed): no document was embedded again.
    assert_eq!(loads.lock().unwrap()[0].prompts_embedded(), 6);
}

#[test]
fn device_rules_pause_indexing_and_it_resumes_from_the_checkpoint() {
    let a = app_with(None);
    a.install_model();
    // T2 indexes on external power, or on battery >= 50% only when allowed.
    a.device.set(DeviceState {
        power: Power::Battery { percent: Some(80) },
        ..DeviceState::DESKTOP
    });
    a.state
        .save_settings(&serde_json::json!({"search": {"semantic": true, "tierOverride": "t2"}}))
        .unwrap();
    let until = std::time::Instant::now() + WAIT;
    while a.state.vectors.status().state != IndexingState::Paused {
        assert!(std::time::Instant::now() < until, "paused");
        std::thread::sleep(Duration::from_millis(20));
    }
    let st = a.state.vectors.status();
    assert_eq!(
        st.message.as_deref(),
        Some("Indexing continues while the device is charging.")
    );
    assert!(!a.state.semantic_status(&a.ctx()).active);
    // Plugged in: the next refresh resumes and finishes.
    a.device.set(DeviceState::DESKTOP);
    a.state.refresh_semantic_index();
    let until = std::time::Instant::now() + WAIT;
    while a.state.vectors.status().state != IndexingState::Ready {
        assert!(std::time::Instant::now() < until, "resumed");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(a.state.semantic_status(&a.ctx()).active);
}

#[test]
fn a_nan_vector_turns_semantic_search_off_and_search_stays_keyword() {
    // Every chunk mentioning "creature" comes back NaN.
    let a = app_with(Some("creature"));
    a.install_model();
    a.settings(serde_json::json!({"semantic": true, "tierOverride": "t2"}));
    let st = a.state.semantic_status(&a.ctx());
    assert_eq!(st.indexing.state, IndexingState::Disabled);
    assert!(st
        .indexing
        .message
        .as_deref()
        .is_some_and(|m| m.contains("Keyword search still works")));
    assert!(!st.active);
    assert!(a.state.vectors.query("fireball").is_none());
    assert_eq!(
        a.search("Fireball")[0].title,
        "Fireball",
        "keyword search works"
    );
}

#[test]
fn delete_stops_the_indexer_and_removes_the_vectors() {
    let a = app_with(None);
    a.install_model();
    a.settings(serde_json::json!({"semantic": true, "tierOverride": "t2"}));
    assert!(a.vector_file("rules", 256).is_file());
    let st = a.state.semantic_delete(&a.ctx()).unwrap();
    assert!(!st.active && !st.semantic);
    assert_eq!(st.indexing.state, IndexingState::Off);
    assert!(a.state.vectors.wait_idle(WAIT));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!a.vector_file("rules", 256).exists());
    assert!(!a.vector_file("lore", 256).exists());
    assert!(a.state.vectors.query("fireball").is_none());
}

#[test]
fn indexing_statuses_and_events_match_the_ui_contract() {
    let a = app_with(None);
    a.install_model();
    a.settings(serde_json::json!({"semantic": true, "tierOverride": "t2"}));
    let contract = contract_support::Contract::load();
    let mut problems = Vec::new();
    let st = serde_json::to_value(a.state.semantic_status(&a.ctx())).unwrap();
    problems.extend(contract.response("semantic_status", &st));
    for e in a.events.0.lock().unwrap().iter() {
        problems.extend(contract.event("semantic:index", &serde_json::to_value(e).unwrap()));
    }
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A fake that takes `delay` per call (a slow CPU build).
struct Slow(FakeEmbedder, Duration);

impl Embedder for Slow {
    fn model(&self) -> &ModelInfo {
        self.0.model()
    }
    fn dims(&self) -> usize {
        self.0.dims()
    }
    fn embed_prompts(&self, prompts: &[String]) -> borealis_embed::Result<Vec<Vec<f32>>> {
        std::thread::sleep(self.1);
        self.0.embed_prompts(prompts)
    }
}

#[test]
fn a_tier_with_a_query_budget_stays_keyword_only_when_queries_embed_too_slowly() {
    let a = app_with(None);
    a.install_model();
    let loads = Arc::new(Mutex::new(0usize));
    let counted = Arc::clone(&loads);
    let slow: EmbedLoader = Arc::new(move |_: &Path, file: &ModelFile, tier: &TierConfig| {
        *counted.lock().unwrap() += 1;
        let fake = FakeEmbedder::new(tier.dims.unwrap()).with_sha256(file.sha256);
        Ok(Arc::new(Slow(fake, Duration::from_millis(60))) as Arc<dyn Embedder>)
    });
    let service = VectorService::with_loader(&a.data, true, slow);
    service.set_device(desktop(), Arc::clone(&a.device) as _);
    let request = |budget| IndexRequest {
        semantic: true,
        index_on_battery: false,
        tier: TierConfig {
            query_budget_ms: Some(budget),
            ..Tier::T2.config(8)
        },
        inputs: Arc::new(Vec::new),
    };
    service.refresh(request(40));
    assert!(service.wait_idle(WAIT));
    let st = service.status();
    assert_eq!(st.state, IndexingState::Failed);
    assert!(
        st.message
            .as_deref()
            .is_some_and(|m| m.contains("too slow")),
        "{st:?}"
    );
    // HW1 (emulator, PH1): the same line shows on phones, so it names "this device", not a computer.
    assert!(
        st.message
            .as_deref()
            .is_some_and(|m| m.contains("too slow on this device") && !m.contains("computer")),
        "{st:?}"
    );
    assert!(!service.queryable() && service.query("fireball").is_none());
    // The verdict is kept: a refresh does not load the model again.
    service.refresh(request(40));
    assert!(service.wait_idle(WAIT));
    assert_eq!(*loads.lock().unwrap(), 1);
    // A budget the device meets: loads and indexes (nothing to index here).
    service.refresh(request(1000));
    assert!(service.wait_idle(WAIT));
    assert_eq!(service.status().state, IndexingState::Ready);
}
