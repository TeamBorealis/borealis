//! Error type of the importer. Per-entry problems never abort an import; they go to the
//! [`ImportReport`](crate::report::ImportReport). Only folder-level failures are errors.

use std::path::PathBuf;

/// A failure that stops the whole import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The given path is not a directory.
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),
    /// The folder does not look like a 5etools `data/` folder (no known data file found).
    #[error("{0} does not look like a 5etools data folder (no known data files found)")]
    NotFiveTools(PathBuf),
    /// Reading a file failed.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Serialising or writing an output pack failed.
    #[error("cannot write pack: {0}")]
    Write(String),
    /// A content-model error from `borealis-content`.
    #[error(transparent)]
    Content(#[from] borealis_content::ContentError),
}

impl ImportError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
