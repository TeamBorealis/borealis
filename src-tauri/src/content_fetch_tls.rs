//! TLS setup for content downloads (`.index` updates).
//!
//! Desktop keeps reqwest's default rustls client, which verifies with the OS trust store. Phones
//! use the Mozilla root set compiled into the app (webpki-roots): the OS verifier needs JNI glue on
//! Android that the app does not have (spike S0.3). Trade-off: user-installed and MDM roots are not
//! trusted on phones, and root updates ship with app releases.

/// A client builder with the platform's TLS configuration.
///
/// # Errors
/// Never on desktop; on phones, the reason the TLS configuration could not be built.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn client_builder() -> Result<reqwest::blocking::ClientBuilder, String> {
    Ok(reqwest::blocking::Client::builder())
}

/// A client builder with the platform's TLS configuration.
///
/// # Errors
/// The reason the TLS configuration could not be built.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn client_builder() -> Result<reqwest::blocking::ClientBuilder, String> {
    use std::sync::Arc;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("could not set up a secure connection: {e}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(reqwest::blocking::Client::builder().tls_backend_preconfigured(config))
}
