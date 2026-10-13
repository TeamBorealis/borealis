//! Desktop-only LAN AI host (TLS with a pinned self-signed certificate, pairing string, token
//! revocation). Mobile builds compile this file without the host: every call answers
//! `supported: false` / `invalid_argument`.

use crate::ai_dto::AiHostStatus;
use crate::ai_service::AiService;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop {
    use super::*;
    use crate::ai_dto::HostDeviceDto;
    use crate::error::ErrorCode;
    use borealis_ai::host::{
        write_private_file, AiHost, DeviceRecord, HostConfig, HostHandle, HostIdentity, PairingCode,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Instant;

    const DEVICES_FILE: &str = "ai-host.json";

    /// A running host plus the pairing window we opened.
    pub struct Hosting {
        handle: HostHandle,
        /// The open window. `PairingCode::uri` is the `borealis-pair:v2?...` string the phone
        /// needs (QR + copyable text).
        pairing: Option<(PairingCode, Instant)>,
    }

    /// Holder inside the service.
    #[derive(Default)]
    pub struct HostSlot(pub tokio::sync::Mutex<Option<Hosting>>);

    fn devices_path(state: &AppState) -> PathBuf {
        state.paths.data.join(DEVICES_FILE)
    }

    fn load_devices(state: &AppState) -> Vec<DeviceRecord> {
        std::fs::read_to_string(devices_path(state))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Atomic (tmp + fsync + rename), mode 0600 on unix. Holds token hashes only.
    fn write_devices(path: &Path, devices: &[DeviceRecord]) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(devices).map_err(ApiError::internal)?;
        write_private_file(path, text.as_bytes())
            .map_err(|e| ApiError::io("save paired devices", e))
    }

    fn save_devices(state: &AppState, devices: &[DeviceRecord]) -> ApiResult<()> {
        write_devices(&devices_path(state), devices)
    }

    fn status_of(h: &Hosting) -> AiHostStatus {
        let open = h.pairing.as_ref().and_then(|(code, at)| {
            code.valid_for
                .checked_sub(at.elapsed())
                .filter(|d| !d.is_zero())
                .map(|d| (code.code.clone(), d.as_secs(), code.uri.to_string()))
        });
        let pairing_uri = open.as_ref().map(|(_, _, uri)| uri.clone());
        AiHostStatus {
            enabled: true,
            supported: true,
            address: Some(h.handle.public_addr().to_string()),
            pairing_code: open.as_ref().map(|(c, _, _)| c.clone()),
            pairing_expires_in_secs: open.map(|(_, s, _)| s),
            pairing_uri,
            fingerprint: Some(h.handle.fingerprint().short_hex()),
            devices: h
                .handle
                .devices()
                .into_iter()
                .map(|d| HostDeviceDto {
                    id: d.id,
                    name: d.name,
                })
                .collect(),
        }
    }

    fn off() -> AiHostStatus {
        AiHostStatus {
            enabled: false,
            supported: true,
            address: None,
            pairing_code: None,
            pairing_expires_in_secs: None,
            pairing_uri: None,
            fingerprint: None,
            devices: Vec::new(),
        }
    }

    fn open_pairing(h: &mut Hosting) -> ApiResult<()> {
        let code = h
            .handle
            .begin_pairing()
            .map_err(|e| ApiError::new(ErrorCode::AiUnavailable, e.to_string()))?;
        h.pairing = Some((code, Instant::now()));
        Ok(())
    }

    /// Drop a host that stopped by itself (idle timeout), saving its devices.
    fn reap_stopped(state: &AppState, slot: &mut Option<Hosting>) -> ApiResult<()> {
        if slot.as_ref().is_some_and(|h| !h.handle.is_running()) {
            if let Some(h) = slot.take() {
                save_devices(state, &h.handle.devices())?;
            }
        }
        Ok(())
    }

    impl AiService {
        /// Start the LAN host (idempotent) and open a pairing window.
        ///
        /// # Errors
        /// `ai_unavailable` without a local provider, without a LAN address, or when the socket
        /// or TLS identity cannot be set up.
        pub async fn host_enable(
            &self,
            state: &AppState,
            allow_untrusted_with_codex: bool,
        ) -> ApiResult<AiHostStatus> {
            let mut slot = self.host.0.lock().await;
            reap_stopped(state, &mut slot)?;
            if let Some(h) = slot.as_ref() {
                return Ok(status_of(h));
            }
            let runner = self
                .host_runner(state, allow_untrusted_with_codex)
                .await
                .map_err(|e| {
                    if e.code == ErrorCode::AiUnavailable {
                        ApiError::new(
                            ErrorCode::AiUnavailable,
                            "Install the claude or codex CLI before sharing AI with other devices.",
                        )
                    } else {
                        e
                    }
                })?;
            let unavailable =
                |e: borealis_ai::AiError| ApiError::new(ErrorCode::AiUnavailable, e.to_string());
            let identity = HostIdentity::load_or_create(&state.paths.data).map_err(unavailable)?;
            let handle = AiHost::start(
                HostConfig::default(),
                &identity,
                runner,
                None,
                load_devices(state),
            )
            .await
            .map_err(unavailable)?;
            // Persist every change (pair, unpair, revoke, expiry) immediately.
            let path = devices_path(state);
            handle.set_devices_hook(Arc::new(move |devices| {
                if let Err(e) = write_devices(&path, devices) {
                    tracing::warn!("persisting paired devices failed: {}", e.message);
                }
            }));
            let mut hosting = Hosting {
                handle,
                pairing: None,
            };
            open_pairing(&mut hosting)?;
            let status = status_of(&hosting);
            *slot = Some(hosting);
            Ok(status)
        }

        /// Stop the host; paired devices stay saved.
        pub async fn host_disable(&self, state: &AppState) -> ApiResult<AiHostStatus> {
            let taken = self.host.0.lock().await.take();
            if let Some(h) = taken {
                save_devices(state, &h.handle.devices())?;
                h.handle.shutdown().await;
            }
            Ok(off())
        }

        /// Current host state (reports `enabled: false` once the host stopped for being idle).
        pub async fn host_status(&self, state: &AppState) -> ApiResult<AiHostStatus> {
            let mut slot = self.host.0.lock().await;
            reap_stopped(state, &mut slot)?;
            Ok(slot.as_ref().map_or_else(off, status_of))
        }

        /// Open a fresh pairing window (replaces any earlier code).
        pub async fn host_begin_pairing(&self) -> ApiResult<AiHostStatus> {
            let mut slot = self.host.0.lock().await;
            if slot.as_ref().is_some_and(|h| !h.handle.is_running()) {
                *slot = None; // stopped while idle; its devices were persisted by the hook
            }
            let h = slot
                .as_mut()
                .ok_or_else(|| ApiError::invalid("Turn the AI host on first."))?;
            open_pairing(h)?;
            Ok(status_of(h))
        }

        /// Revoke a paired device's token (persisted immediately). Returns the new status.
        pub async fn host_revoke(
            &self,
            state: &AppState,
            device_id: &str,
        ) -> ApiResult<AiHostStatus> {
            let slot = self.host.0.lock().await;
            let h = slot
                .as_ref()
                .ok_or_else(|| ApiError::invalid("Turn the AI host on first."))?;
            if !h.handle.revoke(device_id) {
                return Err(ApiError::not_found("No such paired device."));
            }
            // The hook already wrote the file; write again to surface any error to the caller.
            save_devices(state, &h.handle.devices())?;
            Ok(status_of(h))
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::HostSlot;

/// Phones and tablets never host.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Default)]
pub struct HostSlot;

#[cfg(any(target_os = "android", target_os = "ios"))]
impl AiService {
    fn no_host() -> ApiError {
        ApiError::invalid("The AI host runs on the desktop app only.")
    }

    /// Unsupported on mobile.
    pub async fn host_enable(
        &self,
        _state: &AppState,
        _allow_untrusted_with_codex: bool,
    ) -> ApiResult<AiHostStatus> {
        Err(Self::no_host())
    }
    /// Unsupported on mobile.
    pub async fn host_disable(&self, _state: &AppState) -> ApiResult<AiHostStatus> {
        Err(Self::no_host())
    }
    /// Never enabled on mobile.
    pub async fn host_status(&self, _state: &AppState) -> ApiResult<AiHostStatus> {
        Ok(AiHostStatus {
            enabled: false,
            supported: false,
            address: None,
            pairing_code: None,
            pairing_expires_in_secs: None,
            pairing_uri: None,
            fingerprint: None,
            devices: Vec::new(),
        })
    }
    /// Unsupported on mobile.
    pub async fn host_begin_pairing(&self) -> ApiResult<AiHostStatus> {
        Err(Self::no_host())
    }
    /// Unsupported on mobile.
    pub async fn host_revoke(
        &self,
        _state: &AppState,
        _device_id: &str,
    ) -> ApiResult<AiHostStatus> {
        Err(Self::no_host())
    }
}
