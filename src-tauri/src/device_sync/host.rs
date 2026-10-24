//! The app side of a transfer: building the outgoing bundle and receiving into this device's
//! folders (plan, validate, apply, reload content).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use borealis_bundle::{
    apply_plan, build_bundle, is_plain_key, plan_receive, scan_characters, scan_packs, sweep_trash,
    trash_dir, ApplyReport, ApplyTargets, BuildRequest, Bundle, BundleMeta, CharacterSelection,
    Limits, Manifest, PackIndex, PackSelection, Plan,
};
use borealis_transfer::ReceiveHost;

use super::validate::AppValidator;
use super::{now_ms, StateRef, SyncEvents};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::sync_dto::{LinkPackChoice, LinkSelection};

/// `<data>/sync`.
pub fn sync_dir(state: &AppState) -> PathBuf {
    state.paths.data.join("sync")
}

/// A bundle file ready to send (deleted when dropped).
pub struct OutgoingBundle {
    /// The temp file.
    pub file: tempfile::NamedTempFile,
    /// Its manifest.
    pub manifest: Manifest,
    /// Character display names by key.
    pub labels: BTreeMap<String, String>,
}

fn character_label(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.pointer("/meta/name")?.as_str().map(str::to_string)
}

/// Build the bundle for `selection` into `<data>/sync/outgoing`.
///
/// # Errors
/// `invalid_argument` for unknown or unsafe ids, `io_error`.
pub fn build_outgoing(
    state: &AppState,
    selection: &LinkSelection,
    device_name: &str,
) -> ApiResult<OutgoingBundle> {
    if let Some(bad) = selection.character_ids.iter().find(|id| !is_plain_key(id)) {
        return Err(ApiError::invalid(format!(
            "\"{bad}\" is not a character id."
        )));
    }
    if selection.character_ids.is_empty() && selection.packs != LinkPackChoice::All {
        return Err(ApiError::invalid("Choose at least one character to send."));
    }
    let dir = sync_dir(state).join("outgoing");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("prepare the transfer", e))?;
    let file = tempfile::Builder::new()
        .prefix("send-")
        .suffix(".borealis")
        .tempfile_in(&dir)
        .map_err(|e| ApiError::io("prepare the transfer", e))?;
    let characters_dir = state.characters_dir();
    let library_dir = state.paths.library();
    let req = BuildRequest {
        characters_dir: &characters_dir,
        library_dir: &library_dir,
        settings_file: None,
        meta: BundleMeta {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source_device: device_name.to_string(),
            created_ms: now_ms(),
        },
        characters: CharacterSelection::Keys(selection.character_ids.clone()),
        packs: match selection.packs {
            LinkPackChoice::Required => PackSelection::Required,
            LinkPackChoice::All => PackSelection::All,
            LinkPackChoice::None => PackSelection::None,
        },
        bundled_index: PackIndex::new(),
        limits: Limits::default(),
    };
    let out = file
        .reopen()
        .map_err(|e| ApiError::io("prepare the transfer", e))?;
    let (_, manifest, _) = build_bundle(out, &req)
        .map_err(|e| ApiError::invalid(format!("Cannot prepare the transfer: {e}")))?;
    let labels = selection
        .character_ids
        .iter()
        .filter_map(|id| {
            let label = character_label(&characters_dir.join(format!("{id}.json")))?;
            Some((id.clone(), label))
        })
        .collect();
    Ok(OutgoingBundle {
        file,
        manifest,
        labels,
    })
}

/// Receives into the app's folders.
pub struct AppReceiver {
    /// The app state.
    pub state: Arc<dyn StateRef>,
    /// Where content reload events go.
    pub events: Arc<dyn SyncEvents>,
}

impl AppReceiver {
    fn targets(state: &AppState) -> ApplyTargets {
        let sync = sync_dir(state);
        ApplyTargets {
            characters_dir: state.characters_dir(),
            library_dir: state.paths.library(),
            trash_dir: trash_dir(&state.paths.data),
            staging_dir: sync.join("staging"),
        }
    }
}

impl ReceiveHost for AppReceiver {
    fn plan(&self, manifest: &Manifest) -> Result<Plan, String> {
        let state = self.state.state();
        let t = Self::targets(state);
        let mut local = scan_characters(&t.characters_dir).map_err(|e| e.to_string())?;
        local.extend(scan_packs(&t.library_dir, &Limits::default()).map_err(|e| e.to_string())?);
        let installed: BTreeSet<String> = state
            .hub()
            .map(|h| h.store.packs().iter().map(|p| p.id.clone()).collect())
            .unwrap_or_default();
        Ok(plan_receive(manifest, &local, &installed))
    }

    fn apply(
        &self,
        bundle: &mut Bundle<std::fs::File>,
        plan: &Plan,
    ) -> Result<ApplyReport, String> {
        let state = self.state.state();
        let t = Self::targets(state);
        let _ = sweep_trash(&t.trash_dir, now_ms());
        let report = {
            let _guard = state.content_lock().map_err(|e| e.message)?;
            apply_plan(bundle, plan, &t, &AppValidator).map_err(|e| e.to_string())?
        };
        let packs_changed = report
            .added
            .iter()
            .chain(&report.replaced)
            .any(|(k, _)| *k == borealis_bundle::ItemKind::Pack);
        if packs_changed {
            let prefs = state.content_prefs();
            if let Err(e) = state.reload_content(&prefs, self.events.content(), "device-sync") {
                tracing::warn!("device sync: content reload failed: {}", e.message);
            }
        }
        Ok(report)
    }

    fn incoming_dir(&self) -> PathBuf {
        sync_dir(self.state.state()).join("incoming")
    }
}
