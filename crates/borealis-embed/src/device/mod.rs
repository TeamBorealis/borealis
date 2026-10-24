//! Device state the indexing scheduler pauses on (spec sec. 7): power source and battery level, Low Power Mode /
//! Battery Saver, thermal state, and whether the app is in the background.
//!
//! Providers sit behind [`DeviceStateProvider`]:
//! * [`DesktopDevice`]: always "on external power, nominal, foreground" (desktop tiers index per the table
//!   without battery rules; T3 always).
//! * iOS: [`ios::IosDevice`] reads `ProcessInfo` (Low Power Mode, thermal state) and `UIDevice` (battery) through
//!   safe objc2 bindings.
//! * Android: the app's Kotlin plugin answers an [`android::AndroidReport`] (`BatteryManager`, `PowerManager`);
//!   [`android::AndroidReport::to_state`] maps it. Without the plugin the conservative fallback is
//!   [`DeviceState::UNKNOWN_MOBILE`] (treated as on battery, so T1/T2 do not index).
//! * [`SimulatedDevice`]: settable state for tests and simulated-battery e2e runs.
//!
//! The app reports foreground / background through a shared [`Lifecycle`] (Tauri `WindowEvent::Suspended` /
//! `Resumed` on mobile).

pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Where power comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    /// Plugged in (charging or full), or a desktop.
    External,
    /// On battery, with the level when known (0-100).
    Battery { percent: Option<u8> },
    /// Not known (treated as battery with unknown level).
    Unknown,
}

/// Thermal pressure, as iOS `ProcessInfo.ThermalState` (Android `PowerManager` thermal status maps onto it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Thermal {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// A snapshot of everything the pause rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceState {
    pub power: Power,
    /// iOS Low Power Mode / Android Battery Saver.
    pub low_power_mode: bool,
    pub thermal: Thermal,
    /// App not in the foreground.
    pub backgrounded: bool,
}

impl DeviceState {
    /// Desktop: plugged in, cool, foreground.
    pub const DESKTOP: Self = Self {
        power: Power::External,
        low_power_mode: false,
        thermal: Thermal::Nominal,
        backgrounded: false,
    };

    /// Mobile without a working shim: power unknown, so battery rules keep indexing paused.
    pub const UNKNOWN_MOBILE: Self = Self {
        power: Power::Unknown,
        low_power_mode: false,
        thermal: Thermal::Nominal,
        backgrounded: false,
    };

    /// Mark backgrounded when `lifecycle` says so.
    #[must_use]
    pub fn with_lifecycle(self, lifecycle: &Lifecycle) -> Self {
        Self {
            backgrounded: self.backgrounded || lifecycle.is_backgrounded(),
            ..self
        }
    }
}

/// Reads the current device state. Called before each indexing batch, so it must be cheap.
pub trait DeviceStateProvider: Send + Sync {
    fn current(&self) -> DeviceState;
}

/// Foreground / background flag shared between the app's window events and the providers.
#[derive(Debug, Clone, Default)]
pub struct Lifecycle(Arc<AtomicBool>);

impl Lifecycle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_backgrounded(&self, backgrounded: bool) {
        self.0.store(backgrounded, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_backgrounded(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Desktop provider: [`DeviceState::DESKTOP`] (the lifecycle flag is honoured if the app sets it).
#[derive(Debug, Clone, Default)]
pub struct DesktopDevice {
    pub lifecycle: Lifecycle,
}

impl DeviceStateProvider for DesktopDevice {
    fn current(&self) -> DeviceState {
        DeviceState::DESKTOP.with_lifecycle(&self.lifecycle)
    }
}

/// Settable state (tests, simulated-battery e2e).
#[derive(Debug)]
pub struct SimulatedDevice(Mutex<DeviceState>);

impl SimulatedDevice {
    #[must_use]
    pub fn new(state: DeviceState) -> Self {
        Self(Mutex::new(state))
    }

    pub fn set(&self, state: DeviceState) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// Change part of the state.
    pub fn update(&self, f: impl FnOnce(DeviceState) -> DeviceState) {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *slot = f(*slot);
    }
}

impl DeviceStateProvider for SimulatedDevice {
    fn current(&self) -> DeviceState {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Total physical RAM of this device, when the platform tells us.
#[must_use]
pub fn total_ram_bytes() -> Option<u64> {
    platform_total_ram()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn platform_total_ram() -> Option<u64> {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    Some(sys.total_memory()).filter(|&b| b > 0)
}

#[cfg(target_os = "ios")]
fn platform_total_ram() -> Option<u64> {
    Some(ios::physical_memory()).filter(|&b| b > 0)
}

#[cfg(target_os = "android")]
fn platform_total_ram() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| android::meminfo_total_bytes(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_is_always_allowed_unless_backgrounded() {
        let d = DesktopDevice::default();
        assert_eq!(d.current(), DeviceState::DESKTOP);
        d.lifecycle.set_backgrounded(true);
        assert!(d.current().backgrounded);
    }

    #[test]
    fn simulated_device_updates() {
        let s = SimulatedDevice::new(DeviceState::DESKTOP);
        s.update(|st| DeviceState {
            thermal: Thermal::Serious,
            ..st
        });
        assert_eq!(s.current().thermal, Thermal::Serious);
        assert!(Thermal::Critical > Thermal::Serious);
    }
}
