//! B-221 (U17b): the notes view on an Android Storage Access Framework vault. The document tree here is a fake over a
//! copy of the fixture vault with the framework's rules (no paths, creates and renames never replace, writes rewrite in
//! place), installed the way the Android app installs the Kotlin `VaultPlugin`. Everything above it is the real code:
//! the same notes flow must give the same results as on a local folder (`tests/vault.rs`).

use borealis_lib::error::ErrorCode;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::vault::dto::{DeleteStatus, SaveStatus};
use borealis_lib::vault::text;
use borealis_lib::vault::tree_store::{DocEntry, DocumentTree, TreeError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const TREE: &str =
    "content://com.android.externalstorage.documents/tree/primary%3ADocuments%2FCampaign";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// A document tree over a real folder, with the Storage Access Framework's rules.
#[derive(Debug)]
struct FolderTree {
    root: PathBuf,
    granted: AtomicBool,
    released: Mutex<Vec<String>>,
    /// Every path written or created, to show `.obsidian` is never touched.
    writes: Mutex<Vec<String>>,
}

impl FolderTree {
    fn at(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .filter(|p| !p.is_empty())
            .fold(self.root.clone(), |p, part| p.join(part))
    }

    fn entry(&self, rel: &str, path: &Path) -> Option<DocEntry> {
        let meta = std::fs::metadata(path).ok()?;
        Some(DocEntry {
            path: rel.to_string(),
            dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            mtime_ms: 1,
        })
    }

    fn guard(&self) -> Result<(), TreeError> {
        if self.granted.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(TreeError::Failed("permission revoked".into()))
        }
    }

    fn walk(&self, rel: &str, out: &mut Vec<DocEntry>) {
        for e in self.children_of(rel) {
            let name = e.path.rsplit('/').next().unwrap_or_default().to_string();
            let descend = e.dir && !name.starts_with('.');
            let path = e.path.clone();
            out.push(e);
            if descend {
                self.walk(&path, out);
            }
        }
    }

    fn children_of(&self, dir: &str) -> Vec<DocEntry> {
        let Ok(read) = std::fs::read_dir(self.at(dir)) else {
            return Vec::new();
        };
        read.filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let rel = if dir.is_empty() {
                    name
                } else {
                    format!("{dir}/{name}")
                };
                self.entry(&rel, &e.path())
            })
            .collect()
    }
}

impl DocumentTree for FolderTree {
    fn check(&self, tree: &str) -> Result<(), TreeError> {
        assert_eq!(tree, TREE);
        self.guard()
    }
    fn list(&self, _: &str) -> Result<Vec<DocEntry>, TreeError> {
        self.guard()?;
        let mut out = Vec::new();
        self.walk("", &mut out);
        Ok(out)
    }
    fn children(&self, _: &str, dir: &str) -> Result<Vec<DocEntry>, TreeError> {
        self.guard()?;
        Ok(self.children_of(dir))
    }
    fn stat(&self, _: &str, rel: &str) -> Result<Option<DocEntry>, TreeError> {
        self.guard()?;
        Ok(self.entry(rel, &self.at(rel)))
    }
    fn read(&self, _: &str, rel: &str, max: u64) -> Result<Option<Vec<u8>>, TreeError> {
        self.guard()?;
        let bytes = std::fs::read(self.at(rel)).map_err(|_| TreeError::NotFound)?;
        Ok((bytes.len() as u64 <= max).then_some(bytes))
    }
    fn write(&self, _: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        self.guard()?;
        let path = self.at(rel);
        if !path.is_file() {
            return Err(TreeError::NotFound);
        }
        self.writes.lock().unwrap().push(rel.to_string());
        std::fs::write(path, bytes).map_err(|e| TreeError::Failed(e.to_string()))
    }
    fn create(&self, _: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        self.guard()?;
        let path = self.at(rel);
        let mut dir = self.root.clone();
        for part in rel.split('/').collect::<Vec<_>>().split_last().unwrap().1 {
            dir.push(part);
            if dir.is_file() {
                return Err(TreeError::NotAFolder);
            }
            if !dir.exists() {
                std::fs::create_dir(&dir).unwrap();
            }
        }
        if path.exists() {
            return Err(TreeError::Exists);
        }
        self.writes.lock().unwrap().push(rel.to_string());
        std::fs::write(path, bytes).map_err(|e| TreeError::Failed(e.to_string()))
    }
    fn rename(&self, _: &str, rel: &str, name: &str) -> Result<(), TreeError> {
        self.guard()?;
        let from = self.at(rel);
        let to = from.with_file_name(name);
        let exact = std::fs::read_dir(from.parent().unwrap())
            .unwrap()
            .any(|e| e.unwrap().file_name().to_str() == Some(name));
        if exact {
            return Err(TreeError::Exists);
        }
        std::fs::rename(from, to).map_err(|_| TreeError::NotFound)
    }
    fn delete(&self, _: &str, rel: &str) -> Result<(), TreeError> {
        self.guard()?;
        std::fs::remove_file(self.at(rel)).map_err(|_| TreeError::NotFound)
    }
    fn release(&self, tree: &str) {
        self.released.lock().unwrap().push(tree.to_string());
    }
}

