//! 5etools import from a pasted address (decision IM5), against an in-process mock of GitHub
//! (API, ref list, raw files) and of a plain folder address serving the importer's SRD fixture.
//! No network: the real client is only exercised by `fivetools_url_live.rs` (env-gated).

use borealis_lib::content_dto::FiveToolsImportRequest;
use borealis_lib::content_fivetools_remote::{FetchRequest, FetchResponse, RemoteFetcher};
use borealis_lib::content_fivetools_url::STAGE_DIR;
use borealis_lib::content_index::sha256_hex;
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::state::{AppPaths, AppState};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

const REPO: &str = "https://github.com/probe/fivetools-data";
const FOLDER: &str = "https://mirror.example/5e/data/";
const API: &str = "https://api.github.com/repos/probe/fivetools-data/commits/";
const REFS: &str = "https://github.com/probe/fivetools-data.git/info/refs?service=git-upload-pack";
const RAW: &str = "https://raw.githubusercontent.com/probe/fivetools-data/";
const DWARF: &str = "ID_5ET_PHB_RACE_DWARF";

fn fixture_files() -> BTreeMap<String, Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data");
    let mut out = BTreeMap::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)
            .expect("fixture dir")
            .map(Result::unwrap)
        {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    // The fixture has no books.json or manifests; a real data folder has both.
    out.insert("books.json".into(), br#"{"book":[]}"#.to_vec());
    let manifest = |dir: &str| {
        let names: BTreeMap<String, String> = out
            .keys()
            .filter_map(|k| k.strip_prefix(&format!("{dir}/")).map(str::to_string))
            .map(|n| (n.trim_end_matches(".json").to_string(), n))
            .collect();
        serde_json::to_vec(&names).unwrap()
    };
    let class = manifest("class");
    let spells = manifest("spells");
    out.insert("class/index.json".into(), class);
    out.insert("spells/index.json".into(), spells);
    out
}

/// Fake GitHub + folder host. Files are served at the current commit only.
struct Mock {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    sha: Mutex<String>,
    api_status: Mutex<u16>,
    log: Mutex<Vec<String>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            files: Mutex::new(fixture_files()),
            sha: Mutex::new("a".repeat(40)),
            api_status: Mutex::new(200),
            log: Mutex::new(Vec::new()),
        }
    }
    fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
    fn edit(&self, rel: &str, f: impl FnOnce(&mut serde_json::Value)) {
        let mut files = self.files.lock().unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&files[rel]).unwrap();
        f(&mut v);
        files.insert(rel.into(), serde_json::to_vec(&v).unwrap());
    }
    fn file(&self, rel: &str, if_none_match: Option<&str>) -> FetchResponse {
        let files = self.files.lock().unwrap();
        let Some(body) = files.get(rel) else {
            return FetchResponse {
                status: 404,
                ..FetchResponse::default()
            };
        };
        let etag = format!("\"{}\"", sha256_hex(body));
        if if_none_match == Some(etag.as_str()) {
            return FetchResponse {
                status: 304,
                etag: Some(etag),
                ..FetchResponse::default()
            };
        }
        FetchResponse {
            status: 200,
            etag: Some(etag),
            body: body.clone(),
            rate_limited: false,
        }
    }
}

