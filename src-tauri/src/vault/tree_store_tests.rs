use super::*;
use std::sync::Mutex;

const URI: &str =
    "content://com.android.externalstorage.documents/tree/primary%3ADocuments%2FCampaign%20vault";

/// An in-memory document tree with the Storage Access Framework's rules: creates and renames never replace, and
/// nothing is atomic. `None` is a folder.
#[derive(Debug, Default)]
struct MemTree {
    files: Mutex<BTreeMap<String, Option<Vec<u8>>>>,
    /// Every write and create, in order (to show nothing touched `.obsidian`).
    writes: Mutex<Vec<String>>,
    granted: bool,
}

impl MemTree {
    fn with(files: &[(&str, Option<&str>)]) -> Arc<Self> {
        Arc::new(Self {
            files: Mutex::new(
                files
                    .iter()
                    .map(|(p, b)| (p.to_string(), b.map(|t| t.as_bytes().to_vec())))
                    .collect(),
            ),
            writes: Mutex::default(),
            granted: true,
        })
    }

    fn get(&self, rel: &str) -> Option<Option<Vec<u8>>> {
        self.files.lock().unwrap().get(rel).cloned()
    }

    fn entry(path: &str, v: &Option<Vec<u8>>) -> DocEntry {
        DocEntry {
            path: path.to_string(),
            dir: v.is_none(),
            size: v.as_ref().map_or(0, |b| b.len() as u64),
            mtime_ms: 1,
        }
    }
}

impl DocumentTree for MemTree {
    fn check(&self, _: &str) -> Result<(), TreeError> {
        if self.granted {
            Ok(())
        } else {
            Err(TreeError::Failed("no permission".into()))
        }
    }
    fn list(&self, _: &str) -> Result<Vec<DocEntry>, TreeError> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            // Hidden folders are listed but not entered.
            .filter(|(p, _)| !p.split('/').rev().skip(1).any(|part| part.starts_with('.')))
            .map(|(p, v)| Self::entry(p, v))
            .collect())
    }
    fn children(&self, _: &str, dir: &str) -> Result<Vec<DocEntry>, TreeError> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| paths::parent(p) == dir)
            .map(|(p, v)| Self::entry(p, v))
            .collect())
    }
    fn stat(&self, _: &str, rel: &str) -> Result<Option<DocEntry>, TreeError> {
        Ok(self.get(rel).map(|v| Self::entry(rel, &v)))
    }
    fn read(&self, _: &str, rel: &str, max: u64) -> Result<Option<Vec<u8>>, TreeError> {
        match self.get(rel) {
            Some(Some(b)) => Ok((b.len() as u64 <= max).then_some(b)),
            _ => Err(TreeError::NotFound),
        }
    }
    fn write(&self, _: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        let mut files = self.files.lock().unwrap();
        match files.get_mut(rel) {
            Some(Some(b)) => {
                *b = bytes.to_vec();
                self.writes.lock().unwrap().push(rel.to_string());
                Ok(())
            }
            _ => Err(TreeError::NotFound),
        }
    }
    fn create(&self, _: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        let mut files = self.files.lock().unwrap();
        let mut dir = String::new();
        for part in paths::parent(rel).split('/').filter(|p| !p.is_empty()) {
            dir = paths::join(&dir, part);
            match files.get(&dir) {
                Some(None) => {}
                Some(Some(_)) => return Err(TreeError::NotAFolder),
                None => {
                    files.insert(dir.clone(), None);
                }
            }
        }
        if files.contains_key(rel) {
            return Err(TreeError::Exists);
        }
        files.insert(rel.to_string(), Some(bytes.to_vec()));
        self.writes.lock().unwrap().push(rel.to_string());
        Ok(())
    }
    fn rename(&self, _: &str, rel: &str, name: &str) -> Result<(), TreeError> {
        let mut files = self.files.lock().unwrap();
        let to = paths::join(paths::parent(rel), name);
        if files.contains_key(&to) {
            return Err(TreeError::Exists);
        }
        let v = files.remove(rel).ok_or(TreeError::NotFound)?;
        files.insert(to, v);
        Ok(())
    }
    fn delete(&self, _: &str, rel: &str) -> Result<(), TreeError> {
        self.files
            .lock()
            .unwrap()
            .remove(rel)
            .map(|_| ())
            .ok_or(TreeError::NotFound)
    }
    fn release(&self, _: &str) {}
}

