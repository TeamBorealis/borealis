//! S3.2 semantic model download in the app, network-mocked: an in-memory transport stands in for HTTPS (the
//! crate's own tests cover real HTTP against a loopback server). Proves: no request before consent; resume after
//! pause; cancel deletes the partial; delete removes the model and vectors but not keyword indexes; Clear caches
//! keeps the model; T1 reports "not available yet"; statuses match the UI contract.

mod contract_support;

use borealis_embed::download::{DownloadSpec, Response, Transport, TransportError};
use borealis_embed::{DeviceProfile, FormFactor, Tier};
use borealis_lib::semantic_service::{
    Catalog, DownloadState, SemanticContext, SemanticDownload, SemanticService, SemanticSink,
};
use borealis_lib::state::{AppPaths, AppState};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const GIB: u64 = 1024 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

fn data() -> Vec<u8> {
    (0..(1_048_576u32 + 777))
        .map(|i| (i.wrapping_mul(40_503) >> 7) as u8)
        .collect()
}

fn spec(bytes: &[u8], name: &str) -> DownloadSpec {
    DownloadSpec {
        file_name: name.into(),
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        bytes: bytes.len() as u64,
        sources: vec!["https://mirror.test/model.gguf".into()],
    }
}

/// Serves `data` with Range support; optionally slow (each read waits `delay`). Counts every GET.
struct FakeTransport {
    data: Arc<Vec<u8>>,
    delay: Duration,
    calls: Mutex<Vec<(String, u64)>>,
}

struct SlowBody {
    data: Arc<Vec<u8>>,
    at: usize,
    delay: Duration,
}

impl Read for SlowBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(self.delay);
        let n = buf.len().min(CHUNK).min(self.data.len() - self.at);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

impl Transport for FakeTransport {
    fn get(&self, url: &str, from: u64) -> Result<Response, TransportError> {
        self.calls.lock().unwrap().push((url.to_owned(), from));
        let total = self.data.len() as u64;
        let start = usize::try_from(from).unwrap();
        Ok(Response {
            status: if from > 0 { 206 } else { 200 },
            content_range: (from > 0).then(|| format!("bytes {from}-{}/{total}", total - 1)),
            content_length: Some(total - from),
            body: Box::new(SlowBody {
                data: Arc::clone(&self.data),
                at: start,
                delay: self.delay,
            }),
        })
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<SemanticDownload>>);

impl SemanticSink for Events {
    fn download(&self, status: &SemanticDownload) {
        self.0.lock().unwrap().push(status.clone());
    }
}

struct App {
    _dir: tempfile::TempDir,
    data_dir: PathBuf,
    state: AppState,
    transport: Arc<FakeTransport>,
    /// Times the HTTPS client was built (it is built only on the first consented start).
    clients: Arc<AtomicUsize>,
    events: Arc<Events>,
    spec: DownloadSpec,
}

/// A backend counts as linked (the post-B-46 flow); [`app_full`] chooses.
fn app_with(delay: Duration, catalog: impl FnOnce(&DownloadSpec) -> Catalog) -> App {
    app_full(delay, catalog, true)
}

fn app_full(delay: Duration, catalog: impl FnOnce(&DownloadSpec) -> Catalog, backend: bool) -> App {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    let data_dir = paths.data.clone();
    let mut state = AppState::new(paths).unwrap();
    let bytes = data();
    let spec = spec(&bytes, "embeddinggemma-2-TEST.gguf");
    let transport = Arc::new(FakeTransport {
        data: Arc::new(bytes),
        delay,
        calls: Mutex::default(),
    });
    let clients = Arc::new(AtomicUsize::new(0));
    let (t, c) = (Arc::clone(&transport), Arc::clone(&clients));
    state.semantic = SemanticService::with_parts(
        &data_dir,
        catalog(&spec),
        Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::clone(&t) as Arc<dyn Transport>)
        }),
        backend,
    );
    let events = Arc::new(Events::default());
    state
        .semantic
        .set_sink(Arc::clone(&events) as Arc<dyn SemanticSink>);
    App {
        _dir: dir,
        data_dir,
        state,
        transport,
        clients,
        events,
        spec,
    }
}

