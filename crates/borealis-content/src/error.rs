//! Error type for loading, parsing and validating content.

use std::path::PathBuf;

/// Everything that can go wrong in `borealis-content`.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// Reading a file or directory failed.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The path being accessed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A JSON document is malformed or does not match the model.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A JSON document in a named file failed to parse.
    #[error("{path}: {source}")]
    InFile {
        /// The offending file.
        path: PathBuf,
        /// What went wrong.
        #[source]
        source: Box<ContentError>,
    },
    /// The embedded JSON schema could not be compiled (a build defect).
    #[error("schema error: {0}")]
    Schema(String),
    /// A pack manifest lists a path that escapes the pack directory.
    #[error("manifest entry {0:?} escapes the pack directory")]
    UnsafePath(String),
}

impl ContentError {
    /// Wrap an I/O error with its path.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Attach a file path to an error.
    pub(crate) fn in_file(self, path: impl Into<PathBuf>) -> Self {
        Self::InFile {
            path: path.into(),
            source: Box::new(self),
        }
    }
}
