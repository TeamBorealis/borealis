//! Messages of the transfer protocol (after [`borealis_link::auth`] succeeded).
//!
//! Every frame starts with a tag byte: [`TAG_CONTROL`] + JSON [`Control`], or [`TAG_CHUNK`] +
//! raw bundle bytes.
//!
//! ```text
//! both      -> Hello {v, name, platform, role}
//! sender    -> Offer {manifest, bundle_size, bundle_sha256, labels}
//! receiver  -> Waiting (every 15 s while the user looks at the preview) ... Accept | Decline
//! sender    -> chunk* -> Done
//! receiver  -> Finished {summary} | Failed {reason}
//! either    -> Cancel (then closes)
//! ```

use std::collections::BTreeMap;

use borealis_bundle::Manifest;
use borealis_link::{LinkError, Platform};
use serde::{Deserialize, Serialize};

use crate::error::TransferError;

/// Protocol version.
pub const PROTOCOL: u8 = 1;
/// Tag of a JSON control frame.
pub const TAG_CONTROL: u8 = 0;
/// Tag of a bundle data frame.
pub const TAG_CHUNK: u8 = 1;

/// Which way the data goes from this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// This device sends.
    Send,
    /// This device receives.
    Receive,
}

/// Counts shown when a transfer finished.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Items new to the receiver.
    pub added: usize,
    /// Items that replaced an older copy (old copy in the trash).
    pub replaced: usize,
    /// Items the receiver kept because its copy was newer.
    pub kept_local: usize,
    /// Items already identical.
    pub identical: usize,
}

/// A control message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    /// First message from both sides.
    Hello {
        /// [`PROTOCOL`].
        v: u8,
        /// Device name (display only).
        name: String,
        /// Platform (display only).
        platform: Platform,
        /// What the sender of this message does.
        role: Role,
    },
    /// The bundle on offer.
    Offer {
        /// The bundle's manifest (the receiver checks the streamed bundle matches it exactly).
        manifest: Box<Manifest>,
        /// Exact byte size of the bundle file.
        bundle_size: u64,
        /// Lower-case hex SHA-256 of the bundle file.
        bundle_sha256: String,
        /// Display names by item key (characters' names; display only, never paths).
        #[serde(default)]
        labels: BTreeMap<String, String>,
    },
    /// Keep-alive while the receiving user decides.
    Waiting,
    /// Start streaming.
    Accept,
    /// The receiving user declined.
    Decline,
    /// All bytes sent.
    Done,
    /// Received, verified and applied.
    Finished {
        /// What happened.
        summary: Summary,
    },
    /// The receiver refused the bundle after receiving it.
    Failed {
        /// User-facing reason.
        reason: String,
    },
    /// The user cancelled.
    Cancel,
}

/// Encode a control frame.
pub fn control(msg: &Control) -> Vec<u8> {
    let mut out = vec![TAG_CONTROL];
    // Serializing these plain types cannot fail.
    out.extend(serde_json::to_vec(msg).unwrap_or_default());
    out
}

/// A decoded frame.
#[derive(Debug)]
pub enum Frame {
    /// A control message.
    Control(Control),
    /// Bundle bytes (without the tag).
    Chunk(Vec<u8>),
}

/// Decode a frame; control frames are capped at `max_control` bytes.
///
/// # Errors
/// [`TransferError::Protocol`] for an unknown tag, oversized or malformed control message.
pub fn decode(mut frame: Vec<u8>, max_control: usize) -> Result<Frame, TransferError> {
    match frame.first().copied() {
        Some(TAG_CHUNK) => {
            frame.remove(0);
            Ok(Frame::Chunk(frame))
        }
        Some(TAG_CONTROL) if frame.len() <= max_control + 1 => serde_json::from_slice(&frame[1..])
            .map(Frame::Control)
            .map_err(|e| TransferError::Protocol(format!("bad message: {e}"))),
        Some(TAG_CONTROL) => Err(TransferError::Refused("message too large".into())),
        _ => Err(TransferError::Protocol("unknown frame".into())),
    }
}

/// Map a transport error.
pub(crate) fn link(e: LinkError) -> TransferError {
    TransferError::Link(e.to_string())
}
