//! Offer validation and the receiver's preview (what Accept would do, per A1's planner).

use std::collections::BTreeMap;

use borealis_bundle::{is_plain_key, Action, ItemKind, Manifest, Plan, FORMAT};
use borealis_link::registry::clean_name;

use crate::error::TransferError;
use crate::limits::TransferLimits;
use crate::session::PeerInfo;

/// What Accept does with one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewAction {
    /// Not on this device yet.
    New,
    /// Replaces an older local copy (which goes to the trash for 30 days).
    Replace,
    /// The local copy is newer (or tied) and stays.
    KeepLocal,
    /// Already identical.
    Identical,
}

impl From<Action> for PreviewAction {
    fn from(a: Action) -> Self {
        match a {
            Action::New => Self::New,
            Action::ReplaceWithIncoming => Self::Replace,
            Action::KeepLocalNewer | Action::KeepLocalTie => Self::KeepLocal,
            Action::Identical => Self::Identical,
        }
    }
}

/// One row of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewItem {
    /// Character or pack.
    pub kind: ItemKind,
    /// Item key (plain id).
    pub key: String,
    /// Display name the sender gave (cleaned), or the key.
    pub label: String,
    /// Decision.
    pub action: PreviewAction,
    /// Bytes.
    pub size: u64,
    /// Packs a character needs that neither the bundle nor this device has.
    pub missing_packs: Vec<String>,
}

/// The receiver's preview of an offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The sending device.
    pub peer: PeerInfo,
    /// Rows in manifest order.
    pub items: Vec<PreviewItem>,
    /// Bundle bytes to transfer.
    pub total_bytes: u64,
}

fn refused(why: impl Into<String>) -> TransferError {
    TransferError::Refused(why.into())
}

/// Check an offer against the caps before anything is planned or streamed.
///
/// # Errors
/// [`TransferError::Refused`].
pub fn check_offer(
    manifest: &Manifest,
    bundle_size: u64,
    bundle_sha256: &str,
    labels: &BTreeMap<String, String>,
    limits: &TransferLimits,
) -> Result<(), TransferError> {
    if bundle_sha256.len() != 64
        || !bundle_sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(refused("bad checksum"));
    }
    if labels.len() > manifest.items.len() {
        return Err(refused("too many items"));
    }
    check_manifest(manifest, bundle_size, limits)
}

/// The caps a bundle must meet whether it arrives over the wire or as a file: format, no
/// campaigns, size, item count, plain keys and per-item sizes.
///
/// # Errors
/// [`TransferError::Refused`].
pub fn check_manifest(
    manifest: &Manifest,
    bundle_size: u64,
    limits: &TransferLimits,
) -> Result<(), TransferError> {
    if manifest.format != FORMAT {
        return Err(refused("unsupported bundle format"));
    }
    if !manifest.campaigns.is_empty() {
        return Err(refused("campaigns are not supported yet"));
    }
    if bundle_size == 0 || bundle_size > limits.max_bundle_bytes {
        return Err(refused("the bundle is too large"));
    }
    if manifest.items.len() > limits.bundle.max_items {
        return Err(refused("too many items"));
    }
    let mut total = 0u64;
    for item in &manifest.items {
        if !is_plain_key(&item.key) {
            return Err(refused("an item has an unsafe name"));
        }
        let files: u64 = item.files.iter().map(|f| f.size).sum();
        let file_too_big = item
            .files
            .iter()
            .any(|f| f.size > limits.bundle.max_file_bytes);
        if files != item.size || item.size > limits.max_item_bytes || file_too_big {
            return Err(refused("an item is too large"));
        }
        total = total.saturating_add(item.size);
    }
    if total > limits.bundle.max_total_bytes {
        return Err(refused("the bundle is too large"));
    }
    Ok(())
}

/// Build the preview rows from the plan.
pub fn preview(
    peer: &PeerInfo,
    manifest: &Manifest,
    plan: &Plan,
    labels: &BTreeMap<String, String>,
    bundle_size: u64,
) -> Preview {
    let sizes: BTreeMap<(ItemKind, &str), u64> = manifest
        .items
        .iter()
        .map(|i| ((i.kind, i.key.as_str()), i.size))
        .collect();
    let items = plan
        .entries
        .iter()
        .map(|e| PreviewItem {
            kind: e.kind,
            key: e.key.clone(),
            label: labels
                .get(&e.key)
                .map_or_else(|| e.key.clone(), |l| clean_name(l)),
            action: e.action.into(),
            size: sizes.get(&(e.kind, e.key.as_str())).copied().unwrap_or(0),
            // Peer-supplied names: cleaned like every other label before they are shown.
            missing_packs: e.missing_packs.iter().map(|p| clean_name(&p.id)).collect(),
        })
        .collect();
    Preview {
        peer: peer.clone(),
        items,
        total_bytes: bundle_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_bundle::{PackRef, PlanEntry};
    use borealis_link::Platform;

    #[test]
    fn peer_supplied_missing_pack_names_are_cleaned() {
        let manifest = Manifest {
            format: FORMAT.into(),
            app_version: "1".into(),
            created_ms: 0,
            source_device: "x".into(),
            items: Vec::new(),
            settings: None,
            campaigns: Vec::new(),
        };
        let plan = Plan {
            entries: vec![PlanEntry {
                kind: ItemKind::Character,
                key: "c_a".into(),
                action: Action::New,
                incoming_edited_at_ms: 1,
                local_edited_at_ms: None,
                missing_packs: vec![PackRef {
                    id: format!("evil\u{202E}\n{}", "x".repeat(500)),
                    bundled: false,
                }],
            }],
        };
        let peer = PeerInfo {
            name: "Mac".into(),
            platform: Platform::Macos,
        };
        let p = preview(&peer, &manifest, &plan, &BTreeMap::new(), 1);
        let shown = &p.items[0].missing_packs[0];
        assert!(shown.starts_with("evilxx"), "{shown}");
        assert!(shown.chars().count() <= borealis_link::registry::MAX_NAME_CHARS);
    }
}
