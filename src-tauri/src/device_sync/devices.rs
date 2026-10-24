//! Settings > Devices (C1): the paired-device list, Remove (revocation) and the 180-day expiry.
//!
//! The registry (`<data>/link/devices.json`, no secrets) names every device this one completed
//! a Send/Receive with; the pair key lives in the secure store as `peer-token.<id>`. Remove and
//! expiry drop both, so the device's certificate is no longer in
//! [`DeviceRegistry::allowed_fingerprints`] and any listener built from the registry refuses
//! it at the handshake. A removed device can come back only through a fresh QR or typed code
//! shown on this device (a new pairing the user starts).

use borealis_link::registry::DeviceRegistry;
use borealis_link::store::forget_peer;
use borealis_link::DeviceId;

use super::{now_ms, DeviceSync};
use crate::error::{ApiError, ApiResult};
use crate::sync_dto::LinkDevice;

impl DeviceSync {
    fn now_secs() -> u64 {
        now_ms() / 1000
    }

    /// Delete the pair keys of devices that left the registry (logged, never fatal: the
    /// registry already refuses them).
    fn forget_keys(&self, ids: &[DeviceId]) {
        if ids.is_empty() {
            return;
        }
        match self.store() {
            Ok(store) => {
                for id in ids {
                    if let Err(e) = forget_peer(store.as_ref(), id) {
                        tracing::warn!("device sync: could not delete the key of {id}: {e}");
                    }
                }
            }
            Err(e) => tracing::warn!("device sync: keys of removed devices kept: {e:?}"),
        }
    }

    /// Loads the registry and drops pairings past their 180-day expiry (saved at once).
    fn live_registry(&self) -> ApiResult<DeviceRegistry> {
        let data = &self.state.state().paths.data;
        let registry = DeviceRegistry::load(data)
            .map_err(|e| ApiError::io("read the list of paired devices", e))?;
        let (live, expired) = registry.pruned(Self::now_secs());
        if !expired.is_empty() {
            live.save(data)
                .map_err(|e| ApiError::io("update the list of paired devices", e))?;
            self.forget_keys(&expired);
        }
        Ok(live)
    }

    /// Paired devices, most recently synced first (expired pairings are removed first).
    ///
    /// # Errors
    /// `io_error` when the registry cannot be read or updated.
    pub fn devices(&self) -> ApiResult<Vec<LinkDevice>> {
        let mut list: Vec<LinkDevice> = self
            .live_registry()?
            .devices
            .iter()
            .map(LinkDevice::of)
            .collect();
        list.sort_by(|a, b| {
            b.last_synced_at
                .cmp(&a.last_synced_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(list)
    }

    /// Remove a device: its certificate is refused from now on and its pair key is deleted.
    /// Removing an unknown device is a no-op. Returns the remaining list.
    ///
    /// # Errors
    /// `invalid_argument` for a malformed id; `io_error` when the registry cannot be saved.
    pub fn forget_device(&self, device_id: &str) -> ApiResult<Vec<LinkDevice>> {
        let id = DeviceId::parse(device_id).ok_or_else(|| ApiError::invalid("Unknown device."))?;
        let data = &self.state.state().paths.data;
        let registry = self.live_registry()?;
        if registry.get(&id).is_some() {
            registry
                .without(&id)
                .save(data)
                .map_err(|e| ApiError::io("update the list of paired devices", e))?;
        }
        // Delete the key even when the entry was already gone (an earlier failed delete).
        self.forget_keys(&[id]);
        self.devices()
    }
}
