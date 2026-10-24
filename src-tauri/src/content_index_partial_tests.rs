//! Partial fetches of an Aurora index tree (det-src-3): a fake fetcher injects 429, 5xx, timeouts,
//! truncated bodies and a failed nested index. A failure that may pass is retried; one that still
//! fails holds the whole index (nothing installed or replaced, every file reported with its code),
//! and the next Update fills the gap. All content is invented; nothing touches the network.

use crate::content_fetch_retry::{FetchError, RetryPolicy, CODE_TRUNCATED};
use crate::content_index::Fetcher;
use crate::content_packs::NullSink;
use crate::state::{AppPaths, AppState};
use borealis_content::IssueCode;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

const BASE: &str = "https://example.invalid/repo/master/";

fn url(rel: &str) -> String {
    format!("{BASE}{rel}")
}

fn index(name: &str, files: &[&str]) -> Vec<u8> {
    let rows: String = files
        .iter()
        .map(|f| format!("    <file name=\"{f}\" url=\"{}\" />\n", url(f)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<index>\n  <info><name>{name}</name></info>\n  <files>\n{rows}  </files>\n</index>"
    )
    .into_bytes()
}

fn feat(id: &str, text: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Feat {id}" type="Feat" source="Invented" id="ID_INVENTED_FEAT_{id}">
    <description><p>{text}</p></description>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#
    )
    .into_bytes()
}

/// top.index -> book.index -> book/a.xml, book/b.xml; failures are injected per URL.
struct Fake {
    files: HashMap<String, Vec<u8>>,
    /// Served in order before the file; the last one repeats forever when `stuck`.
    errors: Mutex<HashMap<String, (VecDeque<FetchError>, bool)>>,
}

impl Fake {
    fn repo(b_text: &str) -> Self {
        let files = HashMap::from([
            (url("top.index"), index("Invented Repo", &["book.index"])),
            (
                url("book.index"),
                index("Book", &["book/a.xml", "book/b.xml"]),
            ),
            (url("book/a.xml"), feat("A", "first")),
            (url("book/b.xml"), feat("B", b_text)),
        ]);
        Self {
            files,
            errors: Mutex::new(HashMap::new()),
        }
    }
    /// `url` fails with `e` `times` times, then answers.
    fn flaky(self, rel: &str, e: FetchError, times: usize) -> Self {
        let queue = std::iter::repeat_n(e, times).collect();
        self.errors.lock().unwrap().insert(url(rel), (queue, false));
        self
    }
    /// `url` always fails with `e`.
    fn down(self, rel: &str, e: FetchError) -> Self {
        let queue = VecDeque::from([e]);
        self.errors.lock().unwrap().insert(url(rel), (queue, true));
        self
    }
}

impl Fetcher for Fake {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max).map_err(|e| e.wire())
    }
    fn get_classified(&self, url: &str, _max: usize) -> Result<Vec<u8>, FetchError> {
        if let Some((queue, stuck)) = self.errors.lock().unwrap().get_mut(url) {
            let next = if *stuck {
                queue.front().cloned()
            } else {
                queue.pop_front()
            };
            if let Some(e) = next {
                return Err(e);
            }
        }
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| FetchError::http(404, "Not Found", None))
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    let state = AppState::new(paths).unwrap();
    state.add_index(&url("top.index")).unwrap();
    (dir, state)
}

/// One update with recorded (not slept) pauses.
fn update(
    state: &AppState,
    fetcher: &dyn Fetcher,
) -> (crate::content_dto::UpdateIndexesResult, Vec<Duration>) {
    let slept = Mutex::new(Vec::new());
    let sleep = |d: Duration| slept.lock().unwrap().push(d);
    let result = state
        .update_indexes_with(fetcher, &NullSink, RetryPolicy::default(), &sleep)
        .unwrap();
    (result, slept.into_inner().unwrap())
}

fn has(state: &AppState, id: &str) -> bool {
    state.hub().unwrap().store.contains_id(id)
}

fn text_of(state: &AppState, id: &str) -> String {
    let hub = state.hub().unwrap();
    format!("{:?}", hub.store.element(id).map(|e| &e.description))
}

fn problem(state: &AppState) -> String {
    state.list_indexes()[0].problem.clone().unwrap_or_default()
}

fn too_many() -> FetchError {
    FetchError::http(429, "Too Many Requests", Some(Duration::from_secs(5)))
}

#[test]
fn flaky_downloads_are_retried_and_the_index_installs_complete() {
    let (_d, state) = app();
    let fake = Fake::repo("second")
        .flaky("top.index", FetchError::network("connection reset"), 1)
        .flaky("book.index", FetchError::timeout("deadline"), 1)
        .flaky("book/a.xml", too_many(), 2)
        .flaky(
            "book/b.xml",
            FetchError::http(503, "Service Unavailable", None),
            3,
        );
    let (result, slept) = update(&state, &fake);
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.updated, 1);
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
    assert_eq!(problem(&state), "");
    let waited: Vec<u64> = slept.iter().map(Duration::as_secs).collect();
    assert_eq!(
        waited,
        [1, 1, 5, 5, 1, 2, 4],
        "Retry-After for 429, doubling otherwise"
    );
}

