//! Classified download failures and bounded retries for Update indexes (desktop and phones share
//! [`crate::content_index::HttpsFetcher`], so the rules are the same everywhere).
//!
//! A failure that may pass (HTTP 408/425/429/5xx, a timeout, a failed connection, a body that ended
//! early) is tried again up to [`RetryPolicy::attempts`] times, after a doubling pause or the
//! server's `Retry-After` (capped). All pauses of one update share one budget, and a host that is
//! still rate limiting after the retries is not asked again during that update, so a refusing
//! server cannot hold the update for long. What still fails carries a stable code (`http_429`,
//! `timeout`, ...) next to its address; callers report it and never install a partial Aurora index
//! because of it.

use crate::content_index::{Fetcher, Tagged};
use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// The server did not answer in time.
pub const CODE_TIMEOUT: &str = "timeout";
/// No connection could be made, or it failed before an answer.
pub const CODE_NETWORK: &str = "network";
/// The body ended before the length the server announced.
pub const CODE_TRUNCATED: &str = "truncated";
/// The server says the client is rate limited (a 403 with no requests left).
pub const CODE_RATE_LIMITED: &str = "rate_limited";
/// The body is larger than the cap.
pub const CODE_TOO_LARGE: &str = "too_large";
/// The address or a redirect was refused (not https://, too many hops).
pub const CODE_REFUSED: &str = "refused";
/// Anything else (a test stub's message, a client that could not be built).
pub const CODE_OTHER: &str = "other";

const ANSWERED: &str = "the server answered ";
const RETRY_HINT: &str = " (retry after ";
const TIMED_OUT: &str = "the download timed out";
const NO_CONNECTION: &str = "could not connect";
const ENDED_EARLY: &str = "the download ended early";
const RATE_LIMITED: &str = "the server is rate limiting downloads";
const TOO_LARGE: &str = "the file is too large";
const HTTP_TOO_MANY: u16 = 429;

/// Why one download failed: a stable code, a user-presentable message and the server's
/// `Retry-After`, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    pub code: String,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl FetchError {
    /// A failure with `code` and `message`.
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retry_after: None,
        }
    }

    /// The server answered `status` (`reason` is its canonical phrase, may be empty).
    #[must_use]
    pub fn http(status: u16, reason: &str, retry_after: Option<Duration>) -> Self {
        Self {
            code: format!("http_{status}"),
            message: format!("{ANSWERED}{status} {reason}")
                .trim_end()
                .to_string(),
            retry_after,
        }
    }

    /// The download timed out (`detail` is the transport's text).
    #[must_use]
    pub fn timeout(detail: &str) -> Self {
        Self::new(CODE_TIMEOUT, format!("{TIMED_OUT}: {detail}"))
    }

    /// No connection, or it failed before an answer.
    #[must_use]
    pub fn network(detail: &str) -> Self {
        Self::new(CODE_NETWORK, format!("{NO_CONNECTION}: {detail}"))
    }

    /// The body ended early.
    #[must_use]
    pub fn truncated(detail: &str) -> Self {
        Self::new(CODE_TRUNCATED, format!("{ENDED_EARLY}: {detail}"))
    }

    /// A 403 that says no requests are left.
    #[must_use]
    pub fn rate_limited(retry_after: Option<Duration>) -> Self {
        Self {
            retry_after,
            ..Self::new(CODE_RATE_LIMITED, RATE_LIMITED)
        }
    }

    /// The body exceeds the cap.
    #[must_use]
    pub fn too_large() -> Self {
        Self::new(CODE_TOO_LARGE, TOO_LARGE)
    }

    /// Classify a plain message (from [`Fetcher::get`]); the inverse of [`FetchError::wire`] for
    /// the messages [`crate::content_index::HttpsFetcher`] produces. Unknown text is
    /// [`CODE_OTHER`], which is never retried.
    #[must_use]
    pub fn from_message(text: &str) -> Self {
        let (message, retry_after) = split_retry_hint(text);
        let lower = message.to_lowercase();
        let status = lower
            .strip_prefix(ANSWERED)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|s| s.parse::<u16>().ok());
        let code = match status {
            Some(status) => format!("http_{status}"),
            None => [
                (TIMED_OUT, CODE_TIMEOUT),
                (NO_CONNECTION, CODE_NETWORK),
                (ENDED_EARLY, CODE_TRUNCATED),
                (RATE_LIMITED, CODE_RATE_LIMITED),
                (TOO_LARGE, CODE_TOO_LARGE),
                ("redirect", CODE_REFUSED),
                ("only https://", CODE_REFUSED),
            ]
            .iter()
            .find(|(needle, _)| lower.contains(needle))
            .map_or(CODE_OTHER, |(_, code)| code)
            .to_string(),
        };
        Self {
            code,
            message: message.to_string(),
            retry_after,
        }
    }

    /// The HTTP status, when the server answered.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        self.code.strip_prefix("http_")?.parse().ok()
    }

    /// True when trying again later may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self.status() {
            Some(status) => matches!(status, 408 | 425 | HTTP_TOO_MANY | 500..=599),
            None => matches!(
                self.code.as_str(),
                CODE_TIMEOUT | CODE_NETWORK | CODE_TRUNCATED | CODE_RATE_LIMITED
            ),
        }
    }

    /// A 429, or a 403 that says no requests are left.
    #[must_use]
    pub fn is_rate_limit(&self) -> bool {
        self.status() == Some(HTTP_TOO_MANY) || self.code == CODE_RATE_LIMITED
    }

    /// What the Indexes list and the report show: the message and its code.
    #[must_use]
    pub fn reason(&self) -> String {
        format!("{} [{}]", self.message, self.code)
    }

    /// The plain message [`Fetcher::get`] returns (the `Retry-After` survives as a hint).
    #[must_use]
    pub fn wire(&self) -> String {
        match self.retry_after {
            Some(d) => format!("{}{RETRY_HINT}{} s)", self.message, d.as_secs()),
            None => self.message.clone(),
        }
    }
}

