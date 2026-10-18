//! The retry layer against a scripted fetcher (no network, no real pauses).

use super::*;
use std::collections::{HashMap, VecDeque};

const URL: &str = "https://example.invalid/repo/a.xml";
const OTHER: &str = "https://other.invalid/repo/b.xml";

/// Serves each URL's scripted failures in order, then its body (404 when it has none).
#[derive(Default)]
struct Script {
    errors: Mutex<HashMap<String, VecDeque<FetchError>>>,
    bodies: HashMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl Script {
    fn fail(self, url: &str, errors: Vec<FetchError>) -> Self {
        self.errors
            .lock()
            .unwrap()
            .insert(url.to_string(), errors.into());
        self
    }
    fn body(mut self, url: &str) -> Self {
        self.bodies.insert(url.to_string(), b"<elements/>".to_vec());
        self
    }
    fn tries(&self, url: &str) -> usize {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|u| *u == url)
            .count()
    }
}

impl Fetcher for Script {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max).map_err(|e| e.wire())
    }
    fn get_classified(&self, url: &str, _max: usize) -> Result<Vec<u8>, FetchError> {
        self.asked.lock().unwrap().push(url.to_string());
        if let Some(e) = self
            .errors
            .lock()
            .unwrap()
            .get_mut(url)
            .and_then(VecDeque::pop_front)
        {
            return Err(e);
        }
        self.bodies
            .get(url)
            .cloned()
            .ok_or_else(|| FetchError::http(404, "Not Found", None))
    }
}

