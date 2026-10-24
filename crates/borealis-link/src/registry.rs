//! Paired-device registry: who this device trusts, persisted as `<data>/link/devices.json`
//! (0600, atomic). It holds **no secrets**: certificates are public and tokens live in the
//! [`SecureStore`](crate::store::SecureStore). The whole `link/` directory is still excluded from
//! sync bundles and bug reports.
//!
//! Pairings expire 180 days after the device was last seen (sec. 6 "stale pairing"); re-pair
//! by QR afterwards. All updates return a new registry (the value is never mutated in place).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::LinkError;
use crate::fingerprint::{CertFingerprint, DeviceId};
use crate::fs::write_private_file;

/// Directory inside the app data dir that holds link state (never synced or reported).
pub const LINK_DIR: &str = "link";
/// Registry file name inside [`LINK_DIR`].
pub const REGISTRY_FILE: &str = "devices.json";
/// Format tag of the registry file.
pub const REGISTRY_FORMAT: &str = "borealis-link-devices/1";
/// Days a pairing stays valid after the device was last seen.
pub const PAIRING_TTL_DAYS: u64 = 180;
/// Most paired devices kept.
pub const MAX_DEVICES: usize = 32;
/// Longest device name kept (characters).
pub const MAX_NAME_CHARS: usize = 64;
const DAY_SECS: u64 = 24 * 60 * 60;
const PAIRING_TTL_SECS: u64 = PAIRING_TTL_DAYS * DAY_SECS;
/// Largest registry file read (a hostile or corrupt file is refused, not parsed).
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// `<data>/link`.
pub fn link_dir(data: &Path) -> PathBuf {
    data.join(LINK_DIR)
}

/// `<data>/link/devices.json`.
pub fn registry_path(data: &Path) -> PathBuf {
    link_dir(data).join(REGISTRY_FILE)
}

/// Operating system family of a device (display and per-platform behaviour only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// macOS.
    Macos,
    /// Windows.
    Windows,
    /// Linux or another desktop unix.
    Linux,
    /// iPhone / iPad.
    Ios,
    /// Android.
    Android,
    /// Anything else, or a value from a newer version.
    #[serde(other)]
    Unknown,
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else if cfg!(unix) {
            Self::Linux
        } else {
            Self::Unknown
        }
    }
}

/// One paired device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    /// Derived from `fingerprint` ([`DeviceId::of`]); checked on load.
    pub device_id: DeviceId,
    /// Name the device gave when pairing (sanitised, at most [`MAX_NAME_CHARS`]).
    pub name: String,
    /// Its platform.
    pub platform: Platform,
    /// SHA-256 of its certificate: what this device pins.
    pub fingerprint: CertFingerprint,
    /// Unix seconds.
    pub paired_at: u64,
    /// Unix seconds of the last successful connection.
    pub last_seen: u64,
    /// Unix seconds; `last_seen` + 180 days.
    pub expires_at: u64,
}

impl PairedDevice {
    /// A device paired at `now`.
    pub fn new(name: &str, platform: Platform, fingerprint: CertFingerprint, now: u64) -> Self {
        Self {
            device_id: DeviceId::of(&fingerprint),
            name: clean_name(name),
            platform,
            fingerprint,
            paired_at: now,
            last_seen: now,
            expires_at: now.saturating_add(PAIRING_TTL_SECS),
        }
    }

    /// True at or after `expires_at`.
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    fn is_consistent(&self) -> bool {
        self.device_id == DeviceId::of(&self.fingerprint) && self.paired_at <= self.last_seen
    }
}

/// True for characters a peer must not get onto this device's screen: control characters and
/// invisible formatting (bidi overrides and isolates, zero-width marks, BOM), which could reorder
/// or hide the surrounding text ("Mira" + RLO + "nosj.exe").
#[must_use]
pub fn is_unsafe_display_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Trim, drop control and invisible formatting characters and cap at [`MAX_NAME_CHARS`]; empty
/// becomes "Device".
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !is_unsafe_display_char(*c))
        .take(MAX_NAME_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Device".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The registry file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRegistry {
    /// Always [`REGISTRY_FORMAT`].
    pub format: String,
    /// Paired devices, sorted by id.
    pub devices: Vec<PairedDevice>,
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self {
            format: REGISTRY_FORMAT.to_string(),
            devices: Vec::new(),
        }
    }
}

impl DeviceRegistry {
    /// Read `<data>/link/devices.json`; a missing file is an empty registry.
    ///
    /// # Errors
    /// [`LinkError::Invalid`] for an unknown format, oversized or inconsistent file (the caller
    /// decides whether to start over; the file is never silently rewritten).
    pub fn load(data: &Path) -> Result<Self, LinkError> {
        let path = registry_path(data);
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        if meta.len() > MAX_FILE_BYTES {
            return Err(LinkError::Invalid("device registry is too large".into()));
        }
        let text = std::fs::read_to_string(&path)?;
        Self::from_json(&text)
    }

