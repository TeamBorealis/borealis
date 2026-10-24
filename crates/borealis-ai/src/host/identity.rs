//! The host's self-signed TLS identity, persisted in the app data dir and pinned by clients.
//! A thin wrapper over [`borealis_link::tls::TlsIdentity`] that keeps the v2 file names.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use borealis_link::tls::TlsIdentity;
use borealis_link::LinkError;
use rustls::ServerConfig;

use crate::protocol::CertFingerprint;
use crate::AiError;

/// Certificate file name inside the identity directory.
pub const CERT_FILE: &str = "ai-host-cert.der";
/// Private key file name (PKCS#8 DER, mode 0600 on unix).
pub const KEY_FILE: &str = "ai-host-key.der";
const CERT_SUBJECT: &str = "borealis-ai-host";
const ALPN_HTTP1: &[u8] = b"http/1.1";

/// Self-signed certificate plus key. Cheap to clone.
#[derive(Clone)]
pub struct HostIdentity(TlsIdentity);

impl fmt::Debug for HostIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostIdentity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

fn ai_err(e: LinkError) -> AiError {
    match e {
        LinkError::Io(m) => AiError::io(m),
        LinkError::Invalid(m) => AiError::Invalid(m),
        LinkError::Tls(m) => AiError::Transport(m),
        other => AiError::Transport(other.to_string()),
    }
}

impl HostIdentity {
    /// Fresh ECDSA P-256 self-signed identity (not persisted).
    ///
    /// # Errors
    /// Key generation failure.
    pub fn generate() -> Result<Self, AiError> {
        TlsIdentity::generate(CERT_SUBJECT)
            .map(Self)
            .map_err(ai_err)
    }

    /// Load the identity from `dir`, or create and persist one on first use (or when the files
    /// are missing or unusable). The key file is written atomically with mode 0600.
    ///
    /// # Errors
    /// I/O failures while writing a new identity.
    pub fn load_or_create(dir: &Path) -> Result<Self, AiError> {
        TlsIdentity::load_or_create_files(dir, CERT_FILE, KEY_FILE, CERT_SUBJECT)
            .map(Self)
            .map_err(ai_err)
    }

    /// SHA-256 of the certificate DER; goes into the pairing string.
    pub fn fingerprint(&self) -> CertFingerprint {
        self.0.fingerprint()
    }

    /// The certificate (DER).
    pub fn cert_der(&self) -> &[u8] {
        self.0.cert_der()
    }

    /// TLS 1.3-only rustls server config (aws-lc-rs provider, like reqwest).
    pub(crate) fn server_config(&self) -> Result<Arc<ServerConfig>, AiError> {
        self.0.server_config(&[ALPN_HTTP1]).map_err(ai_err)
    }
}

/// Atomically replace `path` with `bytes`: write a sibling temp file (mode 0600 on unix), fsync,
/// rename over the target, then fsync the directory (best effort).
///
/// # Errors
/// Any I/O failure; the target is left untouched in that case.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), AiError> {
    borealis_link::write_private_file(path, bytes).map_err(ai_err)
}
