//! Aurora XML `.index` files in Update indexes (aurora-index-1): nested indexes, `<obsolete>`,
//! missing files, HTTPS-only entries and redirects, cycles, the depth and file limits, and trust on
//! first use of file hashes. All content here is invented; nothing touches the network.

use borealis_lib::content_auto::{update_message, MSG_UPDATED, MSG_UP_TO_DATE};
use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_index::{raw_file_url, redirect_refusal, Fetcher};
use borealis_lib::content_index_aurora::{
    crawl, parse_index, relative_path, MAX_INDEX_FILES, MAX_NESTED_DEPTH,
};
use borealis_lib::content_packs::{EventSink, NullSink};
use borealis_lib::settings_dto::ContentUpdateState;
use borealis_lib::state::{AppPaths, AppState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const BASE: &str = "https://example.invalid/repo/master/";

/// Serves a fixed map of URLs and records every request.
#[derive(Default)]
struct Stub {
    files: HashMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl Stub {
    fn with(mut self, url: &str, body: &str) -> Self {
        self.files.insert(url.to_string(), body.as_bytes().to_vec());
        self
    }
    fn asked(&self, url: &str) -> bool {
        self.asked.lock().unwrap().iter().any(|u| u == url)
    }
}

impl Fetcher for Stub {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(url.to_string());
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| "the server answered 404 Not Found".into())
    }
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn url(rel: &str) -> String {
    format!("{BASE}{rel}")
}

/// An Aurora index listing `(tag, name, url)` entries.
fn index(name: &str, entries: &[(&str, &str, String)]) -> String {
    let rows: String = entries
        .iter()
        .map(|(tag, n, u)| format!("    <{tag} name=\"{n}\" url=\"{u}\" />\n"))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<index>\n  <info><name>{name}</name></info>\n  <files>\n{rows}  </files>\n</index>"
    )
}

/// An Aurora content file with one feat.
fn feat(id: &str, text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Feat {id}" type="Feat" source="Invented" id="ID_INVENTED_FEAT_{id}">
    <description><p>{text}</p></description>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#
    )
}

/// top.index -> book.index -> a.xml, b.xml (+ an obsolete c.xml)
fn nested_repo() -> Stub {
    let top = index(
        "Invented Repo",
        &[("file", "book.index", url("top/book.index"))],
    );
    let book = index(
        "Book",
        &[
            ("file", "a.xml", url("top/book/a.xml")),
            ("file", "b.xml", url("top/book/b.xml")),
            ("obsolete", "c.xml", url("top/book/c.xml")),
        ],
    );
    Stub::default()
        .with(&url("top.index"), &top)
        .with(&url("top/book.index"), &book)
        .with(&url("top/book/a.xml"), &feat("A", "first"))
        .with(&url("top/book/b.xml"), &feat("B", "second"))
        .with(&url("top/book/c.xml"), &feat("C", "retired"))
}

fn has(state: &AppState, id: &str) -> bool {
    state.hub().expect("hub").store.contains_id(id)
}

#[test]
fn nested_aurora_index_installs_one_unverified_pack_and_skips_obsolete() {
    let (_d, state) = app();
    let stub = nested_repo();
    state.add_index(&url("top.index")).expect("add");
    let result = state.update_indexes(&stub, &NullSink).expect("update");
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.updated, 1, "one pack for the whole index tree");
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
    assert!(!has(&state, "ID_INVENTED_FEAT_C"));
    assert!(
        !stub.asked(&url("top/book/c.xml")),
        "obsolete files are not fetched"
    );
    let idx = &state.list_indexes()[0];
    assert_eq!(
        (idx.name.as_str(), idx.file_count, idx.verified),
        ("Invented Repo", 2, false)
    );
    assert_eq!(idx.problem, None);
    let packs: Vec<_> = state
        .list_packs()
        .unwrap()
        .into_iter()
        .filter(|p| p.kind != PackKind::Bundled)
        .collect();
    assert_eq!(packs.len(), 1);
    assert_eq!(
        (packs[0].name.as_str(), packs[0].element_count),
        ("Invented Repo", 2)
    );

    // A second update with nothing changed installs nothing.
    let again = state.update_indexes(&stub, &NullSink).expect("update");
    assert_eq!((again.updated, again.changed.len()), (0, 0));
}

