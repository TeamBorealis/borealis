//! 5etools URL import and Update when downloads fail (det-src-4): passing failures (429, 5xx,
//! timeout, dropped connection, body shorter than announced) are retried; a file that still fails
//! installs nothing and replaces nothing, the problem names each failed path with its code, and the
//! next Update tries again and ends with exactly the pack a clean import gives. A fake host serves
//! the importer's SRD fixture; no network, and pauses are recorded instead of slept.

use borealis_lib::content_dto::FiveToolsImportRequest;
use borealis_lib::content_fetch_retry::{check_complete, FetchError, RetryPolicy};
use borealis_lib::content_fivetools_fetch::classify_answer;
use borealis_lib::content_fivetools_remote::{FetchRequest, FetchResponse, RemoteFetcher};
use borealis_lib::content_fivetools_url::STAGE_DIR;
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const FOLDER: &str = "https://mirror.example/5e/data/";
const REPO: &str = "https://github.com/probe/fivetools-data";
const API: &str = "https://api.github.com/repos/probe/fivetools-data/commits/";
const RAW: &str = "https://raw.githubusercontent.com/probe/fivetools-data/";
const WIZARD: &str = "class/class-wizard.json";
/// More tries than the policy makes: the file keeps failing.
const ALWAYS: usize = 50;

/// One injected failure.
#[derive(Debug, Clone, Copy)]
enum Fault {
    /// An HTTP answer, with an optional `Retry-After` in seconds.
    Status(u16, Option<u64>),
    Timeout,
    /// The connection dropped before an answer (a plain message, classified by the default).
    Dropped,
    /// The body ended before its `Content-Length`.
    Truncated,
}

/// A fake 5etools host: a folder address and a GitHub repository at one commit.
struct Host {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    faults: Mutex<BTreeMap<String, VecDeque<Fault>>>,
    sha: Mutex<String>,
    log: Mutex<Vec<String>>,
}

fn fixture_files() -> BTreeMap<String, Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data");
    let mut out = tree(&root);
    out.insert("books.json".into(), br#"{"book":[]}"#.to_vec());
    out
}

/// `rel -> bytes` of every file under `root`.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for p in read.map(|e| e.unwrap().path()) {
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy();
                out.insert(rel.replace('\\', "/"), std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

impl Host {
    fn new() -> Self {
        let host = Self {
            files: Mutex::new(fixture_files()),
            faults: Mutex::new(BTreeMap::new()),
            sha: Mutex::new("a".repeat(40)),
            log: Mutex::new(Vec::new()),
        };
        host.write_manifests();
        host
    }

    /// `class/index.json` and `spells/index.json` list the folder's files, as in a real folder.
    fn write_manifests(&self) {
        let mut files = self.files.lock().unwrap();
        for dir in ["class", "spells"] {
            let names: BTreeMap<String, String> = files
                .keys()
                .filter_map(|k| k.strip_prefix(&format!("{dir}/")))
                .filter(|n| *n != "index.json")
                .map(|n| (n.trim_end_matches(".json").to_string(), n.to_string()))
                .collect();
            files.insert(
                format!("{dir}/index.json"),
                serde_json::to_vec(&names).unwrap(),
            );
        }
    }

    fn fail(&self, rel: &str, fault: Fault, times: usize) {
        let mut faults = self.faults.lock().unwrap();
        let queue = faults.entry(rel.to_string()).or_default();
        queue.extend(std::iter::repeat(fault).take(times));
    }

    fn heal(&self) {
        self.faults.lock().unwrap().clear();
    }

    /// Upstream moves on: a race is added and the commit changes.
    fn change_upstream(&self) {
        let mut files = self.files.lock().unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&files["races.json"]).unwrap();
        let mut extra = v["race"][0].clone();
        extra["name"] = "Mirefolk Probe".into();
        v["race"].as_array_mut().unwrap().push(extra);
        files.insert("races.json".into(), serde_json::to_vec(&v).unwrap());
        *self.sha.lock().unwrap() = "b".repeat(40);
    }

    fn requests_for(&self, rel: &str) -> usize {
        let log = self.log.lock().unwrap();
        log.iter()
            .filter(|u| u.ends_with(&format!("/{rel}")))
            .count()
    }

    fn rel_of(&self, url: &str) -> Option<String> {
        let sha = self.sha.lock().unwrap().clone();
        url.strip_prefix(FOLDER)
            .or_else(|| url.strip_prefix(&format!("{RAW}{sha}/data/")))
            .map(str::to_string)
    }

    fn next_fault(&self, rel: &str) -> Option<Fault> {
        self.faults.lock().unwrap().get_mut(rel)?.pop_front()
    }
}

