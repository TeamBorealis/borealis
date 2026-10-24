//! Optional local embeddings for search (EM1), work package S3.1 of `specs/local-search-spec.md` (sec. 7, 9).
//!
//! * [`embedder`]: the [`Embedder`] trait (prompts, Matryoshka truncation, L2 normalisation) and the
//!   [`GuardedEmbedder`] whose NaN / zero-norm guard turns semantic mode off and reports it locally.
//! * [`fake`]: a deterministic [`FakeEmbedder`] for tests (the llama.cpp backend is not linked yet, B-46).
//! * [`models`]: the model files per tier (SHA-256, sizes, pinned URLs) and the consent-dialog size text.
//! * [`tier`]: the device tier table T0-T3 and the startup probe; [`device`]: battery / thermal / low power /
//!   background state providers behind [`DeviceStateProvider`].
//! * [`scheduler`]: the pause rules per tier and the checkpointed, resumable [`IndexJob`].
//! * [`store`]: the per-corpus int8 [`VectorStore`] (`vectors-<sha8>-<dims>.bin` + `keys.json`), exact dot product.
//! * [`fusion`]: reciprocal rank fusion of keyword and vector hits with pinned exact names, and the vector-only
//!   snippet rule. Pure functions over keys, so `borealis-search` can use them without a dependency cycle.
//!
//! * [`download`]: the consented model download (pinned URLs, compiled SHA-256 and size, `Range` resume, atomic
//!   rename, other-URL retry, pause / cancel, delete).
//!
//! The only network code is [`download`], which the app runs after the user accepted the consent dialog (spec
//! sec. 8). Embedding inputs and queries never leave the device (C4).

pub mod device;
pub mod download;
pub mod embedder;
pub mod error;
pub mod fake;
pub mod fusion;
#[cfg(feature = "llama")]
pub mod llama;
pub mod models;
pub mod prompt;
pub mod scheduler;
pub mod store;
pub mod tier;
pub mod vector;

pub use device::{DeviceState, DeviceStateProvider, Lifecycle, Power, SimulatedDevice, Thermal};
pub use embedder::{
    DisabledReport, DocText, EmbedStage, Embedder, GuardedEmbedder, ModelInfo, SemanticGuard,
};
pub use error::{EmbedError, Result};
pub use fake::FakeEmbedder;
pub use fusion::{rrf, vector_only_snippet, FusedHit, FUSION_DEPTH, RRF_K};
pub use models::ModelFile;
pub use scheduler::{
    Decision, IndexItem, IndexJob, IndexPolicy, PauseReason, RunOutcome, Scheduler,
    SchedulerSettings,
};
pub use store::{OpenStatus, StoreId, VectorStore};
pub use tier::{DeviceProfile, FormFactor, Tier, TierConfig, TierOverride};
pub use vector::{truncate_normalise, Degeneracy, MATRYOSHKA_DIMS};

/// A real embedding backend is linked into this build. False until the llama.cpp binding lands (B-46): the app
/// can download the model, but semantic search stays off and Settings says it arrives in a later update.
#[must_use]
pub const fn backend_available() -> bool {
    #[cfg(feature = "llama")]
    {
        llama::LINKED
    }
    #[cfg(not(feature = "llama"))]
    {
        false
    }
}