fn app() -> App {
    app_with(Duration::ZERO, |s| {
        Catalog::for_tests(None, Some(s.clone()))
    })
}

fn desktop() -> DeviceProfile {
    DeviceProfile {
        form: FormFactor::Desktop,
        total_ram_bytes: Some(16 * GIB),
        cores: 8,
        pointer_bits: 64,
    }
}

impl App {
    fn ctx(&self) -> SemanticContext {
        self.state.semantic_context(&desktop(), false).unwrap()
    }
    fn requests(&self) -> Vec<(String, u64)> {
        self.transport.calls.lock().unwrap().clone()
    }
    fn model(&self) -> PathBuf {
        self.data_dir.join("models").join(&self.spec.file_name)
    }
    fn partial(&self) -> PathBuf {
        self.data_dir
            .join("models/.partial")
            .join(format!("{}.part", self.spec.sha256))
    }
    fn download_now(&self) {
        self.state.semantic_start(&self.ctx(), None).unwrap();
        self.state.semantic.wait();
    }
}

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn no_request_before_consent_then_the_consented_download_installs_the_model() {
    let a = app();
    // Status, the semantic setting itself, a leftover partial and a tier change: none of them makes a request.
    let st = a.state.semantic_status(&a.ctx());
    assert_eq!(st.download.state, DownloadState::Idle);
    assert!(st.available && !st.model_present && !st.semantic && !st.active);
    a.state
        .save_settings(&serde_json::json!({"search": {"semantic": true, "tierOverride": "t2"}}))
        .unwrap();
    std::fs::create_dir_all(a.partial().parent().unwrap()).unwrap();
    std::fs::write(a.partial(), &data()[..1000]).unwrap();
    let st = a.state.semantic_status(&a.ctx());
    assert_eq!(st.download.state, DownloadState::Paused);
    assert_eq!(st.download.done, 1000);
    let _ = a.state.semantic_pause(&a.ctx());
    assert_eq!(
        a.clients.load(Ordering::SeqCst),
        0,
        "no HTTPS client before consent"
    );
    assert!(a.requests().is_empty(), "no request before consent");

    a.download_now();
    assert_eq!(a.clients.load(Ordering::SeqCst), 1);
    assert_eq!(
        a.requests(),
        vec![("https://mirror.test/model.gguf".to_owned(), 1000)],
        "resumed from the partial with Range"
    );
    assert_eq!(std::fs::read(a.model()).unwrap(), data());
    let st = a.state.semantic_status(&a.ctx());
    assert!(st.model_present && st.semantic);
    assert_eq!(st.download.state, DownloadState::Done);
    assert!(st.backend_available && st.active, "semantic search answers");
    assert_eq!(st.download.message, None);
    assert!(a.state.settings().unwrap().search.semantic);
    let events = a.events.0.lock().unwrap();
    assert_eq!(events.last().map(|e| e.state), Some(DownloadState::Done));
    assert!(events.iter().any(|e| e.state == DownloadState::Verifying));
}

#[test]
fn t1_and_t0_devices_cannot_download_and_make_no_request() {
    let a = app_with(Duration::ZERO, |_| Catalog::compiled());
    a.state
        .save_settings(&serde_json::json!({"search": {"tierOverride": "t1"}}))
        .unwrap();
    let ctx = a.ctx();
    assert_eq!(ctx.tier, Tier::T1);
    let st = a.state.semantic_status(&ctx);
    assert!(!st.available);
    assert_eq!(
        st.unavailable_reason.as_deref(),
        Some("The model for this device is not available yet.")
    );
    assert!(st
        .consent
        .as_ref()
        .unwrap()
        .body
        .contains("210 MB on this device"));
    let err = a.state.semantic_start(&ctx, None).unwrap_err();
    assert_eq!(
        err.message,
        "The model for this device is not available yet."
    );
    assert!(
        !a.state.settings().unwrap().search.semantic,
        "not turned on"
    );

    let small = DeviceProfile {
        total_ram_bytes: Some(2 * GIB),
        form: FormFactor::Mobile,
        ..desktop()
    };
    let ctx = a.state.semantic_context(&small, true).unwrap();
    assert_eq!(ctx.tier, Tier::T0);
    let st = a.state.semantic_status(&ctx);
    assert!(st.file.is_none() && st.consent.is_none());
    assert!(st.unavailable_reason.unwrap().contains("3 GB"));
    assert!(a.state.semantic_start(&ctx, None).is_err());
    assert_eq!(a.clients.load(Ordering::SeqCst), 0);
    assert!(a.requests().is_empty());
}

