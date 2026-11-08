//! Where the vault's files live. The notes logic ([`super`]) names files only by vault-relative paths and reaches them
//! through a [`VaultStore`]:
//! - [`LocalStore`]: a folder on a file system. Desktop, the Mac App Store build, and iOS / iPadOS, where the folder
//!   comes from the document picker and is reached through a security-scoped bookmark ([`crate::folder_access`]).
//! - [`super::tree_store::TreeStore`]: an Android Storage Access Framework document tree (a `content://` URI), which
//!   has no file paths at all.
//!
//! Every store checks each path the webview sends with [`paths::parts`], never lists or opens hidden names
//! (`.obsidian`, `.trash`, dot files), never writes under `.obsidian/`, and writes a note only while it still hashes to
//! what the editor loaded ([`write::Outcome`]).

use super::dto::DeleteStatus;
use super::paths::{self, Vault};
use super::tree::{self, Listing};
use super::write::{self, Outcome};
use crate::error::{ApiError, ApiResult, ErrorCode};
use std::io::Read as _;
use std::path::Path;

/// A file read from the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileBytes {
    pub bytes: Vec<u8>,
    /// Last change, milliseconds since 1970 (0 when unknown).
    pub mtime_ms: u64,
    /// The name of the real file: a symbolic link's target may have another name, and so another kind.
    pub real_name: String,
}

/// The OS trash: moves a file there, or says why it cannot.
pub type Trash<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

/// The file operations the notes view needs, on vault-relative `/` paths.
pub trait VaultStore {
    /// What identifies this vault (the link and tag cache starts over when it changes).
    fn identity(&self) -> String;
    /// The vault folder's name (the vault's name in Obsidian).
    fn name(&self) -> Option<String>;
    /// The vault has an `.obsidian/` settings folder.
    fn has_obsidian(&self) -> bool;
    /// The text of `.obsidian/app.json` when it is there and small enough; never written.
    fn app_json(&self) -> Option<String>;
    /// Every visible folder and file, depth-first.
    fn list(&self) -> Listing;
    /// Something is at `rel` (a path that is not allowed counts as nothing there).
    fn exists(&self, rel: &str) -> bool;
    /// The real name of the existing file `rel`.
    ///
    /// # Errors
    /// `invalid_argument` for a bad path or a folder; `not_found` when nothing is there.
    fn file(&self, rel: &str) -> ApiResult<String>;
    /// Read the existing file `rel`; `None` when it holds more than `max` bytes.
    ///
    /// # Errors
    /// As [`VaultStore::file`], and `io_error` when it cannot be read.
    fn read(&self, rel: &str, max: u64) -> ApiResult<Option<FileBytes>>;
    /// Last change of `rel`, milliseconds since 1970 (0 when unknown).
    fn mtime(&self, rel: &str) -> u64;
    /// Write `bytes` over the existing file `rel`, only while it still hashes to `base_hash`.
    ///
    /// # Errors
    /// `invalid_argument` for a bad path; `io_error` when it cannot be read or written.
    fn guarded_write(&self, rel: &str, base_hash: &str, bytes: &[u8]) -> ApiResult<Outcome>;
    /// Create the new file `rel` (and any missing folder on the way); never replaces anything.
    ///
    /// # Errors
    /// `invalid_argument` for a bad path or a taken name; `io_error` when it cannot be written.
    fn create_new(&self, rel: &str, bytes: &[u8]) -> ApiResult<()>;
    /// Copy the local file `src` to the new file `rel`, as [`VaultStore::create_new`].
    ///
    /// # Errors
    /// As [`VaultStore::create_new`], and `io_error` when `src` cannot be read.
    fn copy_new(&self, src: &Path, rel: &str) -> ApiResult<()>;
    /// `name` is taken in the folder of `rel`. With `case_only` (a new case of the file's own name), only a file
    /// with exactly that name counts.
    ///
    /// # Errors
    /// `io_error` when the folder cannot be read.
    fn rename_taken(&self, rel: &str, name: &str, case_only: bool) -> ApiResult<bool>;
    /// Rename the file `rel` to `name` in the same folder, never replacing a file.
    ///
    /// # Errors
    /// `invalid_argument` when the name is taken; `io_error` when the rename fails.
    fn rename(&self, rel: &str, name: &str, case_only: bool) -> ApiResult<()>;
    /// Delete the file `rel`: to the trash, or for good when `permanent`.
    ///
    /// # Errors
    /// As [`VaultStore::file`], and `io_error` when it cannot be deleted.
    fn delete(&self, rel: &str, permanent: bool, trash: Trash<'_>) -> ApiResult<DeleteStatus>;
    /// The folder, when the vault is one on this file system (the desktop watcher needs it).
    fn local(&self) -> Option<&Vault> {
        None
    }
}

/// The error for a name that is taken.
pub(crate) fn taken() -> ApiError {
    ApiError::invalid("A file with that name is already there.")
}

/// A vault folder on a file system.
pub struct LocalStore {
    vault: Vault,
    /// iOS / iPadOS: the security scope, started for this store's life and stopped when it is dropped.
    _access: Option<crate::folder_access::Access>,
    /// iCloud Drive placeholders are listed, and opening one starts its download (iOS / iPadOS).
    placeholders: bool,
}

impl std::fmt::Debug for LocalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalStore")
            .field("root", &self.vault.root())
            .finish_non_exhaustive()
    }
}

