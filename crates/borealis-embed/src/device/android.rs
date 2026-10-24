//! Android shim data. The Kotlin side lives in the app (`DeviceStatePlugin.kt`, same pattern as `SharePlugin.kt`)
//! because `ActivityManager`, `BatteryManager` and `PowerManager` need an `Activity`; it answers an
//! [`AndroidReport`]. This module is plain data so the mapping is tested on every host.

use serde::Deserialize;

use super::{DeviceState, Power, Thermal};

/// Answer of the Kotlin `deviceState` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidReport {
    /// `ActivityManager.MemoryInfo.totalMem` in bytes (0 when unknown).
    pub total_mem: u64,
    /// Plugged in (AC/USB/wireless) or `BATTERY_STATUS_CHARGING` / `FULL`.
    pub charging: bool,
    /// Battery level 0-100, or -1 when unknown.
    pub battery_percent: i32,
    /// `PowerManager.isPowerSaveMode` (Battery Saver).
    pub power_save: bool,
    /// `PowerManager.getCurrentThermalStatus` (API 29+), or -1 when unavailable.
    pub thermal_status: i32,
}

/// `PowerManager.THERMAL_STATUS_*` values.
const THERMAL_MODERATE: i32 = 2;
const THERMAL_SEVERE: i32 = 3;
const THERMAL_CRITICAL: i32 = 4;

impl AndroidReport {
    /// Map onto the scheduler's state. `backgrounded` comes from the app's lifecycle flag.
    #[must_use]
    pub fn to_state(&self, backgrounded: bool) -> DeviceState {
        let power = if self.charging {
            Power::External
        } else {
            Power::Battery {
                percent: u8::try_from(self.battery_percent)
                    .ok()
                    .filter(|p| *p <= 100),
            }
        };
        // LIGHT (1) is still nominal for our purposes; SEVERE is Android's equivalent of iOS "serious".
        let thermal = match self.thermal_status {
            s if s >= THERMAL_CRITICAL => Thermal::Critical,
            THERMAL_SEVERE => Thermal::Serious,
            THERMAL_MODERATE => Thermal::Fair,
            _ => Thermal::Nominal,
        };
        DeviceState {
            power,
            low_power_mode: self.power_save,
            thermal,
            backgrounded,
        }
    }

    /// Total RAM, when the plugin knew it.
    #[must_use]
    pub fn total_ram_bytes(&self) -> Option<u64> {
        Some(self.total_mem).filter(|&b| b > 0)
    }
}

/// `MemTotal` of `/proc/meminfo` in bytes (the fallback when the plugin is not available).
#[must_use]
pub fn meminfo_total_bytes(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line
        .trim_start_matches("MemTotal:")
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    kib.checked_mul(1024).filter(|&b| b > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_answer_parses_and_maps() {
        let json = r#"{"totalMem":3865470976,"charging":false,"batteryPercent":64,"powerSave":false,"thermalStatus":3}"#;
        let r: AndroidReport = serde_json::from_str(json).expect("parse");
        assert_eq!(r.total_ram_bytes(), Some(3_865_470_976));
        let s = r.to_state(false);
        assert_eq!(s.power, Power::Battery { percent: Some(64) });
        assert_eq!(s.thermal, Thermal::Serious);
        assert!(!s.low_power_mode);
    }

    #[test]
    fn unknown_fields_map_conservatively() {
        let r = AndroidReport {
            total_mem: 0,
            charging: false,
            battery_percent: -1,
            power_save: true,
            thermal_status: -1,
        };
        let s = r.to_state(true);
        assert_eq!(s.power, Power::Battery { percent: None });
        assert_eq!(s.thermal, Thermal::Nominal);
        assert!(s.low_power_mode && s.backgrounded);
        assert_eq!(r.total_ram_bytes(), None);
        let charging = AndroidReport {
            charging: true,
            thermal_status: 6,
            ..r
        };
        assert_eq!(charging.to_state(false).power, Power::External);
        assert_eq!(charging.to_state(false).thermal, Thermal::Critical);
    }

    #[test]
    fn meminfo_fallback() {
        let text = "MemTotal:        3774864 kB\nMemFree:          123 kB\n";
        assert_eq!(meminfo_total_bytes(text), Some(3_774_864 * 1024));
        assert_eq!(meminfo_total_bytes("MemFree: 1 kB"), None);
        assert_eq!(meminfo_total_bytes("MemTotal: x kB"), None);
    }
}