/// `"... (retry after 30 s)"` -> (`"..."`, 30 s).
fn split_retry_hint(text: &str) -> (&str, Option<Duration>) {
    let Some((message, hint)) = text.rsplit_once(RETRY_HINT) else {
        return (text, None);
    };
    match hint.strip_suffix(" s)").and_then(|s| s.parse::<u64>().ok()) {
        Some(secs) => (message, Some(Duration::from_secs(secs))),
        None => (text, None),
    }
}

/// A `Retry-After` header value in seconds (the HTTP-date form is not used by the hosts we meet
/// and falls back to the normal pause).
#[must_use]
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// `Err(truncated)` when a body of `got` bytes is shorter than the announced `Content-Length`.
///
/// # Errors
/// [`CODE_TRUNCATED`].
pub fn check_complete(got: usize, announced: Option<u64>) -> Result<(), FetchError> {
    match announced {
        Some(n) if (got as u64) < n => Err(FetchError::truncated(&format!(
            "received {got} of {n} bytes"
        ))),
        _ => Ok(()),
    }
}

/// How hard one update tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Tries per download, the first included.
    pub attempts: u32,
    /// Pause after the first failure; it doubles per try up to `max_pause`.
    pub first_pause: Duration,
    pub max_pause: Duration,
    /// Longest `Retry-After` honoured; a longer one ends the retries for that file.
    pub max_retry_after: Duration,
    /// All pauses of one update together.
    pub budget: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 4,
            first_pause: Duration::from_secs(1),
            max_pause: Duration::from_secs(8),
            max_retry_after: Duration::from_secs(60),
            budget: Duration::from_secs(120),
        }
    }
}

