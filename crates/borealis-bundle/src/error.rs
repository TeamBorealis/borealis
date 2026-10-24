//! Errors of the bundle reader, writer and scanners.

use std::path::PathBuf;

/// Everything that can go wrong with a bundle.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// The file is not a `borealis-bundle` zip at all.
    #[error("not a Borealis bundle: {0}")]
    NotABundle(String),
    /// A `borealis-bundle/N` this build does not understand.
    #[error("unsupported bundle format `{0}` (this app reads `borealis-bundle/1`)")]
    UnsupportedFormat(String),
    /// The bundle uses a reserved slot that is not built yet (campaigns).
    #[error("the bundle contains {0}, which this version cannot read")]
    UnsupportedFeature(&'static str),
    /// An entry or key that could escape the bundle, or is otherwise malformed.
    #[error("unsafe or malformed path `{0}`")]
    UnsafePath(String),
    /// Something the exclusion list forbids (secrets, indexes, models, drafts, device settings).
    #[error("`{0}` is excluded from bundles")]
    Excluded(String),
    /// A size, count or compression-ratio cap was exceeded.
    #[error("bundle limit exceeded: {0}")]
    LimitExceeded(String),
    /// A file does not hash to the digest the manifest promises.
    #[error("checksum mismatch for `{0}`")]
    HashMismatch(String),
    /// The manifest contradicts the zip (missing, unlisted or duplicate entries, wrong sizes).
    #[error("inconsistent bundle: {0}")]
    Inconsistent(String),
    /// An item failed the receiving app's validation (or its key is not a plain id); the whole
    /// bundle was refused and nothing was written.
    #[error("refused: {0}")]
    Rejected(String),
    /// JSON that failed to parse.
    #[error("invalid JSON in `{path}`: {source}")]
    Json {
        /// The offending bundle path.
        path: String,
        /// The parser error.
        source: serde_json::Error,
    },
    /// Zip container error.
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// I/O error, optionally tied to a path.
    #[error("I/O error{}: {source}", .path.as_ref().map(|p| format!(" at {}", p.display())).unwrap_or_default())]
    Io {
        /// Local path involved, when known.
        path: Option<PathBuf>,
        /// The OS error.
        source: std::io::Error,
    },
}

impl From<std::io::Error> for BundleError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { path: None, source }
    }
}

impl BundleError {
    pub(crate) fn io_at(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: Some(path.into()),
            source,
        }
    }
}

/// Result alias.
pub type Result<T> = std::result::Result<T, BundleError>;
