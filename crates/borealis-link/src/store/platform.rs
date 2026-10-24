//! The platform secure store through the keyring-core credential stores (all MIT OR Apache-2.0,
//! maintained by the open-source-cooperative keyring-rs project):
//!
//! | Target | Store crate | Backend |
//! |---|---|---|
//! | macOS | `apple-native-keyring-store::keychain` | login keychain |
//! | iOS | `apple-native-keyring-store::protected` | data-protection keychain, iCloud sync **off** |
//! | Android | `android-native-keyring-store` | SharedPreferences encrypted with an Android Keystore key (needs Tauri's ndk-context) |
//! | Windows | `windows-native-keyring-store` | Credential Manager (generic credentials) |
//! | Linux/BSD | `zbus-secret-service-keyring-store` | Secret Service over D-Bus (GNOME Keyring, KWallet) |
//!
//! Each secret is one credential: service [`SERVICE`], user = the [`SecretName`].

use std::fmt;
use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error as KrError};

use super::{Secret, SecretName, SecureStore, StoreBackend, SERVICE};
use crate::error::StoreError;

/// The OS secure store for this target.
pub struct PlatformStore {
    inner: Arc<CredentialStore>,
    backend: StoreBackend,
}

impl fmt::Debug for PlatformStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlatformStore({:?})", self.backend)
    }
}

fn platform(e: impl fmt::Display) -> StoreError {
    StoreError::Platform(e.to_string())
}

/// Map keyring errors; never echo returned secret bytes (`BadEncoding`/`BadDataFormat` carry
/// them).
fn map_err(e: KrError) -> StoreError {
    match e {
        KrError::NoStorageAccess(p) => StoreError::AccessDenied(p.to_string()),
        KrError::NoDefaultStore => StoreError::Unavailable("no credential store".into()),
        KrError::NotSupportedByStore(m) => StoreError::Unavailable(m),
        KrError::BadEncoding(_) | KrError::BadDataFormat(..) => {
            StoreError::Platform("stored secret has an unexpected format".into())
        }
        KrError::PlatformFailure(p) => StoreError::Platform(p.to_string()),
        other => platform(other),
    }
}

impl PlatformStore {
    /// Open this target's store.
    ///
    /// # Errors
    /// [`StoreError::Unavailable`] when the OS has no usable store (for example no Secret
    /// Service on a headless Linux machine, or Android before Tauri set the ndk-context).
    pub fn open() -> Result<Self, StoreError> {
        let (inner, backend) = open_native()?;
        Ok(Self { inner, backend })
    }

    fn entry(&self, name: &SecretName) -> Result<Entry, StoreError> {
        self.inner
            .build(SERVICE, name.as_str(), None)
            .map_err(map_err)
    }
}

impl SecureStore for PlatformStore {
    fn get(&self, name: &SecretName) -> Result<Option<Secret>, StoreError> {
        match self.entry(name)?.get_secret() {
            Ok(bytes) => Ok(Some(Secret::new(bytes))),
            Err(KrError::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, name: &SecretName, value: &Secret) -> Result<(), StoreError> {
        self.entry(name)?
            .set_secret(value.expose())
            .map_err(map_err)
    }

    fn delete(&self, name: &SecretName) -> Result<bool, StoreError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(KrError::NoEntry) => Ok(false),
            Err(e) => Err(map_err(e)),
        }
    }

    fn backend(&self) -> StoreBackend {
        self.backend
    }
}

fn unavailable(e: KrError) -> StoreError {
    StoreError::Unavailable(map_err(e).to_string())
}

#[cfg(target_os = "macos")]
fn open_native() -> Result<(Arc<CredentialStore>, StoreBackend), StoreError> {
    let s: Arc<CredentialStore> =
        apple_native_keyring_store::keychain::Store::new().map_err(unavailable)?;
    Ok((s, StoreBackend::MacKeychain))
}

#[cfg(target_os = "ios")]
fn open_native() -> Result<(Arc<CredentialStore>, StoreBackend), StoreError> {
    // Default configuration: the app's own access group, no iCloud keychain sync (a synced key
    // would be a cloned device identity).
    let s: Arc<CredentialStore> =
        apple_native_keyring_store::protected::Store::new().map_err(unavailable)?;
    Ok((s, StoreBackend::IosKeychain))
}

#[cfg(target_os = "android")]
fn open_native() -> Result<(Arc<CredentialStore>, StoreBackend), StoreError> {
    let s: Arc<CredentialStore> =
        android_native_keyring_store::Store::new().map_err(unavailable)?;
    Ok((s, StoreBackend::AndroidKeystore))
}

#[cfg(windows)]
fn open_native() -> Result<(Arc<CredentialStore>, StoreBackend), StoreError> {
    let s: Arc<CredentialStore> =
        windows_native_keyring_store::Store::new().map_err(unavailable)?;
    Ok((s, StoreBackend::WindowsCredentialManager))
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn open_native() -> Result<(Arc<CredentialStore>, StoreBackend), StoreError> {
    let s: Arc<CredentialStore> =
        zbus_secret_service_keyring_store::Store::new().map_err(unavailable)?;
    Ok((s, StoreBackend::LinuxSecretService))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real OS store (keychain prompt possible on macOS): run by hand with
    /// `cargo test -p borealis-link --features platform-store -- --ignored`.
    #[test]
    #[ignore = "uses the real OS secure store"]
    fn real_store_round_trip() {
        let store = PlatformStore::open().expect("store");
        let name = SecretName::new("selftest.borealis-link").unwrap();
        let _ = store.delete(&name);
        assert!(store.get(&name).unwrap().is_none());
        store
            .set(&name, &Secret::new(b"\x00\x01binary".to_vec()))
            .unwrap();
        assert_eq!(
            store.get(&name).unwrap().unwrap().expose(),
            b"\x00\x01binary"
        );
        assert!(store.delete(&name).unwrap());
        assert!(store.get(&name).unwrap().is_none());
    }
}