#[test]
fn a_missing_file_is_reported_and_the_rest_installs() {
    let (_d, state) = app();
    let mut stub = nested_repo();
    stub.files.remove(&url("top/book/b.xml"));
    state.add_index(&url("top.index")).expect("add");
    let result = state.update_indexes(&stub, &NullSink).expect("update");
    assert_eq!(result.failed, vec![url("top/book/b.xml")]);
    assert!(has(&state, "ID_INVENTED_FEAT_A"));
    let problem = state.list_indexes()[0].problem.clone().expect("problem");
    assert!(problem.contains("404"), "{problem}");
}

#[test]
fn http_entries_are_refused_and_never_fetched() {
    let (_d, state) = app();
    let plain = "http://example.invalid/repo/master/top/plain.xml";
    let top = index(
        "Mixed",
        &[
            ("file", "a.xml", url("top/a.xml")),
            ("file", "plain.xml", plain.to_string()),
            ("file", "local.xml", "file:///etc/passwd.xml".to_string()),
        ],
    );
    let stub = Stub::default()
        .with(&url("top.index"), &top)
        .with(&url("top/a.xml"), &feat("A", "ok"))
        .with(plain, &feat("P", "plain"));
    state.add_index(&url("top.index")).expect("add");
    let result = state.update_indexes(&stub, &NullSink).expect("update");
    assert!(!stub.asked(plain));
    assert!(result.failed.iter().any(|u| u == plain));
    assert!(has(&state, "ID_INVENTED_FEAT_A") && !has(&state, "ID_INVENTED_FEAT_P"));
    let problem = state.list_indexes()[0].problem.clone().expect("problem");
    assert!(problem.contains("refused http://"), "{problem}");
    assert!(state.add_index("http://example.invalid/x.index").is_err());
}

#[test]
fn redirects_to_anything_but_https_are_refused() {
    assert!(redirect_refusal("http", 0).is_some_and(|w| w.contains("http:")));
    assert!(redirect_refusal("file", 0).is_some());
    assert!(redirect_refusal("https", 0).is_none());
    assert!(redirect_refusal("https", 5).is_some(), "hop limit");

    // The refusal the real client reports reaches the Indexes list.
    struct Redirected;
    impl Fetcher for Redirected {
        fn get(&self, _: &str, _: usize) -> Result<Vec<u8>, String> {
            Err(redirect_refusal("http", 0).unwrap())
        }
    }
    let (_d, state) = app();
    state.add_index(&url("top.index")).expect("add");
    let result = state
        .update_indexes(&Redirected, &NullSink)
        .expect("update");
    assert_eq!(result.failed, vec![url("top.index")]);
    let problem = state.list_indexes()[0].problem.clone().expect("problem");
    assert!(problem.contains("redirect"), "{problem}");
}

#[test]
fn index_cycles_terminate_and_are_reported() {
    let a = index("A", &[("file", "b.index", url("b.index"))]);
    let b = index(
        "B",
        &[
            ("file", "a.index", url("a.index")),
            ("file", "x.xml", url("b/x.xml")),
        ],
    );
    let stub = Stub::default()
        .with(&url("a.index"), &a)
        .with(&url("b.index"), &b);
    let top = parse_index(a.as_bytes(), &url("a.index")).expect("parse");
    let found = crawl(&stub, &url("a.index"), &top);
    assert_eq!(found.files.len(), 1);
    assert!(found.problems.iter().any(|(_, why)| why.contains("cycle")));
}

#[test]
fn nesting_deeper_than_the_limit_is_cut() {
    let mut stub = Stub::default();
    let depth = MAX_NESTED_DEPTH + 2;
    for level in 0..=depth {
        let entries = vec![
            ("file", "next.index", url(&format!("l{}.index", level + 1))),
            ("file", "f.xml", url(&format!("l{level}/f.xml"))),
        ];
        stub = stub.with(&url(&format!("l{level}.index")), &index("L", &entries));
    }
    let top_bytes = stub.files[&url("l0.index")].clone();
    let top = parse_index(&top_bytes, &url("l0.index")).expect("parse");
    let found = crawl(&stub, &url("l0.index"), &top);
    // The top level plus MAX_NESTED_DEPTH nested levels contribute one file each.
    assert_eq!(found.files.len(), MAX_NESTED_DEPTH + 1);
    assert!(found
        .problems
        .iter()
        .any(|(_, why)| why.contains("levels deep")));
    assert!(!stub.asked(&url(&format!("l{}.index", MAX_NESTED_DEPTH + 1))));
}

