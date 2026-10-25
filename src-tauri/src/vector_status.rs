//! Status types of the semantic index (S3.3): what `vector_service` reports to Settings, the search footer and the
//! `borealis://semantic-index` event, the pause sentences, and the query-speed probe.

use borealis_embed::{GuardedEmbedder, PauseReason};
use serde::Serialize;
use std::time::Instant;

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

/// Median query-embedding time in ms over [`PROBE_QUERIES`] after one warm-up (the app's speed check; also timed by
/// the real-model test `tests/semantic_real_model.rs`); `None` when a probe fails (the guard then reports the reason).
pub fn probe_query_ms(embedder: &GuardedEmbedder) -> Option<u32> {
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
