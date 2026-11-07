//! Obsidian vault support (GitHub #15, B-102; decision U17a). Borealis opens one vault folder in place and works on
//! its Markdown files where they are: nothing is copied or imported, and Obsidian sees every change at once.
//!
//! - The folder is a backend-picked token (`vault-dir`), remembered in `vaultDir` (Settings > Content), with a
//!   security-scoped bookmark in the Mac App Store build ([`crate::folder_access`]). A folder that cannot be read is
//!   reported as unavailable and kept (as B-188).
//! - The webview names files only by vault-relative paths, and every one is checked here ([`paths`]): nothing outside
//!   the vault, nothing hidden (so nothing under `.obsidian/` is ever written), no links out.
//! - Notes are read and written as raw text ([`text`]); a write happens only on a user change and only while the file
//!   is still what the editor loaded ([`write`]).
//! - Links, backlinks, tags and search follow Obsidian's rules ([`syntax`], [`resolve`], [`index`]); renaming can
//!   update the links to a file ([`rename`]); `.obsidian/app.json` is honoured read-only ([`config`]).

pub mod config;
pub mod dto;
pub mod index;
pub mod paths;
pub mod rename;
mod renaming;
pub mod resolve;
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
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
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

fn mtime_of(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| tree::mtime_ms(&m))
}

/// The note `rel` as read from `bytes`.
fn note_of(rel: &str, real: &Path, bytes: &[u8]) -> VaultNote {
    let decoded = text::decode(bytes);
    let read_only = if !decoded.utf8 {
        Some("This file is not UTF-8 text, so Borealis shows it but cannot save it.".to_string())
    } else if tree::kind_of(real.file_name().and_then(|n| n.to_str()).unwrap_or(rel))
        != EntryKind::Note
    {
        Some("Borealis shows this file as plain text and does not change it.".to_string())
    } else {
        None
    };
    VaultNote {
        path: rel.to_string(),
        hash: text::hash(bytes),
        mtime_ms: mtime_of(real),
        line_ending: decoded.line_ending,
        bom: decoded.bom,
        text: decoded.text,
        read_only,
    }
}

/// Read at most `max` bytes of `path`; `None` when it is larger.
fn read_capped(path: &Path, max: u64) -> ApiResult<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(max + 1).read_to_end(&mut bytes))
        .map_err(|e| ApiError::io("read the file", e))?;
    Ok((bytes.len() as u64 <= max).then_some(bytes))
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
    vault: &Vault,
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
        .find(|p| files.get(p).is_none() && vault.existing(p).is_err())
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

/// Phones: no trash.
///
/// # Errors
/// Always.
#[cfg(not(desktop))]
pub fn os_trash(_: &Path) -> Result<(), String> {
    Err("this device has no trash".into())
}

impl AppState {
    /// The chosen vault while it can be read, `Err(path)` while it cannot, `None` when none is chosen.
    fn vault_root(&self) -> ApiResult<Option<Result<Vault, String>>> {
        let Some(dir) = self.settings()?.vault_dir else {
            return Ok(None);
        };
        let ready = crate::data_dirs::validate_user_dir(
            Path::new(&dir),
            &self.secondary_forbidden(),
            false,
        )
        .ok()
        .filter(|root| std::fs::read_dir(root).is_ok())
        .and_then(|root| Vault::open(&root).ok());
        Ok(Some(ready.ok_or(dir)))
    }

    /// The open vault, or why there is none.
    fn vault(&self) -> ApiResult<Vault> {
        match self.vault_root()? {
            None => Err(no_vault()),
            Some(Err(_)) => Err(unavailable()),
            Some(Ok(v)) => Ok(v),
        }
    }

