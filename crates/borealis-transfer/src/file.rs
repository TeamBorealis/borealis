//! A bundle that arrives as a file (Export to bundle, a share sheet, "Open with") instead of over
//! the wire: the same checks, the same preview rows and the same apply path as a device transfer,
//! only without a peer.

use std::collections::BTreeMap;
use std::path::Path;

use borealis_bundle::{ItemKind, Manifest};
use borealis_link::registry::clean_name;
use borealis_link::Platform;

use crate::error::TransferError;
use crate::limits::TransferLimits;
use crate::preview::{check_manifest, preview, Preview};
use crate::receive::{summary, verify_and_apply};
use crate::session::PeerInfo;
use crate::wire::Summary;
use crate::ReceiveHost;

fn refused(e: borealis_bundle::BundleError) -> TransferError {
    TransferError::Refused(crate::error::plain_reason(&e))
}

/// Display names of the characters in a bundle (`meta.name`), by key.
fn character_labels(
    bundle: &mut borealis_bundle::Bundle<std::fs::File>,
) -> Result<BTreeMap<String, String>, TransferError> {
    let keys: Vec<String> = bundle
        .manifest()
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Character)
        .map(|i| i.key.clone())
        .collect();
    let mut labels = BTreeMap::new();
    for key in keys {
        let data = bundle
            .read_item(ItemKind::Character, &key)
            .map_err(refused)?;
        let name = data
            .files
            .first()
            .and_then(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(bytes).ok())
            .and_then(|v| v.pointer("/meta/name")?.as_str().map(str::to_string));
        if let Some(name) = name {
            labels.insert(key, name);
        }
    }
    Ok(labels)
}

/// Open a bundle file, check it against the caps and plan it against this device.
///
/// Returns the manifest (hand it back to [`apply_bundle_file`] so a file swapped in between is
/// refused) and the preview rows.
///
/// # Errors
/// [`TransferError::Refused`] for anything that is not a sound bundle within the caps,
/// [`TransferError::Local`] when the file cannot be read or planned.
pub fn open_bundle_file(
    path: &Path,
    host: &dyn ReceiveHost,
    limits: &TransferLimits,
) -> Result<(Manifest, Preview), TransferError> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut bundle = borealis_bundle::Bundle::open_with(file, limits.bundle).map_err(refused)?;
    let manifest = bundle.manifest().clone();
    check_manifest(&manifest, size, limits)?;
    bundle.verify_all().map_err(refused)?;
    let labels = character_labels(&mut bundle)?;
    let plan = host.plan(&manifest).map_err(TransferError::Local)?;
    let peer = PeerInfo {
        name: clean_name(&manifest.source_device),
        platform: Platform::Unknown,
    };
    let rows = preview(&peer, &manifest, &plan, &labels, size);
    Ok((manifest, rows))
}

/// Apply a bundle file whose `previewed` manifest the user accepted: verify it again, re-plan and
/// apply through the host (validation, trash, edit stamps).
///
/// # Errors
/// [`TransferError::Refused`] when the file changed since the preview or fails a check; nothing is
/// written then.
pub fn apply_bundle_file(
    path: &Path,
    previewed: &Manifest,
    host: &dyn ReceiveHost,
    limits: &TransferLimits,
) -> Result<Summary, TransferError> {
    let file = std::fs::File::open(path)?;
    let report = verify_and_apply(file, previewed, host, limits)?;
    Ok(summary(&report))
}