/// Run `f` with a retry layer whose pauses are recorded instead of slept.
fn with_retry<T>(
    script: &Script,
    policy: RetryPolicy,
    f: impl FnOnce(&Retrying<'_>) -> T,
) -> (T, Vec<Duration>) {
    let slept = Mutex::new(Vec::new());
    let sleep = |d: Duration| slept.lock().unwrap().push(d);
    let out = f(&Retrying::new(script, policy, &sleep));
    (out, slept.into_inner().unwrap())
}

fn secs(list: &[u64]) -> Vec<Duration> {
    list.iter().copied().map(Duration::from_secs).collect()
}

fn too_many(after: Option<u64>) -> FetchError {
    FetchError::http(429, "Too Many Requests", after.map(Duration::from_secs))
}

#[test]
fn a_429_waits_the_servers_retry_after_then_succeeds() {
    let script = Script::default()
        .fail(URL, vec![too_many(Some(7))])
        .body(URL);
    let (got, slept) = with_retry(&script, RetryPolicy::default(), |r| r.get(URL, 100));
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(slept, secs(&[7]));
    assert_eq!(script.tries(URL), 2);
}

#[test]
fn server_errors_are_retried_with_doubling_pauses() {
    let script = Script::default()
        .fail(
            URL,
            vec![
                FetchError::http(503, "Service Unavailable", None),
                FetchError::http(500, "Internal Server Error", None),
                FetchError::timeout("operation timed out"),
            ],
        )
        .body(URL);
    let (got, slept) = with_retry(&script, RetryPolicy::default(), |r| r.get(URL, 100));
    assert!(got.is_ok(), "{got:?}");
    assert_eq!(slept, secs(&[1, 2, 4]));
}

#[test]
fn the_attempt_limit_ends_with_the_last_code() {
    let truncated = FetchError::truncated("received 10 of 99 bytes");
    let script = Script::default().fail(URL, vec![truncated.clone(); 9]);
    let (got, slept) = with_retry(&script, RetryPolicy::default(), |r| {
        r.get_classified(URL, 100)
    });
    let e = got.unwrap_err();
    assert_eq!((e.code.as_str(), e.is_transient()), (CODE_TRUNCATED, true));
    assert_eq!(script.tries(URL), 4, "attempts are bounded");
    assert_eq!(slept, secs(&[1, 2, 4]));
}

#[test]
fn failures_that_will_not_pass_are_not_retried() {
    for e in [
        FetchError::http(404, "Not Found", None),
        FetchError::new(CODE_REFUSED, "refused a redirect to a http: address"),
        FetchError::too_large(),
        FetchError::from_message("not found"),
    ] {
        let script = Script::default().fail(URL, vec![e.clone(); 4]).body(URL);
        let (got, slept) = with_retry(&script, RetryPolicy::default(), |r| {
            r.get_classified(URL, 100)
        });
        assert_eq!(got.unwrap_err(), e);
        assert!(slept.is_empty(), "{e:?}");
        assert_eq!(script.tries(URL), 1);
    }
}

#[test]
fn a_retry_after_beyond_the_cap_is_not_waited_for() {
    let script = Script::default()
        .fail(URL, vec![too_many(Some(3_600))])
        .body(URL);
    let (got, slept) = with_retry(&script, RetryPolicy::default(), |r| {
        r.get_classified(URL, 100)
    });
    assert_eq!(got.unwrap_err().code, "http_429");
    assert!(slept.is_empty());
}

#[test]
fn one_pause_budget_serves_the_whole_update() {
    let policy = RetryPolicy {
        budget: Duration::from_secs(5),
        ..RetryPolicy::default()
    };
    let busy = || vec![FetchError::http(502, "Bad Gateway", None); 9];
    let script = Script::default()
        .fail(URL, busy())
        .fail("https://example.invalid/repo/c.xml", busy());
    let (_, slept) = with_retry(&script, policy, |r| {
        let _ = r.get(URL, 100);
        let _ = r.get("https://example.invalid/repo/c.xml", 100);
    });
    // a.xml: 1 + 2 s, then 4 s would exceed 5 s; c.xml: 1 s, then 2 s would exceed it.
    assert_eq!(slept, secs(&[1, 2, 1]));
    assert!(slept.iter().sum::<Duration>() <= policy.budget);
}

#[test]
fn a_host_still_rate_limiting_is_not_asked_again_this_update() {
    let next = "https://example.invalid/repo/c.xml";
    let script = Script::default()
        .fail(URL, vec![too_many(None); 9])
        .body(next)
        .body(OTHER);
    let (got, _) = with_retry(&script, RetryPolicy::default(), |r| {
        (
            r.get_classified(URL, 100),
            r.get_classified(next, 100),
            r.get_classified(OTHER, 100),
        )
    });
    assert_eq!(got.0.unwrap_err().code, "http_429");
    let skipped = got.1.unwrap_err();
    assert_eq!(skipped.code, CODE_RATE_LIMITED);
    assert!(
        skipped.is_transient(),
        "a skipped file is not a missing one"
    );
    assert_eq!(script.tries(next), 0);
    assert!(got.2.is_ok(), "other hosts are still asked");
}

#[test]
fn plain_messages_keep_their_code_and_retry_after() {
    let cases = [
        (too_many(Some(30)), "http_429", true),
        (
            FetchError::http(503, "Service Unavailable", None),
            "http_503",
            true,
        ),
        (FetchError::http(404, "Not Found", None), "http_404", false),
        (FetchError::timeout("deadline"), CODE_TIMEOUT, true),
        (FetchError::network("dns error"), CODE_NETWORK, true),
        (FetchError::truncated("eof"), CODE_TRUNCATED, true),
        (FetchError::rate_limited(None), CODE_RATE_LIMITED, true),
        (FetchError::too_large(), CODE_TOO_LARGE, false),
    ];
    for (e, code, transient) in cases {
        let back = FetchError::from_message(&e.wire());
        assert_eq!(back, e, "{}", e.wire());
        assert_eq!((back.code.as_str(), back.is_transient()), (code, transient));
    }
    assert_eq!(
        too_many(Some(30)).wire(),
        "the server answered 429 Too Many Requests (retry after 30 s)"
    );
    assert_eq!(
        too_many(None).reason(),
        "the server answered 429 Too Many Requests [http_429]"
    );
    let plain = FetchError::from_message("the server answered 404 Not Found");
    assert_eq!(plain.code, "http_404");
    assert_eq!(FetchError::from_message("not found").code, CODE_OTHER);
}

#[test]
fn a_body_shorter_than_announced_is_truncated() {
    assert_eq!(
        check_complete(10, Some(99)).unwrap_err().code,
        CODE_TRUNCATED
    );
    assert!(check_complete(99, Some(99)).is_ok());
    assert!(check_complete(10, None).is_ok(), "no length announced");
    assert_eq!(parse_retry_after(" 12 "), Some(Duration::from_secs(12)));
    assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
}
