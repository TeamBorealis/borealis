//! Update on an Aurora XML index asks the server only for what changed (B-25): a stored ETag
//! makes the request conditional, an unchanged file is read from the local copy, and a file the
//! index no longer lists leaves the pack. All content is invented; nothing touches the network.

use borealis_lib::content_dto::PackKind;
use borealis_lib::content_fetch_retry::FetchError;
use borealis_lib::content_index::{Fetcher, Tagged};
use borealis_lib::content_packs::NullSink;
use borealis_lib::state::{AppPaths, AppState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const BASE: &str = "https://example.invalid/repo/master/";

/// One served file: its body and the ETag the server sends with it.
type Served = (Vec<u8>, String);

/// One request seen: the address, the `If-None-Match` it carried, and whether a body was sent.
type Asked = (String, Option<String>, bool);

/// Serves fixed files with ETags, honours `If-None-Match`, and records what it was asked.
#[derive(Default)]
struct TaggedStub {
    files: HashMap<String, Served>,
    asked: Mutex<Vec<Asked>>,
}

impl TaggedStub {
    fn with(mut self, rel: &str, body: &str, etag: &str) -> Self {
        self.files
            .insert(url(rel), (body.as_bytes().to_vec(), etag.to_string()));
        self
    }
    fn same_files(other: &Self) -> Self {
        Self {
            files: other.files.clone(),
            ..Self::default()
        }
    }
    fn bodies_sent(&self) -> Vec<String> {
        let asked = self.asked.lock().unwrap();
        let mut urls: Vec<String> = asked
            .iter()
            .filter(|(_, _, body)| *body)
            .map(|(u, _, _)| u.clone())
            .collect();
        urls.sort();
        urls
    }
    fn conditional(&self, rel: &str) -> bool {
        let asked = self.asked.lock().unwrap();
        asked
            .iter()
            .any(|(u, tag, _)| *u == url(rel) && tag.is_some())
    }
}

impl Fetcher for TaggedStub {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max).map_err(|e| e.wire())
    }

    fn get_classified(&self, url: &str, max: usize) -> Result<Vec<u8>, FetchError> {
        match self.get_tagged(url, max, None)? {
            Tagged::Body { bytes, .. } => Ok(bytes),
            Tagged::NotModified => Err(FetchError::from_message("unexpected 304")),
        }
    }

    fn get_tagged(&self, url: &str, _max: usize, etag: Option<&str>) -> Result<Tagged, FetchError> {
        let sent = etag.map(str::to_string);
        let Some((body, tag)) = self.files.get(url) else {
            self.asked.lock().unwrap().push((url.into(), sent, false));
            return Err(FetchError::from_message(
                "the server answered 404 Not Found",
            ));
        };
        let fresh = etag != Some(tag.as_str());
        self.asked.lock().unwrap().push((url.into(), sent, fresh));
        if fresh {
            Ok(Tagged::Body {
                bytes: body.clone(),
                etag: Some(tag.clone()),
            })
        } else {
            Ok(Tagged::NotModified)
        }
    }
}

fn url(rel: &str) -> String {
    format!("{BASE}{rel}")
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

fn index(files: &[&str]) -> String {
    let rows: String = files
        .iter()
        .map(|f| {
            format!(
                "    <file name=\"{f}\" url=\"{}\" />\n",
                url(&format!("top/{f}"))
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<index>\n  <info><name>Invented</name></info>\n  <files>\n{rows}  </files>\n</index>"
    )
}

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

fn has(state: &AppState, id: &str) -> bool {
    state.hub().expect("hub").store.contains_id(id)
}

fn first_run(state: &AppState) -> TaggedStub {
    let stub = TaggedStub::default()
        .with("top.index", &index(&["a.xml", "b.xml"]), "t1")
        .with("top/a.xml", &feat("A", "first"), "a1")
        .with("top/b.xml", &feat("B", "second"), "b1");
    state.add_index(&url("top.index")).expect("add");
    let result = state.update_indexes(&stub, &NullSink).expect("update");
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.updated, 1);
    stub
}

#[test]
fn update_downloads_only_the_files_whose_tag_changed() {
    let (_d, state) = app();
    first_run(&state);

    let second = TaggedStub::default()
        .with("top.index", &index(&["a.xml", "b.xml"]), "t1")
        .with("top/a.xml", &feat("A", "first"), "a1")
        .with("top/b.xml", &feat("B", "rewritten"), "b2");
    let result = state.update_indexes(&second, &NullSink).expect("second");
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.updated, 1, "the changed content is applied");
    assert_eq!(result.changed, vec![url("top/b.xml")]);
    assert_eq!(
        second.bodies_sent(),
        vec![url("top.index"), url("top/b.xml")],
        "a.xml was answered 304 and not downloaded again"
    );
    assert!(second.conditional("top/a.xml") && second.conditional("top/b.xml"));
    assert!(has(&state, "ID_INVENTED_FEAT_A"), "unchanged file kept");
    let b = state
        .hub()
        .unwrap()
        .store
        .element("ID_INVENTED_FEAT_B")
        .map(|e| format!("{:?}", e.description))
        .unwrap_or_default();
    assert!(b.contains("rewritten"), "{b}");
}

#[test]
fn nothing_changed_installs_nothing_and_downloads_no_content() {
    let (_d, state) = app();
    let again = TaggedStub::same_files(&first_run(&state));
    let result = state.update_indexes(&again, &NullSink).expect("again");
    assert_eq!((result.updated, result.failed.len()), (0, 0));
    assert_eq!(again.bodies_sent(), vec![url("top.index")]);
}

#[test]
fn an_entry_deleted_from_the_index_leaves_the_pack() {
    let (_d, state) = app();
    first_run(&state);
    let second = TaggedStub::default()
        .with("top.index", &index(&["a.xml"]), "t2")
        .with("top/a.xml", &feat("A", "first"), "a1")
        .with("top/b.xml", &feat("B", "second"), "b1");
    let result = state.update_indexes(&second, &NullSink).expect("second");
    assert_eq!(result.updated, 1);
    assert!(has(&state, "ID_INVENTED_FEAT_A"));
    assert!(!has(&state, "ID_INVENTED_FEAT_B"), "deleted entry removed");
    assert!(!second.bodies_sent().contains(&url("top/a.xml")));
    let packs: Vec<_> = state
        .list_packs()
        .unwrap()
        .into_iter()
        .filter(|p| p.kind != PackKind::Bundled)
        .collect();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].element_count, 1);
}

#[test]
fn a_missing_local_copy_is_downloaded_again_in_full() {
    let (d, state) = app();
    let again = TaggedStub::same_files(&first_run(&state));
    let cache = d.path().join("index_cache");
    assert!(
        cache.is_dir(),
        "downloaded files are kept for the next Update"
    );
    std::fs::remove_dir_all(&cache).unwrap();
    let result = state.update_indexes(&again, &NullSink).expect("again");
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(again.bodies_sent().len(), 3, "index plus both files");
    assert!(has(&state, "ID_INVENTED_FEAT_A") && has(&state, "ID_INVENTED_FEAT_B"));
}
