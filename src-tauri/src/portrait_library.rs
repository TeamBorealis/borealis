//! The portrait library folder (GitHub #32): a folder of the user's own images shown in the portraits gallery.
//!
//! The reference app lists the image files of its `<documents>/portraits` folder in the gallery. Borealis lets the user
//! choose that folder in Settings > Content (`portraitLibraryDir`, a backend-picked folder token, never a raw path).
//! The webview never reads the folder: it gets the listing ([`AppState::portrait_library`]) and then one image at a
//! time as a `data:` URL ([`AppState::portrait_library_image`]), which it passes to `set_portrait` like a browsed
//! image, so the portrait is embedded in the character file and the file stays portable.
//!
//! Only images the portrait itself accepts are listed (PNG, JPEG, GIF, WebP, at most
//! [`MAX_PORTRAIT_BYTES`], content matching the extension). Hidden files, symbolic links that lead out of the folder
//! and linked sub-folders are skipped; images in sub-folders one level down are listed too.

use crate::error::{ApiError, ApiResult};
use crate::extras::{magic_matches, MAX_PORTRAIT_BYTES};
use crate::picked_paths::PickPurpose;
use crate::settings_dto::{DataDirs, PortraitLibrary, PortraitLibraryImage};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// File extensions listed in the library (the types `set_portrait` accepts).
pub const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
/// Most images listed; a bigger folder is cut off (`truncated`).
pub const MAX_LIBRARY_IMAGES: usize = 500;
/// Bytes read from each file to check that its content matches its extension.
const HEADER_LEN: u64 = 16;
/// The separator of the two parts of an image id (`folder/name`).
const ID_SEPARATOR: char = '/';

/// What one directory entry is to the library.
enum Kind {
    /// A usable image; the normalised extension (`jpg` for `.jpeg`).
    Image(&'static str),
    /// A real sub-folder (not a link).
    Folder,
    /// An image file that cannot be used: too large, empty, wrong content, or a link out of the folder.
    Refused,
    /// Anything else: other files, hidden files, linked folders.
    Ignored,
}

/// The normalised image extension of `name`, if it is one the library lists.
fn image_ext(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        _ => None,
    }
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// The first bytes of `path` match `ext`.
fn header_matches(path: &Path, ext: &str) -> bool {
    let mut head = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(HEADER_LEN).read_to_end(&mut head))
        .is_ok_and(|_| magic_matches(ext, &head))
}

/// Classify `path` (named `name`) inside the canonical `root`.
fn classify(root: &Path, path: &Path, name: &str) -> Kind {
    if name.starts_with('.') {
        return Kind::Ignored;
    }
    let Ok(link) = std::fs::symlink_metadata(path) else {
        return Kind::Ignored;
    };
    if link.is_dir() {
        return Kind::Folder;
    }
    let Some(ext) = image_ext(name) else {
        return Kind::Ignored;
    };
    if link.file_type().is_symlink() {
        let inside = std::fs::canonicalize(path).is_ok_and(|t| t.starts_with(root) && t.is_file());
        if !inside {
            return Kind::Refused;
        }
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return Kind::Refused;
    };
    let size_ok = meta.len() > 0 && meta.len() <= MAX_PORTRAIT_BYTES as u64;
    if !meta.is_file() || !size_ok || !header_matches(path, ext) {
        return Kind::Refused;
    }
    Kind::Image(ext)
}

/// The entries of `dir` with UTF-8 names, sorted case-insensitively (names that are not UTF-8 cannot be ids).
fn sorted_entries(dir: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .collect();
    out.sort_by_cached_key(|(name, _)| (name.to_lowercase(), name.clone()));
    Ok(out)
}

/// The listing of a library folder, without the folder fields.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub images: Vec<PortraitLibraryImage>,
    pub refused: usize,
    pub truncated: bool,
}

impl Scan {
    /// Add `kind` found at `name` in `folder`; false once the listing is full.
    fn add(&mut self, kind: &Kind, name: &str, folder: Option<&str>) -> bool {
        match kind {
            Kind::Image(_) if self.images.len() >= MAX_LIBRARY_IMAGES => {
                self.truncated = true;
                return false;
            }
            Kind::Image(_) => self.images.push(PortraitLibraryImage {
                id: folder.map_or_else(|| name.to_string(), |f| format!("{f}{ID_SEPARATOR}{name}")),
                name: name.to_string(),
                folder: folder.map(str::to_string),
            }),
            Kind::Refused => self.refused += 1,
            Kind::Folder | Kind::Ignored => {}
        }
        true
    }
}

