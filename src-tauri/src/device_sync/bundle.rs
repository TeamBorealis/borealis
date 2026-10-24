//! Stage 0 (A2): bundles as files. Export writes the same `borealis-bundle/1` a device transfer
//! sends; opening one (File > Open bundle, a drop, the OS "open with") previews it with the
//! receiver's planner and applies it through the same validated apply path as a transfer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borealis_bundle::{ItemKind, Manifest};
use borealis_transfer::{apply_bundle_file, open_bundle_file, TransferError, TransferLimits};

use super::host::{build_outgoing, sync_dir, AppReceiver, OutgoingBundle};
use super::run::preview_dto;
use super::{lock, DeviceSync};
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::picked_paths::PickPurpose;
use crate::sync_dto::{BundleExport, BundlePreview, LinkSelection, LinkSummary};

/// Bundles open in the preview at once (the dialog shows one; a burst of opens queues the rest).
const MAX_OPEN_BUNDLES: usize = 8;

/// A previewed bundle waiting for Accept.
pub(crate) struct OpenBundle {
    path: PathBuf,
    manifest: Manifest,
    /// The file is our own copy (an Android share, an iOS inbox file): delete it when done.
    temp: bool,
}

/// The previews of the bundles currently open.
pub(crate) type OpenBundles = Mutex<HashMap<String, OpenBundle>>;

/// Folder for copies of bundles that arrived as bytes (Android intents).
pub fn incoming_dir(state: &crate::state::AppState) -> PathBuf {
    sync_dir(state).join("incoming")
}

/// Files in our incoming folder or in the iOS `Inbox` are ours to delete after use; a file the
/// user picked or dropped is never touched.
fn is_temp_copy(state: &crate::state::AppState, path: &Path) -> bool {
    path.starts_with(incoming_dir(state))
        || path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == "Inbox")
}

fn plain(e: TransferError) -> ApiError {
    match e {
        TransferError::Refused(why) => ApiError::new(
            ErrorCode::InvalidArgument,
            format!("This bundle cannot be used: {why}. Nothing was changed."),
        ),
        TransferError::Local(why) => ApiError::new(ErrorCode::IoError, why),
        other => ApiError::new(ErrorCode::IoError, other.to_string()),
    }
}

fn counts(manifest: &Manifest) -> (usize, usize) {
    let of = |k: ItemKind| manifest.items.iter().filter(|i| i.kind == k).count();
    (of(ItemKind::Character), of(ItemKind::Pack))
}

fn new_id() -> ApiResult<String> {
    borealis_link::random::random_bytes::<8>()
        .map(|b| format!("b_{}", borealis_link::random::hex(&b)))
        .map_err(ApiError::internal)
}

fn forget_temp(open: &OpenBundle) {
    if open.temp {
        let _ = std::fs::remove_file(&open.path);
    }
}

impl DeviceSync {
    fn receiver(&self) -> AppReceiver {
        AppReceiver {
            state: std::sync::Arc::clone(&self.state),
            events: std::sync::Arc::clone(&self.events),
        }
    }

    /// Build the bundle for `selection` (a temp file under `<data>/sync/outgoing`).
    ///
    /// # Errors
    /// Invalid selection, `io_error`.
    pub fn build_bundle_file(&self, selection: &LinkSelection) -> ApiResult<OutgoingBundle> {
        build_outgoing(self.state.state(), selection, &self.config.device_name)
    }

    /// Write a built bundle to `dest` (chosen in a save dialog the backend opened). The file
    /// appears whole or not at all.
    ///
    /// # Errors
    /// `io_error`.
    pub fn save_bundle(built: &OutgoingBundle, dest: &Path) -> ApiResult<BundleExport> {
        write_atomically(built.file.path(), dest)?;
        let mut done = export_dto(built, Some(dest.display().to_string()));
        if let Some(name) = dest.file_name() {
            done.file_name = name.to_string_lossy().into_owned();
        }
        Ok(done)
    }

