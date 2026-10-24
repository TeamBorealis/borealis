//! Transfer errors. Messages are user-facing and never carry paths or secrets.

/// Why a transfer ended without applying.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    /// This user cancelled.
    #[error("cancelled")]
    Cancelled,
    /// The other device's user cancelled.
    #[error("the other device cancelled")]
    PeerCancelled,
    /// The receiving user declined the offer.
    #[error("the other device declined")]
    Declined,
    /// The peer broke the protocol.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The peer's data was refused (limits, bad hash, invalid items, unsafe names).
    #[error("refused: {0}")]
    Refused(String),
    /// The connection failed.
    #[error("connection: {0}")]
    Link(String),
    /// Local file or apply failure.
    #[error("{0}")]
    Local(String),
}

impl TransferError {
    /// A short machine code for the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::PeerCancelled => "peer_cancelled",
            Self::Declined => "declined",
            Self::Protocol(_) => "protocol",
            Self::Refused(_) => "refused",
            Self::Link(_) => "connection",
            Self::Local(_) => "local",
        }
    }
}

impl From<std::io::Error> for TransferError {
    fn from(e: std::io::Error) -> Self {
        Self::Local(format!("file error: {e}"))
    }
}
