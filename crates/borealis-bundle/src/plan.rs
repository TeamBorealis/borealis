//! Apply-on-receive planner (SY4: the most recently edited copy wins).
//!
//! Pure function of the bundle's manifest and the local inventory; it touches no files. The caller
//! applies the plan: write `ReplaceWithIncoming` and `New` items (after [`crate::Bundle::read_item`]
//! verified them), move the old local copy to `<data>/sync/trash/` first (30 days), and set the
//! written file's modification time to the item's `edited_at_ms` so the next comparison sees equal
//! stamps instead of "just edited".

use crate::inventory::LocalItem;
use crate::manifest::{ItemKind, Manifest, PackRef};
use std::collections::{BTreeMap, BTreeSet};

/// What to do with one incoming item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Not on this device: write it.
    New,
    /// Same digest: nothing to do.
    Identical,
    /// Different content and the incoming copy was edited later: the local file goes to the trash
    /// and the incoming one is written.
    ReplaceWithIncoming,
    /// Different content and the local copy was edited later: keep it, drop the incoming one.
    KeepLocalNewer,
    /// Different content with exactly equal edit stamps: keep the local copy and report it, since
    /// neither side can claim to be newer.
    KeepLocalTie,
}

impl Action {
    /// True when the incoming item is written to disk.
    #[must_use]
    pub fn writes(self) -> bool {
        matches!(self, Self::New | Self::ReplaceWithIncoming)
    }

    /// True when an existing local file is overwritten and so must go to the trash first.
    #[must_use]
    pub fn trashes_local(self) -> bool {
        self == Self::ReplaceWithIncoming
    }

    /// True when the user should be told (the incoming copy was discarded without being identical).
    #[must_use]
    pub fn needs_report(self) -> bool {
        matches!(self, Self::KeepLocalNewer | Self::KeepLocalTie)
    }
}

/// The decision for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEntry {
    /// Kind.
    pub kind: ItemKind,
    /// Key.
    pub key: String,
    /// Decision.
    pub action: Action,
    /// Incoming edit stamp (Unix ms).
    pub incoming_edited_at_ms: u64,
    /// Local edit stamp, when the item exists locally.
    pub local_edited_at_ms: Option<u64>,
    /// Packs a character needs that are neither in this bundle nor installed here.
    pub missing_packs: Vec<PackRef>,
}

/// The plan for a whole bundle, in manifest order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// One entry per incoming item.
    pub entries: Vec<PlanEntry>,
}

impl Plan {
    /// Entries whose incoming copy was discarded without being identical (for the status message).
    pub fn reported(&self) -> impl Iterator<Item = &PlanEntry> {
        self.entries.iter().filter(|e| e.action.needs_report())
    }
}

/// Decide one item. Rule: identical digest wins over everything (no-op); otherwise the larger
/// `edited_at_ms` wins; on an exact tie the local copy stays and the tie is reported.
#[must_use]
pub fn decide(incoming_sha: &str, incoming_ms: u64, local: Option<&LocalItem>) -> Action {
    let Some(local) = local else {
        return Action::New;
    };
    if local.sha256 == incoming_sha {
        return Action::Identical;
    }
    match incoming_ms.cmp(&local.edited_at_ms) {
        std::cmp::Ordering::Greater => Action::ReplaceWithIncoming,
        std::cmp::Ordering::Less => Action::KeepLocalNewer,
        std::cmp::Ordering::Equal => Action::KeepLocalTie,
    }
}

/// Plan the receive of `manifest` onto a device holding `local`. `installed_pack_ids` are the ids
/// of every pack available on this device (bundled and user), used only for `missing_packs`.
#[must_use]
pub fn plan_receive(
    manifest: &Manifest,
    local: &[LocalItem],
    installed_pack_ids: &BTreeSet<String>,
) -> Plan {
    let local_by_key: BTreeMap<(ItemKind, &str), &LocalItem> = local
        .iter()
        .map(|l| ((l.kind, l.key.as_str()), l))
        .collect();
    let bundle_packs: BTreeSet<&str> = manifest
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Pack)
        .map(|i| i.key.as_str())
        .collect();
    let entries = manifest
        .items
        .iter()
        .map(|item| {
            let found = local_by_key.get(&(item.kind, item.key.as_str())).copied();
            let missing_packs = item
                .required_packs
                .iter()
                .filter(|p| {
                    !bundle_packs.contains(p.id.as_str()) && !installed_pack_ids.contains(&p.id)
                })
                .cloned()
                .collect();
            PlanEntry {
                kind: item.kind,
                key: item.key.clone(),
                action: decide(&item.sha256, item.edited_at_ms, found),
                incoming_edited_at_ms: item.edited_at_ms,
                local_edited_at_ms: found.map(|l| l.edited_at_ms),
                missing_packs,
            }
        })
        .collect();
    Plan { entries }
}