impl RemoteFetcher for Host {
    fn fetch(&self, r: &FetchRequest<'_>) -> Result<FetchResponse, String> {
        self.log.lock().unwrap().push(r.url.to_string());
        if r.url.starts_with(API) {
            let sha = self.sha.lock().unwrap().clone();
            return Ok(answer(200, sha.into_bytes()));
        }
        let Some(rel) = self.rel_of(r.url) else {
            return Ok(answer(404, Vec::new()));
        };
        match self.next_fault(&rel) {
            Some(Fault::Status(status, _)) => return Ok(answer(status, Vec::new())),
            Some(Fault::Timeout) => return Err(FetchError::timeout("operation timed out").wire()),
            Some(Fault::Dropped) => return Err("could not connect: connection reset".into()),
            Some(Fault::Truncated) => {
                let n = self.files.lock().unwrap().get(&rel).map_or(10, Vec::len);
                return Err(check_complete(n / 2, Some(n as u64)).unwrap_err().wire());
            }
            None => {}
        }
        Ok(match self.files.lock().unwrap().get(&rel) {
            Some(body) => answer(200, body.clone()),
            None => answer(404, Vec::new()),
        })
    }

    /// Carries `Retry-After` like the real client; everything else is the trait's default path.
    fn fetch_classified(&self, r: &FetchRequest<'_>) -> Result<FetchResponse, FetchError> {
        let peek = self.rel_of(r.url).and_then(|rel| {
            let faults = self.faults.lock().unwrap();
            faults.get(&rel).and_then(|q| q.front().copied())
        });
        if let Some(Fault::Status(status, Some(secs))) = peek {
            let rel = self.rel_of(r.url).unwrap();
            self.next_fault(&rel);
            self.log.lock().unwrap().push(r.url.to_string());
            let after = Some(Duration::from_secs(secs));
            return Err(FetchError::http(status, "", after));
        }
        let got = self.fetch(r).map_err(|m| FetchError::from_message(&m))?;
        classify_answer(got, None)
    }
}

fn answer(status: u16, body: Vec<u8>) -> FetchResponse {
    FetchResponse {
        status,
        body,
        ..FetchResponse::default()
    }
}

/// Records pauses instead of sleeping.
#[derive(Default)]
struct Pauses(Mutex<Vec<Duration>>);

