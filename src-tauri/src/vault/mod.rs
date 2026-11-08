//! Obsidian vault support (GitHub #15, B-102; decision U17a). Borealis opens one vault folder in place and works on
//! its Markdown files where they are: nothing is copied or imported, and Obsidian sees every change at once.
//!
//! - The folder is a backend-picked token (`vault-dir`), remembered in `vaultDir` (Settings > Content), with a
//!   security-scoped bookmark in the Mac App Store build and on iOS / iPadOS ([`crate::folder_access`]). A folder that
//!   cannot be read is reported as unavailable and kept (as B-188).
//! - All file access goes through a [`store::VaultStore`].
//! - The webview names files only by vault-relative paths, and every one is checked here ([`paths`]): nothing outside
//!   the vault, nothing hidden (so nothing under `.obsidian/` is ever written), no links out.
//! - Notes are read and written as raw text ([`text`]); a write happens only on a user change and only while the file
//!   is still what the editor loaded ([`write`]).
//! - Links, backlinks, tags and search follow Obsidian's rules ([`syntax`], [`resolve`], [`index`]); renaming can
//!   update the links to a file ([`rename`]); `.obsidian/app.json` is honoured read-only ([`config`]).

pub mod character;
pub mod config;
pub mod dto;
pub mod index;
pub mod paths;
pub mod rename;
mod renaming;
pub mod resolve;
pub mod store;
pub mod syntax;
pub mod text;
pub mod tree;
#[cfg(desktop)]
pub mod watch;
pub mod write;

use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::picked_paths::PickPurpose;
use crate::settings_dto::DataDirs;
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use config::VaultConfig;
use dto::{DeleteStatus, SaveStatus, VaultAttachment, VaultNote, VaultSave, VaultTree};
use index::{Backlink, SearchHit, TagCount};
use paths::Vault;
use resolve::FileSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use store::{FileBytes, LocalStore, VaultStore};
use tree::{EntryKind, Listing};

/// Largest image sent to the notes view (as a `data:` URL).
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Largest file copied into the vault as an attachment.
pub const MAX_ATTACHMENT_BYTES: u64 = 512 * 1024 * 1024;
/// The name of a new note when none is given (Obsidian's).
pub const UNTITLED: &str = "Untitled";
/// Most numbered names tried for a new file (`Untitled 1`, `Untitled 2`, ...).
const MAX_NAME_TRIES: usize = 10_000;

/// The notes view's backend state: the link and tag cache and, on desktop, the folder watcher.
#[derive(Debug, Default)]
pub struct VaultService {
    index: Mutex<index::Index>,
    #[cfg(desktop)]
    watcher: Mutex<Option<watch::VaultWatcher>>,
}

fn lock<T>(m: &Mutex<T>) -> ApiResult<MutexGuard<'_, T>> {
    m.lock()
        .map_err(|_| ApiError::internal("vault state lock poisoned"))
}

fn no_vault() -> ApiError {
    ApiError::invalid("No vault is open. Choose your vault folder in Settings > Content.")
}

fn unavailable() -> ApiError {
    ApiError::invalid(
        "Your vault folder is unavailable. Connect it, or choose it again in Settings > Content.",
    )
}

fn not_a_note() -> ApiError {
    ApiError::invalid("Only Markdown notes can be edited.")
}

/// The files (not folders) of a listing.
fn file_set(listing: &Listing) -> FileSet {
    FileSet::new(
        listing
            .entries
            .iter()
            .filter(|e| e.kind != EntryKind::Folder)
            .map(|e| e.path.clone()),
    )
}

/// The note `rel` as read from `file`.
fn note_of(rel: &str, file: &FileBytes) -> VaultNote {
    let decoded = text::decode(&file.bytes);
    let read_only = if !decoded.utf8 {
        Some("This file is not UTF-8 text, so Borealis shows it but cannot save it.".to_string())
    } else if tree::kind_of(if file.real_name.is_empty() {
        rel
    } else {
        &file.real_name
    }) != EntryKind::Note
    {
        Some("Borealis shows this file as plain text and does not change it.".to_string())
    } else {
        None
    };
    VaultNote {
        path: rel.to_string(),
        hash: text::hash(&file.bytes),
        mtime_ms: file.mtime_ms,
        line_ending: decoded.line_ending,
        bom: decoded.bom,
        text: decoded.text,
        read_only,
    }
}