impl LocalStore {
    /// The folder `vault`, reachable while `access` lives (`None`: no scope needed).
    #[must_use]
    pub fn new(vault: Vault, access: Option<crate::folder_access::Access>) -> Self {
        Self {
            vault,
            _access: access,
            placeholders: crate::folder_access::CLOUD_PLACEHOLDERS,
        }
    }

    /// The same store, with iCloud Drive placeholders handled (`true`) or ignored (tests set this).
    #[must_use]
    pub fn with_placeholders(self, placeholders: bool) -> Self {
        Self {
            placeholders,
            ..self
        }
    }

    /// The existing file `rel`. A file that iCloud Drive has not downloaded yet is not there: its download starts and
    /// the error says it is on its way.
    fn existing_file(&self, rel: &str) -> ApiResult<paths::Located> {
        self.vault.existing_file(rel).map_err(|e| {
            if self.placeholders && e.code == ErrorCode::NotFound && self.placeholder(rel) {
                crate::folder_access::start_download(&self.vault.root().join(rel));
                downloading()
            } else {
                e
            }
        })
    }

    /// An iCloud Drive placeholder stands for `rel` (inside the vault).
    fn placeholder(&self, rel: &str) -> bool {
        let Ok(parts) = paths::parts(rel) else {
            return false;
        };
        let Some((name, folders)) = parts.split_last() else {
            return false;
        };
        let at = folders
            .iter()
            .fold(self.vault.root().to_path_buf(), |p, f| p.join(f))
            .join(tree::icloud_placeholder_name(name));
        std::fs::canonicalize(&at).is_ok_and(|real| self.vault.contains(&real) && real.is_file())
    }
}

/// The error for a file iCloud Drive is still downloading. `details.downloading` tells the notes view to try again.
#[must_use]
pub fn downloading() -> ApiError {
    let mut e = ApiError::new(
        ErrorCode::IoError,
        "This file is still downloading from iCloud. It opens when it is ready.",
    );
    e.details = Some(serde_json::json!({ "downloading": true }));
    e
}

/// Read at most `max` bytes of `path`; `None` when it is larger.
fn read_capped(path: &Path, max: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    crate::folder_access::coordinated_read(path, |p| {
        std::fs::File::open(p).and_then(|f| f.take(max + 1).read_to_end(&mut bytes))
    })?;
    Ok((bytes.len() as u64 <= max).then_some(bytes))
}

fn mtime_of(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| tree::mtime_ms(&m))
}

impl VaultStore for LocalStore {
    fn identity(&self) -> String {
        self.vault.root().to_string_lossy().into_owned()
    }

    fn name(&self) -> Option<String> {
        self.vault
            .root()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }

    fn has_obsidian(&self) -> bool {
        self.vault.root().join(".obsidian").is_dir()
    }

    fn app_json(&self) -> Option<String> {
        let path = self.vault.root().join(super::config::APP_JSON);
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let bytes = read_capped(&path, super::config::MAX_APP_JSON).ok()??;
        String::from_utf8(bytes).ok()
    }

    fn list(&self) -> Listing {
        tree::list_with(&self.vault, self.placeholders)
    }

    fn exists(&self, rel: &str) -> bool {
        self.vault.existing(rel).is_ok()
    }

    fn file(&self, rel: &str) -> ApiResult<String> {
        let found = self.existing_file(rel)?;
        Ok(Vault::real_name(&found).to_string())
    }

    fn read(&self, rel: &str, max: u64) -> ApiResult<Option<FileBytes>> {
        let found = self.existing_file(rel)?;
        let bytes = read_capped(&found.real, max).map_err(|e| ApiError::io("read the file", e))?;
        Ok(bytes.map(|bytes| FileBytes {
            bytes,
            mtime_ms: mtime_of(&found.real),
            real_name: Vault::real_name(&found).to_string(),
        }))
    }

    fn mtime(&self, rel: &str) -> u64 {
        self.vault
            .existing(rel)
            .map_or(0, |found| mtime_of(&found.real))
    }

    fn guarded_write(&self, rel: &str, base_hash: &str, bytes: &[u8]) -> ApiResult<Outcome> {
        let found = match self.vault.existing_file(rel) {
            Ok(f) => f,
            Err(e) if e.code == ErrorCode::NotFound => return Ok(Outcome::Missing),
            Err(e) => return Err(e),
        };
        crate::folder_access::coordinated_write(&found.real, |p| {
            write::guarded_write(p, base_hash, bytes)
        })
        .map_err(|e| ApiError::io("save the note", e))
    }

