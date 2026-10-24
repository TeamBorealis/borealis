//! iOS shim: `ProcessInfo.physicalMemory`, `isLowPowerModeEnabled` and `thermalState` (thread-safe, read on the
//! calling thread) and `UIDevice` battery state / level (main-thread only: refreshed asynchronously on the main
//! queue into a cache that [`IosDevice::current`] reads). Until the first refresh lands the power source is
//! [`Power::Unknown`], so battery rules keep indexing paused (conservative).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
use objc2_ui_kit::{UIDevice, UIDeviceBatteryState};

use super::{DeviceState, DeviceStateProvider, Lifecycle, Power, Thermal};

/// `ProcessInfo.processInfo.physicalMemory`.
#[must_use]
pub fn physical_memory() -> u64 {
    NSProcessInfo::processInfo().physicalMemory()
}

const POWER_UNKNOWN: u8 = 0;
const POWER_EXTERNAL: u8 = 1;
const POWER_BATTERY: u8 = 2;
const LEVEL_UNKNOWN: u8 = u8::MAX;

/// Battery fields last read on the main thread.
#[derive(Debug)]
struct BatteryCache {
    power: AtomicU8,
    level: AtomicU8,
}

/// iOS provider.
#[derive(Debug, Clone)]
pub struct IosDevice {
    lifecycle: Lifecycle,
    battery: Arc<BatteryCache>,
}

impl IosDevice {
    /// A provider sharing the app's lifecycle flag. Starts the first battery read.
    #[must_use]
    pub fn new(lifecycle: Lifecycle) -> Self {
        let device = Self {
            lifecycle,
            battery: Arc::new(BatteryCache {
                power: AtomicU8::new(POWER_UNKNOWN),
                level: AtomicU8::new(LEVEL_UNKNOWN),
            }),
        };
        device.refresh_battery();
        device
    }

    /// Queue a battery read on the main thread (never blocks the caller).
    fn refresh_battery(&self) {
        let cache = Arc::clone(&self.battery);
        DispatchQueue::main().exec_async(move || {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let device = UIDevice::currentDevice(mtm);
            device.setBatteryMonitoringEnabled(true);
            let state = device.batteryState();
            let power =
                if state == UIDeviceBatteryState::Charging || state == UIDeviceBatteryState::Full {
                    POWER_EXTERNAL
                } else if state == UIDeviceBatteryState::Unplugged {
                    POWER_BATTERY
                } else {
                    POWER_UNKNOWN
                };
            let level = device.batteryLevel(); // 0.0-1.0, or -1.0 when unknown
            let level = if (0.0..=1.0).contains(&level) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct = (level * 100.0).round() as u8;
                pct
            } else {
                LEVEL_UNKNOWN
            };
            cache.power.store(power, Ordering::SeqCst);
            cache.level.store(level, Ordering::SeqCst);
        });
    }
}

impl DeviceStateProvider for IosDevice {
    fn current(&self) -> DeviceState {
        let info = NSProcessInfo::processInfo();
        let thermal = match info.thermalState() {
            NSProcessInfoThermalState::Critical => Thermal::Critical,
            NSProcessInfoThermalState::Serious => Thermal::Serious,
            NSProcessInfoThermalState::Fair => Thermal::Fair,
            _ => Thermal::Nominal,
        };
        let level = self.battery.level.load(Ordering::SeqCst);
        let power = match self.battery.power.load(Ordering::SeqCst) {
            POWER_EXTERNAL => Power::External,
            POWER_BATTERY => Power::Battery {
                percent: (level <= 100).then_some(level),
            },
            _ => Power::Unknown,
        };
        // The next call sees a fresh value; batches are seconds apart.
        self.refresh_battery();
        DeviceState {
            power,
            low_power_mode: info.isLowPowerModeEnabled(),
            thermal,
            backgrounded: self.lifecycle.is_backgrounded(),
        }
    }
}
