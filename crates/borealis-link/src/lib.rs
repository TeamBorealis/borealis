//! Device link: identity, pairing, certificate pinning, the secure store, the paired-device
//! registry and the authenticated device-to-device [`Transport`].
//!
//! Shared by the LAN AI host (`borealis-ai` features `host`/`remote`) and device sync
//! (specs/device-sync-investigation.md, WP B1). Features:
//! * none: fingerprints, pairing strings and codes, pairing limits, registry, store trait.
//! * `tls`: rustls (aws-lc-rs) identities, pinned client/server configs. No sockets.
//! * `tcp`: the tokio + rustls TCP [`Transport`] (desktop and mobile).
//! * `platform-store`: [`PlatformStore`](store::PlatformStore) on the OS secure store.
//!
//! Secrets (private keys, tokens) are only ever held in a [`store::SecureStore`] or in memory;
//! `Debug` output of every secret-bearing type is redacted, and nothing secret is logged.

pub mod error;
pub mod fingerprint;
pub mod fs;
pub mod limits;
pub mod pairing;
pub mod random;
pub mod registry;
pub mod store;
pub mod transport;

#[cfg(feature = "tls")]
pub mod device;
#[cfg(feature = "tls")]
pub mod tls;

pub use error::{LinkError, StoreError};
pub use fingerprint::{CertFingerprint, DeviceId};
pub use fs::write_private_file;
pub use pairing::{PairingUri, PairingUriError, PAIRING_CODE_LEN, PAIRING_URI_PREFIX};
pub use registry::{DeviceRegistry, PairedDevice, Platform};
pub use store::{MemoryStore, Secret, SecretName, SecureStore, StoreBackend};
pub use transport::{Connection, Listener, PeerIdentity, PeerPolicy, Transport};

/// Paths (relative to the app data dir, `/`-separated) where link state lives. Sync bundles
/// and bug reports must never include them (tested in borealis-bundle and the app).
pub const DATA_PATHS: &[&str] = &["link/devices.json", "link/"];
