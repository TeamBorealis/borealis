//! Error type of the search crate.

use thiserror::Error;

/// Everything that can go wrong while building, opening or querying an index.
#[derive(Debug, Error)]
pub enum SearchError {
    /// Tantivy failed (index, writer, reader, query).
    #[error("search index error: {0}")]
    Index(String),
    /// Filesystem failure.
    #[error("search io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (manifest, rules-text pack) failure.
    #[error("search json error: {0}")]
    Json(#[from] serde_json::Error),
    /// The synonym table is malformed.
    #[error("synonym table error: {0}")]
    Synonyms(String),
    /// A rules-text pack is not a valid `borealis-docs/1` file.
    #[error("invalid rules-text pack: {0}")]
    InvalidDocs(String),
    /// A read-only open found no index (the app has not built it yet).
    #[error("search index not built yet")]
    NotBuilt,
    /// A read-only open found an index built by another schema/analyzer/synonym version.
    #[error("search index is stale: {0}")]
    Stale(String),
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(e: tantivy::TantivyError) -> Self {
        Self::Index(e.to_string())
    }
}

impl From<tantivy::directory::error::OpenDirectoryError> for SearchError {
    fn from(e: tantivy::directory::error::OpenDirectoryError) -> Self {
        Self::Index(e.to_string())
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, SearchError>;
