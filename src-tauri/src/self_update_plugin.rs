//! The Tauri updater behind Install and restart (`cfg(self_update)`, desktop builds without `mas`). The decisions
//! live in [`crate::self_update`]; this file gathers the facts, registers `tauri-plugin-updater` when they allow it,
//! and runs one check, download and install for the `install_app_update` command.
//!
//! The plugin also registers its own web view commands (check, download, install). No capability grants an
//! `updater:` permission, so the web view cannot call them (tests/acl.rs).

use crate::self_update::{
    configured_pubkey, in_mac_app_bundle, install_support, same_version, InstallFacts,
    InstallFailure, InstallProgress, InstallSupport, Package, ProgressGate,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Error as UpdaterError, Update, UpdaterExt};

/// The update feed (`latest.json`) is tiny; GitHub answers in well under this.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// The installers are 100-200 MB; a download that takes longer than this has stalled.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// This copy's [`InstallSupport`], decided once at startup.
pub struct SelfUpdateState(InstallSupport);

fn package() -> Package {
    use tauri::utils::{config::BundleType, platform::bundle_type};
    match bundle_type() {
        Some(BundleType::Nsis) => Package::Nsis,
        Some(BundleType::Msi) => Package::Msi,
        Some(BundleType::App | BundleType::Dmg) => Package::MacApp,
        Some(BundleType::AppImage) => Package::AppImage,
        Some(BundleType::Deb) => Package::Deb,
        Some(BundleType::Rpm) => Package::Rpm,
        _ => Package::Unknown,
    }
}

/// The facts of this process, judged by [`install_support`].
fn detect<R: Runtime>(app: &AppHandle<R>) -> InstallSupport {
    let config = app.config();
    let exe = std::env::current_exe().ok();
    install_support(&InstallFacts {
        target_os: std::env::consts::OS,
        store_build: crate::distribution::is_store_build(),
        compiled: true,
        package: package(),
        appimage_env: std::env::var_os("APPIMAGE").is_some_and(|v| !v.is_empty()),
        mac_app_bundle: exe.as_deref().is_some_and(in_mac_app_bundle),
        pubkey: configured_pubkey(config.plugins.0.get("updater")),
    })
}

/// Startup: register the updater plugin when this copy can install updates, and remember the outcome.
pub fn register<R: Runtime>(app: &AppHandle<R>) -> InstallSupport {
    let mut support = detect(app);
    if support.registers_updater() {
        if let Err(e) = app.plugin(tauri_plugin_updater::Builder::new().build()) {
            tracing::warn!("in-app updates: the updater did not start: {e}");
            support = InstallSupport::Failed;
        }
    }
    tracing::info!("in-app updates: {support:?}");
    app.manage(SelfUpdateState(support));
    support
}

/// This copy's support ([`register`] ran in setup; `NotInstalled` before it).
#[must_use]
pub fn support<R: Runtime>(app: &AppHandle<R>) -> InstallSupport {
    app.try_state::<SelfUpdateState>()
        .map_or(InstallSupport::NotInstalled, |s| s.0)
}

/// What a plugin error means for the user.
#[must_use]
pub fn classify(e: &UpdaterError) -> InstallFailure {
    match e {
        UpdaterError::ReleaseNotFound | UpdaterError::Serialization(_) => InstallFailure::NotReady,
        UpdaterError::TargetNotFound(_)
        | UpdaterError::TargetsNotFound(_)
        | UpdaterError::UnsupportedArch
        | UpdaterError::UnsupportedOs => InstallFailure::NotForThisPlatform,
        UpdaterError::Minisign(_)
        | UpdaterError::Base64(_)
        | UpdaterError::SignatureUtf8(_)
        | UpdaterError::SignedVersionMismatch { .. }
        | UpdaterError::MissingSignedVersion => InstallFailure::BadSignature,
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => InstallFailure::Network,
        _ => InstallFailure::InstallFailed,
    }
}

fn logged(e: &UpdaterError) -> InstallFailure {
    let failure = classify(e);
    tracing::warn!("in-app update: {failure:?}: {e}");
    failure
}

/// Ask the feed for the update of this platform. `expected` is the version the notice showed: a feed announcing
/// another version (or nothing newer) is not installed.
///
/// # Errors
/// What went wrong, for the user ([`InstallFailure::message`]).
pub async fn find_update<R: Runtime>(
    app: &AppHandle<R>,
    expected: Option<&str>,
) -> Result<Update, InstallFailure> {
    let updater = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| logged(&e))?;
    let update = updater
        .check()
        .await
        .map_err(|e| logged(&e))?
        .ok_or(InstallFailure::NotReady)?;
    if expected.is_some_and(|v| !same_version(&update.version, v)) {
        tracing::warn!(
            "in-app update: the feed offers {} but the notice showed {expected:?}",
            update.version
        );
        return Err(InstallFailure::NotReady);
    }
    Ok(update)
}

/// Download the update and check its signature; `on_progress` gets about one call per percent.
///
/// # Errors
/// What went wrong, for the user ([`InstallFailure::message`]).
pub async fn download(
    update: &Update,
    mut on_progress: impl FnMut(InstallProgress),
) -> Result<Vec<u8>, InstallFailure> {
    let mut update = update.clone();
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut gate = ProgressGate::default();
    let mut received = 0u64;
    update
        .download(
            |chunk, total| {
                received += chunk as u64;
                if let Some(p) = gate.next(received, total) {
                    on_progress(p);
                }
            },
            || {},
        )
        .await
        .map_err(|e| logged(&e))
}

/// Clears the "install running" flag when the attempt ends, however it ends.
struct Running;

static RUNNING: AtomicBool = AtomicBool::new(false);

impl Running {
    fn start() -> Result<Self, InstallFailure> {
        RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self)
            .map_err(|_| InstallFailure::Busy)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Install and restart, up to the restart: find the update the notice showed, download and verify it, install it.
/// On Windows the installer takes over and the app exits here; elsewhere the caller restarts.
///
/// # Errors
/// What went wrong, for the user ([`InstallFailure::message`]). Nothing is installed unless the signature matched.
pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    expected: Option<&str>,
    on_progress: impl FnMut(InstallProgress),
) -> Result<(), InstallFailure> {
    if !support(app).registers_updater() {
        return Err(InstallFailure::Unavailable);
    }
    let _running = Running::start()?;
    let update = find_update(app, expected).await?;
    let bytes = download(&update, on_progress).await?;
    tracing::info!("in-app update: installing {}", update.version);
    update.install(&bytes).map_err(|e| logged(&e))
}
