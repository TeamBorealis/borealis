//! In-app updates for the direct desktop builds (#25, decision UP2; specs/mac-app-store.md Part 2).
//!
//! The "new version available" notice (`app_update.rs`) still finds the new version. Where this copy of Borealis can
//! replace itself, the notice also offers **Install and restart**: the backend then fetches the signed `latest.json`
//! of the latest release, downloads the update for this platform, checks its minisign signature against the public
//! key in `tauri.conf.json` (`plugins.updater.pubkey`), installs it and restarts. Nothing is installed without that
//! click, and the web view has no updater permission: only the `install_app_update` command reaches it.
//!
//! Which copies can install (this file decides, from facts gathered at startup):
//! - Windows: the NSIS install (per-user, so no UAC prompt).
//! - macOS: the DMG app, running from its `.app` bundle. Never the Mac App Store flavour (`mas`).
//! - Linux: the AppImage. A deb or rpm install keeps the notice and is told to update the way it was installed.
//! - Phones and every store build: never (no updater compiled in, see build.rs `cfg(self_update)`).
//!
//! Until the release key exists, `plugins.updater.pubkey` holds a placeholder. Anything that is not a minisign public
//! key leaves the updater unregistered and the notice offers Download only.

use base64::Engine as _;
use serde::Serialize;
use std::path::Path;

/// How a copy of Borealis gets a new version (`AppUpdateStatus::update_method`, the notice's buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateMethod {
    /// Install and restart from inside the app (plus Download).
    Install,
    /// A deb or rpm install: update it the way it was installed, or download the new package.
    PackageManager,
    /// Download from the website only.
    #[default]
    Download,
}

/// How this copy was packaged (`tauri::utils::platform::bundle_type`, which the bundler writes into each binary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Nsis,
    Msi,
    MacApp,
    AppImage,
    Deb,
    Rpm,
    /// Not packaged (a development or test build) or not known.
    Unknown,
}

/// Whether this copy can install updates itself, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSupport {
    /// Register the updater; the notice offers Install and restart.
    Available,
    /// iOS or Android: the stores (or the website, for the sideloaded APK) update them.
    Phone,
    /// An app store build (the Mac App Store flavour): no update check at all.
    StoreBuild,
    /// Built without `cfg(self_update)`.
    NotCompiled,
    /// A deb or rpm install, which the system package manager owns.
    PackageManager,
    /// Not running from an installed copy that the updater can replace (a development build, a macOS binary outside
    /// its `.app`, an AppImage started without its runtime).
    NotInstalled,
    /// `plugins.updater.pubkey` is the placeholder or not a minisign public key.
    NoKey,
    /// The updater plugin failed to start.
    Failed,
}

impl InstallSupport {
    /// Only an available updater is registered.
    #[must_use]
    pub fn registers_updater(self) -> bool {
        self == Self::Available
    }

    /// The buttons the notice shows for this copy.
    #[must_use]
    pub fn update_method(self) -> UpdateMethod {
        match self {
            Self::Available => UpdateMethod::Install,
            Self::PackageManager => UpdateMethod::PackageManager,
            _ => UpdateMethod::Download,
        }
    }
}

/// What [`install_support`] decides from; gathered once at startup.
#[derive(Debug, Clone, Copy)]
pub struct InstallFacts<'a> {
    /// `std::env::consts::OS`.
    pub target_os: &'a str,
    /// [`crate::distribution::is_store_build`].
    pub store_build: bool,
    /// `cfg!(self_update)`.
    pub compiled: bool,
    pub package: Package,
    /// The AppImage runtime set `APPIMAGE` (the path the updater replaces).
    pub appimage_env: bool,
    /// The executable sits in `<name>.app/Contents/MacOS/` ([`in_mac_app_bundle`]).
    pub mac_app_bundle: bool,
    /// `plugins.updater.pubkey` from the app config.
    pub pubkey: &'a str,
}

/// Whether this copy may register the updater and offer Install and restart.
#[must_use]
pub fn install_support(f: &InstallFacts<'_>) -> InstallSupport {
    if matches!(f.target_os, "android" | "ios") {
        return InstallSupport::Phone;
    }
    if f.store_build {
        return InstallSupport::StoreBuild;
    }
    if !f.compiled {
        return InstallSupport::NotCompiled;
    }
    let installed = match f.target_os {
        "windows" => f.package == Package::Nsis,
        "macos" => f.mac_app_bundle,
        "linux" if matches!(f.package, Package::Deb | Package::Rpm) => {
            return InstallSupport::PackageManager;
        }
        "linux" => f.package == Package::AppImage && f.appimage_env,
        _ => false,
    };
    if !installed {
        return InstallSupport::NotInstalled;
    }
    if !is_public_key(f.pubkey) {
        return InstallSupport::NoKey;
    }
    InstallSupport::Available
}

