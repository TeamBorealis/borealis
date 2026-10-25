//! The llama.cpp backend against sentence-transformers fp32 reference vectors (spike S0.2 method, spec sec. 12
//! rows S0.2 / S3.4): the 20 fixed strings (already in their prompt templates) must reach cosine >= 0.99 at 768
//! and 256 dims, both one string per call and packed into one call. Also prints load / throughput / query
//! timings as one JSON line.
//!
//! Needs a model file, so it is ignored by default:
//! ```text
//! BOREALIS_EMBED_MODEL=/path/embeddinggemma-2-Q8_0.gguf \
//!   cargo test -p borealis-embed --features llama --release --test llama_reference -- --ignored --nocapture
//! ```
//! Optional: `BOREALIS_EMBED_THREADS` (default 4, as S0.2), `BOREALIS_EMBED_REF` (reference JSON; default
//! `tests/fixtures/embeddinggemma-2-ref-fp32.json`, the spike's `ref-fp32.json`), `BOREALIS_EMBED_MAX_TOKENS`
//! (a tier's chunk cap, e.g. 512 or 256: the context the app loads with, `LlamaOptions::for_max_tokens`; default
//! the 2048 / 16 of S0.2).
#![cfg(feature = "llama")]

use std::path::{Path, PathBuf};
use std::time::Instant;

use borealis_embed::download::file_sha256;
use borealis_embed::llama::{LlamaEmbedder, LlamaOptions};
use borealis_embed::models::{self, ALL_FILES};
use borealis_embed::vector::dot;
use borealis_embed::{truncate_normalise, Embedder, ModelInfo};

const MIN_COSINE: f32 = 0.99;
const CHECK_DIMS: [usize; 2] = [768, 256];
const DEFAULT_THREADS: usize = 4;
/// S0.2 throughput chunk: 16 document prompts of about 206 tokens.
const BENCH_CHUNKS: usize = 16;

struct Reference {
    strings: Vec<String>,
    vectors: Vec<Vec<f32>>,
}

fn reference() -> Reference {
    let path = std::env::var_os("BOREALIS_EMBED_REF").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/embeddinggemma-2-ref-fp32.json")
        },
        PathBuf::from,
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reference vectors {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("reference JSON");
    let reference = Reference {
        strings: serde_json::from_value(json["strings"].clone()).expect("strings"),
        vectors: serde_json::from_value(json["vectors"].clone()).expect("vectors"),
    };
    assert_eq!(reference.strings.len(), reference.vectors.len());
    assert!(reference
        .vectors
        .iter()
        .all(|v| v.len() == models::NATIVE_DIMS));
    reference
}

/// The compiled entry for a known file, else an ad-hoc identity (a local re-quantisation under test).
fn model_info(path: &Path) -> ModelInfo {
    let sha256 = file_sha256(path).expect("read model");
    ALL_FILES.iter().find(|f| f.sha256 == sha256).map_or_else(
        || ModelInfo {
            id: models::MODEL_ID.into(),
            sha256: sha256.clone(),
            quant: "local".into(),
            native_dims: models::NATIVE_DIMS,
        },
        models::ModelFile::info,
    )
}

/// Smallest cosine per checked dims between `ours` (raw) and the reference (raw), both truncated + normalised.
fn min_cosines(ours: &[Vec<f32>], reference: &[Vec<f32>]) -> [f32; 2] {
    assert_eq!(ours.len(), reference.len());
    CHECK_DIMS.map(|dims| {
        ours.iter()
            .zip(reference)
            .map(|(a, b)| {
                let a = truncate_normalise(a, dims).expect("our vector is usable");
                let b = truncate_normalise(b, dims).expect("reference vector");
                dot(&a, &b)
            })
            .fold(f32::MAX, f32::min)
    })
}

fn ms(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 1e4).round() / 10.0
}

/// Query timings taken for the median.
const QUERY_RUNS: usize = 10;

/// Resident set size of this process in MB (desktop; the weights are mmapped, so mapped pages count once read).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn rss_mb() -> Option<u64> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = sysinfo::get_current_pid().ok()?;
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    sys.process(pid).map(|p| p.memory() / 1_048_576)
}