struct Opened {
    _data: tempfile::TempDir,
    vault: tempfile::TempDir,
    tree: Arc<FolderTree>,
    state: AppState,
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    })
    .unwrap();
    (dir, state)
}

fn pick(state: &AppState, what: &str) -> String {
    state
        .picked
        .grant(PathBuf::from(what), PickPurpose::VaultDir)
        .unwrap()
        .token
}

/// An app whose vault is the document tree over a fixture copy.
fn opened() -> Opened {
    let vault = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy_dir(&fixture(), vault.path());
    let (data, state) = app();
    let tree = Arc::new(FolderTree {
        root: vault.path().to_path_buf(),
        granted: AtomicBool::new(true),
        released: Mutex::default(),
        writes: Mutex::default(),
    });
    state.vault.set_document_tree(tree.clone());
    let token = pick(&state, TREE);
    let dirs = state.set_vault_dir(Some(&token)).unwrap();
    assert_eq!(dirs.vault_dir.as_deref(), Some(TREE));
    Opened {
        _data: data,
        vault,
        tree,
        state,
    }
}

fn read(dir: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(dir.join(rel)).unwrap()
}

#[test]
fn a_document_tree_lists_like_the_same_folder_opened_in_place() {
    let o = opened();
    let tree = o.state.vault_tree().unwrap();
    assert_eq!(tree.dir.as_deref(), Some(TREE));
    assert_eq!(tree.name.as_deref(), Some("Campaign"));
    assert!(tree.obsidian && !tree.unavailable && !tree.truncated);
    assert_eq!(tree.config.new_file_folder_path, "Inbox");

    // The same folder opened as a local vault.
    let (_d, local) = app();
    let token = local
        .picked
        .grant(o.vault.path().to_path_buf(), PickPurpose::VaultDir)
        .unwrap()
        .token;
    local.set_vault_dir(Some(&token)).unwrap();
    let local_tree = local.vault_tree().unwrap();
    let shape = |t: &borealis_lib::vault::dto::VaultTree| {
        t.entries
            .iter()
            .map(|e| (e.path.clone(), e.name.clone(), e.kind, e.size))
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&tree), shape(&local_tree));
    assert_eq!(tree.config, local_tree.config);
}

#[test]
fn notes_round_trip_and_an_edit_keeps_the_bom_and_line_endings() {
    let o = opened();
    for rel in [
        "Home.md",
        "Session 1.md",
        "Odd/Odd front matter.md",
        "Odd/Empty.md",
        "Places/Harbor Town.md",
    ] {
        let before = read(o.vault.path(), rel);
        let note = o.state.vault_read(rel).unwrap();
        assert_eq!(note.hash, text::hash(&before));
        let saved = o
            .state
            .vault_save(rel, &note.text.replace("\r\n", "\n"), Some(&note.hash))
            .unwrap();
        assert_eq!(saved.status, SaveStatus::Saved, "{rel}");
        assert_eq!(read(o.vault.path(), rel), before, "{rel} changed");
    }
    assert!(
        o.tree.writes.lock().unwrap().is_empty(),
        "unchanged notes are not written"
    );
    let before = read(o.vault.path(), "Session 1.md");
    let note = o.state.vault_read("Session 1.md").unwrap();
    let edited = note
        .text
        .replace("\r\n", "\n")
        .replace("- [ ] Ask", "- [x] Ask");
    o.state
        .vault_save("Session 1.md", &edited, Some(&note.hash))
        .unwrap();
    let expected = String::from_utf8(before)
        .unwrap()
        .replace("- [ ] Ask", "- [x] Ask");
    assert_eq!(read(o.vault.path(), "Session 1.md"), expected.into_bytes());
    let latin = o.state.vault_read("Odd/Not unicode.md").unwrap();
    assert!(latin.read_only.is_some());
}

#[test]
fn a_note_changed_by_obsidian_sync_is_never_overwritten() {
    let o = opened();
    let rel = "People/Bramblewick.md";
    let note = o.state.vault_read(rel).unwrap();
    std::fs::write(o.vault.path().join(rel), "Obsidian Sync wrote this.\n").unwrap();
    let out = o
        .state
        .vault_save(rel, "Borealis edit\n", Some(&note.hash))
        .unwrap();
    assert_eq!(out.status, SaveStatus::Conflict);
    let disk = out.disk.unwrap();
    assert_eq!(disk.text, "Obsidian Sync wrote this.\n");
    assert_eq!(read(o.vault.path(), rel), b"Obsidian Sync wrote this.\n");
    let kept = o
        .state
        .vault_save(rel, "Borealis edit\n", Some(&disk.hash))
        .unwrap();
    assert_eq!(kept.status, SaveStatus::Saved);
    assert_eq!(read(o.vault.path(), rel), b"Borealis edit\n");
    std::fs::remove_file(o.vault.path().join(rel)).unwrap();
    let gone = o
        .state
        .vault_save(rel, "again\n", Some(&kept.hash.unwrap()))
        .unwrap();
    assert_eq!(gone.status, SaveStatus::Missing);
    assert!(!o.vault.path().join(rel).exists(), "not recreated silently");
    assert_eq!(
        o.state.vault_save(rel, "again\n", None).unwrap().status,
        SaveStatus::Saved
    );
    assert!(
        o.state.vault_save(rel, "x", None).is_err(),
        "never replaced"
    );
}

