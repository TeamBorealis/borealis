//! rustls verifier that accepts exactly one certificate: the one whose SHA-256 was pinned at
//! pairing. No WebPKI, no name checks; the TLS 1.3 handshake signature is still verified, so the
//! peer must hold the pinned certificate's private key.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls13_signature, CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, SignatureScheme};

use crate::protocol::CertFingerprint;

#[derive(Debug)]
pub(super) struct PinnedVerifier {
    pin: CertFingerprint,
    algs: WebPkiSupportedAlgorithms,
    /// Set when a presented certificate did not match, so callers can report a typed error.
    pub(super) mismatch: Arc<AtomicBool>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if CertFingerprint::of_der(end_entity).ct_eq(&self.pin) {
            return Ok(ServerCertVerified::assertion());
        }
        self.mismatch.store(true, Ordering::SeqCst);
        Err(rustls::Error::InvalidCertificate(
            CertificateError::ApplicationVerificationFailure,
        ))
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // The client config is TLS 1.3 only; never accept a TLS 1.2 handshake.
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// TLS 1.3 client config trusting only `pin`, plus the mismatch flag its verifier sets.
pub(super) fn pinned_config(
    pin: CertFingerprint,
) -> Result<(ClientConfig, Arc<AtomicBool>), rustls::Error> {
    let provider: Arc<CryptoProvider> = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mismatch = Arc::new(AtomicBool::new(false));
    let verifier = Arc::new(PinnedVerifier {
        pin,
        algs: provider.signature_verification_algorithms,
        mismatch: Arc::clone(&mismatch),
    });
    let mut cfg = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok((cfg, mismatch))
}