/// A minisign public key as `tauri signer generate` writes it to `<name>.key.pub`: base64 of the two-line text
/// "untrusted comment: minisign public key: <id>" and the key itself. The config placeholder is not.
#[must_use]
pub fn is_public_key(pubkey: &str) -> bool {
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(pubkey.trim()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(raw) else {
        return false;
    };
    let mut lines = text.lines();
    let comment = lines.next().unwrap_or_default();
    let key = lines.next().unwrap_or_default().trim();
    comment.starts_with("untrusted comment:")
        && key.len() >= 40
        && base64::engine::general_purpose::STANDARD
            .decode(key)
            .is_ok()
}

/// `plugins.updater.pubkey` from the app config (empty when absent, as in the Mac App Store overlay).
#[must_use]
pub fn configured_pubkey(updater_config: Option<&serde_json::Value>) -> &str {
    updater_config
        .and_then(|c| c.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// The macOS executable runs from inside an app bundle (`/Applications/Borealis.app/Contents/MacOS/borealis`), so
/// the updater replaces that bundle and not the folder of a loose binary.
#[must_use]
pub fn in_mac_app_bundle(exe: &Path) -> bool {
    let Some(macos) = exe.parent() else {
        return false;
    };
    let Some(contents) = macos.parent() else {
        return false;
    };
    let is_bundle = contents
        .parent()
        .and_then(Path::extension)
        .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    macos.file_name().is_some_and(|n| n == "MacOS")
        && contents.file_name().is_some_and(|n| n == "Contents")
        && is_bundle
}

/// Why Install and restart did not finish. The messages are shown as they are: no paths, no raw errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallFailure {
    /// This copy cannot install updates ([`InstallSupport`] is not `Available`).
    Unavailable,
    /// Another install is already running.
    Busy,
    /// The release has no update feed yet, it is not newer, or it is not the version the notice showed.
    NotReady,
    /// The feed has no update for this platform.
    NotForThisPlatform,
    /// The download does not match its signature (or the signature is for another version).
    BadSignature,
    /// The feed or the download could not be fetched.
    Network,
    /// Installing the downloaded update failed.
    InstallFailed,
}

impl InstallFailure {
    /// The message the notice shows.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Unavailable => {
                "This copy of Borealis cannot install updates itself. Download the new version instead."
            }
            Self::Busy => "The update is already being installed.",
            Self::NotReady => {
                "This version cannot be installed from inside Borealis yet. Click Check now and try again, or use Download."
            }
            Self::NotForThisPlatform => {
                "This update cannot be installed from inside Borealis on this computer. Use Download instead."
            }
            Self::BadSignature => {
                "The download did not pass the signature check, so nothing was installed. Use Download instead."
            }
            Self::Network => {
                "Could not download the update. Check your connection and try again, or use Download."
            }
            Self::InstallFailed => {
                "Borealis could not install the update. Your current version is unchanged. Use Download instead."
            }
        }
    }

    /// The error the `install_app_update` command returns.
    #[must_use]
    pub fn to_api_error(self) -> crate::error::ApiError {
        use crate::error::{ApiError, ErrorCode};
        let code = match self {
            Self::Network | Self::InstallFailed => ErrorCode::IoError,
            _ => ErrorCode::RequirementFailed,
        };
        ApiError::new(code, self.message())
    }
}