fn image_mime(name: &str) -> &'static str {
    match paths::extension(name).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("avif") => "image/avif",
        _ => "image/png",
    }
}

/// A free path `folder/stem[ n].ext` (Obsidian numbers names the same way).
fn free_path(
    files: &FileSet,
    store: &dyn VaultStore,
    folder: &str,
    stem: &str,
    ext: &str,
) -> ApiResult<String> {
    let dot_ext = if ext.is_empty() {
        String::new()
    } else {
        format!(".{ext}")
    };
    (0..MAX_NAME_TRIES)
        .map(|n| {
            let name = if n == 0 {
                format!("{stem}{dot_ext}")
            } else {
                format!("{stem} {n}{dot_ext}")
            };
            paths::join(folder, &name)
        })
        .find(|p| files.get(p).is_none() && !store.exists(p))
        .ok_or_else(|| ApiError::invalid("Too many files have that name."))
}

/// A file name from outside the vault made safe for it: forbidden characters become `-`.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|#^[]".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "Attachment".to_string()
    } else {
        cleaned
    }
}

/// Delete `path` to the OS trash (desktop). Phones have no trash for app files.
#[cfg(desktop)]
pub fn os_trash(path: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut ctx = trash::TrashContext::default();
    // NSFileManager needs no Finder automation permission and works in the App Sandbox.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos as _};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete(path).map_err(|e| e.to_string())
}

/// iOS / iPadOS: the file provider's Recently Deleted, where it has one.
///
/// # Errors
/// When the provider has no trash.
#[cfg(target_os = "ios")]
pub fn os_trash(path: &Path) -> Result<(), String> {
    crate::folder_access_ios::trash(path)
}

/// Android: no trash for app files.
///
/// # Errors
/// Always.
#[cfg(target_os = "android")]
pub fn os_trash(_: &Path) -> Result<(), String> {
    Err("this device has no trash".into())
}

/// A chosen vault: open, or the reason it cannot be opened right now (the setting is kept).
enum Chosen {
    Open(Box<dyn VaultStore>),
    Unavailable,
}

impl AppState {
    /// The vault in the folder `dir`, while it can be read.
    fn open_store(&self, dir: &str) -> Option<Box<dyn VaultStore>> {
        let scoped = crate::folder_access::access(
            &self.paths.data,
            crate::folder_access::RememberedFolder::Vault,
            Path::new(dir),
        );
        let (path, access) = match scoped {
            Some(s) => (s.path, Some(s.access)),
            None => (PathBuf::from(dir), None),
        };
        if path != Path::new(dir) {
            self.vault_moved(dir, &path);
        }
        let root = crate::data_dirs::validate_user_dir(&path, &self.secondary_forbidden(), false)
            .ok()
            .filter(|root| std::fs::read_dir(root).is_ok())?;
        let vault = Vault::open(&root).ok()?;
        Some(Box::new(LocalStore::new(vault, access)))
    }

    /// iOS: the bookmark found the vault folder somewhere else (the app that holds it was reinstalled or updated, so
    /// its folder has a new path): the setting follows it.
    fn vault_moved(&self, from: &str, to: &Path) {
        let Ok(mut next) = self.settings() else {
            return;
        };
        if next.vault_dir.as_deref() != Some(from) {
            return;
        }
        next.vault_dir = Some(to.to_string_lossy().into_owned());
        let saved = next
            .save(&self.paths.settings_file())
            .and_then(|()| self.set_settings(next));
        match saved {
            Ok(_) => tracing::info!("vault: the folder moved; the setting follows it"),
            Err(e) => tracing::warn!("vault: the moved folder could not be saved: {e:?}"),
        }
    }

