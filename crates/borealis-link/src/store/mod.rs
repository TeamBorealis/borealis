//! Secure store for device keys and paired-device tokens.
//!
//! Secrets live only in the platform store ([`PlatformStore`], feature `platform-store`):
//! Keychain on macOS/iOS, Android Keystore, Windows Credential Manager, Linux Secret Service.
//! There is deliberately no plain-file fallback: when no store is available the caller gets
//! [`StoreError::Unavailable`] and device sync stays off. Nothing here writes to the app data
//! dir, so secrets can never end up in a sync bundle, a log or a bug report.

#[cfg(feature = "platform-store")]
mod platform;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::error::StoreError;
use crate::fingerprint::DeviceId;

#[cfg(feature = "platform-store")]
pub use platform::PlatformStore;

/// Service name the secrets are filed under (matches the app identifier).
pub const SERVICE: &str = "app.borealis.link";
/// Longest secret name.
const MAX_NAME_LEN: usize = 64;

/// Secret bytes, zeroed on drop. `Debug` never shows the content.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    /// Take ownership of secret bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The secret bytes. Never log or persist them outside a [`SecureStore`].
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(<redacted, {} bytes>)", self.0.len())
    }
}

/// Validated secret name: 1-64 chars of `[a-z0-9.-]`, not starting with `.`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(String);

impl SecretName {
    /// This device's own TLS identity (certificate + private key).
    pub fn device_identity() -> Self {
        Self("device-identity.v1".into())
    }

    /// The token shared with one paired device.
    pub fn peer_token(device: &DeviceId) -> Self {
        Self(format!("peer-token.{device}"))
    }

    /// Parse an arbitrary name.
    ///
    /// # Errors
    /// [`StoreError::BadName`].
    pub fn new(name: &str) -> Result<Self, StoreError> {
        let ok = !name.is_empty()
            && name.len() <= MAX_NAME_LEN
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-'));
        ok.then(|| Self(name.to_string()))
            .ok_or(StoreError::BadName)
    }

    /// The name text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which backend a store uses (for diagnostics and the Devices screen; never secret).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreBackend {
    /// macOS login keychain.
    MacKeychain,
    /// iOS data-protection keychain.
    IosKeychain,
    /// Android Keystore-encrypted SharedPreferences.
    AndroidKeystore,
    /// Windows Credential Manager.
    WindowsCredentialManager,
    /// freedesktop Secret Service (GNOME Keyring, KWallet).
    LinuxSecretService,
    /// Process memory only (tests, or a caller that chose not to persist).
    Memory,
}

/// A store for small secrets (keys, tokens). Implementations must never write secrets to the
/// app data dir.
pub trait SecureStore: Send + Sync {
    /// Read a secret; `Ok(None)` when it was never set or was deleted.
    ///
    /// # Errors
    /// [`StoreError`] when the store is unavailable or refuses access.
    fn get(&self, name: &SecretName) -> Result<Option<Secret>, StoreError>;

    /// Create or replace a secret.
    ///
    /// # Errors
    /// [`StoreError`].
    fn set(&self, name: &SecretName, value: &Secret) -> Result<(), StoreError>;

    /// Delete a secret; `Ok(false)` when there was nothing to delete.
    ///
    /// # Errors
    /// [`StoreError`].
    fn delete(&self, name: &SecretName) -> Result<bool, StoreError>;

    /// The backend in use.
    fn backend(&self) -> StoreBackend;
}

/// In-memory store (tests; or a session that must not persist anything).
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<BTreeMap<SecretName, Secret>>,
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.items.lock().map_or(0, |m| m.len());
        write!(f, "MemoryStore({n} secrets)")
    }
}

impl MemoryStore {
    fn items(&self) -> std::sync::MutexGuard<'_, BTreeMap<SecretName, Secret>> {
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SecureStore for MemoryStore {
    fn get(&self, name: &SecretName) -> Result<Option<Secret>, StoreError> {
        Ok(self.items().get(name).cloned())
    }

    fn set(&self, name: &SecretName, value: &Secret) -> Result<(), StoreError> {
        self.items().insert(name.clone(), value.clone());
        Ok(())
    }

    fn delete(&self, name: &SecretName) -> Result<bool, StoreError> {
        Ok(self.items().remove(name).is_some())
    }

    fn backend(&self) -> StoreBackend {
        StoreBackend::Memory
    }
}

/// Store the token shared with a paired device.
///
/// # Errors
/// [`StoreError`].
pub fn save_peer_token(
    store: &dyn SecureStore,
    device: &DeviceId,
    token: &Secret,
) -> Result<(), StoreError> {
    store.set(&SecretName::peer_token(device), token)
}

/// The token shared with a paired device, if any.
///
/// # Errors
/// [`StoreError`].
pub fn peer_token(
    store: &dyn SecureStore,
    device: &DeviceId,
) -> Result<Option<Secret>, StoreError> {
    store.get(&SecretName::peer_token(device))
}

/// Forget a paired device's token (unpair, revoke, expiry).
///
/// # Errors
/// [`StoreError`].
pub fn forget_peer(store: &dyn SecureStore, device: &DeviceId) -> Result<bool, StoreError> {
    store.delete(&SecretName::peer_token(device))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::CertFingerprint;

    #[test]
    fn names_are_validated() {
        assert!(SecretName::new("device-identity.v1").is_ok());
        for bad in ["", ".hidden", "UPPER", "a/b", "a b", &"x".repeat(65)] {
            assert_eq!(SecretName::new(bad), Err(StoreError::BadName), "{bad}");
        }
        let id = DeviceId::of(&CertFingerprint::of_der(b"c"));
        let n = SecretName::peer_token(&id);
        assert_eq!(SecretName::new(n.as_str()), Ok(n));
    }

    #[test]
    fn memory_store_round_trip_and_delete() {
        let s = MemoryStore::default();
        let id = DeviceId::of(&CertFingerprint::of_der(b"c"));
        assert!(peer_token(&s, &id).unwrap().is_none());
        save_peer_token(&s, &id, &Secret::new(b"tok".to_vec())).unwrap();
        assert_eq!(peer_token(&s, &id).unwrap().unwrap().expose(), b"tok");
        assert!(forget_peer(&s, &id).unwrap());
        assert!(!forget_peer(&s, &id).unwrap());
        assert_eq!(s.backend(), StoreBackend::Memory);
    }

    #[test]
    fn debug_output_never_contains_secret_bytes() {
        let s = MemoryStore::default();
        let secret = Secret::new(b"hunter2-very-secret".to_vec());
        s.set(&SecretName::device_identity(), &secret).unwrap();
        for text in [format!("{secret:?}"), format!("{s:?}")] {
            assert!(!text.contains("hunter2"), "{text}");
        }
        assert_eq!(format!("{secret:?}"), "Secret(<redacted, 19 bytes>)");
    }
}