impl RetryPolicy {
    /// The pause before the next try after try number `tried` failed with `e`, having paused
    /// `spent` so far; `None` = give up.
    #[must_use]
    pub fn pause(&self, tried: u32, e: &FetchError, spent: Duration) -> Option<Duration> {
        if tried >= self.attempts || !e.is_transient() {
            return None;
        }
        let pause = match e.retry_after {
            Some(d) if d > self.max_retry_after => return None,
            Some(d) => d,
            None => self
                .first_pause
                .saturating_mul(1 << tried.saturating_sub(1).min(16))
                .min(self.max_pause),
        };
        (spent + pause <= self.budget).then_some(pause)
    }
}

/// How the retry layer waits (the real update sleeps; tests record).
pub type Sleep<'a> = &'a (dyn Fn(Duration) + Send + Sync);

/// The retry state of one update: the policy, the pause function, the pause budget spent so far
/// and the hosts that are still rate limiting. Every download of the update goes through the same
/// instance, whatever client it uses ([`Retrying`] for index files, the 5etools
/// [`crate::content_fivetools_fetch::RetryingRemote`] for 5etools folders).
pub struct RetryRun<'a> {
    policy: RetryPolicy,
    sleep: Sleep<'a>,
    spent: Mutex<Duration>,
    limited: Mutex<BTreeSet<String>>,
}

impl<'a> RetryRun<'a> {
    #[must_use]
    pub fn new(policy: RetryPolicy, sleep: Sleep<'a>) -> Self {
        Self {
            policy,
            sleep,
            spent: Mutex::new(Duration::ZERO),
            limited: Mutex::new(BTreeSet::new()),
        }
    }

    fn host_limited(&self, host: &str) -> bool {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(host)
    }

    /// Run `attempt` (one download of `url`) until it succeeds, fails for a reason that will not
    /// pass, or the policy gives up.
    ///
    /// # Errors
    /// The last failure; [`CODE_RATE_LIMITED`] without a try when `url`'s host is still limiting.
    pub fn run<T>(
        &self,
        url: &str,
        mut attempt: impl FnMut() -> Result<T, FetchError>,
    ) -> Result<T, FetchError> {
        let host = crate::content_index_aurora::host_of(url);
        if self.host_limited(&host) {
            return Err(FetchError::new(
                CODE_RATE_LIMITED,
                format!("{RATE_LIMITED}; not asked again during this update"),
            ));
        }
        let mut tried = 0;
        loop {
            tried += 1;
            let e = match attempt() {
                Ok(body) => return Ok(body),
                Err(e) => e,
            };
            let mut spent = self.spent.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(pause) = self.policy.pause(tried, &e, *spent) else {
                drop(spent);
                tracing::warn!(url, code = %e.code, tries = tried, "download failed: {}", e.message);
                if e.is_rate_limit() {
                    self.limited
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(host);
                }
                return Err(e);
            };
            *spent += pause;
            drop(spent);
            tracing::info!(url, code = %e.code, tries = tried, pause_ms = pause.as_millis() as u64, "retrying a download");
            (self.sleep)(pause);
        }
    }
}

/// A [`Fetcher`] that retries another one under a [`RetryPolicy`]. One instance per update: the
/// pause budget and the rate-limited hosts are shared by every download of that update.
pub struct Retrying<'a> {
    inner: &'a dyn Fetcher,
    run: RetryRun<'a>,
}

impl<'a> Retrying<'a> {
    #[must_use]
    pub fn new(inner: &'a dyn Fetcher, policy: RetryPolicy, sleep: Sleep<'a>) -> Self {
        Self {
            inner,
            run: RetryRun::new(policy, sleep),
        }
    }
}

impl Fetcher for Retrying<'_> {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max_bytes).map_err(|e| e.wire())
    }

    fn get_classified(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        self.run
            .run(url, || self.inner.get_classified(url, max_bytes))
    }

    fn get_tagged(
        &self,
        url: &str,
        max_bytes: usize,
        etag: Option<&str>,
    ) -> Result<Tagged, FetchError> {
        self.run
            .run(url, || self.inner.get_tagged(url, max_bytes, etag))
    }
}

#[cfg(test)]
#[path = "content_fetch_retry_tests.rs"]
mod tests;