/// The two versions are the same release (`X.Y.Z`, with or without a leading `v`).
#[must_use]
pub fn same_version(a: &str, b: &str) -> bool {
    match (
        crate::app_update::parse_version(a),
        crate::app_update::parse_version(b),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// `borealis://app-install` payload while the update downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    /// Bytes received so far.
    pub downloaded: u64,
    /// The size the server announced, when it did.
    pub total: Option<u64>,
}

/// Lets a progress event through only when the whole percentage changed (or every 4 MB without a known size), so a
/// big download sends about a hundred events and not one per network chunk.
#[derive(Debug, Default)]
pub struct ProgressGate {
    last: Option<u64>,
}

const UNSIZED_STEP: u64 = 4 * 1024 * 1024;

impl ProgressGate {
    /// The event to send for this point of the download, if any.
    pub fn next(&mut self, downloaded: u64, total: Option<u64>) -> Option<InstallProgress> {
        let bucket = match total {
            Some(t) if t > 0 => downloaded.saturating_mul(100) / t,
            _ => downloaded / UNSIZED_STEP,
        };
        if self.last == Some(bucket) {
            return None;
        }
        self.last = Some(bucket);
        Some(InstallProgress { downloaded, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made-up bytes in the `.key.pub` shape of a minisign public key; no secret key exists for it.
    const REAL_SHAPED_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDFCOTM0OEFCQ0QyNzhCQUIKUldTcmkxZk5xMGlUR3d6Y2JTSmhIdm5Ga1luRXNPYUJ5bnJGdzd5S2Z5dzBOcFg0aFFyQmJPZ2YK";

    fn facts(target_os: &'static str, package: Package) -> InstallFacts<'static> {
        InstallFacts {
            target_os,
            store_build: false,
            compiled: true,
            package,
            appimage_env: false,
            mac_app_bundle: false,
            pubkey: REAL_SHAPED_KEY,
        }
    }

    #[test]
    fn the_test_key_has_the_public_key_shape_and_the_placeholder_does_not() {
        assert!(is_public_key(REAL_SHAPED_KEY));
        assert!(!is_public_key(""));
        assert!(!is_public_key("PLACEHOLDER-NOT-A-KEY: replace me"));
        // Base64 of something that is not a minisign key file.
        assert!(!is_public_key("aGVsbG8gd29ybGQ="));
    }

    #[test]
    fn the_direct_installs_can_install() {
        assert_eq!(
            install_support(&facts("windows", Package::Nsis)),
            InstallSupport::Available
        );
        let mac = InstallFacts {
            mac_app_bundle: true,
            ..facts("macos", Package::MacApp)
        };
        assert_eq!(install_support(&mac), InstallSupport::Available);
        let appimage = InstallFacts {
            appimage_env: true,
            ..facts("linux", Package::AppImage)
        };
        assert_eq!(install_support(&appimage), InstallSupport::Available);
        assert!(install_support(&appimage).registers_updater());
        assert_eq!(
            install_support(&appimage).update_method(),
            UpdateMethod::Install
        );
    }

    #[test]
    fn the_mac_app_store_build_never_registers_or_offers_the_updater() {
        let mas = InstallFacts {
            store_build: true,
            compiled: false,
            mac_app_bundle: true,
            ..facts("macos", Package::MacApp)
        };
        assert_eq!(install_support(&mas), InstallSupport::StoreBuild);
        assert!(!install_support(&mas).registers_updater());
        assert_eq!(
            install_support(&mas).update_method(),
            UpdateMethod::Download
        );
    }

    #[test]
    fn phones_never_register_or_offer_the_updater() {
        for os in ["ios", "android"] {
            for store_build in [true, false] {
                let phone = InstallFacts {
                    store_build,
                    ..facts(os, Package::Unknown)
                };
                assert_eq!(install_support(&phone), InstallSupport::Phone);
                assert!(!install_support(&phone).registers_updater());
                assert_eq!(
                    install_support(&phone).update_method(),
                    UpdateMethod::Download
                );
            }
        }
    }

    #[test]
    fn deb_and_rpm_installs_keep_the_notice_and_point_to_the_package_manager() {
        for package in [Package::Deb, Package::Rpm] {
            // Even with a real key and a stray APPIMAGE variable.
            let f = InstallFacts {
                appimage_env: true,
                ..facts("linux", package)
            };
            assert_eq!(install_support(&f), InstallSupport::PackageManager);
            assert!(!install_support(&f).registers_updater());
            assert_eq!(
                install_support(&f).update_method(),
                UpdateMethod::PackageManager
            );
        }
    }

    #[test]
    fn a_build_without_the_updater_compiled_in_never_registers_it() {
        let f = InstallFacts {
            compiled: false,
            ..facts("windows", Package::Nsis)
        };
        assert_eq!(install_support(&f), InstallSupport::NotCompiled);
        assert!(!install_support(&f).registers_updater());
    }

    #[test]
    fn copies_the_updater_cannot_replace_are_not_offered_an_install() {
        let cases = [
            facts("windows", Package::Unknown),
            facts("windows", Package::Msi),
            facts("macos", Package::MacApp),
            facts("linux", Package::AppImage),
            facts("linux", Package::Unknown),
            facts("freebsd", Package::Unknown),
        ];
        for f in cases {
            assert_eq!(install_support(&f), InstallSupport::NotInstalled, "{f:?}");
            assert_eq!(install_support(&f).update_method(), UpdateMethod::Download);
        }
    }

    #[test]
    fn the_placeholder_key_leaves_the_updater_unregistered() {
        let f = InstallFacts {
            pubkey: "PLACEHOLDER-NOT-A-KEY",
            ..facts("windows", Package::Nsis)
        };
        assert_eq!(install_support(&f), InstallSupport::NoKey);
        assert!(!install_support(&f).registers_updater());
        assert_eq!(install_support(&f).update_method(), UpdateMethod::Download);
        assert!(!InstallSupport::Failed.registers_updater());
    }

    #[test]
    fn this_build_compiles_the_updater_only_into_direct_desktop_builds() {
        assert_eq!(
            cfg!(self_update),
            !cfg!(any(
                target_os = "android",
                target_os = "ios",
                feature = "mas"
            ))
        );
    }

    #[test]
    fn the_shipped_config_has_the_endpoint_and_a_placeholder_until_the_key_exists() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let read = |name: &str| -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(dir.join(name)).expect(name)).expect(name)
        };
        let conf = read("tauri.conf.json");
        let updater = &conf["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/TeamBorealis/borealis/releases/latest/download/latest.json"
            ])
        );
        assert_eq!(updater["requireSignedVersion"], true);
        let key = configured_pubkey(Some(updater));
        assert!(
            key.starts_with("PLACEHOLDER-NOT-A-KEY") || is_public_key(key),
            "plugins.updater.pubkey must be the placeholder or a minisign public key"
        );
        assert!(
            conf["bundle"].get("createUpdaterArtifacts").is_none(),
            "the release workflow turns updater artifacts on only when the signing key exists"
        );
        // The Mac App Store overlay removes the updater config, so the store binary carries no update endpoint.
        let mas = read("tauri.mas.conf.json");
        assert!(mas["plugins"]
            .as_object()
            .is_some_and(|p| p.get("updater") == Some(&serde_json::Value::Null)));
    }

    #[test]
    fn the_pubkey_is_read_from_the_updater_config() {
        let c = serde_json::json!({ "pubkey": "abc" });
        assert_eq!(configured_pubkey(Some(&c)), "abc");
        assert_eq!(configured_pubkey(None), "");
        assert_eq!(configured_pubkey(Some(&serde_json::json!({}))), "");
    }

    #[test]
    fn only_an_executable_inside_an_app_bundle_counts() {
        assert!(in_mac_app_bundle(Path::new(
            "/Applications/Borealis.app/Contents/MacOS/borealis"
        )));
        assert!(!in_mac_app_bundle(Path::new(
            "/Users/me/src/target/release/borealis"
        )));
        assert!(!in_mac_app_bundle(Path::new("/x/Contents/MacOS/borealis")));
        assert!(!in_mac_app_bundle(Path::new("borealis")));
    }

    #[test]
    fn versions_match_with_or_without_v() {
        assert!(same_version("v1.4.0", "1.4.0"));
        assert!(same_version("1.4", "1.4.0"));
        assert!(!same_version("1.4.1", "1.4.0"));
        assert!(!same_version("nightly", "nightly"));
    }

    #[test]
    fn failures_have_plain_messages_and_codes() {
        use crate::error::ErrorCode;
        let all = [
            InstallFailure::Unavailable,
            InstallFailure::Busy,
            InstallFailure::NotReady,
            InstallFailure::NotForThisPlatform,
            InstallFailure::BadSignature,
            InstallFailure::Network,
            InstallFailure::InstallFailed,
        ];
        for f in all {
            let e = f.to_api_error();
            assert_eq!(e.message, f.message());
            assert!(e.message.ends_with('.') && !e.message.contains('/'));
        }
        assert_eq!(
            InstallFailure::Network.to_api_error().code,
            ErrorCode::IoError
        );
        assert_eq!(
            InstallFailure::BadSignature.to_api_error().code,
            ErrorCode::RequirementFailed
        );
    }

    #[test]
    fn progress_is_sent_once_per_percent() {
        let mut gate = ProgressGate::default();
        assert!(gate.next(0, Some(1000)).is_some());
        assert!(gate.next(5, Some(1000)).is_none());
        assert_eq!(
            gate.next(10, Some(1000)),
            Some(InstallProgress {
                downloaded: 10,
                total: Some(1000)
            })
        );
        assert!(gate.next(1000, Some(1000)).is_some());
        let mut no_size = ProgressGate::default();
        assert!(no_size.next(1, None).is_some());
        assert!(no_size.next(UNSIZED_STEP - 1, None).is_none());
        assert!(no_size.next(UNSIZED_STEP, None).is_some());
    }
}
