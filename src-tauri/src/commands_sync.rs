//! `#[tauri::command]` wrappers for device transfer (B3, feature `device-link`). Status changes
//! stream on `borealis://link-status`; the UI (B4) shows the QR from `qrPayload`, the `code`,
//! the preview, progress and the result.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands_content::TauriSink;
use crate::content_packs::EventSink;
use crate::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents, EVENT_LINK_STATUS};
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::{PickPurpose, PickedPathDto};
use crate::state::AppState;
use crate::sync_dto::{
    BundleExport, BundlePreview, BundleShare, LinkDevice, LinkSelection, LinkStatus, LinkSummary,
};

type S<'a> = State<'a, Arc<DeviceSync>>;

impl StateRef for AppHandle {
    fn state(&self) -> &AppState {
        Manager::state::<AppState>(self).inner()
    }
}

/// Emits session updates and content reloads through the Tauri handle.
pub struct TauriSyncEvents(pub TauriSink);

impl SyncEvents for TauriSyncEvents {
    fn status(&self, status: &LinkStatus) {
        let _ = self.0 .0.emit(EVENT_LINK_STATUS, status);
    }
    fn content(&self) -> &dyn EventSink {
        &self.0
    }
}

/// A short name other devices see in the preview ("Borealis on Mac").
fn device_name() -> String {
    let what = match borealis_link::Platform::current() {
        borealis_link::Platform::Macos => "Mac",
        borealis_link::Platform::Windows => "Windows PC",
        borealis_link::Platform::Linux => "Linux PC",
        borealis_link::Platform::Ios => "iPhone or iPad",
        borealis_link::Platform::Android => "Android device",
        borealis_link::Platform::Unknown => "device",
    };
    format!("Borealis on {what}")
}

/// Create the managed service (call after `AppState` is managed).
pub fn install(handle: &AppHandle) {
    let events = Arc::new(TauriSyncEvents(TauriSink(handle.clone())));
    let config = SyncConfig {
        bind_ip: None,
        device_name: device_name(),
        discovery: cfg!(not(any(target_os = "android", target_os = "ios"))),
    };
    let sync = DeviceSync::new(Arc::new(handle.clone()), events, config, || {
        borealis_link::store::PlatformStore::open()
            .map(|s| Arc::new(s) as Arc<dyn borealis_link::SecureStore>)
            .map_err(|e| e.to_string())
    });
    handle.manage(Arc::new(sync));
}

#[tauri::command]
pub async fn link_start_send(sync: S<'_>, selection: LinkSelection) -> ApiResult<LinkStatus> {
    sync.start_send(selection).await
}

#[tauri::command]
pub async fn link_start_receive(sync: S<'_>) -> ApiResult<LinkStatus> {
    sync.start_receive().await
}

#[tauri::command]
pub async fn link_connect(
    sync: S<'_>,
    target: String,
    host: Option<String>,
    selection: Option<LinkSelection>,
) -> ApiResult<LinkStatus> {
    sync.connect(&target, host.as_deref(), selection).await
}

#[tauri::command]
pub fn link_status(sync: S<'_>, session_id: String) -> ApiResult<LinkStatus> {
    sync.status(&session_id)
}

#[tauri::command]
pub fn link_accept(sync: S<'_>, session_id: String) -> ApiResult<LinkStatus> {
    sync.decide(&session_id, true)
}

#[tauri::command]
pub fn link_decline(sync: S<'_>, session_id: String) -> ApiResult<LinkStatus> {
    sync.decide(&session_id, false)
}

#[tauri::command]
pub fn link_cancel(sync: S<'_>, session_id: String) -> ApiResult<LinkStatus> {
    sync.cancel(&session_id)
}