/// List the images of the library folder `root` and of its sub-folders one level down (top-level images first,
/// then each sub-folder in name order).
///
/// # Errors
/// When `root` itself cannot be read (an unreadable sub-folder is skipped).
pub fn scan(root: &Path) -> std::io::Result<Scan> {
    let root = std::fs::canonicalize(root)?;
    let mut out = Scan::default();
    let mut folders = Vec::new();
    for (name, path) in sorted_entries(&root)? {
        let kind = classify(&root, &path, &name);
        if matches!(kind, Kind::Folder) {
            folders.push((name, path));
        } else if !out.add(&kind, &name, None) {
            return Ok(out);
        }
    }
    for (folder, dir) in folders {
        let entries = match sorted_entries(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("portrait library: a sub-folder was not read: {}", e.kind());
                continue;
            }
        };
        for (name, path) in entries {
            if !out.add(&classify(&root, &path, &name), &name, Some(&folder)) {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// The parts of a library image id: `name` or `folder/name`, plain names only.
fn id_parts(id: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = id.split(ID_SEPARATOR).collect();
    let plain = |p: &&str| {
        !p.is_empty()
            && !p.starts_with('.')
            && !p.contains(['\\', ':', '\0'])
            && Path::new(p).components().count() == 1
    };
    (parts.len() <= 2 && parts.iter().all(plain)).then_some(parts)
}

fn not_in_library() -> ApiError {
    ApiError::invalid("That image is not in your portrait library.")
}

/// Read the library image `id` from the folder `root` as a `data:` URL (the same checks as the listing).
///
/// # Errors
/// `invalid_argument` for an id that is not a listed image of `root`; `io_error` when it cannot be read.
pub fn read_image(root: &Path, id: &str) -> ApiResult<String> {
    let parts = id_parts(id).ok_or_else(not_in_library)?;
    let root = std::fs::canonicalize(root).map_err(|_| unavailable())?;
    if let [folder, _] = parts.as_slice() {
        let real_folder = std::fs::symlink_metadata(root.join(folder)).is_ok_and(|m| m.is_dir());
        if !real_folder {
            return Err(not_in_library());
        }
    }
    let path = parts.iter().fold(root.clone(), |p, part| p.join(part));
    let name = parts.last().copied().unwrap_or_default();
    let Kind::Image(ext) = classify(&root, &path, name) else {
        return Err(not_in_library());
    };
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|f| {
            f.take(MAX_PORTRAIT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| ApiError::io("read the portrait image", e))?;
    if bytes.len() > MAX_PORTRAIT_BYTES || !magic_matches(ext, &bytes) {
        return Err(not_in_library());
    }
    Ok(format!(
        "data:{};base64,{}",
        mime(ext),
        STANDARD.encode(bytes)
    ))
}

fn unavailable() -> ApiError {
    ApiError::invalid("Your portrait library folder is unavailable. Connect it, or choose it again in Settings > Content.")
}

impl AppState {
    /// The chosen library folder while it can be read, `Err(path)` while it cannot, `None` when none is chosen.
    fn portrait_library_root(&self) -> ApiResult<Option<Result<PathBuf, String>>> {
        let Some(dir) = self.settings()?.portrait_library_dir else {
            return Ok(None);
        };
        let ready = crate::data_dirs::validate_user_dir(
            Path::new(&dir),
            &self.secondary_forbidden(),
            false,
        )
        .ok()
        .filter(|root| std::fs::read_dir(root).is_ok());
        Ok(Some(ready.ok_or(dir)))
    }

    /// The chosen library folder cannot be read right now (unplugged drive, offline share, permissions). The setting
    /// is kept.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn portrait_library_unavailable(&self) -> ApiResult<bool> {
        Ok(matches!(self.portrait_library_root()?, Some(Err(_))))
    }

    /// Use the folder behind a `portrait-library-dir` pick (`None` = no library).
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or an unusable folder; `io_error` if the settings cannot be written.
    pub fn set_portrait_library_dir(&self, token: Option<&str>) -> ApiResult<DataDirs> {
        let chosen = match token {
            Some(t) => {
                let picked = self.picked.take(t, PickPurpose::PortraitLibraryDir)?;
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
        next.portrait_library_dir = chosen;
        next.save(&self.paths.settings_file())?;
        self.set_settings(next)?;
        self.data_dirs()
    }

    /// The portraits gallery's library section: the chosen folder, whether it can be read, and its images.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn portrait_library(&self) -> ApiResult<PortraitLibrary> {
        let dir = self.settings()?.portrait_library_dir;
        let scan = match self.portrait_library_root()? {
            None => Ok(Scan::default()),
            Some(Ok(root)) => scan(&root),
            Some(Err(_)) => Err(std::io::ErrorKind::NotFound.into()),
        };
        let unavailable = scan.is_err();
        if unavailable && dir.is_some() {
            tracing::warn!(
                "portrait library: the chosen folder is unavailable; the setting is kept"
            );
        }
        let scan = scan.unwrap_or_default();
        Ok(PortraitLibrary {
            dir,
            unavailable,
            images: scan.images,
            refused: scan.refused,
            truncated: scan.truncated,
        })
    }

    /// One library image as a `data:` URL, for the gallery tile and for `set_portrait`.
    ///
    /// # Errors
    /// `invalid_argument` when no library is chosen, it is unavailable, or `id` is not one of its images.
    pub fn portrait_library_image(&self, id: &str) -> ApiResult<String> {
        match self.portrait_library_root()? {
            None => Err(ApiError::invalid("No portrait library folder is chosen.")),
            Some(Err(_)) => Err(unavailable()),
            Some(Ok(root)) => read_image(&root, id),
        }
    }
}

#[cfg(test)]
#[path = "portrait_library_tests.rs"]
mod tests;
