//! Remembered folders under the App Sandbox (MAS1, B-190; specs/mac-app-store.md section B).
//!
//! The Mac App Store build may read a folder the user picked in a panel only for that run. Three settings
//! remember a folder across launches: the custom documents folder (`documents_dir`), the secondary content
//! folder (`secondary_content_dir`), the portrait library (`portrait_library_dir`, GitHub #32) and the Obsidian vault
//! (`vault_dir`, GitHub #15; read-write, like the user-selected entitlement). For each, a security-scoped bookmark is stored when the user picks it, and at
//! launch, before anything reads the folder, the bookmark is resolved and access is started; a stale bookmark is
//! replaced by a fresh one. A bookmark that no longer resolves is kept and reported (the folder must be chosen
//! again); this module never changes the settings or deletes anything.
//!
//! The bookmarks live in `<data>/folder-bookmarks.json`, next to the settings. Only the Mac App Store build has a
//! [`SecurityScope`] ([`platform_scope`]); everywhere else every function here is a no-op.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The bookmark file in the app data folder.
pub const BOOKMARKS_FILE: &str = "folder-bookmarks.json";

/// A folder the app keeps across launches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RememberedFolder {
    /// Settings > Data Files: the documents folder (`<dir>/characters`).
    Documents,
    /// Settings > Data Files: the secondary content folder.
    SecondaryContent,
    /// Settings > Content: the portrait library folder (GitHub #32).
    PortraitLibrary,
    /// Settings > Content: the Obsidian vault (GitHub #15), read and written in place.
    Vault,
}

impl RememberedFolder {
    /// Every remembered folder.
    pub const ALL: [Self; 4] = [
        Self::Documents,
        Self::SecondaryContent,
        Self::PortraitLibrary,
        Self::Vault,
    ];

    /// The key in the bookmark file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Documents => "documents-dir",
            Self::SecondaryContent => "secondary-content-dir",
            Self::PortraitLibrary => "portrait-library-dir",
            Self::Vault => "vault-dir",
        }
    }
}

/// A resolved bookmark: where the folder is now and whether the bookmark should be recreated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    pub stale: bool,
}

/// The OS side of security-scoped bookmarks (macOS `NSURL` in the Mac App Store build; fakes in tests).
pub trait SecurityScope {
    /// A bookmark for `folder`, which the app can access right now (just picked, or just resolved).
    ///
    /// # Errors
    /// The OS message when no bookmark can be made.
    fn create(&self, folder: &Path) -> Result<Vec<u8>, String>;

    /// Resolve `bookmark` and start accessing the folder for the rest of the process.
    ///
    /// # Errors
    /// The OS message when the bookmark no longer resolves or access is refused.
    fn resolve_and_start(&self, bookmark: &[u8]) -> Result<Resolved, String>;
}

/// The result of restoring one remembered folder at launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restored {
    /// Access started; the folder is where it was.
    Ready(PathBuf),
    /// Access started and the bookmark was renewed (it was stale).
    Refreshed(PathBuf),
    /// Access started, but the folder was moved or renamed: `to` is its new path (the setting still says `from`).
    Moved { from: PathBuf, to: PathBuf },
    /// The bookmark no longer resolves: the user must choose the folder again. The bookmark is kept.
    NeedsChoosing { path: PathBuf, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    /// The folder's path when the bookmark was made (display and comparison only).
    path: String,
    /// The bookmark bytes, base64.
    bookmark: String,
}

type Entries = BTreeMap<String, Entry>;

fn file(data: &Path) -> PathBuf {
    data.join(BOOKMARKS_FILE)
}

/// The stored bookmarks; a missing or unreadable file reads as none (logged).
fn load(data: &Path) -> Entries {
    match std::fs::read_to_string(file(data)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("folder bookmarks: unreadable file ignored: {e}");
            Entries::new()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Entries::new(),
        Err(e) => {
            tracing::warn!("folder bookmarks: file not read: {e}");
            Entries::new()
        }
    }
}

fn save(data: &Path, entries: &Entries) -> Result<(), String> {
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(file(data), text)
        .map_err(|e| format!("could not save the folder bookmarks: {e}"))
}

fn entry(scope: &dyn SecurityScope, folder: &Path) -> Result<Entry, String> {
    Ok(Entry {
        path: folder.to_string_lossy().into_owned(),
        bookmark: STANDARD.encode(scope.create(folder)?),
    })
}

