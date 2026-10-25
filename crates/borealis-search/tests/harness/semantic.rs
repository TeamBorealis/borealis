//! V4 (spec sections 6 and 7, WP S3.3): the adopted keyword configuration fused with EmbeddingGemma 2 vectors by
//! reciprocal rank fusion, on the app's own path: keyword top [`FUSION_DEPTH`] + int8 [`VectorStore`] top
//! [`FUSION_DEPTH`] through `borealis_search::hybrid::fuse_hits` (exact names pinned, per-document fusion,
//! vector-only documents filtered by the query's edition).
//!
//! Document vectors are expensive (minutes per model), so the raw native-width vectors of every chunk are cached
//! per model file and chunk cap ([`RawCache`], under `BOREALIS_V4_CACHE`). One cache serves every Matryoshka
//! size: truncating and re-normalising a cached raw vector is exactly what the embedder does. Query vectors are
//! always computed live, so the measured latency includes the query embedding.
use super::data::{canon, EvalQuery};
use super::variants::{edition_of, Fixture};
use super::Ranker;
use borealis_embed::download::file_sha256;
use borealis_embed::llama::{LlamaEmbedder, LlamaOptions};
use borealis_embed::models::{self, ALL_FILES, NATIVE_DIMS};
use borealis_embed::prompt::{document_prompt, PROMPT_VERSION};
use borealis_embed::store::StoreId;
use borealis_embed::{truncate_normalise, Embedder, ModelInfo, VectorStore, FUSION_DEPTH};
use borealis_search::hybrid::fuse_hits;
use borealis_search::{Corpus, EditionScope, SearchConfig, SearchEngine, SearchRequest};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// Chunks per embedding call while filling the cache.
const CACHE_BATCH: usize = 16;
/// Default embedding threads (S0.2 measured with 4).
const DEFAULT_THREADS: usize = 4;
const HASH_HEX: usize = 64;

/// One model file, chunk cap and thread count under test.
#[derive(Debug, Clone)]
pub struct EmbedSpec {
    pub path: PathBuf,
    pub info: ModelInfo,
    pub max_tokens: usize,
    pub threads: usize,
}

impl EmbedSpec {
    /// Identify `path` by its SHA-256 (the compiled entry when it is a published file, else a local label from the
    /// file name, e.g. `Q5_K_M`).
    pub fn new(path: &Path, max_tokens: usize) -> Self {
        let sha256 = file_sha256(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let info = ALL_FILES
            .iter()
            .find(|f| f.sha256 == sha256)
            .map_or_else(|| local_info(path, &sha256), models::ModelFile::info);
        let threads = std::env::var("BOREALIS_EMBED_THREADS")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(DEFAULT_THREADS);
        Self {
            path: path.to_path_buf(),
            info,
            max_tokens,
            threads,
        }
    }

    pub fn load(&self, dims: usize) -> LlamaEmbedder {
        LlamaEmbedder::load_with(
            &self.path,
            self.info.clone(),
            dims,
            self.threads,
            LlamaOptions::for_max_tokens(self.max_tokens),
        )
        .unwrap_or_else(|e| panic!("load {}: {e}", self.path.display()))
    }
}

fn local_info(path: &Path, sha256: &str) -> ModelInfo {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("local")
        .trim_start_matches("embeddinggemma-2-");
    ModelInfo {
        id: models::MODEL_ID.into(),
        sha256: sha256.into(),
        quant: stem.chars().take(16).collect(),
        native_dims: NATIVE_DIMS,
    }
}

/// Cache directory (`BOREALIS_V4_CACHE`).
pub fn cache_dir() -> PathBuf {
    std::env::var_os("BOREALIS_V4_CACHE")
        .map(PathBuf::from)
        .expect("set BOREALIS_V4_CACHE to a folder for the cached document vectors")
}

/// Raw native-width document vectors of one model file and chunk cap, keyed by `(doc_key, content_hash)`.
///
/// File `raw-<sha8>-t<max_tokens>.bin`: records of `u16` key length, key bytes, 64-byte hex content hash, then
/// [`NATIVE_DIMS`] little-endian `f32`. Appended batch by batch, so an interrupted fill resumes.
pub struct RawCache {
    path: PathBuf,
    rows: HashMap<(String, String), Vec<f32>>,
}

impl RawCache {
    pub fn open(dir: &Path, spec: &EmbedSpec) -> Self {
        std::fs::create_dir_all(dir).expect("cache dir");
        let path = dir.join(format!("raw-{}-t{}.bin", spec.info.sha8(), spec.max_tokens));
        let rows = std::fs::File::open(&path)
            .map(|f| read_records(BufReader::new(f)))
            .unwrap_or_default();
        Self { path, rows }
    }