    /// Open the `bundle-file` pick `token` and preview what Accept would do.
    ///
    /// # Errors
    /// Unknown or expired token; a file that is not a sound bundle within the caps (plain message,
    /// nothing is written).
    pub fn open_bundle(&self, token: &str) -> ApiResult<BundlePreview> {
        let state = self.state.state();
        let path = state.picked.take(token, PickPurpose::BundleFile)?;
        let temp = is_temp_copy(state, &path);
        let file_name = crate::picked_paths::file_name(&path);
        let id = new_id()?;
        let host = self.receiver();
        let opened = open_bundle_file(&path, &host, &TransferLimits::default());
        let (manifest, preview) = match opened {
            Ok(ok) => ok,
            Err(e) => {
                if temp {
                    let _ = std::fs::remove_file(&path);
                }
                return Err(plain(e));
            }
        };
        let dto = BundlePreview {
            bundle_id: id.clone(),
            file_name,
            source_device: preview.peer.name.clone(),
            preview: preview_dto(&preview),
        };
        let mut open = lock(&self.bundles);
        if open.len() >= MAX_OPEN_BUNDLES {
            if let Some(old) = open.keys().next().cloned().and_then(|k| open.remove(&k)) {
                forget_temp(&old);
            }
        }
        open.insert(
            id,
            OpenBundle {
                path,
                manifest,
                temp,
            },
        );
        Ok(dto)
    }

    /// Accept a previewed bundle: verify it again, then apply it (validation, old copies to the
    /// sync trash, edit stamps) exactly as a device transfer does.
    ///
    /// # Errors
    /// `not_found` for an unknown id; a plain message when the file changed or fails a check
    /// (nothing is written then).
    pub fn accept_bundle(&self, bundle_id: &str) -> ApiResult<LinkSummary> {
        let open = lock(&self.bundles)
            .remove(bundle_id)
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "That bundle is no longer open."))?;
        let host = self.receiver();
        let result = apply_bundle_file(
            &open.path,
            &open.manifest,
            &host,
            &TransferLimits::default(),
        );
        forget_temp(&open);
        let s = result.map_err(plain)?;
        Ok(LinkSummary {
            added: s.added,
            replaced: s.replaced,
            kept_local: s.kept_local,
            identical: s.identical,
        })
    }

    /// Close a previewed bundle without applying it. Unknown ids are ignored.
    pub fn discard_bundle(&self, bundle_id: &str) {
        if let Some(open) = lock(&self.bundles).remove(bundle_id) {
            forget_temp(&open);
        }
    }
}

/// A short name for the save dialog: the character's name for one, else a generic one.
#[must_use]
pub fn suggested_file_name(built: &OutgoingBundle) -> String {
    let (characters, _) = counts(&built.manifest);
    let base = match (characters, built.labels.values().next()) {
        (1, Some(name)) => name.clone(),
        _ => "Borealis characters".to_string(),
    };
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.borealis", safe.trim())
}

/// The result DTO for a built bundle.
#[must_use]
pub fn export_dto(built: &OutgoingBundle, path: Option<String>) -> BundleExport {
    let (characters, packs) = counts(&built.manifest);
    BundleExport {
        file_name: suggested_file_name(built),
        path,
        characters,
        packs,
        bytes: std::fs::metadata(built.file.path()).map_or(0, |m| m.len()),
    }
}

/// Copy `src` to `dest` through a sibling `.part` file so the destination is never half written.
fn write_atomically(src: &Path, dest: &Path) -> ApiResult<()> {
    let name = dest
        .file_name()
        .ok_or_else(|| ApiError::invalid("That location cannot be used."))?
        .to_string_lossy()
        .into_owned();
    let part = dest.with_file_name(format!("{name}.part"));
    let copied = std::fs::copy(src, &part).and_then(|_| std::fs::rename(&part, dest));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(ApiError::io("save the bundle", e));
    }
    Ok(())
}