/// Each failure that may pass, still failing after the retries, holds a first install.
#[test]
fn a_file_that_keeps_failing_holds_the_first_install_and_is_reported() {
    let cases = [
        (too_many(), "http_429"),
        (FetchError::http(502, "Bad Gateway", None), "http_502"),
        (FetchError::timeout("deadline"), "timeout"),
        (
            FetchError::truncated("received 10 of 999 bytes"),
            CODE_TRUNCATED,
        ),
    ];
    for (e, code) in cases {
        let (_d, state) = app();
        let fake = Fake::repo("second").down("book/b.xml", e);
        let (result, _) = update(&state, &fake);
        assert_eq!(result.failed, vec![url("book/b.xml")], "{code}");
        assert_eq!(result.updated, 0, "{code}: nothing installed");
        assert!(
            !has(&state, "ID_INVENTED_FEAT_A"),
            "{code}: no partial pack"
        );
        let problem = problem(&state);
        assert!(
            problem.starts_with("1 of 3 files could not be downloaded"),
            "{problem}"
        );
        assert!(problem.contains(&format!("[{code}]")), "{problem}");
        assert!(problem.contains(&url("book/b.xml")), "{problem}");
        let [report] = result.reports.as_slice() else {
            panic!("one report: {:?}", result.reports);
        };
        assert_eq!(report.converted, 0);
        let issue = &report.issues[0];
        assert_eq!(issue.file.as_deref(), Some(url("book/b.xml").as_str()));
        assert_eq!(
            (issue.severity, &issue.code),
            ("error", &IssueCode::IndexEntryMissing)
        );
        assert!(
            issue.message.contains(&format!("[{code}]")),
            "{}",
            issue.message
        );
        assert!(state.list_indexes()[0].updates_available > 0);
    }
}

#[test]
fn a_nested_index_that_keeps_failing_holds_the_install() {
    let (_d, state) = app();
    let fake = Fake::repo("second").down("book.index", FetchError::http(500, "", None));
    let (result, _) = update(&state, &fake);
    assert_eq!(result.failed, vec![url("book.index")]);
    assert_eq!(result.updated, 0);
    assert!(!has(&state, "ID_INVENTED_FEAT_A"));
    let problem = problem(&state);
    assert!(
        problem.starts_with("1 of 1 files could not be downloaded"),
        "{problem}"
    );
    assert!(problem.contains("[http_500]"), "{problem}");
}

#[test]
fn a_failed_update_never_removes_or_replaces_installed_content() {
    let (_d, state) = app();
    let (first, _) = update(&state, &Fake::repo("second"));
    assert_eq!((first.updated, first.failed.len()), (1, 0));
    // b.xml changed upstream, but a.xml is rate limited: the installed pack stays as it was.
    let fake = Fake::repo("rewritten").down("book/a.xml", too_many());
    let (second, _) = update(&state, &fake);
    assert_eq!(second.updated, 0);
    // The host is not asked again once it still rate limits, so b.xml is reported too.
    assert_eq!(second.failed, vec![url("book/a.xml"), url("book/b.xml")]);
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
    assert!(text_of(&state, "ID_INVENTED_FEAT_B").contains("second"));
    // A nested index that is down keeps everything too.
    let fake = Fake::repo("rewritten").down("book.index", FetchError::timeout("deadline"));
    let (third, _) = update(&state, &fake);
    assert_eq!(third.updated, 0);
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
}

#[test]
fn the_next_update_fills_the_gap() {
    let (_d, state) = app();
    let (held, _) = update(&state, &Fake::repo("second").down("book/b.xml", too_many()));
    assert_eq!(held.updated, 0);
    let (healthy, _) = update(&state, &Fake::repo("second"));
    assert_eq!((healthy.updated, healthy.failed.len()), (1, 0));
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
    assert_eq!(problem(&state), "");
    let idx = &state.list_indexes()[0];
    assert_eq!((idx.file_count, idx.updates_available), (2, 0));
}

#[test]
fn a_missing_file_installs_the_rest_and_is_reported_as_partial() {
    let (_d, state) = app();
    let mut fake = Fake::repo("second");
    fake.files.remove(&url("book/b.xml"));
    let (result, slept) = update(&state, &fake);
    assert!(slept.is_empty(), "a 404 is not retried");
    assert_eq!(
        (result.updated, result.failed.clone()),
        (1, vec![url("book/b.xml")])
    );
    assert!(has(&state, "ID_INVENTED_FEAT_A"));
    let problem = problem(&state);
    assert!(
        problem.starts_with("1 of 3 files could not be used"),
        "{problem}"
    );
    assert!(problem.contains("[http_404]"), "{problem}");
    let issue = &result.reports[0].issues[0];
    assert_eq!(issue.file.as_deref(), Some(url("book/b.xml").as_str()));
}
