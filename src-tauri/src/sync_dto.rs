//! Device transfer contract types (B3; camelCase on the wire). The UI (B4) shows them in the
//! "Send to device" / "Receive from device" dialogs; status changes stream on
//! `borealis://link-status`.

use serde::{Deserialize, Serialize};

/// Which user packs travel with the selected characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkPackChoice {
    /// The user packs the characters need (default, SY2).
    #[default]
    Required,
    /// Every user pack.
    All,
    /// None.
    None,
}

/// What to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSelection {
    /// Character ids (`c_...`); empty with `packs: all` sends packs only.
    pub character_ids: Vec<String>,
    /// Packs to include.
    #[serde(default)]
    pub packs: LinkPackChoice,
}

/// Stage of a transfer session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkPhase {
    /// Showing the QR / code, listening.
    Waiting,
    /// Connecting / authenticating.
    Connecting,
    /// Receiver: the preview is ready; Accept or Decline.
    Preview,
    /// Sender: waiting for the other user to accept.
    WaitingForPeer,
    /// Bytes are moving.
    Transferring,
    /// Receiver: verifying and writing.
    Applying,
    /// Finished.
    Done,
    /// Ended with an error (`error`).
    Failed,
    /// Cancelled on this device.
    Cancelled,
    /// The receiving user declined.
    Declined,
    /// The QR / code expired before anyone connected.
    Expired,
}

/// One preview row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreviewItem {
    /// `character` or `pack`.
    pub kind: String,
    /// Item id.
    pub key: String,
    /// Display name.
    pub label: String,
    /// `new`, `replace`, `keep_local` or `identical`.
    pub action: String,
    /// Bytes.
    pub size: u64,
    /// User packs a character needs that are missing here and not in the transfer.
    pub missing_packs: Vec<String>,
}

/// The receiver's preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    /// Rows in transfer order.
    pub items: Vec<LinkPreviewItem>,
    /// Bytes to transfer.
    pub total_bytes: u64,
}

/// Counts after a finished transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSummary {
    /// New on the receiver.
    pub added: usize,
    /// Replaced an older copy (old copy kept in the sync trash for 30 days).
    pub replaced: usize,
    /// Kept because the receiver's copy was newer.
    pub kept_local: usize,
    /// Already identical.
    pub identical: usize,
}

/// State of one Send/Receive session (also the `borealis://link-status` payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    /// Session id (opaque).
    pub session_id: String,
    /// `send` or `receive`: what this device does.
    pub role: String,
    /// Stage.
    pub phase: LinkPhase,
    /// Listening side: the `borealis-link:v1?...` string to show as a QR code.
    pub qr_payload: Option<String>,
    /// Listening side: the typed code (`42-ember-lantern`).
    pub code: Option<String>,
    /// Listening side: `ip:port` (typed next to the code where discovery is unavailable).
    pub address: Option<String>,
    /// This device's short certificate fingerprint, for a visual check.
    pub fingerprint: Option<String>,
    /// Seconds until the QR / code expires.
    pub expires_in_secs: Option<u64>,
    /// The other device's name (display only).
    pub peer_name: Option<String>,
    /// Receiver: what Accept would do.
    pub preview: Option<LinkPreview>,
    /// Bytes moved.
    pub bytes_done: Option<u64>,
    /// Bytes in total.
    pub bytes_total: Option<u64>,
    /// After `done`.
    pub summary: Option<LinkSummary>,
    /// Machine code after `failed` (`refused`, `connection`, `wrong_code`, ...).
    pub error_code: Option<String>,
    /// User-facing message after `failed`.
    pub error: Option<String>,
}

impl LinkStatus {
    /// A fresh status.
    #[must_use]
    pub fn new(session_id: &str, role: &str, phase: LinkPhase) -> Self {
        Self {
            session_id: session_id.to_string(),
            role: role.to_string(),
            phase,
            qr_payload: None,
            code: None,
            address: None,
            fingerprint: None,
            expires_in_secs: None,
            peer_name: None,
            preview: None,
            bytes_done: None,
            bytes_total: None,
            summary: None,
            error_code: None,
            error: None,
        }
    }
}

/// One paired device (Settings > Devices, C1). Times are unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDevice {
    /// Stable id derived from the device's certificate (16 hex characters).
    pub id: String,
    /// The name the device gave when pairing (display only).
    pub name: String,
    /// `macos`, `windows`, `linux`, `ios`, `android` or `unknown`.
    pub platform: borealis_link::Platform,
    /// First paired (or re-paired) at.
    pub paired_at: u64,
    /// Last successful Send/Receive with it.
    pub last_synced_at: u64,
    /// The pairing lapses at this time (180 days after the last transfer); re-pair by QR.
    pub expires_at: u64,
    /// Short certificate fingerprint (`9b12 89ce 4fd2 8eff`), the one that device shows for itself.
    pub fingerprint: String,
}

impl LinkDevice {
    /// The DTO of a registry entry.
    #[must_use]
    pub fn of(d: &borealis_link::PairedDevice) -> Self {
        Self {
            id: d.device_id.to_string(),
            name: d.name.clone(),
            platform: d.platform,
            paired_at: d.paired_at,
            last_synced_at: d.last_seen,
            expires_at: d.expires_at,
            fingerprint: d.fingerprint.short_hex(),
        }
    }
}

/// A `.borealis` file opened for preview (`bundle_open`): the same rows a device transfer shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePreview {
    /// Opaque id for `bundle_accept` / `bundle_discard`.
    pub bundle_id: String,
    /// The file's name (never a path).
    pub file_name: String,
    /// The device that made the bundle (display only).
    pub source_device: String,
    /// What Accept would do, one row per item.
    pub preview: LinkPreview,
}

/// Result of `bundle_export`: the file was written (desktop) or the save dialog was cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleExport {
    /// Suggested / written file name.
    pub file_name: String,
    /// Full path when the file was written; `None` when the user cancelled the dialog.
    pub path: Option<String>,
    /// Characters in the bundle.
    pub characters: usize,
    /// Content packs in the bundle.
    pub packs: usize,
    /// Size of the bundle file.
    pub bytes: u64,
}

/// Result of `bundle_share` (phones): Android opens the share sheet from the backend; elsewhere
/// the bytes come back for the web view's own share sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleShare {
    /// File name to share.
    pub file_name: String,
    /// Characters in the bundle.
    pub characters: usize,
    /// Content packs in the bundle.
    pub packs: usize,
    /// Size of the bundle file.
    pub bytes: u64,
    /// True when the backend already opened the system share sheet.
    pub shared: bool,
    /// The bundle, base64, when the UI has to share it (`shared` is false).
    pub data_base64: Option<String>,
}
