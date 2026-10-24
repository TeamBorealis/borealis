//! Device facts for semantic search (local-search-spec sec. 7, S3.1): the tier probe's RAM figure and the
//! battery / thermal / low-power / background state the indexing scheduler pauses on.
//!
//! One managed [`SemanticDevice`] per app, installed by the `borealis-device` plugin ([`init`]):
//! * desktop: `borealis_embed::device::DesktopDevice` (always allowed per the tier table), RAM via `sysinfo`;
//! * iOS: `borealis_embed::device::ios::IosDevice` (`ProcessInfo` + `UIDevice`, safe objc2 calls in the crate);
//! * Android: the in-app Kotlin `DeviceStatePlugin` (same pattern as `SharePlugin`, see `native_share`), which
//!   answers `ActivityManager.MemoryInfo.totalMem`, battery, Battery Saver and thermal status. If a plugin call
//!   fails the state is `DeviceState::UNKNOWN_MOBILE` (treated as on battery: T1/T2 do not index) and RAM falls
//!   back to `/proc/meminfo`.
//!
//! Background / foreground comes from `WindowEvent::Suspended` / `Resumed` on mobile ([`on_window_event`]).
//! Nothing here leaves the device. Call [`SemanticDevice::profile`] and the provider off the main thread: on
//! Android they block on a plugin call.

use std::sync::{Arc, OnceLock};

use borealis_embed::tier::effective_tier;
use borealis_embed::{DeviceProfile, DeviceStateProvider, Lifecycle, Tier, TierOverride};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

type RamShim = Box<dyn Fn() -> Option<u64> + Send + Sync>;

/// Tier inputs and pause-rule state for this device.
pub struct SemanticDevice {
    lifecycle: Lifecycle,
    provider: Arc<dyn DeviceStateProvider>,
    ram_shim: Option<RamShim>,
    profile: OnceLock<DeviceProfile>,
}

impl SemanticDevice {
    fn new(
        lifecycle: Lifecycle,
        provider: Arc<dyn DeviceStateProvider>,
        ram_shim: Option<RamShim>,
    ) -> Self {
        Self {
            lifecycle,
            provider,
            ram_shim,
            profile: OnceLock::new(),
        }
    }

    /// The desktop configuration (also what tests use).
    #[must_use]
    pub fn desktop() -> Self {
        let lifecycle = Lifecycle::new();
        let provider = Arc::new(borealis_embed::device::DesktopDevice {
            lifecycle: lifecycle.clone(),
        });
        Self::new(lifecycle, provider, None)
    }

    /// The shared foreground / background flag.
    #[must_use]
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// The state provider the indexing scheduler reads.
    #[must_use]
    pub fn provider(&self) -> Arc<dyn DeviceStateProvider> {
        Arc::clone(&self.provider)
    }

    /// RAM, cores and pointer width, probed once.
    pub fn profile(&self) -> DeviceProfile {
        *self.profile.get_or_init(|| {
            let shim_ram = self.ram_shim.as_ref().and_then(|f| f());
            DeviceProfile::probe().with_ram(shim_ram)
        })
    }

    /// The probed tier lowered by `settings.search.tierOverride` (never raised).
    pub fn tier(&self, user: TierOverride) -> Tier {
        effective_tier(self.profile().tier(), user)
    }
}

/// Registers the `borealis-device` plugin, which manages the [`SemanticDevice`].
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("borealis-device")
        .setup(|app, api| {
            app.manage(build(api)?);
            Ok(())
        })
        .build()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn build<R: Runtime, C: serde::de::DeserializeOwned>(
    _api: tauri::plugin::PluginApi<R, C>,
) -> Result<SemanticDevice, Box<dyn std::error::Error>> {
    Ok(SemanticDevice::desktop())
}

#[cfg(target_os = "ios")]
fn build<R: Runtime, C: serde::de::DeserializeOwned>(
    _api: tauri::plugin::PluginApi<R, C>,
) -> Result<SemanticDevice, Box<dyn std::error::Error>> {
    let lifecycle = Lifecycle::new();
    let provider = Arc::new(borealis_embed::device::ios::IosDevice::new(
        lifecycle.clone(),
    ));
    Ok(SemanticDevice::new(lifecycle, provider, None))
}

#[cfg(target_os = "android")]
fn build<R: Runtime, C: serde::de::DeserializeOwned>(
    api: tauri::plugin::PluginApi<R, C>,
) -> Result<SemanticDevice, Box<dyn std::error::Error>> {
    let handle = api.register_android_plugin("app.borealis", "DeviceStatePlugin")?;
    let lifecycle = Lifecycle::new();
    let ram_handle = handle.clone();
    let ram_shim: RamShim =
        Box::new(move || android::report(&ram_handle).and_then(|r| r.total_ram_bytes()));
    let provider = Arc::new(android::AndroidDevice {
        handle,
        lifecycle: lifecycle.clone(),
    });
    Ok(SemanticDevice::new(lifecycle, provider, Some(ram_shim)))
}

#[cfg(target_os = "android")]
mod android {
    use borealis_embed::device::android::AndroidReport;
    use borealis_embed::{DeviceState, DeviceStateProvider, Lifecycle};
    use tauri::plugin::PluginHandle;
    use tauri::Runtime;

    /// One `deviceState` call; failures are logged (no device data in the message) and answer None.
    pub fn report<R: Runtime>(handle: &PluginHandle<R>) -> Option<AndroidReport> {
        match handle.run_mobile_plugin::<AndroidReport>("deviceState", ()) {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!("device state unavailable: {e}");
                None
            }
        }
    }

    pub struct AndroidDevice<R: Runtime> {
        pub handle: PluginHandle<R>,
        pub lifecycle: Lifecycle,
    }

    impl<R: Runtime> DeviceStateProvider for AndroidDevice<R> {
        fn current(&self) -> DeviceState {
            let backgrounded = self.lifecycle.is_backgrounded();
            report(&self.handle).map_or_else(
                || DeviceState::UNKNOWN_MOBILE.with_lifecycle(&self.lifecycle),
                |r| r.to_state(backgrounded),
            )
        }
    }
}

/// Mobile: `Suspended` marks the app backgrounded (indexing pauses), `Resumed` clears it.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    #[cfg(mobile)]
    {
        let backgrounded = match event {
            tauri::WindowEvent::Suspended => true,
            tauri::WindowEvent::Resumed => false,
            _ => return,
        };
        if let Some(device) = window.try_state::<SemanticDevice>() {
            device.lifecycle().set_backgrounded(backgrounded);
        }
    }
    #[cfg(desktop)]
    let _ = (window, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_device_probes_a_desktop_tier_and_honours_the_override() {
        let device = SemanticDevice::desktop();
        let probed = device.tier(TierOverride::Auto);
        assert!(probed >= Tier::T2, "{:?}", device.profile());
        assert_eq!(device.tier(TierOverride::T1), Tier::T1);
        assert_eq!(device.tier(TierOverride::T3), probed);
    }

    #[test]
    fn lifecycle_reaches_the_provider() {
        let device = SemanticDevice::desktop();
        assert!(!device.provider().current().backgrounded);
        device.lifecycle().set_backgrounded(true);
        assert!(device.provider().current().backgrounded);
    }
}
