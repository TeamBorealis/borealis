//! `#[tauri::command]` wrappers for device transfer (B3, feature `device-link`). Status changes
//! stream on `borealis://link-status`; the UI (B4) shows the QR from `qrPayload`, the `code`,
//! the preview, progress and the result.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands_content::TauriSink;
use crate::content_packs::EventSink;
use crate::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents, EVENT_LINK_STATUS};
use crate::error::ApiResult;
use crate::state::AppState;
use crate::sync_dto::{LinkSelection, LinkStatus};

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