    /// The chosen vault folder cannot be read right now (unplugged drive, offline share). The setting is kept.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn vault_unavailable(&self) -> ApiResult<bool> {
        Ok(matches!(self.vault_root()?, Some(Err(_))))
    }

    /// Use the folder behind a `vault-dir` pick as the vault (`None` = no vault). Any folder works; a `.obsidian`
    /// folder is optional.
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an unusable folder; `io_error` if the settings cannot be written.
    pub fn set_vault_dir(&self, token: Option<&str>) -> ApiResult<DataDirs> {
        let chosen = match token {
            Some(t) => {
                let picked = self.picked.take(t, PickPurpose::VaultDir)?;
                let dir = crate::data_dirs::validate_user_dir(
                    &picked,
                    &self.secondary_forbidden(),
                    false,
                )?;
                Some(dir.to_string_lossy().into_owned())
            }
            None => None,
        };
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
        let vault = match self.vault_root()? {
            Some(Ok(v)) => Some(v),
            Some(Err(_)) => {
                tracing::warn!("vault: the chosen folder is unavailable; the setting is kept");
                None
            }
            None => None,
        };
        let Some(vault) = vault else {
            return Ok(VaultTree {
                unavailable: dir.is_some(),
                name: dir
                    .as_deref()
                    .and_then(|d| Path::new(d).file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
                dir,
                obsidian: false,
                config: VaultConfig::default(),
                entries: Vec::new(),
                truncated: false,
            });
        };
        let listing = tree::list(&vault);
        lock(&self.vault.index)?.refresh(&vault, &listing.entries);
        Ok(VaultTree {
            dir,
            name: vault
                .root()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            unavailable: false,
            obsidian: vault.root().join(".obsidian").is_dir(),
            config: VaultConfig::load(vault.root()),
            entries: listing.entries,
            truncated: listing.truncated,
        })
    }

    /// Read a note or another text file of the vault.
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault, a folder or an oversized file; `not_found` when it is gone.
    pub fn vault_read(&self, path: &str) -> ApiResult<VaultNote> {
        let vault = self.vault()?;
        let found = vault.existing_file(path)?;
        // Notes and canvases only (judged by the name asked for and by the real file behind a link).
        let text_kinds = [EntryKind::Note, EntryKind::Canvas];
        let kinds = [paths::file_name(path), Vault::real_name(&found)].map(tree::kind_of);
        if !kinds.iter().all(|k| text_kinds.contains(k)) {
            return Err(ApiError::invalid(
                "Borealis opens only notes and canvases as text.",
            ));
        }
        let bytes = read_capped(&found.real, index::MAX_NOTE_BYTES)?
            .ok_or_else(|| ApiError::invalid("That file is too large to open."))?;
        Ok(note_of(path, &found.real, &bytes))
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
        let vault = self.vault()?;
        if tree::kind_of(paths::file_name(path)) != EntryKind::Note {
            return Err(not_a_note());
        }
        let Some(base) = base_hash else {
            let target = vault.new_file(path)?;
            let bytes = text::encode(text_in, false, text::LineEnding::Lf);
            write::create_new(&target, &bytes).map_err(|e| ApiError::io("create the note", e))?;
            return Ok(saved(&target, &bytes));
        };
        let found = match vault.existing_file(path) {
            Ok(f) => f,
            Err(e) if e.code == ErrorCode::NotFound => return Ok(missing()),
            Err(e) => return Err(e),
        };
        if tree::kind_of(Vault::real_name(&found)) != EntryKind::Note {
            return Err(not_a_note());
        }
        let current = std::fs::read(&found.real).map_err(|e| ApiError::io("read the note", e))?;
        let decoded = text::decode(&current);
        if !decoded.utf8 {
            return Err(ApiError::invalid(
                "This file is not UTF-8 text, so Borealis cannot save it.",
            ));
        }
        // Unchanged lines keep their own line ending (files with mixed endings stay as they are around an edit).
        let bytes = text::encode_keeping(text_in, &decoded);
        let outcome = write::guarded_write(&found.real, base, &bytes)
            .map_err(|e| ApiError::io("save the note", e))?;
        lock(&self.vault.index)?.invalidate(path);
        Ok(match outcome {
            write::Outcome::Saved { hash } => VaultSave {
                status: SaveStatus::Saved,
                hash: Some(hash),
                mtime_ms: Some(mtime_of(&found.real)),
                disk: None,
            },
            write::Outcome::Changed { disk } => VaultSave {
                status: SaveStatus::Conflict,
                hash: None,
                mtime_ms: None,
                disk: Some(note_of(path, &found.real, &disk)),
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
        let vault = self.vault()?;
        let listing = tree::list(&vault);
        let files = file_set(&listing);
        let cfg = VaultConfig::load(vault.root());
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
                free_path(&files, &vault, &base, UNTITLED, "md")?
            }
        };
        let target = vault.new_file(&rel)?;
        write::create_new(&target, b"").map_err(|e| ApiError::io("create the note", e))?;
        Ok(note_of(&rel, &target, b""))
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
        let vault = self.vault()?;
        let found = vault.existing_file(path)?;
        lock(&self.vault.index)?.invalidate(path);
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

    /// Resolve link targets written in the note `from`: the file each one opens, or `None` (unresolved).
    ///
    /// # Errors
    /// When no vault is open.
    pub fn vault_resolve(&self, from: &str, targets: &[String]) -> ApiResult<Vec<Option<String>>> {
        let vault = self.vault()?;
        let files = file_set(&tree::list(&vault));
        Ok(targets
            .iter()
            .map(|t| files.resolve(from, t).map(str::to_string))
            .collect())
    }

    /// Fresh tree and index (for the queries below).
    fn indexed(&self) -> ApiResult<(Vault, Listing)> {
        let vault = self.vault()?;
        let listing = tree::list(&vault);
        lock(&self.vault.index)?.refresh(&vault, &listing.entries);
        Ok((vault, listing))
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
        let (vault, listing) = self.indexed()?;
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
        Ok(index::search(&vault, &listing.entries, query))
    }

    /// An image of the vault as a `data:` URL (for the image view and `![[embeds]]`).
    ///
    /// # Errors
    /// `invalid_argument` for a path outside the vault, a file that is not an image, or one that is too large.
    pub fn vault_image(&self, path: &str) -> ApiResult<String> {
        let vault = self.vault()?;
        if tree::kind_of(paths::file_name(path)) != EntryKind::Image {
            return Err(ApiError::invalid("That file is not an image."));
        }
        let found = vault.existing_file(path)?;
        let bytes = read_capped(&found.real, MAX_IMAGE_BYTES)?
            .ok_or_else(|| ApiError::invalid("That image is too large to show."))?;
        Ok(format!(
            "data:{};base64,{}",
            image_mime(path),
            STANDARD.encode(bytes)
        ))
    }

    /// The link to insert in the note `from` for the vault file `to`, written as `.obsidian/app.json` says.
    ///
    /// # Errors
    /// When no vault is open or `to` is not one of its files.
    pub fn vault_link_text(&self, from: &str, to: &str, embed: bool) -> ApiResult<String> {
        let vault = self.vault()?;
        let files = file_set(&tree::list(&vault));
        let to = files
            .get(to)
            .ok_or_else(|| ApiError::invalid("That file is not in your vault."))?;
        Ok(VaultConfig::load(vault.root()).link_text(&files, from, to, embed))
    }

    /// Copy the file behind a `vault-attachment` pick into the attachment folder `.obsidian/app.json` names for the
    /// note `from`, and return the link to insert (an embed for images).
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an oversized file; `io_error` when copying fails.
    pub fn vault_attach(&self, token: &str, from: &str) -> ApiResult<VaultAttachment> {
        let vault = self.vault()?;
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
        let cfg = VaultConfig::load(vault.root());
        let listing = tree::list(&vault);
        let files = file_set(&listing);
        let rel = free_path(&files, &vault, &cfg.attachment_folder(from), stem, ext)?;
        let target = vault.new_file(&rel)?;
        write::copy_new(&picked, &target).map_err(|e| ApiError::io("copy the file", e))?;
        let after = FileSet::new(files.paths().iter().cloned().chain([rel.clone()]));
        let embed = tree::kind_of(&rel) == EntryKind::Image;
        Ok(VaultAttachment {
            link: cfg.link_text(&after, from, &rel, embed),
            path: rel,
        })
    }

    /// Start watching the vault (replacing any earlier watcher); `on_change` gets each batch of changes.
    ///
    /// # Errors
    /// When no vault is open or the OS refuses to watch it.
    #[cfg(desktop)]
    pub fn vault_watch_start(
        &self,
        on_change: impl Fn(watch::VaultChange) + Send + 'static,
    ) -> ApiResult<()> {
        let vault = self.vault()?;
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

fn saved(path: &Path, bytes: &[u8]) -> VaultSave {
    VaultSave {
        status: SaveStatus::Saved,
        hash: Some(text::hash(bytes)),
        mtime_ms: Some(mtime_of(path)),
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