impl Pauses {
    fn sleep(&self) -> impl Fn(Duration) + Send + Sync + '_ {
        |d| self.0.lock().unwrap().push(d)
    }
    fn taken(&self) -> Vec<Duration> {
        self.0.lock().unwrap().clone()
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// Stage `url`, scan and import PHB, as the 5etools tab does.
fn import(app: &AppState, host: &Host, url: &str, pauses: &Pauses) -> Result<usize, String> {
    let sleep = pauses.sleep();
    let picked = app
        .stage_5etools_url_with(host, url, &NullSink, RetryPolicy::default(), &sleep)
        .map_err(|e| e.message)?;
    app.scan_5etools_folder(&picked.token).expect("scan");
    let request = FiveToolsImportRequest {
        folder: picked.token,
        sources: vec!["PHB".into()],
        include_fluff: false,
    };
    Ok(app
        .import_5etools(&request, &NullSink)
        .expect("import")
        .converted)
}

fn update(app: &AppState, host: &Host, url: &str, pauses: &Pauses) -> Result<bool, String> {
    let sleep = pauses.sleep();
    app.update_5etools_url_with(host, url, &NullSink, RetryPolicy::default(), &sleep)
        .map(|u| u.up_to_date)
        .map_err(|e| e.message)
}

/// Every installed pack file, byte for byte.
fn library(app: &AppState) -> BTreeMap<String, Vec<u8>> {
    tree(&app.paths.library())
}

fn no_stage_left(app: &AppState) -> bool {
    !app.paths.data.join(STAGE_DIR).exists()
}

/// The problem names paths and codes, never 5etools text.
fn assert_clean_problem(message: &str, rel: &str, code: &str) {
    assert!(message.contains(&format!("{rel} [{code}]")), "{message}");
    assert!(
        message.contains("Nothing was installed or changed"),
        "{message}"
    );
    for text in ["Dwarf", "Mirefolk", "{", "\"race\""] {
        assert!(!message.contains(text), "5etools text in: {message}");
    }
}

#[test]
fn passing_failures_are_retried_and_install_the_same_pack_as_a_clean_import() {
    let host = Host::new();
    host.fail("races.json", Fault::Status(429, Some(3)), 1);
    host.fail("items.json", Fault::Status(503, None), 2);
    host.fail(WIZARD, Fault::Timeout, 1);
    host.fail("spells/spells-phb.json", Fault::Truncated, 1);
    host.fail("feats.json", Fault::Dropped, 1);
    host.fail("class/index.json", Fault::Status(502, None), 1);
    let (_d1, flaky) = app();
    let pauses = Pauses::default();
    assert!(import(&flaky, &host, FOLDER, &pauses).expect("retried import") > 0);
    let taken = pauses.taken();
    assert!(
        taken.contains(&Duration::from_secs(3)),
        "Retry-After: {taken:?}"
    );
    assert!(
        taken.contains(&Duration::from_secs(2)),
        "doubling: {taken:?}"
    );
    assert_eq!(host.requests_for("items.json"), 3);

    host.heal();
    let (_d2, clean) = app();
    import(&clean, &host, FOLDER, &Pauses::default()).expect("clean import");
    assert_eq!(library(&flaky), library(&clean));
    assert_eq!(
        flaky.list_5etools_urls()[0].revision,
        clean.list_5etools_urls()[0].revision
    );
}

#[test]
fn a_file_that_still_fails_replaces_nothing_and_the_next_update_matches_a_clean_import() {
    for (fault, code) in [
        (Fault::Status(429, None), "http_429"),
        (Fault::Status(502, None), "http_502"),
        (Fault::Timeout, "timeout"),
        (Fault::Truncated, "truncated"),
        (Fault::Dropped, "network"),
    ] {
        for url in [FOLDER, REPO] {
            let host = Host::new();
            let (_d1, a) = app();
            import(&a, &host, url, &Pauses::default()).expect("first import");
            let before = library(&a);
            let record = a.list_5etools_urls();

            host.change_upstream();
            host.fail(WIZARD, fault, ALWAYS);
            let pauses = Pauses::default();
            let err = update(&a, &host, url, &pauses).expect_err("held");
            assert_clean_problem(&err, WIZARD, code);
            assert!(!pauses.taken().is_empty(), "{code}: retried first");
            assert_eq!(library(&a), before, "{code} {url}: nothing replaced");
            assert!(no_stage_left(&a));
            let after = a.list_5etools_urls();
            assert_eq!(after[0].revision, record[0].revision, "revision kept");
            assert_eq!(after[0].problem.as_deref(), Some(err.as_str()));

            host.heal();
            assert!(!update(&a, &host, url, &Pauses::default()).expect("retry"));
            assert!(a.list_5etools_urls()[0].problem.is_none());
            let (_d2, clean) = app();
            import(&clean, &host, url, &Pauses::default()).expect("clean");
            assert_eq!(library(&a), library(&clean), "{code} {url}: same pack");
            assert_ne!(library(&a), before, "the update did change the pack");
        }
    }
}

#[test]
fn a_failure_midway_through_many_files_stages_and_remembers_nothing() {
    let host = Host::new();
    {
        let mut files = host.files.lock().unwrap();
        for i in 0..150 {
            files.insert(format!("class/class-probe{i:03}.json"), b"{}".to_vec());
        }
    }
    host.write_manifests();
    let failing = "class/class-probe070.json";
    host.fail(failing, Fault::Status(500, None), ALWAYS);
    let (_d, a) = app();
    let err = import(&a, &host, FOLDER, &Pauses::default()).expect_err("failed");
    assert_clean_problem(&err, failing, "http_500");
    let total: usize = err
        .split(" of ")
        .nth(1)
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse().ok())
        .expect("a count");
    assert!(total > 150, "every file of the folder is counted: {err}");
    assert!(no_stage_left(&a));
    assert!(a.list_5etools_urls().is_empty());
    assert!(library(&a).is_empty(), "nothing installed");
}

#[test]
fn a_missing_optional_file_is_skipped_and_a_missing_listed_file_fails() {
    // Root files the folder lacks (skills.json, the fluff files here) are left out, as the
    // importer leaves them out of a folder.
    let host = Host::new();
    host.files.lock().unwrap().remove("languages.json");
    let (_d1, a) = app();
    assert!(import(&a, &host, FOLDER, &Pauses::default()).expect("optional") > 0);

    // A file the class manifest lists must be there.
    let host = Host::new();
    host.files.lock().unwrap().remove(WIZARD);
    let (_d2, b) = app();
    let err = import(&b, &host, FOLDER, &Pauses::default()).expect_err("required");
    assert_clean_problem(&err, WIZARD, "http_404");
    assert!(err.contains("missing"), "{err}");
    assert!(no_stage_left(&b) && library(&b).is_empty());
}

#[test]
fn a_retry_after_longer_than_a_minute_is_not_waited_for() {
    let host = Host::new();
    host.fail("races.json", Fault::Status(429, Some(600)), ALWAYS);
    let (_d, a) = app();
    let pauses = Pauses::default();
    let err = import(&a, &host, FOLDER, &pauses).expect_err("held");
    assert_clean_problem(&err, "races.json", "http_429");
    assert!(pauses.taken().iter().all(|d| *d <= Duration::from_secs(60)));
    assert_eq!(host.requests_for("races.json"), 1);
}
