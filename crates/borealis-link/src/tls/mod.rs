//! Pinned-certificate TLS 1.3 (rustls with the aws-lc-rs provider, the same one reqwest uses).

mod identity;
mod pin;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use rustls::crypto::CryptoProvider;
use rustls::{ClientConfig, ServerConfig};

pub use identity::TlsIdentity;
pub use pin::SharedPolicy;

use crate::error::LinkError;
use crate::fingerprint::CertFingerprint;
use crate::transport::PeerPolicy;
use identity::tls_err;
use pin::PolicyVerifier;

/// The crypto provider every config here uses.
pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn alpn_list(alpn: &[&[u8]]) -> Vec<Vec<u8>> {
    alpn.iter().map(|p| p.to_vec()).collect()
}

/// TLS 1.3 client config trusting only `pin`, without a client certificate, plus the flag its
/// verifier sets on a mismatch (so callers can report [`LinkError::PinMismatch`]).
///
/// # Errors
/// Only if the provider lacks TLS 1.3 (never with aws-lc-rs).
pub fn pinned_client_config(
    pin: CertFingerprint,
    alpn: &[&[u8]],
) -> Result<(ClientConfig, Arc<AtomicBool>), LinkError> {
    let provider = provider();
    let policy = Arc::new(RwLock::new(PeerPolicy::Pinned(pin)));
    let verifier = Arc::new(PolicyVerifier::new(
        policy,
        provider.signature_verification_algorithms,
    ));
    let mismatch = Arc::clone(&verifier.mismatch);
    let mut cfg = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(tls_err)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    cfg.alpn_protocols = alpn_list(alpn);
    Ok((cfg, mismatch))
}

/// Mutual-TLS client config: presents `identity` and accepts only servers allowed by `policy`.
///
/// # Errors
/// [`LinkError::Tls`] for an unusable identity.
pub fn mutual_client_config(
    identity: &TlsIdentity,
    policy: PeerPolicy,
    alpn: &[&[u8]],
) -> Result<(ClientConfig, Arc<AtomicBool>), LinkError> {
    let provider = provider();
    let verifier = Arc::new(PolicyVerifier::new(
        Arc::new(RwLock::new(policy)),
        provider.signature_verification_algorithms,
    ));
    let mismatch = Arc::clone(&verifier.mismatch);
    let mut cfg = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(tls_err)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(identity.cert_chain(), identity.private_key())
        .map_err(tls_err)?;
    cfg.alpn_protocols = alpn_list(alpn);
    Ok((cfg, mismatch))
}

/// Mutual-TLS server config: presents `identity`, requires a client certificate and accepts only
/// clients the *current* value of `policy` allows (read at every handshake).
///
/// # Errors
/// [`LinkError::Tls`] for an unusable identity.
pub fn mutual_server_config(
    identity: &TlsIdentity,
    policy: SharedPolicy,
    alpn: &[&[u8]],
) -> Result<Arc<ServerConfig>, LinkError> {
    let provider = provider();
    let verifier = Arc::new(PolicyVerifier::new(
        policy,
        provider.signature_verification_algorithms,
    ));
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(tls_err)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(identity.cert_chain(), identity.private_key())
        .map_err(tls_err)?;
    cfg.alpn_protocols = alpn_list(alpn);
    Ok(Arc::new(cfg))
}