    pub fn get(&self, key: &str, hash: &str) -> Option<&Vec<f32>> {
        self.rows.get(&(key.to_string(), hash.to_string()))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    fn append(&mut self, rows: Vec<(String, String, Vec<f32>)>) {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("open cache");
        let mut w = BufWriter::new(file);
        for (key, hash, v) in rows {
            let len = u16::try_from(key.len()).expect("short key");
            w.write_all(&len.to_le_bytes()).expect("write");
            w.write_all(key.as_bytes()).expect("write");
            assert_eq!(hash.len(), HASH_HEX);
            w.write_all(hash.as_bytes()).expect("write");
            for x in &v {
                w.write_all(&x.to_le_bytes()).expect("write");
            }
            self.rows.insert((key, hash), v);
        }
        w.flush().expect("flush cache");
    }
}

fn read_records(mut r: impl Read) -> HashMap<(String, String), Vec<f32>> {
    let mut out = HashMap::new();
    loop {
        let mut len = [0u8; 2];
        if r.read_exact(&mut len).is_err() {
            return out;
        }
        let mut key = vec![0u8; usize::from(u16::from_le_bytes(len))];
        let mut hash = [0u8; HASH_HEX];
        let mut raw = vec![0u8; NATIVE_DIMS * 4];
        if r.read_exact(&mut key).is_err()
            || r.read_exact(&mut hash).is_err()
            || r.read_exact(&mut raw).is_err()
        {
            return out; // a torn last record: refilled on the next run
        }
        let v = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        out.insert(
            (
                String::from_utf8(key).expect("utf-8 key"),
                String::from_utf8(hash.to_vec()).expect("hex hash"),
            ),
            v,
        );
    }
}

/// Embed every fixture chunk missing from the cache (the app's prompt: element or section name as title, chunk
/// body as text). Prints progress and throughput.
pub fn fill_cache(fixture: &Fixture, spec: &EmbedSpec) -> RawCache {
    let mut cache = RawCache::open(&cache_dir(), spec);
    let missing: Vec<_> = fixture
        .chunks
        .iter()
        .filter(|c| cache.get(&c.doc_key, &c.content_hash()).is_none())
        .collect();
    println!(
        "cache {}: {} of {} chunks present, {} to embed ({} threads, {} tokens)",
        cache.path.display(),
        cache.len(),
        fixture.chunks.len(),
        missing.len(),
        spec.threads,
        spec.max_tokens
    );
    if missing.is_empty() {
        return cache;
    }
    let embedder = spec.load(NATIVE_DIMS);
    let started = Instant::now();
    for (i, batch) in missing.chunks(CACHE_BATCH).enumerate() {
        let prompts: Vec<String> = batch
            .iter()
            .map(|c| document_prompt(Some(&c.name), &c.body))
            .collect();
        let raw = embedder.embed_prompts(&prompts).expect("embed");
        cache.append(
            batch
                .iter()
                .zip(raw)
                .map(|(c, v)| (c.doc_key.clone(), c.content_hash(), v))
                .collect(),
        );
        let done = (i + 1) * CACHE_BATCH;
        if i % 10 == 9 {
            let rate = done as f64 / started.elapsed().as_secs_f64();
            println!("  {done}/{} chunks, {rate:.1} chunks/s", missing.len());
        }
    }
    println!(
        "  done: {} chunks in {:.0} s",
        missing.len(),
        started.elapsed().as_secs_f64()
    );
    cache
}

/// Hybrid ranker: keyword (adopted config) + vectors at `dims`, fused like the app.
pub struct V4Ranker {
    label: String,
    engine: Rc<SearchEngine>,
    cfg: SearchConfig,
    store: VectorStore,
    embedder: LlamaEmbedder,
    /// Per call: (keyword ms, query embedding ms, vector search + fusion ms).
    timings: RefCell<Vec<[f64; 3]>>,
    _dir: tempfile::TempDir,
}

/// The `p`-quantile of `values` (nearest rank).
fn quantile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}

impl V4Ranker {
    /// p50 / p95 of each stage over every call so far: `[(keyword), (query embedding), (vector search + fusion)]`.
    pub fn stage_ms(&self) -> [(f64, f64); 3] {
        let t = self.timings.borrow();
        [0, 1, 2].map(|i| {
            let col: Vec<f64> = t.iter().map(|r| r[i]).collect();
            (quantile(col.clone(), 0.5), quantile(col, 0.95))
        })
    }

