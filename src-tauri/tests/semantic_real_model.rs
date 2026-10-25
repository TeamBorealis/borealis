//! S3.4 (s3.3-1 sec. 7 item 5): the app's semantic speed check with the real model on this machine. The indexer
//! loads the tier's file exactly as the app does (probed tier, `config_for`: an SSE2-only x86_64 build gets the T2
//! row), times the query embedding against the 40 ms budget, and either indexes (hybrid) or stays keyword-only with
//! the "too slow" line. The rule is not changed here; the test records which way it goes and checks that a miss is
//! honest (no vectors answer a search). Prints one JSON line (labels and numbers only).
//!
//! Needs the model file, so it is ignored by default (CI job `embed-platforms`):
//! ```text
//! BOREALIS_EMBED_MODEL=/path/embeddinggemma-2-Q8_0.gguf \
//!   cargo test -p borealis --release --test semantic_real_model -- --ignored --nocapture
//! ```
#![cfg(feature = "llama")]

use borealis_embed::llama::{LlamaEmbedder, LlamaOptions};
use borealis_embed::{
    DeviceProfile, DeviceState, Embedder, GuardedEmbedder, SemanticGuard, SimulatedDevice,
};
use borealis_lib::vector_service::{IndexRequest, IndexingState, VectorService};
use borealis_lib::vector_status::probe_query_ms;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Loading plus the speed check; generous for an emulated CPU.
const WAIT: Duration = Duration::from_secs(30 * 60);

/// Put the model where the app keeps it (`<app-data>/models/<file>`): a hard link, else a copy.
fn install(model: &Path, models_dir: &Path, file_name: &str) {
    std::fs::create_dir_all(models_dir).expect("models dir");
    let target = models_dir.join(file_name);
    if std::fs::hard_link(model, &target).is_err() {
        std::fs::copy(model, &target).expect("copy the model");
    }
}

#[test]
#[ignore = "needs BOREALIS_EMBED_MODEL=<path to embeddinggemma-2-Q8_0.gguf>"]
fn the_speed_check_decides_hybrid_or_keyword_only_on_this_machine() {
    let model = PathBuf::from(
        std::env::var_os("BOREALIS_EMBED_MODEL").expect("set BOREALIS_EMBED_MODEL to the GGUF"),
    );
    let profile = DeviceProfile::probe();
    let tier = profile.tier().config_for(&profile);
    let file = tier.model.expect("this machine has a semantic tier");
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("data");
    install(&model, &data.join("models"), file.file_name);

    let service = VectorService::new(&data);
    service.set_device(
        profile,
        Arc::new(SimulatedDevice::new(DeviceState::DESKTOP)),
    );
    service.refresh(IndexRequest {
        semantic: true,
        index_on_battery: false,
        tier,
        inputs: Arc::new(Vec::new),
    });
    assert!(service.wait_idle(WAIT), "the indexer finished");
    let status = service.status();

    // The same probe once more on a separately loaded backend, for the record (the verdict above is the app's).
    let backend = LlamaEmbedder::load_with(
        &data.join("models").join(file.file_name),
        file.info(),
        tier.dims.expect("dims"),
        tier.threads,
        LlamaOptions::for_max_tokens(tier.max_tokens),
    )
    .expect("load");
    let probe_ms = probe_query_ms(&GuardedEmbedder::new(
        Arc::new(backend) as Arc<dyn Embedder>,
        SemanticGuard::new(),
    ));

    let keyword_only = status.state == IndexingState::Failed;
    println!(
        "{}",
        serde_json::json!({
            "tier": format!("{:?}", tier.tier),
            "quant": file.quant,
            "dims": tier.dims,
            "threads": tier.threads,
            "max_tokens": tier.max_tokens,
            "cores": profile.cores,
            "ram_gib": profile.total_ram_bytes.map(|b| b / (1 << 30)),
            "portable_x86": borealis_embed::tier::PORTABLE_X86,
            "budget_ms": tier.query_budget_ms,
            "app_state": format!("{:?}", status.state),
            "app_verdict": if keyword_only { "keyword-only" } else { "hybrid" },
            "probe_query_ms_median": probe_ms,
            "app_message": status.message,
        })
    );
    match status.state {
        IndexingState::Ready => assert!(status.message.is_none(), "{status:?}"),
        IndexingState::Failed => {
            let message = status.message.as_deref().unwrap_or_default();
            assert!(
                message
                    .starts_with("Semantic search is too slow on this device: one search takes ")
                    && message
                        .contains("ms to embed (the budget is 40 ms), so search uses keywords."),
                "{status:?}"
            );
            assert!(
                !service.queryable(),
                "a missed budget leaves search keyword-only"
            );
            assert!(service.query("how does grappling work").is_none());
        }
        other => panic!("unexpected state {other:?}: {status:?}"),
    }
    service.shutdown();
}