#[test]
fn create_rename_and_delete_follow_the_obsidian_settings_and_never_replace() {
    let o = opened();
    let obsidian_before: Vec<(PathBuf, Vec<u8>)> = snapshot(&o.vault.path().join(".obsidian"));
    let first = o
        .state
        .vault_create_note(None, Some("Places/Harbor Town.md"), None)
        .unwrap();
    assert_eq!(first.path, "Inbox/Untitled.md");
    assert_eq!(
        o.state.vault_create_note(None, None, None).unwrap().path,
        "Inbox/Untitled 1.md"
    );
    assert!(o
        .state
        .vault_create_note(Some("Home"), None, Some(""))
        .is_err());

    let plan = o
        .state
        .vault_rename_preview("People/Bramblewick.md", "Bramblewick the Bold")
        .unwrap();
    let files: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["Home.md", "Places/Harbor Town.md", "Session 1.md"]);
    let done = o
        .state
        .vault_rename("People/Bramblewick.md", "Bramblewick the Bold", true)
        .unwrap();
    assert_eq!(done.updated.len(), 3);
    assert!(o
        .vault
        .path()
        .join("People/Bramblewick the Bold.md")
        .is_file());
    assert!(String::from_utf8(read(o.vault.path(), "Home.md"))
        .unwrap()
        .contains("[[People/Bramblewick the Bold|"));
    assert!(o.state.vault_rename("Home.md", "Session 1", true).is_err());

    let no_trash = |_: &Path| -> Result<(), String> { panic!("a document tree has no trash") };
    assert_eq!(
        o.state
            .vault_delete_with("map.png", false, no_trash)
            .unwrap(),
        DeleteStatus::TrashUnavailable
    );
    assert!(o.vault.path().join("map.png").exists());
    assert_eq!(
        o.state
            .vault_delete_with("map.png", true, no_trash)
            .unwrap(),
        DeleteStatus::Deleted
    );
    assert!(!o.vault.path().join("map.png").exists());
    assert_eq!(snapshot(&o.vault.path().join(".obsidian")), obsidian_before);
    assert!(o
        .tree
        .writes
        .lock()
        .unwrap()
        .iter()
        .all(|p| !p.starts_with('.')));
}

#[test]
fn links_backlinks_tags_search_and_images_work_on_a_document_tree() {
    let o = opened();
    let resolved = o
        .state
        .vault_resolve("Home.md", &["Bramblewick".into(), "Nowhere".into()])
        .unwrap();
    assert_eq!(resolved[0].as_deref(), Some("People/Bramblewick.md"));
    assert_eq!(resolved[1], None);
    let back = o.state.vault_backlinks("People/Bramblewick.md").unwrap();
    assert!(back.iter().any(|b| b.path == "Home.md"));
    assert!(!o.state.vault_tags().unwrap().is_empty());
    assert!(!o.state.vault_search("harbor", None).unwrap().is_empty());
    assert!(o
        .state
        .vault_image("map.png")
        .unwrap()
        .starts_with("data:image/png;base64,"));
    for bad in [".obsidian/app.json", "../x.md", "Odd/../Home.md"] {
        assert!(o.state.vault_read(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_revoked_tree_is_unavailable_and_kept_and_a_new_choice_releases_the_old_one() {
    let o = opened();
    o.tree.granted.store(false, Ordering::SeqCst);
    assert!(o.state.vault_unavailable().unwrap());
    let tree = o.state.vault_tree().unwrap();
    assert!(tree.unavailable && tree.entries.is_empty());
    assert_eq!(tree.dir.as_deref(), Some(TREE), "the setting is kept");
    assert_eq!(tree.name.as_deref(), Some("Campaign"));
    assert!(o.state.vault_read("Home.md").is_err());
    // Choosing it again is refused while it cannot be read.
    let token = pick(&o.state, TREE);
    assert!(o.state.set_vault_dir(Some(&token)).is_err());
    o.tree.granted.store(true, Ordering::SeqCst);
    assert!(!o.state.vault_unavailable().unwrap());
    // Clearing the vault gives the permission back.
    o.state.set_vault_dir(None).unwrap();
    assert_eq!(*o.tree.released.lock().unwrap(), [TREE.to_string()]);
}

#[test]
fn without_document_tree_access_a_tree_uri_is_refused() {
    let (_d, state) = app();
    let token = pick(&state, TREE);
    let e = state.set_vault_dir(Some(&token)).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
    assert_eq!(state.settings().unwrap().vault_dir, None);
}

/// Every file under `dir`, with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((
                    p.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read(&p).unwrap(),
                ));
            }
        }
    }
    out.sort();
    out
}