#[test]
fn pause_keeps_the_partial_and_cancel_deletes_it() {
    let a = app_with(Duration::from_millis(15), |s| {
        Catalog::for_tests(None, Some(s.clone()))
    });
    a.state.semantic_start(&a.ctx(), None).unwrap();
    wait_until("some bytes", || {
        std::fs::metadata(a.partial()).is_ok_and(|m| m.len() > 0)
    });
    let _ = a.state.semantic_pause(&a.ctx());
    a.state.semantic.wait();
    let st = a.state.semantic_status(&a.ctx());
    assert_eq!(st.download.state, DownloadState::Paused);
    let have = st.download.done;
    assert!(have > 0 && have < a.spec.bytes);
    assert!(a.partial().exists());

    // Resume continues with Range, then cancel stops it and deletes the partial.
    a.state.semantic_start(&a.ctx(), None).unwrap();
    wait_until("the resumed request", || a.requests().len() == 2);
    assert_eq!(a.requests()[1].1, have);
    let st = a.state.semantic_cancel(&a.ctx()).unwrap();
    assert_eq!(st.download.state, DownloadState::Idle);
    assert!(!a.partial().exists(), "cancel deletes the partial");
    assert!(!a.model().exists());
    assert!(
        !a.state.settings().unwrap().search.semantic,
        "cancel turns it off"
    );
}

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

#[test]
fn delete_removes_the_model_and_vectors_but_not_keyword_indexes() {
    let a = app();
    a.download_now();
    let search = a.data_dir.join("search/v1");
    let keyword = [
        search.join("manifest.json"),
        search.join("rules/meta.json"),
        search.join("lore/meta.json"),
    ];
    let vectors = [
        search.join("rules/vectors-0123abcd-256.bin"),
        search.join("rules/keys.json"),
        search.join("lore/vectors-0123abcd-256.bin"),
        search.join("lore/keys.json"),
        search.join("sessions/vectors-0123abcd-128.bin"),
    ];
    for f in keyword.iter().chain(vectors.iter()) {
        touch(f);
    }
    let licence = a.data_dir.join("models/embeddinggemma-2-LICENSE.txt");
    assert!(licence.exists(), "LICENSE next to the model");

    let st = a.state.semantic_delete(&a.ctx()).unwrap();
    assert!(!st.model_present && !st.semantic);
    assert!(!a.model().exists() && !licence.exists());
    for f in &vectors {
        assert!(!f.exists(), "{} should be gone", f.display());
    }
    for f in &keyword {
        assert!(f.exists(), "{} must stay", f.display());
    }
    assert!(!a.state.settings().unwrap().search.semantic);
}

#[test]
fn clear_caches_keeps_the_model() {
    let a = app();
    a.download_now();
    touch(&a.data_dir.join("search/v1/rules/meta.json"));
    a.state.clear_caches(|| 0).unwrap();
    assert!(
        !a.data_dir.join("search/v1/rules/meta.json").exists(),
        "keyword indexes are caches"
    );
    assert_eq!(
        std::fs::read(a.model()).unwrap(),
        data(),
        "the model is not a cache"
    );
    assert!(a.state.semantic_status(&a.ctx()).model_present);
}