#[test]
fn file_count_is_capped() {
    let entries: Vec<(&str, String, String)> = (0..=MAX_INDEX_FILES)
        .map(|i| ("file", format!("f{i}.xml"), url(&format!("big/f{i}.xml"))))
        .collect();
    let rows: Vec<(&str, &str, String)> = entries
        .iter()
        .map(|(t, n, u)| (*t, n.as_str(), u.clone()))
        .collect();
    let text = index("Big", &rows);
    let top = parse_index(text.as_bytes(), &url("big.index")).expect("parse");
    let found = crawl(&Stub::default(), &url("big.index"), &top);
    assert_eq!(found.files.len(), MAX_INDEX_FILES);
    assert!(found
        .problems
        .iter()
        .any(|(_, why)| why.contains("the rest were skipped")));
}

#[test]
fn changed_files_and_new_hosts_are_reported_on_update() {
    let (_d, state) = app();
    let stub = nested_repo();
    state.add_index(&url("top.index")).expect("add");
    let first = state.update_indexes(&stub, &NullSink).expect("first");
    assert!(first.changed.is_empty() && first.new_hosts.is_empty());

    // b.xml changes and the nested index now also pulls a file from another host.
    let other = "https://mirror.invalid/extra/d.xml";
    let book = index(
        "Book",
        &[
            ("file", "a.xml", url("top/book/a.xml")),
            ("file", "b.xml", url("top/book/b.xml")),
            ("file", "d.xml", other.to_string()),
        ],
    );
    let stub = nested_repo()
        .with(&url("top/book.index"), &book)
        .with(&url("top/book/b.xml"), &feat("B", "rewritten"))
        .with(other, &feat("D", "elsewhere"));
    let second = state.update_indexes(&stub, &NullSink).expect("second");
    assert_eq!(second.changed, vec![url("top/book/b.xml")]);
    assert_eq!(second.new_hosts, vec!["mirror.invalid".to_string()]);
    assert_eq!(second.updated, 1, "the changed content is still applied");
    let b = state
        .hub()
        .unwrap()
        .store
        .element("ID_INVENTED_FEAT_B")
        .map(|e| format!("{:?}", e.description))
        .unwrap_or_default();
    assert!(b.contains("rewritten"), "{b}");
    assert!(has(&state, "ID_INVENTED_FEAT_D"));
}

#[test]
fn file_paths_mirror_the_repository_and_stay_inside_it() {
    assert_eq!(
        relative_path(BASE, &url("core/phb/feats.xml")).as_deref(),
        Some("core/phb/feats.xml")
    );
    assert_eq!(
        relative_path(BASE, "https://other.invalid/x/y.xml?raw=1").as_deref(),
        Some("_other/other.invalid/x/y.xml")
    );
    assert_eq!(relative_path(BASE, &url("core/../../etc.xml")), None);
    assert_eq!(relative_path(BASE, &url("core//x.xml")), None);
}

// ---- index-bug-1: a GitHub page link, the self-referencing <update> entry, honest messages ----

const RAW: &str = "https://raw.githubusercontent.com/Owner/elements/master/";
const PAGE: &str = "https://github.com/Owner/elements/blob/master/";

fn raw(rel: &str) -> String {
    format!("{RAW}{rel}")
}

/// What github.com answers for a file's page link: a web page, not the file.
const GITHUB_PAGE_HTML: &str =
    "\n\n<!DOCTYPE html>\n<html lang=\"en\"><head><title>elements/core.index</title></head><body></body></html>";

