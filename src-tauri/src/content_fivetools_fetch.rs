//! The 5etools download client: the real HTTPS client and the retry layer around any
//! [`RemoteFetcher`]. Desktop and phones share [`HttpsFetcher`] (only its TLS roots differ, see
//! [`crate::content_fetch_tls`]) and the retry rules of Update indexes
//! ([`crate::content_fetch_retry`]): a failure that may pass (408/425/429/5xx, a timeout, a dropped
//! connection, a body shorter than announced) is tried again after 1/2/4 s or the server's
//! `Retry-After` (up to 60 s), under one pause budget per download. Deterministic: no AI.

use crate::content_fetch_retry::{
    check_complete, parse_retry_after, FetchError, RetryPolicy, RetryRun, Sleep, CODE_OTHER,
    CODE_REFUSED,
};
use crate::content_fivetools_remote::{gunzip, FetchRequest, FetchResponse, RemoteFetcher};
use crate::content_index::{require_https, send_error, shared_client, HttpsFetcher, USER_AGENT};
use std::time::Duration;

const HTTP_FORBIDDEN: u16 = 403;

/// An answer whose status means "not now" (408/425/429/5xx, or a 403 that says no requests are
/// left) as a classified failure; any other answer is passed on.
///
/// # Errors
/// The transient status, with `retry_after`.
pub fn classify_answer(
    response: FetchResponse,
    retry_after: Option<Duration>,
) -> Result<FetchResponse, FetchError> {
    if response.status == HTTP_FORBIDDEN && response.rate_limited {
        return Err(FetchError::rate_limited(retry_after));
    }
    let reason = reqwest::StatusCode::from_u16(response.status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or_default();
    let e = FetchError::http(response.status, reason, retry_after);
    if e.is_transient() {
        Err(e)
    } else {
        Ok(response)
    }
}

/// A [`RemoteFetcher`] that retries another one. One instance per download (stage or Update): the
/// pause budget and the rate-limited hosts are shared by every file of it.
///
/// [`RemoteFetcher::fetch`] is passed through with one try (the GitHub API probe has its own
/// fallback and must not wait); [`RemoteFetcher::fetch_classified`] retries.
pub struct RetryingRemote<'a> {
    inner: &'a dyn RemoteFetcher,
    run: RetryRun<'a>,
}

impl<'a> RetryingRemote<'a> {
    #[must_use]
    pub fn new(inner: &'a dyn RemoteFetcher, policy: RetryPolicy, sleep: Sleep<'a>) -> Self {
        Self {
            inner,
            run: RetryRun::new(policy, sleep),
        }
    }
}

impl RemoteFetcher for RetryingRemote<'_> {
    fn fetch(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, String> {
        self.inner.fetch(request)
    }

    fn fetch_classified(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, FetchError> {
        self.run
            .run(request.url, || self.inner.fetch_classified(request))
    }
}

impl HttpsFetcher {
    /// One request: any status is an answer (with its `Retry-After`); a refused address, a failed
    /// connection, a timeout, a body that ended early or is too large is a classified failure.
    fn fetch_once(
        request: &FetchRequest<'_>,
    ) -> Result<(FetchResponse, Option<Duration>), FetchError> {
        use std::io::Read;
        require_https(request.url).map_err(|e| FetchError::new(CODE_REFUSED, e.message))?;
        let client = shared_client().map_err(|m| FetchError::new(CODE_OTHER, m))?;
        let mut call = client
            .get(request.url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT_ENCODING, "gzip");
        if let Some(accept) = request.accept {
            call = call.header(reqwest::header::ACCEPT, accept);
        }
        if let Some(tag) = request.if_none_match {
            call = call.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let response = call.send().map_err(|e| send_error(&e))?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let etag = header("etag");
        let retry_after = header(reqwest::header::RETRY_AFTER.as_str())
            .as_deref()
            .and_then(parse_retry_after);
        let gzip = header("content-encoding").is_some_and(|e| e.eq_ignore_ascii_case("gzip"));
        let rate_limited = status == 429
            || (status == HTTP_FORBIDDEN
                && header("x-ratelimit-remaining").as_deref() == Some("0"));
        if status != 200 {
            let answer = FetchResponse {
                status,
                etag,
                body: Vec::new(),
                rate_limited,
            };
            return Ok((answer, retry_after));
        }
        let announced = response.content_length();
        let mut body = Vec::new();
        response
            .take(request.max_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| FetchError::truncated(&e.to_string()))?;
        if body.len() > request.max_bytes {
            return Err(FetchError::too_large());
        }
        check_complete(body.len(), announced)?;
        let body = if gzip {
            // A stream cut short without a length decodes as damaged: try it again.
            gunzip(&body, request.max_bytes).map_err(|why| FetchError::truncated(&why))?
        } else {
            body
        };
        let answer = FetchResponse {
            status,
            etag,
            body,
            rate_limited,
        };
        Ok((answer, retry_after))
    }
}

impl RemoteFetcher for HttpsFetcher {
    fn fetch(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, String> {
        Self::fetch_once(request)
            .map(|(answer, _)| answer)
            .map_err(|e| e.wire())
    }

    fn fetch_classified(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, FetchError> {
        Self::fetch_once(request)
            .and_then(|(answer, retry_after)| classify_answer(answer, retry_after))
    }
}