    fn create_new(&self, rel: &str, bytes: &[u8]) -> ApiResult<()> {
        let target = self.vault.new_file(rel)?;
        crate::folder_access::coordinated_write(&target, |p| write::create_new(p, bytes)).map_err(
            |e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    taken()
                } else {
                    ApiError::io("create the note", e)
                }
            },
        )
    }

    fn copy_new(&self, src: &Path, rel: &str) -> ApiResult<()> {
        let target = self.vault.new_file(rel)?;
        write::copy_new(src, &target).map_err(|e| ApiError::io("copy the file", e))
    }

    fn rename_taken(&self, rel: &str, name: &str, case_only: bool) -> ApiResult<bool> {
        let found = self.vault.existing_file(rel)?;
        let dir = found.path.parent().ok_or_else(outside)?;
        let exact_taken = std::fs::read_dir(dir)
            .map_err(|e| ApiError::io("read the folder", e))?
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_str() == Some(name));
        Ok(exact_taken || (!case_only && std::fs::symlink_metadata(dir.join(name)).is_ok()))
    }

    fn rename(&self, rel: &str, name: &str, case_only: bool) -> ApiResult<()> {
        let found = self.vault.existing_file(rel)?;
        let to = found.path.with_file_name(name);
        write::rename_no_clobber(&found.path, &to, case_only).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                taken()
            } else {
                ApiError::io("rename the file", e)
            }
        })
    }

    fn delete(&self, rel: &str, permanent: bool, trash: Trash<'_>) -> ApiResult<DeleteStatus> {
        let found = self.vault.existing_file(rel)?;
        if permanent {
            std::fs::remove_file(&found.path).map_err(|e| ApiError::io("delete the file", e))?;
            return Ok(DeleteStatus::Deleted);
        }
        match trash(&found.path) {
            Ok(()) => Ok(DeleteStatus::Trashed),
            Err(e) => {
                tracing::warn!("vault: the trash refused a file: {e}");
                Ok(DeleteStatus::TrashUnavailable)
            }
        }
    }

    fn local(&self) -> Option<&Vault> {
        Some(&self.vault)
    }
}

fn outside() -> ApiError {
    ApiError::invalid("That file is not in your vault.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholder_vault() -> (tempfile::TempDir, LocalStore) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("Places")).unwrap();
        std::fs::write(root.join("Home.md"), "home").unwrap();
        // Not downloaded yet: only iCloud Drive's placeholders are there.
        std::fs::write(root.join(".Session 1.md.icloud"), "plist").unwrap();
        std::fs::write(root.join("Places/.Harbor.md.icloud"), "plist").unwrap();
        // Downloaded while the placeholder is still there: listed once, as the file.
        std::fs::write(root.join("Both.md"), "both").unwrap();
        std::fs::write(root.join(".Both.md.icloud"), "plist").unwrap();
        // A hidden file's placeholder stays hidden.
        std::fs::write(root.join("..hidden.md.icloud"), "plist").unwrap();
        let store = LocalStore::new(Vault::open(root).unwrap(), None);
        (dir, store)
    }

    #[test]
    fn icloud_placeholders_name_the_file_they_stand_for() {
        assert_eq!(
            tree::icloud_placeholder(".Session 1.md.icloud"),
            Some("Session 1.md")
        );
        assert_eq!(tree::icloud_placeholder(".map.png.icloud"), Some("map.png"));
        for not_one in [
            ".icloud",
            "..icloud",
            "Session.md.icloud",
            ".x.md",
            "..hidden.md.icloud",
        ] {
            assert_eq!(tree::icloud_placeholder(not_one), None, "{not_one}");
        }
        assert_eq!(tree::icloud_placeholder_name("a.md"), ".a.md.icloud");
    }

    #[test]
    fn placeholders_are_listed_as_their_files_only_where_icloud_makes_them() {
        let (_d, store) = placeholder_vault();
        let hidden: Vec<String> = store.list().entries.into_iter().map(|e| e.path).collect();
        assert_eq!(
            hidden,
            ["Places", "Both.md", "Home.md"],
            "desktop builds list no placeholders"
        );
        let store = store.with_placeholders(true);
        let listed: Vec<(String, bool, u64)> = store
            .list()
            .entries
            .into_iter()
            .map(|e| (e.path, e.cloud, e.size))
            .collect();
        assert_eq!(
            listed,
            [
                ("Places".to_string(), false, 0),
                ("Places/Harbor.md".to_string(), true, 0),
                ("Both.md".to_string(), false, 4),
                ("Home.md".to_string(), false, 4),
                ("Session 1.md".to_string(), true, 0),
            ]
        );
    }

    #[test]
    fn opening_a_placeholder_says_it_is_downloading_and_never_writes() {
        let (dir, store) = placeholder_vault();
        let store = store.with_placeholders(true);
        for rel in ["Session 1.md", "Places/Harbor.md"] {
            let e = store.read(rel, 1024).unwrap_err();
            assert_eq!(e.code, ErrorCode::IoError, "{rel}");
            assert_eq!(e.details, Some(serde_json::json!({ "downloading": true })));
            assert!(store
                .guarded_write(rel, "x", b"y")
                .is_ok_and(|o| o == Outcome::Missing));
        }
        assert!(!dir.path().join("Session 1.md").exists());
        // A file that is not there at all is just missing.
        assert_eq!(
            store.read("Nope.md", 1024).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(store.read("Home.md", 1024).unwrap().unwrap().bytes, b"home");
    }
}
