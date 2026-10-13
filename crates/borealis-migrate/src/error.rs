//! Errors of the migrator library (per-file problems are reported as issues instead).

use std::path::PathBuf;

/// Failures that abort a whole run.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// Reading or writing a path failed.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The path being accessed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The input path is neither a file nor a directory.
    #[error("input {0} is not a file or directory")]
    InvalidInput(PathBuf),
    /// Serializing output failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A content error (for example the schema failed to compile).
    #[error("content error: {0}")]
    Content(#[from] borealis_content::ContentError),
}

impl MigrateError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