    /// Build the int8 store at `dims` from a complete cache (panics on a missing chunk: run the fill first).
    pub fn new(fixture: &Fixture, spec: &EmbedSpec, cache: &RawCache, dims: usize) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let id = StoreId::new(&spec.info.sha256, &spec.info.quant, dims, PROMPT_VERSION)
            .expect("store id");
        let (mut store, _) = VectorStore::open(dir.path(), id).expect("store");
        for c in &fixture.chunks {
            let raw = cache
                .get(&c.doc_key, &c.content_hash())
                .unwrap_or_else(|| panic!("{} is not cached: run --v4-cache first", c.doc_key));
            let v = truncate_normalise(raw, dims).expect("usable vector");
            store
                .upsert(&c.pack, &c.doc_key, &c.content_hash(), &v)
                .expect("upsert");
        }
        Self {
            label: label(spec, dims),
            engine: fixture.engine(),
            cfg: SearchConfig::default(),
            store,
            embedder: spec.load(dims),
            timings: RefCell::new(Vec::new()),
            _dir: dir,
        }
    }
}

/// `V4[Q8_0/256/t512]`.
pub fn label(spec: &EmbedSpec, dims: usize) -> String {
    format!("V4[{}/{dims}/t{}]", spec.info.quant, spec.max_tokens)
}

impl Ranker for V4Ranker {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn rank(&self, q: &EvalQuery, k: usize) -> Vec<String> {
        let mut req = SearchRequest::new(q.text.clone());
        req.limit = FUSION_DEPTH;
        let edition = edition_of(q);
        if let Some(e) = edition {
            req.edition = EditionScope::Only(e);
        }
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let t0 = Instant::now();
        let keyword = self
            .engine
            .search(Corpus::Lore, &req, &self.cfg)
            .expect("search");
        let keyword_ms = ms(t0);
        let t1 = Instant::now();
        let query = self.embedder.embed_query(&q.text).expect("query vector");
        let embed_ms = ms(t1);
        let t2 = Instant::now();
        let vector = self.store.search(&query, FUSION_DEPTH);
        let tag = edition.map(|e| borealis_search::Edition::tag(e).as_str());
        let fused = fuse_hits(
            &q.text,
            keyword,
            &vector,
            FUSION_DEPTH,
            |key| self.engine.passage(Corpus::Lore, key),
            |p| tag.is_none_or(|t| p.ruleset == t || p.ruleset == "none"),
        )
        .expect("fuse");
        self.timings
            .borrow_mut()
            .push([keyword_ms, embed_ms, ms(t2)]);
        let mut seen = HashSet::new();
        fused
            .into_iter()
            .map(|h| canon(&h.doc_key).to_string())
            .filter(|key| seen.insert(key.clone()))
            .take(k)
            .collect()
    }
}