/// Store a bookmark for `folder` as `which` (the user just picked it, so the app can access it now).
///
/// # Errors
/// When no bookmark can be made or the file cannot be written.
pub fn remember(
    data: &Path,
    scope: &dyn SecurityScope,
    which: RememberedFolder,
    folder: &Path,
) -> Result<(), String> {
    let next = entry(scope, folder)?;
    let entries: Entries = load(data)
        .into_iter()
        .filter(|(k, _)| k != which.key())
        .chain([(which.key().to_string(), next)])
        .collect();
    save(data, &entries)
}

/// Drop the bookmark of `which` (the folder was unlinked or reset to the default). No file, no entry: nothing to do.
///
/// # Errors
/// When the file cannot be written.
pub fn forget(data: &Path, which: RememberedFolder) -> Result<(), String> {
    let entries = load(data);
    if !entries.contains_key(which.key()) {
        return Ok(());
    }
    let rest: Entries = entries
        .into_iter()
        .filter(|(k, _)| k != which.key())
        .collect();
    save(data, &rest)
}

/// Resolve one stored entry: the outcome, and the entry to keep (renewed when the bookmark was stale).
fn restore_entry(scope: &dyn SecurityScope, stored: Entry) -> (Restored, Entry) {
    let path = PathBuf::from(&stored.path);
    let resolved = STANDARD
        .decode(&stored.bookmark)
        .map_err(|e| format!("the stored bookmark is damaged: {e}"))
        .and_then(|bytes| scope.resolve_and_start(&bytes));
    let resolved = match resolved {
        Ok(r) => r,
        Err(reason) => return (Restored::NeedsChoosing { path, reason }, stored),
    };
    let (kept, renewed) = if resolved.stale {
        match entry(scope, &resolved.path) {
            Ok(fresh) => (fresh, true),
            Err(e) => {
                tracing::warn!("folder bookmarks: a stale bookmark could not be renewed: {e}");
                (stored, false)
            }
        }
    } else {
        (stored, false)
    };
    let outcome = if resolved.path != path {
        Restored::Moved {
            from: path,
            to: resolved.path,
        }
    } else if renewed {
        Restored::Refreshed(resolved.path)
    } else {
        Restored::Ready(resolved.path)
    };
    (outcome, kept)
}

/// At launch: resolve every stored bookmark and start access, renewing stale ones. Returns one outcome per
/// stored folder; never removes an entry.
pub fn restore(data: &Path, scope: &dyn SecurityScope) -> Vec<(RememberedFolder, Restored)> {
    let stored = load(data);
    let mut changed = false;
    let mut kept = Entries::new();
    let mut outcomes = Vec::new();
    for which in RememberedFolder::ALL {
        let Some(old) = stored.get(which.key()).cloned() else {
            continue;
        };
        let (outcome, entry) = restore_entry(scope, old.clone());
        changed |= entry != old;
        kept.insert(which.key().to_string(), entry);
        outcomes.push((which, outcome));
    }
    // Keep entries this build does not know (a newer build may add keys).
    let unknown = stored
        .into_iter()
        .filter(|(k, _)| !RememberedFolder::ALL.iter().any(|w| w.key() == k));
    kept.extend(unknown);
    if changed {
        if let Err(e) = save(data, &kept) {
            tracing::warn!("folder bookmarks: {e}");
        }
    }
    outcomes
}

/// This build's security scope: the Mac App Store build only. Other builds can read remembered folders
/// directly, so they store no bookmarks.
#[must_use]
pub fn platform_scope() -> Option<&'static dyn SecurityScope> {
    #[cfg(all(target_os = "macos", feature = "mas"))]
    {
        Some(&crate::folder_access_mac::MacScope)
    }
    #[cfg(not(all(target_os = "macos", feature = "mas")))]
    {
        None
    }
}

/// After a pick: store (`Some`) or drop (`None`) the bookmark of `which` in a build with a security scope;
/// logged, never fatal (the folder works for this run either way).
pub fn after_pick(data: &Path, which: RememberedFolder, folder: Option<&Path>) {
    let Some(scope) = platform_scope() else {
        return;
    };
    let result = match folder {
        Some(folder) => remember(data, scope, which, folder),
        None => forget(data, which),
    };
    if let Err(e) = result {
        tracing::warn!("folder bookmarks: {}: {e}", which.key());
    }
}