impl RemoteFetcher for Mock {
    fn fetch(&self, r: &FetchRequest<'_>) -> Result<FetchResponse, String> {
        assert!(
            r.url.starts_with("https://"),
            "only https is fetched: {}",
            r.url
        );
        self.log.lock().unwrap().push(r.url.to_string());
        let sha = self.sha.lock().unwrap().clone();
        let response = if r.url.starts_with(API) {
            let status = *self.api_status.lock().unwrap();
            let limited = status == 403;
            let body = if status == 200 {
                sha.into_bytes()
            } else {
                Vec::new()
            };
            FetchResponse {
                status,
                body,
                rate_limited: limited,
                etag: None,
            }
        } else if r.url == REFS {
            let line = format!("{sha} refs/heads/main\n");
            let head = format!("{sha} HEAD\0symref=HEAD:refs/heads/main\n");
            let pkt = |s: &str| format!("{:04x}{s}", s.len() + 4);
            let body = format!(
                "{}0000{}{}0000",
                pkt("# service=git-upload-pack\n"),
                pkt(&head),
                pkt(&line)
            );
            FetchResponse {
                status: 200,
                body: body.into_bytes(),
                ..FetchResponse::default()
            }
        } else if let Some(rel) = r.url.strip_prefix(&format!("{RAW}{sha}/data/")) {
            self.file(rel, r.if_none_match)
        } else if let Some(rel) = r.url.strip_prefix(FOLDER) {
            self.file(rel, r.if_none_match)
        } else {
            FetchResponse {
                status: 404,
                ..FetchResponse::default()
            }
        };
        if response.body.len() > r.max_bytes {
            return Err("the file is too large".into());
        }
        Ok(response)
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

/// Paste the address, scan, import `sources`: the same three steps the 5etools tab runs.
fn import_url(app: &AppState, mock: &Mock, url: &str, sources: &[&str]) -> usize {
    let picked = app.stage_5etools_url(mock, url, &NullSink).expect("stage");
    let scan = app.scan_5etools_folder(&picked.token).expect("scan");
    for s in sources {
        assert!(scan.sources.iter().any(|x| x.abbr == *s), "{s} offered");
    }
    let request = FiveToolsImportRequest {
        folder: picked.token,
        sources: sources.iter().map(|s| (*s).to_string()).collect(),
        include_fluff: false,
    };
    app.import_5etools(&request, &NullSink)
        .expect("import")
        .converted
}

fn dwarf_character(app: &AppState) -> String {
    let request = NewCharacterRequest {
        name: "Url Probe".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let slot = snap
        .slots
        .iter()
        .find(|s| s.element_type.eq_ignore_ascii_case("Race"))
        .expect("race slot")
        .id
        .clone();
    app.apply_selection(&snap.id, &slot, DWARF)
        .expect("pick dwarf");
    snap.id
}

fn chosen_race(app: &AppState, id: &str) -> Option<String> {
    let snap = app.get_snapshot(id).expect("snapshot");
    snap.slots
        .iter()
        .find(|s| s.element_type.eq_ignore_ascii_case("Race"))
        .and_then(|s| s.chosen.as_ref().map(|c| c.id.clone()))
}

fn no_stage_left(app: &AppState) -> bool {
    !app.paths.data.join(STAGE_DIR).exists()
}

#[test]
fn a_github_repository_imports_and_is_remembered_with_its_commit() {
    let (_dir, app) = app();
    let mock = Mock::new();
    let converted = import_url(&app, &mock, REPO, &["PHB"]);
    assert!(converted > 0);
    let calls = mock.calls();
    assert_eq!(
        calls.iter().filter(|c| c.starts_with(API)).count(),
        1,
        "one API call"
    );
    assert!(calls
        .iter()
        .all(|c| !c.contains("codeload") && !c.contains("/zip/")));
    let urls = app.list_5etools_urls();
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0].url, REPO);
    assert_eq!(urls[0].revision, "a".repeat(12));
    assert_eq!(urls[0].sources, vec!["PHB".to_string()]);
    assert!(app
        .list_packs()
        .unwrap()
        .iter()
        .any(|p| urls[0].packs.contains(&p.id)));
}

#[test]
fn a_rate_limited_api_falls_back_to_the_ref_list() {
    let (_dir, app) = app();
    let mock = Mock::new();
    *mock.api_status.lock().unwrap() = 403;
    assert!(import_url(&app, &mock, &format!("{REPO}/tree/main"), &["PHB"]) > 0);
    assert!(mock.calls().iter().any(|c| c == REFS));
}

#[test]
fn a_folder_address_is_walked_through_its_manifests() {
    let (_dir, app) = app();
    let mock = Mock::new();
    assert!(import_url(&app, &mock, FOLDER, &["PHB", "XPHB"]) > 0);
    let calls = mock.calls();
    for want in [
        "books.json",
        "races.json",
        "class/index.json",
        "class/class-wizard.json",
        "spells/spells-xphb.json",
    ] {
        assert!(calls.contains(&format!("{FOLDER}{want}")), "{want} fetched");
    }
    assert!(calls.iter().all(|c| !c.contains("bestiary")));
    assert_eq!(app.list_5etools_urls()[0].revision.len(), 12);
}

#[test]
fn update_of_an_unchanged_source_imports_nothing() {
    let (_dir, app) = app();
    let mock = Mock::new();
    import_url(&app, &mock, REPO, &["PHB"]);
    import_url(&app, &mock, FOLDER, &["PHB"]);
    let before = app.list_packs().unwrap();
    for url in [REPO, FOLDER] {
        mock.clear();
        let r = app
            .update_5etools_url(&mock, url, &NullSink)
            .expect("update");
        assert!(r.up_to_date && r.report.is_none(), "{url}");
        let calls = mock.calls();
        if url == REPO {
            assert_eq!(calls.len(), 1, "only the commit is checked: {calls:?}");
        }
        assert!(no_stage_left(&app));
        assert!(r
            .urls
            .iter()
            .all(|u| u.last_checked.is_some() && u.problem.is_none()));
    }
    assert_eq!(app.list_packs().unwrap(), before);
}

#[test]
fn update_of_a_changed_source_reimports_the_same_books_and_characters_keep_their_ids() {
    let (_dir, app) = app();
    let mock = Mock::new();
    import_url(&app, &mock, REPO, &["PHB"]);
    import_url(&app, &mock, FOLDER, &["PHB"]);
    let character = dwarf_character(&app);
    assert_eq!(chosen_race(&app, &character).as_deref(), Some(DWARF));

    // Upstream adds a race and moves on to a new commit.
    mock.edit("races.json", |v| {
        let mut extra = v["race"][0].clone();
        extra["name"] = "Mirefolk Probe".into();
        v["race"].as_array_mut().unwrap().push(extra);
    });
    *mock.sha.lock().unwrap() = "b".repeat(40);
    for url in [REPO, FOLDER] {
        let r = app
            .update_5etools_url(&mock, url, &NullSink)
            .expect("update");
        assert!(!r.up_to_date, "{url}");
        let report = r.report.expect("re-import");
        assert!(
            report.replaced > 0 && report.packs.iter().all(|p| !p.id.contains("xphb")),
            "{url}: same books only"
        );
        let remembered = r
            .urls
            .iter()
            .find(|u| u.url == url.trim_end_matches('/'))
            .unwrap();
        assert_eq!(remembered.sources, vec!["PHB".to_string()]);
    }
    assert_eq!(app.list_5etools_urls()[0].revision, "b".repeat(12));
    let store_ids = app.list_packs().unwrap();
    assert!(
        !store_ids.iter().any(|p| p.id.contains("xphb")),
        "XPHB was never chosen"
    );
    // The open character still resolves its imported race after both re-imports.
    assert_eq!(chosen_race(&app, &character).as_deref(), Some(DWARF));
}

#[test]
fn unsafe_names_and_oversized_folders_are_refused_and_leave_nothing_behind() {
    let (dir, app) = app();
    let mock = Mock::new();
    mock.files.lock().unwrap().insert(
        "class/index.json".into(),
        br#"{"x":"../../evil.json"}"#.to_vec(),
    );
    let err = app.stage_5etools_url(&mock, FOLDER, &NullSink).unwrap_err();
    assert!(err.message.contains("unsafe file name"), "{}", err.message);
    assert!(no_stage_left(&app));
    assert!(!dir.path().join("evil.json").exists() && !app.paths.data.join("evil.json").exists());

    let many: BTreeMap<String, String> = (0..2_100)
        .map(|i| (format!("c{i}"), format!("class-c{i}.json")))
        .collect();
    mock.files.lock().unwrap().insert(
        "class/index.json".into(),
        serde_json::to_vec(&many).unwrap(),
    );
    let err = app.stage_5etools_url(&mock, FOLDER, &NullSink).unwrap_err();
    assert!(err.message.contains("too many files"), "{}", err.message);

    let huge = vec![b' '; 2 * 1024 * 1024];
    mock.files
        .lock()
        .unwrap()
        .insert("class/index.json".into(), huge);
    let err = app.stage_5etools_url(&mock, FOLDER, &NullSink).unwrap_err();
    assert!(err.message.contains("too large"), "{}", err.message);
    assert!(no_stage_left(&app));
    assert!(app.list_5etools_urls().is_empty());
}

#[test]
fn plain_http_and_unknown_addresses_are_refused_before_any_download() {
    let (_dir, app) = app();
    let mock = Mock::new();
    for bad in [
        "http://mirror.example/5e/data/",
        "http://github.com/probe/fivetools-data",
    ] {
        let err = app.stage_5etools_url(&mock, bad, &NullSink).unwrap_err();
        assert!(err.message.contains("https://"), "{}", err.message);
    }
    assert!(mock.calls().is_empty());
    let err = app
        .stage_5etools_url(&mock, "https://mirror.example/other/", &NullSink)
        .unwrap_err();
    assert!(err.message.contains("books.json"), "{}", err.message);
    let err = app
        .update_5etools_url(&mock, FOLDER, &NullSink)
        .unwrap_err();
    assert!(err.message.contains("not been imported"), "{}", err.message);
}

#[test]
fn a_failed_update_is_shown_on_the_address_and_forgetting_keeps_the_packs() {
    let (_dir, app) = app();
    let mock = Mock::new();
    import_url(&app, &mock, FOLDER, &["PHB"]);
    mock.files.lock().unwrap().remove("books.json");
    assert!(app.update_5etools_url(&mock, FOLDER, &NullSink).is_err());
    assert!(app.list_5etools_urls()[0]
        .problem
        .as_deref()
        .is_some_and(|p| p.contains("books.json")));
    let packs = app.list_packs().unwrap().len();
    assert!(app.remove_5etools_url(FOLDER).unwrap().is_empty());
    assert_eq!(app.list_packs().unwrap().len(), packs);
}
