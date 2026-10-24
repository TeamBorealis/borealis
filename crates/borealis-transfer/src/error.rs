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

/// Longest text taken from a peer's `Failed` reason.
pub const MAX_PEER_REASON_CHARS: usize = 200;

impl TransferError {
    /// The reason this device sends in `Failed`: a local file or apply problem is described in
    /// general words, never with this device's paths or OS error texts.
    pub fn for_peer(&self) -> String {
        match self {
            Self::Local(_) => "the receiving device could not save the items".into(),
            other => other.to_string(),
        }
    }
}

/// A peer's free-text reason as this device shows it: control and invisible formatting
/// characters removed, at most [`MAX_PEER_REASON_CHARS`].
pub fn peer_reason(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !borealis_link::registry::is_unsafe_display_char(*c))
        .take(MAX_PEER_REASON_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "no reason given".into()
    } else {
        trimmed.to_string()
    }
}

impl From<std::io::Error> for TransferError {
    fn from(e: std::io::Error) -> Self {
        Self::Local(format!("file error: {e}"))
    }
}

/// A bundle problem in words a person can act on (no paths, no zip jargon).
pub fn plain_reason(e: &borealis_bundle::BundleError) -> String {
    use borealis_bundle::BundleError as B;
    match e {
        B::NotABundle(_) => "it is not a Borealis bundle".into(),
        B::UnsupportedFormat(_) | B::UnsupportedFeature(_) => {
            "it was made by a newer version of Borealis, so update Borealis to open it".into()
        }
        B::LimitExceeded(_) => "it is larger than Borealis accepts".into(),
        B::Rejected(why) => why.clone(),
        B::Io { .. } => "it could not be read".into(),
        B::UnsafePath(_)
        | B::Excluded(_)
        | B::HashMismatch(_)
        | B::Inconsistent(_)
        | B::Json { .. }
        | B::Zip(_) => "it is damaged or was changed after it was made".into(),
    }
}
