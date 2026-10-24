//! This device's own link identity, kept only in the secure store.

use crate::error::LinkError;
use crate::store::{SecretName, SecureStore};
use crate::tls::TlsIdentity;

/// Certificate subject of a device link identity.
pub const DEVICE_SUBJECT: &str = "borealis-link-device";

/// Load this device's identity from `store`, or generate one and store it on first use.
///
/// A stored value that no longer decodes is replaced by a fresh identity (paired devices will
/// then refuse this device until it pairs again, which is the safe outcome). The identity is
/// never written anywhere but `store`.
///
/// # Errors
/// [`LinkError::Store`] when the store is unavailable or refuses access; key generation failure.
pub fn load_or_create_device_identity(store: &dyn SecureStore) -> Result<TlsIdentity, LinkError> {
    let name = SecretName::device_identity();
    if let Some(secret) = store.get(&name)? {
        match TlsIdentity::from_secret(&secret) {
            Ok(id) => return Ok(id),
            Err(e) => tracing::warn!("stored device identity unusable, creating a new one: {e}"),
        }
    }
    let id = TlsIdentity::generate(DEVICE_SUBJECT)?;
    store.set(&name, &id.to_secret())?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MemoryStore, Secret};

    #[test]
    fn identity_is_created_once_and_reused() {
        let store = MemoryStore::default();
        let a = load_or_create_device_identity(&store).unwrap();
        let b = load_or_create_device_identity(&store).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn a_corrupt_identity_is_replaced() {
        let store = MemoryStore::default();
        store
            .set(
                &SecretName::device_identity(),
                &Secret::new(b"junk".to_vec()),
            )
            .unwrap();
        let a = load_or_create_device_identity(&store).unwrap();
        let b = load_or_create_device_identity(&store).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
    }
}
