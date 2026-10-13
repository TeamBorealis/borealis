//! The host's self-signed TLS identity, persisted in the app data dir and pinned by clients.

use std::fmt;
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

use crate::protocol::CertFingerprint;
use crate::AiError;

/// Certificate file name inside the identity directory.
pub const CERT_FILE: &str = "ai-host-cert.der";
/// Private key file name (PKCS#8 DER, mode 0600 on unix).
pub const KEY_FILE: &str = "ai-host-key.der";
const CERT_SUBJECT: &str = "borealis-ai-host";

/// Self-signed certificate plus key. Cheap to clone.
#[derive(Clone)]
pub struct HostIdentity {
    cert_der: Arc<Vec<u8>>,
    key_der: Arc<Vec<u8>>,
}

impl fmt::Debug for HostIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostIdentity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

fn tls_err(e: impl fmt::Display) -> AiError {
    AiError::Transport(format!("tls identity: {e}"))
}

impl HostIdentity {
    /// Fresh ECDSA P-256 self-signed identity (not persisted).
    ///
    /// # Errors
    /// Key generation failure.
    pub fn generate() -> Result<Self, AiError> {
        let key = rcgen::KeyPair::generate().map_err(tls_err)?;
        let params =
            rcgen::CertificateParams::new(vec![CERT_SUBJECT.to_string()]).map_err(tls_err)?;
        let cert = params.self_signed(&key).map_err(tls_err)?;
        Ok(Self {
            cert_der: Arc::new(cert.der().to_vec()),
            key_der: Arc::new(key.serialize_der()),
        })
    }

    /// Load the identity from `dir`, or create and persist one on first use (or when the files
    /// are missing or unusable). The key file is written atomically with mode 0600.
    ///
    /// # Errors
    /// I/O failures while writing a new identity.
    pub fn load_or_create(dir: &Path) -> Result<Self, AiError> {
        if let Some(id) = Self::load(dir) {
            return Ok(id);
        }
        let id = Self::generate()?;
        std::fs::create_dir_all(dir).map_err(AiError::io)?;
        write_private_file(&dir.join(KEY_FILE), &id.key_der)?;
        write_private_file(&dir.join(CERT_FILE), &id.cert_der)?;
        Ok(id)
    }

    fn load(dir: &Path) -> Option<Self> {
        let cert = std::fs::read(dir.join(CERT_FILE)).ok()?;
        let key = std::fs::read(dir.join(KEY_FILE)).ok()?;
        let id = Self {
            cert_der: Arc::new(cert),
            key_der: Arc::new(key),
        };
        id.server_config().ok().map(|_| id)
    }

    /// SHA-256 of the certificate DER; goes into the pairing string.
    pub fn fingerprint(&self) -> CertFingerprint {
        CertFingerprint::of_der(&self.cert_der)
    }

    /// The certificate (DER).
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// TLS 1.3-only rustls server config (aws-lc-rs provider, like reqwest).
    pub(crate) fn server_config(&self) -> Result<Arc<ServerConfig>, AiError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.to_vec()));
        let mut cfg = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls_err)?
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(self.cert_der.to_vec())], key)
            .map_err(tls_err)?;
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Arc::new(cfg))
    }
}

/// Atomically replace `path` with `bytes`: write a sibling temp file (mode 0600 on unix), fsync,
/// rename over the target, then fsync the directory (best effort).
///
/// # Errors
/// Any I/O failure; the target is left untouched in that case.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), AiError> {
    let dir = path
        .parent()
        .ok_or_else(|| AiError::Invalid("path has no parent directory".into()))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".tmp-")
        .tempfile_in(dir)
        .map_err(AiError::io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(AiError::io)?;
    }
    tmp.write_all(bytes).map_err(AiError::io)?;
    tmp.as_file().sync_all().map_err(AiError::io)?;
    tmp.persist(path).map_err(|e| AiError::io(e.error))?;
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}
