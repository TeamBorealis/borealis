//! A self-signed TLS identity (ECDSA P-256 certificate + PKCS#8 key) that peers pin by
//! fingerprint. Persisted either as two private files (the AI LAN host, unchanged since v2) or
//! as one secret in the platform secure store (device link identity, never a file).

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use zeroize::Zeroizing;

use crate::error::LinkError;
use crate::fingerprint::CertFingerprint;
use crate::fs::write_private_file;
use crate::store::Secret;

/// Magic prefix of the secure-store encoding (`to_secret`).
const SECRET_MAGIC: &[u8; 5] = b"BLID1";
const MAX_DER_LEN: usize = 16 * 1024;

/// Self-signed certificate plus key. Cheap to clone. `Debug` shows the fingerprint only.
#[derive(Clone)]
pub struct TlsIdentity {
    cert_der: Arc<Vec<u8>>,
    key_der: Arc<Zeroizing<Vec<u8>>>,
}

impl fmt::Debug for TlsIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsIdentity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

pub(crate) fn tls_err(e: impl fmt::Display) -> LinkError {
    LinkError::Tls(format!("tls identity: {e}"))
}

impl TlsIdentity {
    /// Fresh ECDSA P-256 self-signed identity with `subject` as its only name (not persisted).
    ///
    /// # Errors
    /// Key generation failure.
    pub fn generate(subject: &str) -> Result<Self, LinkError> {
        let key = rcgen::KeyPair::generate().map_err(tls_err)?;
        let params = rcgen::CertificateParams::new(vec![subject.to_string()]).map_err(tls_err)?;
        let cert = params.self_signed(&key).map_err(tls_err)?;
        Ok(Self {
            cert_der: Arc::new(cert.der().to_vec()),
            key_der: Arc::new(Zeroizing::new(key.serialize_der())),
        })
    }

    /// Wrap existing DER bytes; fails unless rustls accepts the pair.
    ///
    /// # Errors
    /// [`LinkError::Tls`] for an unusable certificate or key.
    pub fn from_der(cert_der: Vec<u8>, key_der: Vec<u8>) -> Result<Self, LinkError> {
        let id = Self {
            cert_der: Arc::new(cert_der),
            key_der: Arc::new(Zeroizing::new(key_der)),
        };
        id.server_config(&[])?;
        Ok(id)
    }

    /// Load the identity from `dir/<cert_file>` + `dir/<key_file>`, or create and persist one
    /// (also when the files are missing or unusable). The files are written atomically, 0600.
    ///
    /// # Errors
    /// I/O failures while writing a new identity.
    pub fn load_or_create_files(
        dir: &Path,
        cert_file: &str,
        key_file: &str,
        subject: &str,
    ) -> Result<Self, LinkError> {
        if let Some(id) = Self::load_files(dir, cert_file, key_file) {
            return Ok(id);
        }
        let id = Self::generate(subject)?;
        std::fs::create_dir_all(dir)?;
        write_private_file(&dir.join(key_file), &id.key_der)?;
        write_private_file(&dir.join(cert_file), &id.cert_der)?;
        Ok(id)
    }

    fn load_files(dir: &Path, cert_file: &str, key_file: &str) -> Option<Self> {
        let cert = std::fs::read(dir.join(cert_file)).ok()?;
        let key = std::fs::read(dir.join(key_file)).ok()?;
        Self::from_der(cert, key).ok()
    }

    /// Encode for the secure store: `BLID1`, u32 BE certificate length, certificate, key.
    pub fn to_secret(&self) -> Secret {
        let mut out = Vec::with_capacity(9 + self.cert_der.len() + self.key_der.len());
        out.extend_from_slice(SECRET_MAGIC);
        let len = u32::try_from(self.cert_der.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.cert_der);
        out.extend_from_slice(&self.key_der);
        Secret::new(out)
    }

    /// Decode [`to_secret`](Self::to_secret) output.
    ///
    /// # Errors
    /// [`LinkError::Invalid`] for a malformed blob, [`LinkError::Tls`] for unusable keys.
    pub fn from_secret(secret: &Secret) -> Result<Self, LinkError> {
        let bad = || LinkError::Invalid("stored device identity is malformed".into());
        let b = secret.expose();
        let rest = b.strip_prefix(SECRET_MAGIC.as_slice()).ok_or_else(bad)?;
        let (len, rest) = rest.split_first_chunk::<4>().ok_or_else(bad)?;
        let len = usize::try_from(u32::from_be_bytes(*len)).map_err(|_| bad())?;
        if len == 0 || len > MAX_DER_LEN || rest.len() <= len || rest.len() - len > MAX_DER_LEN {
            return Err(bad());
        }
        let (cert, key) = rest.split_at(len);
        Self::from_der(cert.to_vec(), key.to_vec())
    }

    /// SHA-256 of the certificate DER; what peers pin.
    pub fn fingerprint(&self) -> CertFingerprint {
        CertFingerprint::of_der(&self.cert_der)
    }

    /// The certificate (DER). Public; safe to show or send.
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    pub(crate) fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![CertificateDer::from(self.cert_der.to_vec())]
    }

    pub(crate) fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.to_vec()))
    }

    /// TLS 1.3-only rustls server config without client authentication (the AI host: clients
    /// authenticate with a bearer token instead), advertising `alpn`.
    ///
    /// # Errors
    /// [`LinkError::Tls`] for an unusable key.
    pub fn server_config(&self, alpn: &[&[u8]]) -> Result<Arc<ServerConfig>, LinkError> {
        let mut cfg = ServerConfig::builder_with_provider(super::provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls_err)?
            .with_no_client_auth()
            .with_single_cert(self.cert_chain(), self.private_key())
            .map_err(tls_err)?;
        cfg.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
        Ok(Arc::new(cfg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_keeps_the_fingerprint() {
        let id = TlsIdentity::generate("t").unwrap();
        let back = TlsIdentity::from_secret(&id.to_secret()).unwrap();
        assert_eq!(back.fingerprint(), id.fingerprint());
        assert_eq!(back.cert_der(), id.cert_der());
    }

    #[test]
    fn malformed_secrets_are_rejected() {
        let id = TlsIdentity::generate("t").unwrap();
        let good = id.to_secret();
        let b = good.expose();
        for bad in [
            Vec::new(),
            b"BLID1".to_vec(),
            b[..b.len() - 1].iter().copied().skip(1).collect(),
            [b"BLID2".as_slice(), &b[5..]].concat(),
            [&b[..5], &[0xff, 0xff, 0xff, 0xff], &b[9..]].concat(),
        ] {
            assert!(TlsIdentity::from_secret(&Secret::new(bad)).is_err());
        }
    }

    #[test]
    fn debug_shows_no_key_material() {
        let id = TlsIdentity::generate("t").unwrap();
        let dbg = format!("{id:?}");
        assert!(dbg.contains("fingerprint"));
        assert!(!dbg.contains(&format!("{:?}", &id.key_der[..8])));
    }

    #[test]
    fn files_are_created_once_then_reused() {
        let dir = tempfile::tempdir().unwrap();
        let a = TlsIdentity::load_or_create_files(dir.path(), "c.der", "k.der", "s").unwrap();
        let b = TlsIdentity::load_or_create_files(dir.path(), "c.der", "k.der", "s").unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        std::fs::write(dir.path().join("k.der"), b"garbage").unwrap();
        let c = TlsIdentity::load_or_create_files(dir.path(), "c.der", "k.der", "s").unwrap();
        assert_ne!(
            c.fingerprint(),
            a.fingerprint(),
            "unusable files are replaced"
        );
    }
}
