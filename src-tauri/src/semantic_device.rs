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
//! The same plugin answers one UI fact: whether the system lets apps give touch feedback (B-48 O13, see
//! [`SemanticDevice::haptics_enabled`]).
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
type FlagShim = Box<dyn Fn() -> Option<bool> + Send + Sync>;

/// How this platform decides whether haptic feedback may play.
// Each platform builds only its own variant (the tests build all three).
#[allow(dead_code)]
enum HapticsSetting {
    /// Desktop: no haptics.
    None,
    /// iOS: UIKit's feedback generators apply Settings > Sounds & Haptics > System Haptics themselves.
    System,
    /// Android: the haptics plugin vibrates through `Vibrator`, which ignores Settings > Touch feedback, so it is read here.
    Probe(FlagShim),
}

/// Tier inputs and pause-rule state for this device.
pub struct SemanticDevice {
    lifecycle: Lifecycle,
    provider: Arc<dyn DeviceStateProvider>,
    ram_shim: Option<RamShim>,
    haptics: HapticsSetting,
    profile: OnceLock<DeviceProfile>,
}

impl SemanticDevice {
    fn new(
        lifecycle: Lifecycle,
        provider: Arc<dyn DeviceStateProvider>,
        ram_shim: Option<RamShim>,
        haptics: HapticsSetting,
    ) -> Self {
        Self {
            lifecycle,
            provider,
            ram_shim,
            haptics,
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
        Self::new(lifecycle, provider, None, HapticsSetting::None)
    }

    /// B-48 O13: whether light haptics may play. Desktop: never. iOS: yes (the OS applies its own setting). Android: the
    /// system's touch feedback setting, on when it cannot be read (Android's own default).
    pub fn haptics_enabled(&self) -> bool {
        match &self.haptics {
            HapticsSetting::None => false,
            HapticsSetting::System => true,
            HapticsSetting::Probe(read) => read().unwrap_or(true),
        }
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
    Ok(SemanticDevice::new(
        lifecycle,
        provider,
        None,
        HapticsSetting::System,
    ))
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
    let haptics_handle = handle.clone();
    let haptics: FlagShim = Box::new(move || android::touch_feedback_enabled(&haptics_handle));
    let provider = Arc::new(android::AndroidDevice {
        handle,
        lifecycle: lifecycle.clone(),
    });
    Ok(SemanticDevice::new(
        lifecycle,
        provider,
        Some(ram_shim),
        HapticsSetting::Probe(haptics),
    ))
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

    #[derive(serde::Deserialize)]
    struct TouchFeedback {
        enabled: bool,
    }

    /// `Settings.System.HAPTIC_FEEDBACK_ENABLED` through `DeviceStatePlugin.hapticFeedbackEnabled`; None if the call fails.
    pub fn touch_feedback_enabled<R: Runtime>(handle: &PluginHandle<R>) -> Option<bool> {
        match handle.run_mobile_plugin::<TouchFeedback>("hapticFeedbackEnabled", ()) {
            Ok(r) => Some(r.enabled),
            Err(e) => {
                tracing::warn!("touch feedback setting unavailable: {e}");
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
    fn desktop_never_plays_haptics() {
        assert!(!SemanticDevice::desktop().haptics_enabled());
    }

    #[test]
    fn a_phone_follows_its_system_setting_and_defaults_to_on() {
        let device = |haptics| {
            let lifecycle = Lifecycle::new();
            let provider = Arc::new(borealis_embed::device::DesktopDevice {
                lifecycle: lifecycle.clone(),
            });
            SemanticDevice::new(lifecycle, provider, None, haptics)
        };
        assert!(device(HapticsSetting::System).haptics_enabled());
        assert!(!device(HapticsSetting::Probe(Box::new(|| Some(false)))).haptics_enabled());
        assert!(device(HapticsSetting::Probe(Box::new(|| Some(true)))).haptics_enabled());
        assert!(device(HapticsSetting::Probe(Box::new(|| None))).haptics_enabled());
    }

    #[test]
    fn lifecycle_reaches_the_provider() {
        let device = SemanticDevice::desktop();
        assert!(!device.provider().current().backgrounded);
        device.lifecycle().set_backgrounded(true);
        assert!(device.provider().current().backgrounded);
    }
}