    /// Parse and validate registry JSON.
    ///
    /// # Errors
    /// [`LinkError::Invalid`].
    pub fn from_json(text: &str) -> Result<Self, LinkError> {
        let reg: Self = serde_json::from_str(text)
            .map_err(|e| LinkError::Invalid(format!("device registry: {e}")))?;
        if reg.format != REGISTRY_FORMAT {
            return Err(LinkError::Invalid("unknown device registry format".into()));
        }
        if reg.devices.len() > MAX_DEVICES {
            return Err(LinkError::Invalid("too many paired devices".into()));
        }
        if !reg.devices.iter().all(PairedDevice::is_consistent) {
            return Err(LinkError::Invalid("inconsistent device record".into()));
        }
        let devices = reg
            .devices
            .into_iter()
            .map(|d| PairedDevice {
                name: clean_name(&d.name),
                ..d
            })
            .collect();
        Ok(Self::default().with_devices(devices))
    }

    /// Write `<data>/link/devices.json` atomically (0600), creating `link/`.
    ///
    /// # Errors
    /// I/O failure.
    pub fn save(&self, data: &Path) -> Result<(), LinkError> {
        let dir = link_dir(data);
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| LinkError::Invalid(format!("device registry: {e}")))?;
        write_private_file(&registry_path(data), text.as_bytes())
    }

    fn with_devices(&self, mut devices: Vec<PairedDevice>) -> Self {
        devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        devices.dedup_by(|a, b| a.device_id == b.device_id);
        Self {
            format: self.format.clone(),
            devices,
        }
    }

    /// The device with this id.
    pub fn get(&self, id: &DeviceId) -> Option<&PairedDevice> {
        self.devices.iter().find(|d| &d.device_id == id)
    }

    /// The unexpired device holding this certificate (what a handshake proved).
    pub fn find_by_fingerprint(&self, fp: &CertFingerprint, now: u64) -> Option<&PairedDevice> {
        self.devices
            .iter()
            .find(|d| d.fingerprint.ct_eq(fp) && !d.is_expired(now))
    }

    /// Fingerprints a listener should accept now (unexpired devices only).
    pub fn allowed_fingerprints(&self, now: u64) -> Vec<CertFingerprint> {
        self.devices
            .iter()
            .filter(|d| !d.is_expired(now))
            .map(|d| d.fingerprint)
            .collect()
    }

    /// Add or replace (same certificate = same id) a device.
    ///
    /// # Errors
    /// [`LinkError::Invalid`] when [`MAX_DEVICES`] other devices are already paired.
    pub fn with_paired(&self, device: PairedDevice) -> Result<Self, LinkError> {
        let others: Vec<PairedDevice> = self
            .devices
            .iter()
            .filter(|d| d.device_id != device.device_id)
            .cloned()
            .collect();
        if others.len() >= MAX_DEVICES {
            return Err(LinkError::Invalid("too many paired devices".into()));
        }
        Ok(self.with_devices(others.into_iter().chain([device]).collect()))
    }

    /// Record a successful connection at `now`: refreshes `last_seen` and the expiry. Unknown
    /// or expired devices are left out (re-pair instead).
    pub fn with_seen(&self, id: &DeviceId, now: u64) -> Self {
        let devices = self
            .devices
            .iter()
            .map(|d| {
                if &d.device_id == id && !d.is_expired(now) {
                    PairedDevice {
                        last_seen: now.max(d.last_seen),
                        expires_at: now.max(d.last_seen).saturating_add(PAIRING_TTL_SECS),
                        ..d.clone()
                    }
                } else {
                    d.clone()
                }
            })
            .collect();
        self.with_devices(devices)
    }

    /// Remove a device (Settings > Devices > Remove, C1).
    pub fn without(&self, id: &DeviceId) -> Self {
        self.with_devices(
            self.devices
                .iter()
                .filter(|d| &d.device_id != id)
                .cloned()
                .collect(),
        )
    }

    /// Drop expired pairings; returns the new registry and the ids removed (so the caller can
    /// delete their tokens from the secure store).
    pub fn pruned(&self, now: u64) -> (Self, Vec<DeviceId>) {
        let (expired, live): (Vec<_>, Vec<_>) = self
            .devices
            .iter()
            .cloned()
            .partition(|d| d.is_expired(now));
        (
            self.with_devices(live),
            expired.into_iter().map(|d| d.device_id).collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn dev(seed: &[u8], name: &str) -> PairedDevice {
        PairedDevice::new(name, Platform::Android, CertFingerprint::of_der(seed), NOW)
    }

    #[test]
    fn a_new_pairing_expires_after_180_days_of_silence() {
        let d = dev(b"a", "Pixel");
        assert_eq!(d.expires_at - d.paired_at, 180 * DAY_SECS);
        assert!(!d.is_expired(NOW + 180 * DAY_SECS - 1));
        assert!(d.is_expired(NOW + 180 * DAY_SECS));
    }

    #[test]
    fn being_seen_extends_the_expiry_and_never_revives() {
        let d = dev(b"a", "Pixel");
        let id = d.device_id.clone();
        let reg = DeviceRegistry::default().with_paired(d).unwrap();
        let later = NOW + 100 * DAY_SECS;
        let seen = reg.with_seen(&id, later);
        assert_eq!(seen.get(&id).unwrap().last_seen, later);
        assert_eq!(seen.get(&id).unwrap().expires_at, later + 180 * DAY_SECS);
        assert_eq!(reg.get(&id).unwrap().last_seen, NOW, "original untouched");
        let dead = NOW + 181 * DAY_SECS;
        assert_eq!(
            reg.with_seen(&id, dead),
            reg,
            "expired pairings stay expired"
        );
        let (live, removed) = reg.pruned(dead);
        assert!(live.devices.is_empty());
        assert_eq!(removed, vec![id]);
    }

    #[test]
    fn same_certificate_replaces_and_lookups_respect_expiry() {
        let reg = DeviceRegistry::default()
            .with_paired(dev(b"a", "Old name"))
            .unwrap()
            .with_paired(dev(b"b", "Mac"))
            .unwrap()
            .with_paired(dev(b"a", "New name"))
            .unwrap();
        assert_eq!(reg.devices.len(), 2);
        let fp = CertFingerprint::of_der(b"a");
        assert_eq!(reg.find_by_fingerprint(&fp, NOW).unwrap().name, "New name");
        assert!(reg.find_by_fingerprint(&fp, NOW + 200 * DAY_SECS).is_none());
        assert_eq!(reg.allowed_fingerprints(NOW).len(), 2);
        let id = DeviceId::of(&fp);
        assert!(reg.without(&id).get(&id).is_none());
    }

    #[test]
    fn device_cap_is_enforced() {
        let reg = (0..MAX_DEVICES).fold(DeviceRegistry::default(), |r, i| {
            r.with_paired(dev(&i.to_le_bytes(), "d")).unwrap()
        });
        assert!(reg.with_paired(dev(b"one too many", "d")).is_err());
        let first = reg.devices[0].clone();
        assert!(reg.with_paired(first).is_ok(), "replacing is not adding");
    }

    #[test]
    fn names_are_sanitised() {
        assert_eq!(clean_name("  Phone\u{0}\n  "), "Phone");
        assert_eq!(clean_name("\u{7}"), "Device");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), MAX_NAME_CHARS);
        assert_eq!(clean_name("Mira\u{202E}nosj.exe"), "Miranosj.exe");
        assert_eq!(clean_name("\u{2066}Mac\u{2069}\u{200B}\u{FEFF}"), "Mac");
        assert_eq!(clean_name("Zoë の Mac"), "Zoë の Mac");
    }

    #[test]
    fn save_and_load_round_trip_with_private_mode() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(
            DeviceRegistry::load(data.path()).unwrap(),
            DeviceRegistry::default()
        );
        let reg = DeviceRegistry::default()
            .with_paired(dev(b"a", "Pixel"))
            .unwrap();
        reg.save(data.path()).unwrap();
        assert_eq!(DeviceRegistry::load(data.path()).unwrap(), reg);
        let path = registry_path(data.path());
        assert!(path.ends_with("link/devices.json"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn hostile_files_are_refused() {
        let good = DeviceRegistry::default()
            .with_paired(dev(b"a", "Pixel"))
            .unwrap();
        let mut v = serde_json::to_value(&good).unwrap();
        v["devices"][0]["device_id"] = "0000000000000000".into();
        assert!(
            DeviceRegistry::from_json(&v.to_string()).is_err(),
            "id must match cert"
        );
        let mut w = serde_json::to_value(&good).unwrap();
        w["format"] = "borealis-link-devices/9".into();
        assert!(DeviceRegistry::from_json(&w.to_string()).is_err());
        assert!(DeviceRegistry::from_json("{").is_err());
        let mut x = serde_json::to_value(&good).unwrap();
        x["devices"][0]["platform"] = "beos".into();
        let parsed = DeviceRegistry::from_json(&x.to_string()).unwrap();
        assert_eq!(parsed.devices[0].platform, Platform::Unknown);
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(link_dir(data.path())).unwrap();
        std::fs::write(registry_path(data.path()), vec![b' '; 300 * 1024]).unwrap();
        assert!(DeviceRegistry::load(data.path()).is_err(), "oversized file");
    }

    #[test]
    fn registry_json_has_only_public_fields() {
        let reg = DeviceRegistry::default()
            .with_paired(dev(b"a", "Pixel"))
            .unwrap();
        let v = serde_json::to_value(&reg).unwrap();
        let mut keys: Vec<&str> = v["devices"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "device_id",
                "expires_at",
                "fingerprint",
                "last_seen",
                "name",
                "paired_at",
                "platform"
            ]
        );
    }
}