/// File > Export to bundle...: build the bundle, then ask where to save it in a native dialog the
/// backend opens (desktop). A cancelled dialog answers no `path`; nothing is written then.
#[cfg(desktop)]
#[tauri::command(async)]
pub fn bundle_export(
    app: AppHandle,
    sync: S<'_>,
    selection: LinkSelection,
) -> ApiResult<BundleExport> {
    use tauri_plugin_dialog::DialogExt;
    let built = sync.build_bundle_file(&selection)?;
    let name = crate::device_sync::suggested_file_name(&built);
    let picked = app
        .dialog()
        .file()
        .add_filter("Borealis bundle", &[crate::picked_files::BUNDLE_EXTENSION])
        .set_file_name(&name)
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(crate::device_sync::export_dto(&built, None));
    };
    let mut dest = picked
        .into_path()
        .map_err(|_| ApiError::invalid("That location cannot be used."))?;
    if dest.extension().is_none() {
        dest.set_extension(crate::picked_files::BUNDLE_EXTENSION);
    }
    DeviceSync::save_bundle(&built, &dest)
}

/// Phones have no save dialog: sharing is the export (`bundle_share`).
#[cfg(not(desktop))]
#[tauri::command(async)]
pub fn bundle_export(_sync: S<'_>, _selection: LinkSelection) -> ApiResult<BundleExport> {
    Err(ApiError::invalid("Use Share as a file on this device."))
}

/// Most bytes of a bundle handed to the web view's own share sheet (iOS) as base64.
const MAX_WEB_SHARE_BYTES: u64 = 48 << 20;

/// Phones: build the bundle and open the system share sheet. Android opens it from the backend
/// (`ACTION_SEND` + `FileProvider`); elsewhere the bytes come back for the web view's share sheet.
#[tauri::command(async)]
pub fn bundle_share(
    app: AppHandle,
    sync: S<'_>,
    selection: LinkSelection,
) -> ApiResult<BundleShare> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let built = sync.build_bundle_file(&selection)?;
    let info = crate::device_sync::export_dto(&built, None);
    let path = built.file.path().to_string_lossy().into_owned();
    let shared = crate::native_share::share_file(
        &app,
        &crate::native_share::ShareFileArgs {
            path: &path,
            file_name: &info.file_name,
            mime: "application/octet-stream",
            title: "Borealis bundle",
        },
    )?;
    let mut out = BundleShare {
        file_name: info.file_name,
        characters: info.characters,
        packs: info.packs,
        bytes: info.bytes,
        shared,
        data_base64: None,
    };
    if !shared {
        if info.bytes > MAX_WEB_SHARE_BYTES {
            return Err(ApiError::invalid(
                "This bundle is too large to share from here. Use Send to device instead, or choose fewer items.",
            ));
        }
        let bytes =
            std::fs::read(built.file.path()).map_err(|e| ApiError::io("prepare the bundle", e))?;
        out.data_base64 = Some(STANDARD.encode(bytes));
    }
    Ok(out)
}

/// Preview a `.borealis` file (a `bundle-file` token from `pick_path`, a drop or the OS): what
/// Accept would add, replace or keep. Nothing is written.
#[tauri::command(async)]
pub fn bundle_open(sync: S<'_>, token: String) -> ApiResult<BundlePreview> {
    sync.open_bundle(&token)
}

/// Apply a previewed bundle through the same validated apply path as a device transfer.
#[tauri::command(async)]
pub fn bundle_accept(sync: S<'_>, bundle_id: String) -> ApiResult<LinkSummary> {
    sync.accept_bundle(&bundle_id)
}

/// Close a previewed bundle without applying it.
#[tauri::command]
pub fn bundle_discard(sync: S<'_>, bundle_id: String) {
    sync.discard_bundle(&bundle_id);
}

/// `.borealis` files the OS opened with the app since the last call, as `bundle-file` tokens.
/// Call after `take_opened_files` (which sets aside bundles that arrived as bytes).
#[tauri::command]
pub fn take_opened_bundles(state: State<'_, AppState>) -> ApiResult<Vec<PickedPathDto>> {
    crate::device_sync::inbox::take_opened_paths()
        .into_iter()
        .map(|p| state.picked.grant(p, PickPurpose::BundleFile))
        .collect()
}

/// Settings > Devices: paired devices, most recently synced first (expired ones are dropped).
#[tauri::command]
pub fn link_devices(sync: S<'_>) -> ApiResult<Vec<LinkDevice>> {
    sync.devices()
}

/// Settings > Devices > Remove: the device is refused from now on (re-pair by QR or code).
#[tauri::command]
pub fn link_forget_device(sync: S<'_>, device_id: String) -> ApiResult<Vec<LinkDevice>> {
    sync.forget_device(&device_id)
}