/// Android: `VmRSS` of `/proc/self/status` (kB).
#[cfg(target_os = "android")]
fn rss_mb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

#[cfg(target_os = "ios")]
fn rss_mb() -> Option<u64> {
    None
}

#[test]
#[ignore = "needs BOREALIS_EMBED_MODEL=<path to an EmbeddingGemma 2 GGUF>"]
fn llama_backend_matches_the_fp32_reference() {
    let path = PathBuf::from(
        std::env::var_os("BOREALIS_EMBED_MODEL").expect("set BOREALIS_EMBED_MODEL to a GGUF path"),
    );
    let threads = std::env::var("BOREALIS_EMBED_THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(DEFAULT_THREADS);
    let reference = reference();
    let info = model_info(&path);

    let t = Instant::now();
    let sha_ok = file_sha256(&path).expect("hash") == info.sha256;
    let sha_ms = ms(t);
    let max_tokens: Option<usize> = std::env::var("BOREALIS_EMBED_MAX_TOKENS")
        .ok()
        .and_then(|t| t.parse().ok());
    let options = max_tokens.map_or_else(LlamaOptions::default, LlamaOptions::for_max_tokens);
    let t = Instant::now();
    let embedder =
        LlamaEmbedder::load_with(&path, info.clone(), 256, threads, options).expect("load");
    let load_ms = ms(t);

    // One string per call (as S0.2), then all 20 packed into decodes of several sequences.
    let single: Vec<Vec<f32>> = reference
        .strings
        .iter()
        .map(|s| {
            embedder
                .embed_prompts(std::slice::from_ref(s))
                .expect("embed")
                .remove(0)
        })
        .collect();
    let packed = embedder.embed_prompts(&reference.strings).expect("embed");
    let single_cos = min_cosines(&single, &reference.vectors);
    let packed_cos = min_cosines(&packed, &reference.vectors);

    // Throughput on S0.2's chunk (the Fireball text twice, untitled), then one query.
    let body = reference.strings[19].trim_start_matches("title: Fireball | text: ");
    let chunk = format!("title: none | text: {body} {body}");
    let chunks = vec![chunk; BENCH_CHUNKS];
    let t = Instant::now();
    for c in &chunks {
        embedder
            .embed_prompts(std::slice::from_ref(c))
            .expect("bench");
    }
    let sequential_per_s = BENCH_CHUNKS as f64 / t.elapsed().as_secs_f64();
    let t = Instant::now();
    embedder.embed_prompts(&chunks).expect("bench packed");
    let packed_per_s = BENCH_CHUNKS as f64 / t.elapsed().as_secs_f64();
    let mut query_ms: Vec<f64> = (0..QUERY_RUNS)
        .map(|_| {
            let t = Instant::now();
            embedder
                .embed_query("how does grappling work")
                .expect("query");
            ms(t)
        })
        .collect();
    query_ms.sort_by(f64::total_cmp);
    let query_ms = query_ms[QUERY_RUNS / 2];

    println!(
        "{}",
        serde_json::json!({
            "model": path.file_name().and_then(|n| n.to_str()),
            "quant": info.quant,
            "threads": threads,
            "max_tokens": options.max_tokens,
            "max_seqs": options.max_seqs,
            "sha_ms": sha_ms,
            "load_ms_incl_sha": load_ms,
            "min_cos_768_single": single_cos[0],
            "min_cos_256_single": single_cos[1],
            "min_cos_768_packed": packed_cos[0],
            "min_cos_256_packed": packed_cos[1],
            "chunks_per_s_sequential": (sequential_per_s * 10.0).round() / 10.0,
            "chunks_per_s_packed": (packed_per_s * 10.0).round() / 10.0,
            "query_ms_median": query_ms,
            "rss_mb_after": rss_mb(),
        })
    );
    assert!(sha_ok);
    for (label, cos) in [("single", single_cos), ("packed", packed_cos)] {
        for (dims, c) in CHECK_DIMS.iter().zip(cos) {
            assert!(
                c >= MIN_COSINE,
                "{label} min cosine at {dims} dims is {c:.5} < {MIN_COSINE}"
            );
        }
    }
}