/// The shape of a real Aurora repository's `core.index`: an `<info><update>` entry pointing at
/// the index itself, two direct content files, and nested book indexes (one listed by its GitHub
/// page link, as people paste them).
fn github_repo() -> Stub {
    let core = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<index>
  <info>
    <name>Core</name>
    <description>The content from the core rulebooks.</description>
    <update version="0.2.8">
      <file name="core.index" url="{core}" />
    </update>
  </info>
  <files>
    <!-- Aurora-specific -->
    <file name="internal.xml" url="{internal}" />
    <file name="ALE.xml" url="{ale}" />
    <file name="players-handbook.index" url="{phb}" />
    <file name="monster-manual.index" url="{PAGE}core/monster-manual.index" />
  </files>
</index>"#,
        core = raw("core.index"),
        internal = raw("core/internal.xml"),
        ale = raw("core/ALE.xml"),
        phb = raw("core/players-handbook.index"),
    );
    let phb = index(
        "Player's Handbook",
        &[
            ("file", "feats.xml", raw("core/players-handbook/feats.xml")),
            (
                "file",
                "spells.xml",
                raw("core/players-handbook/spells.xml"),
            ),
        ],
    );
    let mm = index(
        "Monster Manual",
        &[("file", "beasts.xml", raw("core/monster-manual/beasts.xml"))],
    );
    let mut stub = Stub::default()
        .with(&raw("core.index"), &core)
        .with(&raw("core/internal.xml"), &feat("INTERNAL", "internal"))
        .with(&raw("core/ALE.xml"), &feat("ALE", "ale"))
        .with(&raw("core/players-handbook.index"), &phb)
        .with(&raw("core/monster-manual.index"), &mm)
        .with(
            &raw("core/players-handbook/feats.xml"),
            &feat("PHB1", "one"),
        )
        .with(
            &raw("core/players-handbook/spells.xml"),
            &feat("PHB2", "two"),
        )
        .with(
            &raw("core/monster-manual/beasts.xml"),
            &feat("MM1", "three"),
        );
    for page in [
        format!("{PAGE}core.index"),
        format!("{PAGE}core/monster-manual.index"),
    ] {
        stub = stub.with(&page, GITHUB_PAGE_HTML);
    }
    stub
}

const GITHUB_IDS: [&str; 5] = [
    "ID_INVENTED_FEAT_INTERNAL",
    "ID_INVENTED_FEAT_ALE",
    "ID_INVENTED_FEAT_PHB1",
    "ID_INVENTED_FEAT_PHB2",
    "ID_INVENTED_FEAT_MM1",
];

/// Keeps every progress event.
#[derive(Default)]
struct Recording(Mutex<Vec<ContentProgress>>);

impl EventSink for Recording {
    fn content_progress(&self, p: &ContentProgress) {
        self.0.lock().unwrap().push(p.clone());
    }
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

impl Recording {
    fn done_message(&self) -> String {
        let events = self.0.lock().unwrap();
        events
            .iter()
            .rev()
            .find(|p| p.phase == "done")
            .map(|p| p.message.clone())
            .expect("a done event")
    }
}

fn installed_packs(state: &AppState) -> Vec<(String, usize)> {
    state
        .list_packs()
        .unwrap()
        .into_iter()
        .filter(|p| p.kind != PackKind::Bundled)
        .map(|p| (p.name, p.element_count))
        .collect()
}

#[test]
fn github_page_links_become_raw_file_addresses() {
    assert_eq!(
        raw_file_url(&format!("{PAGE}core.index")),
        raw("core.index")
    );
    assert_eq!(
        raw_file_url("https://github.com/Owner/elements/raw/master/core.index"),
        raw("core.index")
    );
    assert_eq!(
        raw_file_url(&format!(" {PAGE}core.index?raw=true ")),
        raw("core.index")
    );
    for unchanged in [
        raw("core.index"),
        "https://github.com/Owner/elements".to_string(),
        "https://example.invalid/blob/x/core.index".to_string(),
    ] {
        assert_eq!(raw_file_url(&unchanged), unchanged);
    }
}

#[test]
fn a_github_page_link_downloads_the_whole_index_tree() {
    let (_d, state) = app();
    let stub = github_repo();
    let listed = state.add_index(&format!("{PAGE}core.index")).expect("add");
    assert_eq!(listed[0].url, raw("core.index"), "stored as the raw file");
    let sink = Recording::default();
    let result = state.update_indexes(&stub, &sink).expect("update");
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.updated, 1);
    for id in GITHUB_IDS {
        assert!(has(&state, id), "{id} installed");
    }
    assert_eq!(installed_packs(&state), vec![("Core".to_string(), 5)]);
    assert!(
        !stub.asked(&format!("{PAGE}core.index"))
            && !stub.asked(&format!("{PAGE}core/monster-manual.index")),
        "GitHub pages are never fetched"
    );
    let idx = &state.list_indexes()[0];
    assert_eq!((idx.file_count, idx.problem.clone()), (5, None));
    assert_eq!(sink.done_message(), MSG_UPDATED);
}

