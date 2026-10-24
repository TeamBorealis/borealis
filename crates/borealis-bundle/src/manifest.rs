//! `manifest.json`: the table of contents and integrity record of a bundle.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The only format string this crate reads and writes.
pub const FORMAT: &str = "borealis-bundle/1";
/// Prefix shared by every format version, used to tell "newer bundle" from "not a bundle".
pub const FORMAT_PREFIX: &str = "borealis-bundle/";
/// Path of the manifest inside the zip.
pub const MANIFEST_PATH: &str = "manifest.json";
/// Path of the portable settings file inside the zip.
pub const SETTINGS_PATH: &str = "settings.portable.json";

/// Lower-case hex SHA-256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    /// A character file `characters/<key>.json`.
    Character,
    /// A user content pack folder `packs/<key>/...`.
    Pack,
}

/// One file inside the zip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Full path inside the zip, `/` separated.
    pub path: String,
    /// Lower-case hex SHA-256 of the file bytes.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
}

/// A pack a character needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PackRef {
    /// Pack folder name / id.
    pub id: String,
    /// True for packs that ship with every install (never in the bundle). False: a user pack,
    /// carried in the bundle when it was selected.
    pub bundled: bool,
}

/// One syncable thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Kind.
    pub kind: ItemKind,
    /// Stable key: the character file stem (`c_<uuid>`) or the pack folder name.
    pub key: String,
    /// Item digest, see [`item_digest`].
    pub sha256: String,
    /// Total bytes of the item's files.
    pub size: u64,
    /// When the item was last edited on the source device, Unix milliseconds. Source: the character
    /// file's modification time (the character format carries no edit stamp); for a pack, the newest
    /// file in it.
    pub edited_at_ms: u64,
    /// The files of the item.
    pub files: Vec<FileEntry>,
    /// Packs the character needs (characters only), sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_packs: Vec<PackRef>,
}

/// The whole manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT`] for files this crate writes.
    pub format: String,
    /// Borealis version that wrote the bundle.
    pub app_version: String,
    /// Creation time, Unix milliseconds.
    pub created_ms: u64,
    /// Name of the sending device (display only).
    pub source_device: String,
    /// Characters and packs, sorted by (kind, key).
    pub items: Vec<Item>,
    /// `settings.portable.json`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<FileEntry>,
    /// Reserved for campaigns (EM3, not built). Must be empty for now; readers refuse a non-empty
    /// list.
    #[serde(default)]
    pub campaigns: Vec<serde_json::Value>,
}

/// The digest of an item: the file's own SHA-256 for a single-file item, otherwise the SHA-256 of
/// `"<path>\0<sha256>\n"` for every file in path order. Local scans use the same function so equal
/// content gives equal digests on every device.
#[must_use]
pub fn item_digest(files: &[FileEntry]) -> String {
    if let [only] = files {
        return only.sha256.clone();
    }
    let mut sorted: Vec<&FileEntry> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut h = Sha256::new();
    for f in sorted {
        h.update(f.path.as_bytes());
        h.update([0]);
        h.update(f.sha256.as_bytes());
        h.update(b"\n");
    }
    hex(&h.finalize())
}
