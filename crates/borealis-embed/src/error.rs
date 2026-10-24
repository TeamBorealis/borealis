//! Errors of the embedding runtime and the vector store.

use crate::vector::Degeneracy;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, EmbedError>;

/// Everything that can go wrong while embedding or storing vectors.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// The backend (llama.cpp, or the fake) failed to produce vectors.
    #[error("embedding backend failed: {0}")]
    Backend(String),
    /// The backend returned NaN, infinite or zero-norm vectors (spec sec. 7: f16 yields NaN).
    #[error("degenerate embedding: {0}")]
    Degenerate(Degeneracy),
    /// The backend answered a different number of vectors than inputs.
    #[error("backend returned {got} vectors for {want} inputs")]
    Count { got: usize, want: usize },
    /// Requested output dimensions are not a Matryoshka size of the model.
    #[error("unsupported dimensions {dims} (model has {native})")]
    Dims { dims: usize, native: usize },
    /// The guard turned semantic mode off after a degenerate vector.
    #[error("semantic search is off on this device: {0}")]
    Disabled(String),
    /// Vector store file I/O.
    #[error("vector store I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A vector store file failed its checks (bad magic, digest, truncated rows, bad sidecar).
    #[error("vector store is corrupt: {0}")]
    Corrupt(String),
    /// A vector of the wrong length was added to a store.
    #[error("vector has {got} dimensions, store holds {want}")]
    VectorLength { got: usize, want: usize },
}