#[test]
fn the_self_referencing_update_entry_is_neither_content_nor_a_cycle() {
    let stub = github_repo();
    let top_bytes = stub.files[&raw("core.index")].clone();
    let top = parse_index(&top_bytes, &raw("core.index")).expect("parse");
    let found = crawl(&stub, &raw("core.index"), &top);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    assert_eq!(found.files.len(), 5);
    assert_eq!(
        stub.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|u| **u == raw("core.index"))
            .count(),
        0,
        "the top-level index is not fetched again"
    );
}

#[test]
fn an_index_saved_as_a_github_page_link_is_repaired_on_update() {
    let dir = tempfile::tempdir().expect("temp dir");
    let page = format!("{PAGE}core.index");
    let prefs = serde_json::json!({ "indexes": [{ "url": page, "name": page }] });
    std::fs::write(dir.path().join("content-prefs.json"), prefs.to_string()).unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    let result = state
        .update_indexes(&github_repo(), &NullSink)
        .expect("update");
    assert_eq!((result.updated, result.failed.len()), (1, 0));
    let indexes = state.list_indexes();
    assert_eq!(indexes.len(), 1);
    assert_eq!(
        (indexes[0].url.as_str(), indexes[0].name.as_str()),
        (raw("core.index").as_str(), "Core")
    );
}

#[test]
fn a_web_page_is_reported_and_never_called_up_to_date() {
    let (_d, state) = app();
    let page = "https://example.invalid/elements/core.index";
    let stub = Stub::default().with(page, GITHUB_PAGE_HTML);
    state.add_index(page).expect("add");
    let sink = Recording::default();
    let result = state.update_indexes(&stub, &sink).expect("update");
    assert_eq!(
        (result.updated, result.failed.clone()),
        (0, vec![page.to_string()])
    );
    assert!(installed_packs(&state).is_empty());
    let idx = &state.list_indexes()[0];
    let problem = idx.problem.clone().expect("problem");
    assert!(problem.contains("web page"), "{problem}");
    assert_eq!(idx.last_checked, None, "never downloaded");
    let done = sink.done_message();
    assert!(!done.to_lowercase().contains("up to date"), "{done}");
}

#[test]
fn update_messages_never_call_a_failure_up_to_date() {
    assert_eq!(update_message(0, 0), MSG_UP_TO_DATE);
    assert_eq!(update_message(2, 0), MSG_UPDATED);
    let partial = update_message(1, 3);
    assert!(
        partial.starts_with(MSG_UPDATED) && partial.contains('3'),
        "{partial}"
    );
    let failed = update_message(0, 1);
    assert!(!failed.to_lowercase().contains("up to date"), "{failed}");
    assert!(failed.contains("could not be updated"), "{failed}");
}

#[test]
fn the_startup_check_reports_a_failed_index_as_a_failure() {
    let (_d, state) = app();
    let page = "https://example.invalid/elements/core.index";
    state.add_index(page).expect("add");
    let stub = Stub::default().with(page, GITHUB_PAGE_HTML);
    let status = state.run_auto_content_update(&stub, &NullSink);
    assert_eq!(status.state, ContentUpdateState::Done);
    assert_eq!((status.updated, status.failed.len()), (0, 1));
    assert!(
        !status.message.to_lowercase().contains("up to date"),
        "{}",
        status.message
    );
}
