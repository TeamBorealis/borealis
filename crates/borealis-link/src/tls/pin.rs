//! rustls verifiers that accept only certificates allowed by a [`PeerPolicy`] (normally exactly
//! one: the SHA-256 pinned at pairing). No WebPKI, no name checks; the TLS 1.3 handshake
//! signature is still verified, so the peer must hold the pinned certificate's private key.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, SignatureScheme};

use crate::fingerprint::CertFingerprint;
use crate::transport::PeerPolicy;

/// Policy shared with a running listener so it can change (pairing, revocation) without a
/// restart. Each handshake reads the current value.
pub type SharedPolicy = Arc<RwLock<PeerPolicy>>;

/// Verifies the peer's end-entity certificate against a policy; usable on both sides.
#[derive(Debug)]
pub(crate) struct PolicyVerifier {
    policy: SharedPolicy,
    algs: WebPkiSupportedAlgorithms,
    /// Set when a presented certificate was refused, so callers can report a typed error.
    pub(crate) mismatch: Arc<AtomicBool>,
}

impl PolicyVerifier {
    pub(crate) fn new(policy: SharedPolicy, algs: WebPkiSupportedAlgorithms) -> Self {
        Self {
            policy,
            algs,
            mismatch: Arc::new(AtomicBool::new(false)),
        }
    }

    fn check(&self, end_entity: &CertificateDer<'_>) -> Result<(), rustls::Error> {
        let fp = CertFingerprint::of_der(end_entity);
        let allowed = self
            .policy
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .allows(&fp);
        if allowed {
            return Ok(());
        }
        self.mismatch.store(true, Ordering::SeqCst);
        Err(rustls::Error::InvalidCertificate(
            CertificateError::ApplicationVerificationFailure,
        ))
    }

    fn tls12() -> Result<HandshakeSignatureValid, rustls::Error> {
        // Every config here is TLS 1.3 only; never accept a TLS 1.2 handshake.
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }
}

impl ServerCertVerifier for PolicyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity)
            .map(|()| ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::tls12()
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

impl ClientCertVerifier for PolicyVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.check(end_entity)
            .map(|()| ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::tls12()
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