fn vault() -> (Arc<MemTree>, TreeStore) {
    let tree = MemTree::with(&[
        (".obsidian", None),
        (
            ".obsidian/app.json",
            Some(r#"{"newFileLocation":"folder","newFileFolderPath":"Inbox"}"#),
        ),
        (".obsidian/plugins", None),
        (".obsidian/plugins/x.md", Some("hidden")),
        (".trash", None),
        (".trash/Old.md", Some("old")),
        (".dotfile.md", Some("dot")),
        ("Home.md", Some("# Home\n[[Harbor]]\n")),
        ("b.md", Some("b")),
        ("Places", None),
        ("Places/Harbor.md", Some("Harbor\r\n")),
        ("Places/map.png", Some("png")),
        ("archive", None),
    ]);
    let store = TreeStore::open(tree.clone(), URI).unwrap();
    (tree, store)
}

#[test]
fn tree_uris_are_told_from_folders_and_named_for_people() {
    assert!(is_tree_uri(URI));
    assert!(!is_tree_uri("/storage/emulated/0/Documents/Vault"));
    assert!(!is_tree_uri("content://media/external/file/12"));
    assert_eq!(
        display_path(URI).as_deref(),
        Some("Internal storage/Documents/Campaign vault")
    );
    assert_eq!(display_name(URI).as_deref(), Some("Campaign vault"));
    let sd = "content://com.android.externalstorage.documents/tree/1A2B-3C4D%3ANotes";
    assert_eq!(display_path(sd).as_deref(), Some("1A2B-3C4D/Notes"));
    let root = "content://com.android.externalstorage.documents/tree/primary%3A";
    assert_eq!(display_name(root).as_deref(), Some("Internal storage"));
    // A provider with opaque ids (a document id with a child document part after it).
    let drive = "content://com.example.docs/tree/abc123/document/abc123";
    assert_eq!(display_path(drive).as_deref(), Some("abc123"));
    assert_eq!(display_name("content://x/tree/"), None);
}

#[test]
fn the_listing_hides_hidden_names_and_orders_like_the_folder_listing() {
    let (_, store) = vault();
    let paths: Vec<String> = store.list().entries.into_iter().map(|e| e.path).collect();
    assert_eq!(
        paths,
        [
            "archive",
            "Places",
            "Places/Harbor.md",
            "Places/map.png",
            "b.md",
            "Home.md"
        ]
    );
    assert!(store.has_obsidian());
    assert!(store.app_json().unwrap().contains("Inbox"));
    assert_eq!(store.name().as_deref(), Some("Campaign vault"));
}

#[test]
fn the_listing_is_cut_at_the_limits() {
    let many: Vec<DocEntry> = (0..tree::MAX_ENTRIES + 5)
        .map(|i| DocEntry {
            path: format!("n{i}.md"),
            dir: false,
            size: 1,
            mtime_ms: 0,
        })
        .collect();
    let listing = listing_of(many);
    assert!(listing.truncated);
    assert_eq!(listing.entries.len(), tree::MAX_ENTRIES);
    // Entries with paths that could leave the vault are dropped.
    let odd = listing_of(vec![
        DocEntry {
            path: "../x.md".into(),
            dir: false,
            size: 1,
            mtime_ms: 0,
        },
        DocEntry {
            path: "a\\b.md".into(),
            dir: false,
            size: 1,
            mtime_ms: 0,
        },
    ]);
    assert!(odd.entries.is_empty());
}

#[test]
fn a_write_happens_only_while_the_note_is_what_was_loaded() {
    let (tree, store) = vault();
    let loaded = store.read("Places/Harbor.md", 1024).unwrap().unwrap();
    assert_eq!(loaded.real_name, "Harbor.md");
    let base = text::hash(&loaded.bytes);
    assert_eq!(
        store
            .guarded_write("Places/Harbor.md", &base, b"Harbor town\r\n")
            .unwrap(),
        Outcome::Saved {
            hash: text::hash(b"Harbor town\r\n")
        }
    );
    // Another app changes it: the stale base is refused and the disk text comes back.
    tree.write(URI, "Places/Harbor.md", b"Theirs").unwrap();
    assert_eq!(
        store
            .guarded_write("Places/Harbor.md", &base, b"Mine")
            .unwrap(),
        Outcome::Changed {
            disk: b"Theirs".to_vec()
        }
    );
    assert_eq!(tree.get("Places/Harbor.md"), Some(Some(b"Theirs".to_vec())));
    // Gone: nothing is written.
    tree.delete(URI, "Places/Harbor.md").unwrap();
    assert_eq!(
        store
            .guarded_write("Places/Harbor.md", &text::hash(b"Theirs"), b"Mine")
            .unwrap(),
        Outcome::Missing
    );
    assert_eq!(tree.get("Places/Harbor.md"), None);
}

#[test]
fn an_unchanged_note_is_not_written_again() {
    let (tree, store) = vault();
    let before = tree.writes.lock().unwrap().len();
    let base = text::hash(b"b");
    assert_eq!(
        store.guarded_write("b.md", &base, b"b").unwrap(),
        Outcome::Saved { hash: base }
    );
    assert_eq!(tree.writes.lock().unwrap().len(), before);
}

#[test]
fn hidden_and_escaping_paths_are_refused_and_obsidian_is_never_written() {
    let (tree, store) = vault();
    for bad in [
        ".obsidian/app.json",
        ".obsidian/plugins/x.md",
        ".trash/Old.md",
        ".dotfile.md",
        "../Home.md",
        "Places/../Home.md",
        "/Home.md",
        "Places\\Harbor.md",
        "",
    ] {
        assert!(store.read(bad, 1024).is_err(), "read {bad}");
        assert!(store.guarded_write(bad, "x", b"y").is_err(), "write {bad}");
        assert!(store.create_new(bad, b"y").is_err(), "create {bad}");
        assert!(!store.exists(bad), "exists {bad}");
    }
    assert!(
        store.create_new(".obsidian/new.md", b"x").is_err(),
        "no new file in .obsidian"
    );
    assert!(tree
        .writes
        .lock()
        .unwrap()
        .iter()
        .all(|p| !p.starts_with('.')));
}

#[test]
fn creates_and_renames_never_replace_a_file() {
    let (tree, store) = vault();
    store.create_new("Inbox/New.md", b"new").unwrap();
    assert_eq!(tree.get("Inbox"), Some(None), "the folder was made");
    let e = store.create_new("Home.md", b"x").unwrap_err();
    assert_eq!(e.message, "A file with that name is already there.");
    let e = store.create_new("b.md/inside.md", b"x").unwrap_err();
    assert_eq!(e.message, "A file is in the way of that folder.");
    assert!(store.rename_taken("Home.md", "b.md", false).unwrap());
    assert!(
        store.rename_taken("Home.md", "B.md", false).unwrap(),
        "another case of a taken name"
    );
    assert!(
        !store.rename_taken("Home.md", "home.md", true).unwrap(),
        "a new case of its own name"
    );
    assert!(store.rename("Home.md", "b.md", false).is_err());
    store.rename("Home.md", "Start.md", false).unwrap();
    assert!(tree.get("Start.md").is_some() && tree.get("Home.md").is_none());
    assert!(store.rename("Start.md", "bad/name.md", false).is_err());
}

#[test]
fn deleting_asks_first_because_there_is_no_trash() {
    let (tree, store) = vault();
    let trash = |_: &Path| -> Result<(), String> { panic!("a document tree has no trash") };
    assert_eq!(
        store.delete("b.md", false, &trash).unwrap(),
        DeleteStatus::TrashUnavailable
    );
    assert!(tree.get("b.md").is_some(), "nothing deleted yet");
    assert_eq!(
        store.delete("b.md", true, &trash).unwrap(),
        DeleteStatus::Deleted
    );
    assert!(tree.get("b.md").is_none());
    assert_eq!(
        store.delete("b.md", true, &trash).unwrap_err().code,
        ErrorCode::NotFound
    );
    assert!(
        store.delete("Places", true, &trash).is_err(),
        "not a folder"
    );
}

#[test]
fn folders_and_missing_files_are_reported_as_such() {
    let (_, store) = vault();
    assert_eq!(
        store.file("Places").unwrap_err().message,
        "That is a folder, not a file."
    );
    assert_eq!(store.file("Nope.md").unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(store.file("Places/map.png").unwrap(), "map.png");
    assert!(store.read("b.md", 0).unwrap().is_none(), "too large");
    assert_eq!(store.mtime("Home.md"), 1);
    assert_eq!(store.mtime("Nope.md"), 0);
}

#[test]
fn a_tree_without_permission_does_not_open() {
    let tree = Arc::new(MemTree::default());
    assert!(TreeStore::open(tree, URI).is_err());
}
