//! Error type. Messages never carry key material, tokens or full paths.

use std::io;

/// Errors from identity, store, registry and transport operations.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// The peer presented a certificate other than the pinned/allowed one (possible
    /// impersonation, or the peer's identity was reset). Pair again.
    #[error("the peer's certificate does not match the paired fingerprint")]
    PinMismatch,
    /// File or socket I/O failed.
    #[error("i/o error: {0}")]
    Io(String),
    /// TLS setup or handshake failed.
    #[error("tls: {0}")]
    Tls(String),
    /// A peer, file or caller supplied malformed data.
    #[error("invalid: {0}")]
    Invalid(String),
    /// A frame exceeded the configured maximum size.
    #[error("frame of {size} bytes exceeds the {max}-byte limit")]
    FrameTooLarge {
        /// Announced size.
        size: usize,
        /// Configured limit.
        max: usize,
    },
    /// The operation did not finish in time.
    #[error("timed out: {0}")]
    Timeout(&'static str),
    /// The secure store failed or is unavailable on this machine.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// No secure randomness available.
    #[error("no secure randomness: {0}")]
    Random(String),
}

impl LinkError {
    /// Wrap an I/O error (the message only: callers pass no paths in).
    pub fn io(e: impl std::fmt::Display) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<io::Error> for LinkError {
    fn from(e: io::Error) -> Self {
        Self::io(e)
    }
}

/// Secure store failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// No usable secure store on this machine (for example no Secret Service on a headless
    /// Linux box, or the app is not signed with a keychain entitlement). Device sync stays off;
    /// keys are never written to a plain file instead.
    #[error("no secure store available: {0}")]
    Unavailable(String),
    /// The store exists but refused access (locked keychain, user denied the prompt).
    #[error("secure store access denied: {0}")]
    AccessDenied(String),
    /// Any other platform failure.
    #[error("secure store failure: {0}")]
    Platform(String),
    /// The secret name is not valid.
    #[error("invalid secret name")]
    BadName,
}