/// At launch, before anything reads a remembered folder: restore access in a build with a security scope.
pub fn restore_at_launch(data: &Path) {
    let Some(scope) = platform_scope() else {
        return;
    };
    for (which, outcome) in restore(data, scope) {
        match outcome {
            Restored::Ready(_) | Restored::Refreshed(_) => {
                tracing::info!("folder bookmarks: {} ready", which.key());
            }
            Restored::Moved { .. } => {
                tracing::warn!(
                    "folder bookmarks: {} was moved or renamed; choose it again in Settings",
                    which.key()
                );
            }
            Restored::NeedsChoosing { reason, .. } => {
                tracing::warn!(
                    "folder bookmarks: {} needs to be chosen again: {reason}",
                    which.key()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A fake OS: a bookmark is `b:<path>`; `moved` maps old paths to new ones, `stale` paths resolve stale,
    /// `gone` paths fail. Records every folder access started.
    #[derive(Default)]
    struct Fake {
        moved: HashMap<PathBuf, PathBuf>,
        stale: Vec<PathBuf>,
        gone: Vec<PathBuf>,
        refuse_create: bool,
        started: RefCell<Vec<PathBuf>>,
    }

    impl SecurityScope for Fake {
        fn create(&self, folder: &Path) -> Result<Vec<u8>, String> {
            if self.refuse_create {
                return Err("no bookmark".into());
            }
            Ok(format!("b:{}", folder.display()).into_bytes())
        }
        fn resolve_and_start(&self, bookmark: &[u8]) -> Result<Resolved, String> {
            let text = String::from_utf8(bookmark.to_vec()).map_err(|e| e.to_string())?;
            let path = PathBuf::from(text.strip_prefix("b:").ok_or("not a bookmark")?);
            if self.gone.contains(&path) {
                return Err("the volume is not mounted".into());
            }
            let now = self
                .moved
                .get(&path)
                .cloned()
                .unwrap_or_else(|| path.clone());
            self.started.borrow_mut().push(now.clone());
            Ok(Resolved {
                stale: self.stale.contains(&path) || now != path,
                path: now,
            })
        }
    }

    fn stored(data: &Path) -> Entries {
        load(data)
    }

    #[test]
    fn a_picked_folder_is_bookmarked_and_access_starts_at_the_next_launch() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/Volumes/Docs"),
        )
        .unwrap();
        let outcomes = restore(data.path(), &fake);
        assert_eq!(
            outcomes,
            vec![(
                RememberedFolder::Documents,
                Restored::Ready("/Volumes/Docs".into())
            )]
        );
        assert_eq!(*fake.started.borrow(), vec![PathBuf::from("/Volumes/Docs")]);
    }

    #[test]
    fn picking_again_replaces_the_bookmark_and_keeps_the_other_folder() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/a"),
        )
        .unwrap();
        remember(
            data.path(),
            &fake,
            RememberedFolder::SecondaryContent,
            Path::new("/s"),
        )
        .unwrap();
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/b"),
        )
        .unwrap();
        let entries = stored(data.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries["documents-dir"].path, "/b");
        assert_eq!(entries["secondary-content-dir"].path, "/s");
    }

    #[test]
    fn forgetting_drops_only_that_folder_and_is_a_no_op_without_a_file() {
        let data = tempfile::tempdir().unwrap();
        forget(data.path(), RememberedFolder::Documents).unwrap();
        assert!(!data.path().join(BOOKMARKS_FILE).exists());
        let fake = Fake::default();
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/a"),
        )
        .unwrap();
        remember(
            data.path(),
            &fake,
            RememberedFolder::SecondaryContent,
            Path::new("/s"),
        )
        .unwrap();
        forget(data.path(), RememberedFolder::SecondaryContent).unwrap();
        let entries = stored(data.path());
        assert_eq!(entries.keys().collect::<Vec<_>>(), vec!["documents-dir"]);
    }

    #[test]
    fn a_stale_bookmark_is_renewed() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake {
            stale: vec!["/s".into()],
            ..Fake::default()
        };
        remember(
            data.path(),
            &fake,
            RememberedFolder::SecondaryContent,
            Path::new("/s"),
        )
        .unwrap();
        let before = stored(data.path());
        // The fake makes the same bytes again; renewing still rewrites the entry from a fresh `create`.
        let outcomes = restore(data.path(), &fake);
        assert_eq!(
            outcomes,
            vec![(
                RememberedFolder::SecondaryContent,
                Restored::Refreshed("/s".into())
            )]
        );
        assert_eq!(stored(data.path()), before);
    }

    #[test]
    fn a_moved_folder_gets_a_new_bookmark_and_is_reported() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake {
            moved: HashMap::from([(PathBuf::from("/old"), PathBuf::from("/new"))]),
            ..Fake::default()
        };
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/old"),
        )
        .unwrap();
        let outcomes = restore(data.path(), &fake);
        assert_eq!(
            outcomes,
            vec![(
                RememberedFolder::Documents,
                Restored::Moved {
                    from: "/old".into(),
                    to: "/new".into()
                }
            )]
        );
        assert_eq!(stored(data.path())["documents-dir"].path, "/new");
    }

    #[test]
    fn an_unresolvable_bookmark_is_kept_and_asks_for_the_folder_again() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake {
            gone: vec!["/Volumes/USB".into()],
            ..Fake::default()
        };
        remember(
            data.path(),
            &fake,
            RememberedFolder::SecondaryContent,
            Path::new("/Volumes/USB"),
        )
        .unwrap();
        let before = stored(data.path());
        let outcomes = restore(data.path(), &fake);
        assert_eq!(
            outcomes,
            vec![(
                RememberedFolder::SecondaryContent,
                Restored::NeedsChoosing {
                    path: "/Volumes/USB".into(),
                    reason: "the volume is not mounted".into()
                }
            )]
        );
        assert_eq!(stored(data.path()), before, "nothing is deleted");
        assert!(fake.started.borrow().is_empty());
    }

    #[test]
    fn a_damaged_file_or_entry_never_fails_the_launch() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(BOOKMARKS_FILE), "not json").unwrap();
        assert!(restore(data.path(), &Fake::default()).is_empty());
        std::fs::write(
            data.path().join(BOOKMARKS_FILE),
            r#"{"documents-dir":{"path":"/a","bookmark":"%%%"},"future-key":{"path":"/f","bookmark":""}}"#,
        )
        .unwrap();
        let outcomes = restore(data.path(), &Fake::default());
        assert!(matches!(
            outcomes.as_slice(),
            [(RememberedFolder::Documents, Restored::NeedsChoosing { .. })]
        ));
        assert!(stored(data.path()).contains_key("future-key"));
    }

    #[test]
    fn a_folder_without_a_bookmark_is_an_error_and_stores_nothing() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake {
            refuse_create: true,
            ..Fake::default()
        };
        assert!(remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/a")
        )
        .is_err());
        assert!(!data.path().join(BOOKMARKS_FILE).exists());
    }

    #[test]
    fn only_the_mac_app_store_build_has_a_security_scope() {
        assert_eq!(
            platform_scope().is_some(),
            cfg!(all(target_os = "macos", feature = "mas"))
        );
        // Without a scope the hooks touch nothing.
        if platform_scope().is_none() {
            let data = tempfile::tempdir().unwrap();
            after_pick(
                data.path(),
                RememberedFolder::Documents,
                Some(Path::new("/a")),
            );
            restore_at_launch(data.path());
            assert!(!data.path().join(BOOKMARKS_FILE).exists());
        }
    }

    #[test]
    fn the_portrait_library_is_remembered_and_reopened_with_the_other_folders() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        remember(
            data.path(),
            &fake,
            RememberedFolder::Documents,
            Path::new("/d"),
        )
        .unwrap();
        remember(
            data.path(),
            &fake,
            RememberedFolder::PortraitLibrary,
            Path::new("/Volumes/Art/portraits"),
        )
        .unwrap();
        assert_eq!(
            stored(data.path())["portrait-library-dir"].path,
            "/Volumes/Art/portraits"
        );
        let outcomes = restore(data.path(), &fake);
        assert_eq!(
            outcomes,
            vec![
                (RememberedFolder::Documents, Restored::Ready("/d".into())),
                (
                    RememberedFolder::PortraitLibrary,
                    Restored::Ready("/Volumes/Art/portraits".into())
                ),
            ]
        );
        forget(data.path(), RememberedFolder::PortraitLibrary).unwrap();
        assert_eq!(
            stored(data.path()).keys().collect::<Vec<_>>(),
            vec!["documents-dir"]
        );
    }

    #[test]
    fn the_vault_is_remembered_and_reopened_and_a_lost_one_is_kept() {
        let data = tempfile::tempdir().unwrap();
        let fake = Fake {
            gone: vec![PathBuf::from("/Volumes/Notes/Gone vault")],
            ..Fake::default()
        };
        remember(
            data.path(),
            &fake,
            RememberedFolder::Vault,
            Path::new("/Users/me/Campaign vault"),
        )
        .unwrap();
        assert_eq!(
            stored(data.path())["vault-dir"].path,
            "/Users/me/Campaign vault"
        );
        assert_eq!(
            restore(data.path(), &fake),
            vec![(
                RememberedFolder::Vault,
                Restored::Ready("/Users/me/Campaign vault".into())
            )]
        );
        remember(
            data.path(),
            &fake,
            RememberedFolder::Vault,
            Path::new("/Volumes/Notes/Gone vault"),
        )
        .unwrap();
        let outcomes = restore(data.path(), &fake);
        assert!(matches!(
            outcomes.as_slice(),
            [(RememberedFolder::Vault, Restored::NeedsChoosing { .. })]
        ));
        assert!(
            stored(data.path()).contains_key("vault-dir"),
            "a vault that does not resolve is kept"
        );
    }
}
