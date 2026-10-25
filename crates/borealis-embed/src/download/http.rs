//! The real [`Transport`]: a blocking `reqwest` client, HTTPS only.
//!
//! The app hands in a `ClientBuilder` with its platform TLS set-up (desktop: rustls with the OS verifier; phones:
//! rustls with the bundled Mozilla roots, spike S0.3) and this module locks it down:
//! * HTTPS only, for the first request and every redirect (no TLS downgrade); at most [`MAX_REDIRECTS`] hops
//!   (Hugging Face answers `resolve/` with a redirect to its CDN);
//! * no `Referer` on redirects, no cookies, no `User-Agent`, no other custom header: a request carries `Host`,
//!   reqwest's `Accept: */*` and, when resuming, `Range` (spec sec. 8: no identifiers);
//! * a per-read timeout ([`READ_TIMEOUT`]) instead of reqwest's whole-request default, so a 310 MB body can
//!   stream while a stalled connection still fails.

use super::{Response, Transport, TransportError};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::redirect::Policy;
use std::time::Duration;

/// Redirect hops a download follows.
pub const MAX_REDIRECTS: usize = 5;
/// Longest wait for one read (or for the response headers).
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest wait for a connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// `reqwest` transport for model downloads.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    client: Client,
    https_only: bool,
}

impl HttpTransport {
    /// Lock down `builder` (see the module docs) and build the client.
    ///
    /// # Errors
    /// The client could not be built (TLS set-up).
    pub fn new(builder: ClientBuilder) -> Result<Self, TransportError> {
        Self::build(builder, true)
    }

    /// Plain-HTTP client for the crate's loopback test server only.
    #[cfg(test)]
    pub(crate) fn loopback_for_tests() -> Self {
        Self::build(Client::builder(), false).expect("test client")
    }

    fn build(builder: ClientBuilder, https_only: bool) -> Result<Self, TransportError> {
        let redirects = Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if https_only && attempt.url().scheme() != "https" {
                attempt.error("redirect to a non-HTTPS address")
            } else {
                attempt.follow()
            }
        });
        let client = builder
            .https_only(https_only)
            .redirect(redirects)
            .referer(false)
            .timeout(READ_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|_| TransportError("could not set up a secure connection".into()))?;
        Ok(Self { client, https_only })
    }
}

/// Plain words for a reqwest failure (its Display can carry the URL). The error chain, without the
/// address, goes to the log only.
fn plain(e: &reqwest::Error) -> String {
    let mut chain = Vec::new();
    let mut cause = std::error::Error::source(e);
    while let Some(c) = cause {
        chain.push(c.to_string());
        cause = c.source();
    }
    tracing::info!(
        timeout = e.is_timeout(),
        connect = e.is_connect(),
        "model download request failed: {}",
        chain.join(": ")
    );
    if e.is_timeout() {
        "the server did not answer in time".into()
    } else if e.is_redirect() {
        "the server redirected to an address that is not allowed".into()
    } else if e.is_connect() {
        "could not connect; check your connection".into()
    } else {
        "the request failed".into()
    }
}

impl Transport for HttpTransport {
    fn get(&self, url: &str, from: u64) -> Result<Response, TransportError> {
        if self.https_only && !url.starts_with("https://") {
            return Err(TransportError("only HTTPS addresses are allowed".into()));
        }
        let mut request = self.client.get(url);
        if from > 0 {
            request = request.header(RANGE, format!("bytes={from}-"));
        }
        let response = request.send().map_err(|e| TransportError(plain(&e)))?;
        Ok(Response {
            status: response.status().as_u16(),
            content_range: response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            content_length: response.content_length(),
            body: Box::new(response),
        })
    }
}