#[test]
fn statuses_and_events_match_the_ui_contract() {
    let a = app();
    let contract = contract_support::Contract::load();
    let mut problems = Vec::new();
    let idle = serde_json::to_value(a.state.semantic_status(&a.ctx())).unwrap();
    problems.extend(contract.response("semantic_status", &idle));
    a.download_now();
    let done = serde_json::to_value(a.state.semantic_status(&a.ctx())).unwrap();
    problems.extend(contract.response("semantic_download_start", &done));
    for e in a.events.0.lock().unwrap().iter() {
        problems.extend(contract.event("semantic:download", &serde_json::to_value(e).unwrap()));
    }
    let licence = serde_json::to_value(borealis_lib::semantic_api::model_license()).unwrap();
    problems.extend(contract.response("semantic_model_license", &licence));
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The exact consent wording (spec sec. 8); `ui/src/test/semantic-settings.test.tsx` pins the mock to the same text.
pub const CONSENT_DESKTOP: &str = "Borealis will download the EmbeddingGemma 2 model (Apache 2.0, about 310 MB) once, from Hugging Face or the Borealis mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. Everything runs on this device; your searches and notes never leave it. Indexing takes a few minutes. You can delete the model at any time in Settings.";
pub const CONSENT_T1_PHONE: &str = "Borealis will download the EmbeddingGemma 2 model (Apache 2.0, about 310 MB; 210 MB on this device) once, from Hugging Face or the Borealis mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. Everything runs on this device; your searches and notes never leave it. Indexing takes a few minutes and runs while your device is charging. You can delete the model at any time in Settings.";

#[test]
fn consent_text_is_the_spec_wording() {
    use borealis_embed::models::{consent_body, Q6_K_TE_Q4_K, Q8_0};
    assert_eq!(consent_body(&Q8_0, false), CONSENT_DESKTOP);
    assert_eq!(consent_body(&Q6_K_TE_Q4_K, true), CONSENT_T1_PHONE);
    let a = app_with(Duration::ZERO, |_| Catalog::compiled());
    let c = a.state.semantic_status(&a.ctx()).consent.unwrap();
    assert_eq!(c.title, "Turn on semantic search?");
    assert_eq!(c.body, CONSENT_DESKTOP);
    assert_eq!(
        (c.accept.as_str(), c.decline.as_str()),
        ("Download and turn on", "Not now")
    );
}

#[test]
fn without_a_backend_the_download_command_refuses_and_makes_no_request() {
    let a = app_full(
        Duration::ZERO,
        |s| Catalog::for_tests(None, Some(s.clone())),
        false,
    );
    let st = a.state.semantic_status(&a.ctx());
    assert!(!st.backend_available && !st.active);
    let err = a.state.semantic_start(&a.ctx(), None).unwrap_err();
    assert_eq!(err.code, borealis_lib::error::ErrorCode::RequirementFailed);
    assert_eq!(err.message, borealis_lib::semantic_service::NO_BACKEND_YET);
    assert_eq!(err.message, "Semantic search (search by meaning) arrives in a later update. Search uses keywords until then.");
    // A leftover partial is not resumed either.
    std::fs::create_dir_all(a.partial().parent().unwrap()).unwrap();
    std::fs::write(a.partial(), &data()[..1000]).unwrap();
    assert!(a.state.semantic_start(&a.ctx(), None).is_err());
    assert_eq!(a.clients.load(Ordering::SeqCst), 0, "no HTTPS client");
    assert!(a.requests().is_empty(), "no request");
    assert!(
        !a.state.settings().unwrap().search.semantic,
        "not turned on"
    );
    assert!(!a.model().exists());
}

#[test]
fn without_a_backend_an_existing_model_can_still_be_deleted() {
    let a = app_full(
        Duration::ZERO,
        |s| Catalog::for_tests(None, Some(s.clone())),
        false,
    );
    touch(&a.model());
    std::fs::write(a.model(), data()).unwrap();
    touch(&a.data_dir.join("search/v1/rules/vectors-0123abcd-256.bin"));
    let st = a.state.semantic_status(&a.ctx());
    assert!(st.model_present && !st.active);
    assert_eq!(st.download.state, DownloadState::Done);
    let st = a.state.semantic_delete(&a.ctx()).unwrap();
    assert!(!st.model_present);
    assert!(!a.model().exists());
    assert!(!a
        .data_dir
        .join("search/v1/rules/vectors-0123abcd-256.bin")
        .exists());
    assert!(a.requests().is_empty());
}
