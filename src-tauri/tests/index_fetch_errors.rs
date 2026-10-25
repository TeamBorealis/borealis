//! What a failed `.index` download tells the user (hw1-emu-1 open problem 5): the Indexes list and
//! the status bar show one short sentence per cause (no connection, an HTTP status, a damaged or
//! mismatched download), never the transport's error chain or the address. Invented content only;
//! the one real request goes to a closed port on this machine.

use borealis_lib::content_fetch_retry::{FetchError, RetryPolicy};
use borealis_lib::content_index::{sha256_hex, Fetcher, HttpsFetcher};
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const INDEX_URL: &str = "https://example.invalid/emu.index";
const FILE_URL: &str = "https://example.invalid/emu-book.xml";
/// What reqwest printed for the emulator's failed update (address invented).
const RAW_CHAIN: &str = "error sending request for url (https://example.invalid/emu.index): \
     client error (Connect): dns error: failed to lookup address information";

const NO_CONNECTION: &str =
    "Could not download the index file. Check your connection and try again.";

/// Serves scripted answers per URL: a body or a failure (404 when the URL is unknown).
#[derive(Default)]
struct Script(HashMap<String, Result<Vec<u8>, FetchError>>);

impl Script {
    fn fail(mut self, url: &str, e: FetchError) -> Self {
        self.0.insert(url.to_string(), Err(e));
        self
    }
    fn body(mut self, url: &str, body: &[u8]) -> Self {
        self.0.insert(url.to_string(), Ok(body.to_vec()));
        self
    }
}

impl Fetcher for Script {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max).map_err(|e| e.wire())
    }
    fn get_classified(&self, url: &str, _max: usize) -> Result<Vec<u8>, FetchError> {
        self.0
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err(FetchError::http(404, "Not Found", None)))
    }
}

fn app(index_url: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    state.add_index(index_url).expect("add");
    (dir, state)
}

/// One update (retries without real pauses); the index's problem and the failed addresses.
fn update(state: &AppState, fetcher: &dyn Fetcher) -> (String, Vec<String>) {
    let no_pause = |_: Duration| {};
    let result = state
        .update_indexes_with(fetcher, &NullSink, RetryPolicy::default(), &no_pause)
        .expect("update");
    let problem = state.list_indexes()[0].problem.clone().unwrap_or_default();
    (problem, result.failed)
}

fn json_index(hash: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "format": "borealis-index/1",
        "info": { "name": "Emu Index" },
        "files": [{ "name": "emu-book.xml", "url": FILE_URL, "extra": { "attributes": { "sha256": hash } } }]
    }))
    .unwrap()
}

fn assert_plain(problem: &str) {
    for raw in [
        "error sending",
        "client error",
        "dns error",
        "lookup",
        "Connection refused",
        "example.invalid/emu.index",
        "127.0.0.1",
    ] {
        assert!(!problem.contains(raw), "{raw:?} leaked into {problem:?}");
    }
}

#[test]
fn a_failed_connection_says_check_your_connection() {
    let (_d, state) = app(INDEX_URL);
    let script = Script::default().fail(INDEX_URL, FetchError::network(RAW_CHAIN));
    let (problem, failed) = update(&state, &script);
    assert_eq!(failed, vec![INDEX_URL.to_string()]);
    assert_eq!(problem, NO_CONNECTION);
}

#[test]
fn a_timeout_or_a_cut_download_is_a_connection_problem_too() {
    for e in [
        FetchError::timeout("operation timed out"),
        FetchError::truncated("received 10 of 99 bytes"),
    ] {
        let (_d, state) = app(INDEX_URL);
        let (problem, _) = update(&state, &Script::default().fail(INDEX_URL, e));
        assert_eq!(problem, NO_CONNECTION);
    }
}

#[test]
fn http_statuses_get_their_own_short_sentences() {
    let cases = [
        (
            FetchError::http(404, "Not Found", None),
            "Could not download the index file. The server has no file at this address (error 404).",
        ),
        (
            FetchError::http(403, "Forbidden", None),
            "Could not download the index file. The server refused access (error 403).",
        ),
        (
            FetchError::http(503, "Service Unavailable", None),
            "Could not download the index file. The server is not available right now (error 503). Try again later.",
        ),
        (
            FetchError::http(429, "Too Many Requests", None),
            "Could not download the index file. The server is limiting downloads. Try again later.",
        ),
    ];
    for (e, want) in cases {
        let (_d, state) = app(INDEX_URL);
        let (problem, _) = update(&state, &Script::default().fail(INDEX_URL, e));
        assert_eq!(problem, want);
    }
}

#[test]
fn a_damaged_index_says_so_without_the_parser_detail() {
    let (_d, state) = app("https://example.invalid/emu.index.json");
    let script = Script::default().body(
        "https://example.invalid/emu.index.json",
        b"{\"format\": \"borealis-index/1\", \"files\": [",
    );
    let (problem, _) = update(&state, &script);
    assert_eq!(
        problem,
        "Could not read the index file. The download is damaged or is not an index file."
    );
}

#[test]
fn a_file_that_does_not_match_the_index_is_named_plainly() {
    let index_url = "https://example.invalid/emu.index.json";
    let (_d, state) = app(index_url);
    let script = Script::default()
        .body(
            index_url,
            &json_index(&sha256_hex(b"what the index promised")),
        )
        .body(FILE_URL, b"<elements/>");
    let (problem, failed) = update(&state, &script);
    assert_eq!(failed, vec![FILE_URL.to_string()]);
    assert!(
        problem.contains("the downloaded file is damaged or has changed since the index was made"),
        "{problem}"
    );
    assert!(!problem.contains("SHA-256"), "{problem}");
}

#[test]
fn a_failed_file_reports_the_cause_without_the_transport_chain() {
    let index_url = "https://example.invalid/emu.index.json";
    let (_d, state) = app(index_url);
    let script = Script::default()
        .body(index_url, &json_index(&sha256_hex(b"<elements/>")))
        .fail(FILE_URL, FetchError::network(RAW_CHAIN));
    let (problem, failed) = update(&state, &script);
    assert_eq!(failed, vec![FILE_URL.to_string()]);
    assert!(problem.contains("could not connect"), "{problem}");
    for raw in ["error sending", "client error", "dns error"] {
        assert!(!problem.contains(raw), "{raw:?} leaked into {problem:?}");
    }
}

#[test]
fn the_real_client_maps_a_refused_connection_to_the_plain_sentence() {
    // Port 1 on this machine: refused at once, no network needed.
    let url = "https://127.0.0.1:1/emu.index";
    let (_d, state) = app(url);
    let (problem, failed) = update(&state, &HttpsFetcher);
    assert_eq!(failed, vec![url.to_string()]);
    assert_eq!(problem, NO_CONNECTION);
    assert_plain(&problem);
}

#[test]
fn the_real_client_keeps_the_chain_out_of_the_error_message() {
    let e = HttpsFetcher
        .get_classified("https://127.0.0.1:1/emu-book.xml", 1024)
        .expect_err("closed port");
    assert_eq!(
        (e.code.as_str(), e.message.as_str()),
        ("network", "could not connect")
    );
    assert_plain(&e.reason());
}