    /// The chosen vault, `None` when none is chosen.
    fn chosen_vault(&self) -> ApiResult<Option<Chosen>> {
        let Some(dir) = self.settings()?.vault_dir else {
            return Ok(None);
        };
        Ok(Some(
            self.open_store(&dir)
                .map_or(Chosen::Unavailable, Chosen::Open),
        ))
    }

    /// The open vault, or why there is none.
    fn vault(&self) -> ApiResult<Box<dyn VaultStore>> {
        match self.chosen_vault()? {
            None => Err(no_vault()),
            Some(Chosen::Unavailable) => Err(unavailable()),
            Some(Chosen::Open(v)) => Ok(v),
        }
    }

    /// The chosen vault folder cannot be read right now (unplugged drive, offline share). The setting is kept.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn vault_unavailable(&self) -> ApiResult<bool> {
        Ok(matches!(self.chosen_vault()?, Some(Chosen::Unavailable)))
    }

    /// The folder behind a `vault-dir` pick, checked.
    fn picked_vault(&self, token: &str) -> ApiResult<String> {
        let picked = self.picked.take(token, PickPurpose::VaultDir)?;
        // iOS: the picker's security scope is held while the folder is checked.
        let _access = crate::folder_access::access(
            &self.paths.data,
            crate::folder_access::RememberedFolder::Vault,
            &picked,
        );
        let dir = crate::data_dirs::validate_user_dir(&picked, &self.secondary_forbidden(), false)?;
        Ok(dir.to_string_lossy().into_owned())
    }

    /// Use the folder behind a `vault-dir` pick as the vault (`None` = no vault). Any folder works; a `.obsidian`
    /// folder is optional.
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an unusable folder; `io_error` if the settings cannot be written.
    pub fn set_vault_dir(&self, token: Option<&str>) -> ApiResult<DataDirs> {
        let chosen = token.map(|t| self.picked_vault(t)).transpose()?;
        let mut next = self.settings()?;
        next.vault_dir = chosen;
        next.save(&self.paths.settings_file())?;
        self.set_settings(next)?;
        #[cfg(desktop)]
        self.vault_watch_stop()?;
        self.data_dirs()
    }

    /// The vault and its tree (an empty tree without a vault or while it is unavailable).
    ///
    /// # Errors
    /// `internal` if a lock is poisoned.
    pub fn vault_tree(&self) -> ApiResult<VaultTree> {
        let dir = self.settings()?.vault_dir;
        let store = match self.chosen_vault()? {
            Some(Chosen::Open(v)) => Some(v),
            Some(Chosen::Unavailable) => {
                tracing::warn!("vault: the chosen folder is unavailable; the setting is kept");
                None
            }
            None => None,
        };
        let Some(store) = store else {
            return Ok(VaultTree {
                unavailable: dir.is_some(),
                name: dir.as_deref().and_then(display_name),
                dir,
                obsidian: false,
                config: VaultConfig::default(),
                entries: Vec::new(),
                truncated: false,
            });
        };
        let listing = store.list();
        lock(&self.vault.index)?.refresh(store.as_ref(), &listing.entries);
        Ok(VaultTree {
            // The setting may have followed a moved folder (iOS) while the store opened.
            dir: self.settings()?.vault_dir,
            name: store.name(),
            unavailable: false,
            obsidian: store.has_obsidian(),
            config: VaultConfig::of(store.as_ref()),
            entries: listing.entries,
            truncated: listing.truncated,
        })
    }

    /// Read a note or another text file of the vault.
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault, a folder or an oversized file; `not_found` when it is gone.
    pub fn vault_read(&self, path: &str) -> ApiResult<VaultNote> {
        let store = self.vault()?;
        let real_name = store.file(path)?;
        // Notes and canvases only (judged by the name asked for and by the real file behind a link).
        let text_kinds = [EntryKind::Note, EntryKind::Canvas];
        let kinds = [paths::file_name(path), real_name.as_str()].map(tree::kind_of);
        if !kinds.iter().all(|k| text_kinds.contains(k)) {
            return Err(ApiError::invalid(
                "Borealis opens only notes and canvases as text.",
            ));
        }
        let file = store
            .read(path, index::MAX_NOTE_BYTES)?
            .ok_or_else(|| ApiError::invalid("That file is too large to open."))?;
        Ok(note_of(path, &file))
    }

    /// Save the user's edit of the note `path`. `base_hash` is the hash it was loaded with; `None` creates the note
    /// (refused when one is there). The text may use LF; the note keeps its own line ending and byte order mark.
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault or a file that is not a note; `io_error` when writing fails.
    pub fn vault_save(
        &self,
        path: &str,
        text_in: &str,
        base_hash: Option<&str>,
    ) -> ApiResult<VaultSave> {
        let store = self.vault()?;
        if tree::kind_of(paths::file_name(path)) != EntryKind::Note {
            return Err(not_a_note());
        }
        let Some(base) = base_hash else {
            let bytes = text::encode(text_in, false, text::LineEnding::Lf);
            store.create_new(path, &bytes)?;
            return Ok(saved(store.mtime(path), &bytes));
        };
        let current = match store.read(path, index::MAX_NOTE_BYTES) {
            Ok(Some(f)) => f,
            Ok(None) => return Err(ApiError::invalid("That file is too large to open.")),
            Err(e) if e.code == ErrorCode::NotFound => return Ok(missing()),
            Err(e) => return Err(e),
        };
        if tree::kind_of(&current.real_name) != EntryKind::Note {
            return Err(not_a_note());
        }
        let decoded = text::decode(&current.bytes);
        if !decoded.utf8 {
            return Err(ApiError::invalid(
                "This file is not UTF-8 text, so Borealis cannot save it.",
            ));
        }
        // Unchanged lines keep their own line ending (files with mixed endings stay as they are around an edit).
        let bytes = text::encode_keeping(text_in, &decoded);
        let outcome = store.guarded_write(path, base, &bytes)?;
        lock(&self.vault.index)?.invalidate(path);
        Ok(match outcome {
            write::Outcome::Saved { hash } => VaultSave {
                status: SaveStatus::Saved,
                hash: Some(hash),
                mtime_ms: Some(store.mtime(path)),
                disk: None,
            },
            write::Outcome::Changed { disk } => VaultSave {
                status: SaveStatus::Conflict,
                hash: None,
                mtime_ms: None,
                disk: Some(note_of(
                    path,
                    &FileBytes {
                        bytes: disk,
                        mtime_ms: store.mtime(path),
                        real_name: current.real_name,
                    },
                )),
            },
            write::Outcome::Missing => missing(),
        })
    }

    /// Create a note. `name` may be a path (`Places/Harbor`, from a link); without one the note is `Untitled`
    /// (numbered when taken). It goes in `folder` when given, else where `.obsidian/app.json` says new notes go for a
    /// note created while `from` is open.
    ///
    /// # Errors
    /// `invalid_argument` for a bad name or a taken one; `io_error` when it cannot be written.
    pub fn vault_create_note(
        &self,
        name: Option<&str>,
        from: Option<&str>,
        folder: Option<&str>,
    ) -> ApiResult<VaultNote> {
        let store = self.vault()?;
        let listing = store.list();
        let files = file_set(&listing);
        let cfg = VaultConfig::of(store.as_ref());
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        let rel = match name {
            Some(n) => {
                let n = resolve::strip_md(n.trim_start_matches('/'));
                let parts: Vec<&str> = n.split('/').collect();
                parts.iter().try_for_each(|p| paths::check_new_name(p))?;
                let base = if parts.len() > 1 {
                    String::new()
                } else {
                    folder.map_or_else(|| cfg.new_note_folder(from), str::to_string)
                };
                let rel = paths::join(&base, &format!("{n}.md"));
                if files.get(&rel).is_some() {
                    return Err(ApiError::invalid("A note with that name is already there."));
                }
                rel
            }
            None => {
                let base = folder.map_or_else(|| cfg.new_note_folder(from), str::to_string);
                free_path(&files, store.as_ref(), &base, UNTITLED, "md")?
            }
        };
        store.create_new(&rel, b"")?;
        Ok(note_of(
            &rel,
            &FileBytes {
                bytes: Vec::new(),
                mtime_ms: store.mtime(&rel),
                real_name: paths::file_name(&rel).to_string(),
            },
        ))
    }

    /// Delete a file: to the OS trash, or for good when `permanent` (after the user agreed because there is no trash).
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault or a folder; `io_error` when it cannot be deleted.
    pub fn vault_delete(&self, path: &str, permanent: bool) -> ApiResult<DeleteStatus> {
        self.vault_delete_with(path, permanent, os_trash)
    }

    /// [`AppState::vault_delete`] with the trash given (tests use a fake one).
    ///
    /// # Errors
    /// As [`AppState::vault_delete`].
    pub fn vault_delete_with(
        &self,
        path: &str,
        permanent: bool,
        trash: impl Fn(&Path) -> Result<(), String>,
    ) -> ApiResult<DeleteStatus> {
        let store = self.vault()?;
        store.file(path)?;
        lock(&self.vault.index)?.invalidate(path);
        store.delete(path, permanent, &trash)
    }

    /// Resolve link targets written in the note `from`: the file each one opens, or `None` (unresolved).
    ///
    /// # Errors
    /// When no vault is open.
    pub fn vault_resolve(&self, from: &str, targets: &[String]) -> ApiResult<Vec<Option<String>>> {
        let store = self.vault()?;
        let files = file_set(&store.list());
        Ok(targets
            .iter()
            .map(|t| files.resolve(from, t).map(str::to_string))
            .collect())
    }

    /// Fresh tree and index (for the queries below).
    fn indexed(&self) -> ApiResult<(Box<dyn VaultStore>, Listing)> {
        let store = self.vault()?;
        let listing = store.list();
        lock(&self.vault.index)?.refresh(store.as_ref(), &listing.entries);
        Ok((store, listing))
    }

    /// Notes that link to `path`.
    ///
    /// # Errors
    /// When no vault is open.
    pub fn vault_backlinks(&self, path: &str) -> ApiResult<Vec<Backlink>> {
        let (_, listing) = self.indexed()?;
        let files = file_set(&listing);
        Ok(lock(&self.vault.index)?.backlinks(&files, path))
    }

    /// Every tag in the vault with its note count.
    ///
    /// # Errors
    /// When no vault is open.
    pub fn vault_tags(&self) -> ApiResult<Vec<TagCount>> {
        self.indexed()?;
        Ok(lock(&self.vault.index)?.tags())
    }

    /// Search note names and text for `query`, or list the notes with `tag`.
    ///
    /// # Errors
    /// When no vault is open.
    pub fn vault_search(&self, query: &str, tag: Option<&str>) -> ApiResult<Vec<SearchHit>> {
        let (store, listing) = self.indexed()?;
        if let Some(tag) = tag.filter(|t| !t.trim().is_empty()) {
            let notes = lock(&self.vault.index)?.notes_with_tag(tag);
            return Ok(notes
                .into_iter()
                .take(index::MAX_SEARCH_RESULTS)
                .map(|path| SearchHit {
                    path,
                    name_match: false,
                    lines: Vec::new(),
                })
                .collect());
        }
        Ok(index::search(store.as_ref(), &listing.entries, query))
    }

    /// An image of the vault as a `data:` URL (for the image view and `![[embeds]]`).
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault, a file that is not an image, or one that is too large.
    pub fn vault_image(&self, path: &str) -> ApiResult<String> {
        let store = self.vault()?;
        if tree::kind_of(paths::file_name(path)) != EntryKind::Image {
            return Err(ApiError::invalid("That file is not an image."));
        }
        let file = store
            .read(path, MAX_IMAGE_BYTES)?
            .ok_or_else(|| ApiError::invalid("That image is too large to show."))?;
        Ok(format!(
            "data:{};base64,{}",
            image_mime(path),
            STANDARD.encode(file.bytes)
        ))
    }

    /// The link to insert in the note `from` for the vault file `to`, written as `.obsidian/app.json` says.
    ///
    /// # Errors
    /// When no vault is open or `to` is not one of its files.
    pub fn vault_link_text(&self, from: &str, to: &str, embed: bool) -> ApiResult<String> {
        let store = self.vault()?;
        let files = file_set(&store.list());
        let to = files
            .get(to)
            .ok_or_else(|| ApiError::invalid("That file is not in your vault."))?;
        Ok(VaultConfig::of(store.as_ref()).link_text(&files, from, to, embed))
    }

    /// Copy the file behind a `vault-attachment` pick into the attachment folder `.obsidian/app.json` names for the
    /// note `from`, and return the link to insert (an embed for images).
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an oversized file; `io_error` when copying fails.
    pub fn vault_attach(&self, token: &str, from: &str) -> ApiResult<VaultAttachment> {
        let store = self.vault()?;
        let picked: PathBuf = self.picked.take(token, PickPurpose::VaultAttachment)?;
        let meta = std::fs::metadata(&picked).map_err(|e| ApiError::io("read the file", e))?;
        if !meta.is_file() || meta.len() > MAX_ATTACHMENT_BYTES {
            return Err(ApiError::invalid("That file cannot be attached."));
        }
        let original = safe_name(
            &picked
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let (stem, ext) = match paths::extension(&original) {
            Some(_) => original
                .rsplit_once('.')
                .map_or((original.as_str(), ""), |(s, e)| (s, e)),
            None => (original.as_str(), ""),
        };
        let cfg = VaultConfig::of(store.as_ref());
        let listing = store.list();
        let files = file_set(&listing);
        let rel = free_path(
            &files,
            store.as_ref(),
            &cfg.attachment_folder(from),
            stem,
            ext,
        )?;
        store.copy_new(&picked, &rel)?;
        let after = FileSet::new(files.paths().iter().cloned().chain([rel.clone()]));
        let embed = tree::kind_of(&rel) == EntryKind::Image;
        Ok(VaultAttachment {
            link: cfg.link_text(&after, from, &rel, embed),
            path: rel,
        })
    }

    /// Start watching the vault (replacing any earlier watcher); `on_change` gets each batch of changes. A vault that
    /// is not a local folder is not watched (the notes view re-reads it when it comes back to the foreground).
    ///
    /// # Errors
    /// When no vault is open or the OS refuses to watch it.
    #[cfg(desktop)]
    pub fn vault_watch_start(
        &self,
        on_change: impl Fn(watch::VaultChange) + Send + 'static,
    ) -> ApiResult<()> {
        let store = self.vault()?;
        let Some(vault) = store.local().cloned() else {
            return Ok(());
        };
        let watcher = watch::start(vault, on_change)
            .map_err(|e| ApiError::io("watch the vault", std::io::Error::other(e.to_string())))?;
        *lock(&self.vault.watcher)? = Some(watcher);
        Ok(())
    }

    /// Stop watching the vault.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    #[cfg(desktop)]
    pub fn vault_watch_stop(&self) -> ApiResult<()> {
        lock(&self.vault.watcher)?.take();
        Ok(())
    }
}

/// Where the vault setting `dir` is, for people (the folder path).
#[must_use]
pub fn label(dir: &str) -> String {
    dir.to_string()
}

/// The name to show for the vault setting `dir`: the folder's name.
fn display_name(dir: &str) -> Option<String> {
    Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

fn saved(mtime_ms: u64, bytes: &[u8]) -> VaultSave {
    VaultSave {
        status: SaveStatus::Saved,
        hash: Some(text::hash(bytes)),
        mtime_ms: Some(mtime_ms),
        disk: None,
    }
}

fn missing() -> VaultSave {
    VaultSave {
        status: SaveStatus::Missing,
        hash: None,
        mtime_ms: None,
        disk: None,
    }
}
