//! Paths inside the vault. The webview names files only by vault-relative paths with `/` separators
//! (`Campaign/Session 1.md`); this module turns them into real paths and refuses anything that would leave the vault:
//! `..` and `.` parts, absolute paths, drive prefixes, back-slashes, hidden names (`.obsidian`, `.git`, `.trash`, dot
//! files) and system files, and symbolic links whose target is outside the vault.

use crate::error::{ApiError, ApiResult};
use std::path::{Component, Path, PathBuf};

/// Longest vault-relative path accepted, in bytes.
pub const MAX_PATH_LEN: usize = 1024;
/// File names the OS puts in folders; never listed or opened (compared case-insensitively).
const SYSTEM_FILES: [&str; 4] = ["Thumbs.db", "desktop.ini", "Icon\r", "$RECYCLE.BIN"];
/// Characters a new file name may not hold: the OS-reserved ones, and the ones that break Obsidian links.
const NAME_FORBIDDEN: [char; 13] = [
    '/', '\\', ':', '*', '?', '"', '<', '>', '|', '#', '^', '[', ']',
];
/// Longest new file name, in bytes (most file systems allow 255).
const MAX_NAME_LEN: usize = 200;

/// A name the vault never lists or opens: dot files and folders (`.obsidian`, `.trash`, `.git`), and OS files.
#[must_use]
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') || SYSTEM_FILES.iter().any(|s| s.eq_ignore_ascii_case(name))
}

fn outside() -> ApiError {
    ApiError::invalid("That file is not in your vault.")
}

/// The parts of a vault-relative path, each a plain visible name.
///
/// # Errors
/// `invalid_argument` for anything else (see the module docs).
pub fn parts(rel: &str) -> ApiResult<Vec<&str>> {
    if rel.is_empty() || rel.len() > MAX_PATH_LEN {
        return Err(outside());
    }
    let parts: Vec<&str> = rel.split('/').collect();
    let plain = |p: &&str| {
        !p.is_empty()
            && !is_hidden_name(p)
            && !p.contains(['\\', ':', '\0'])
            && !p.chars().any(char::is_control)
            && matches!(
                Path::new(p).components().collect::<Vec<_>>().as_slice(),
                [Component::Normal(_)]
            )
    };
    if parts.iter().all(plain) {
        Ok(parts)
    } else {
        Err(outside())
    }
}

/// Check a new file or folder name the user typed (create, rename).
///
/// # Errors
/// `invalid_argument` naming the problem.
pub fn check_new_name(name: &str) -> ApiResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::invalid("Type a name."));
    }
    if trimmed != name || name.ends_with('.') {
        return Err(ApiError::invalid(
            "A name cannot start or end with a space, or end with a dot.",
        ));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(ApiError::invalid("That name is too long."));
    }
    if name.contains(NAME_FORBIDDEN) || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "A name cannot contain any of these characters: / \\ : * ? \" < > | # ^ [ ]",
        ));
    }
    if is_hidden_name(name) {
        return Err(ApiError::invalid(
            "A name cannot start with a dot: Obsidian hides those files.",
        ));
    }
    Ok(())
}

/// A file found in the vault: the path as named (a link stays a link) and the real file behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// `root` joined with the vault-relative parts (rename and delete act on this, so a link itself moves).
    pub path: PathBuf,
    /// The canonical file (reads and writes go here, so writing through a link keeps the link).
    pub real: PathBuf,
}

/// An open vault folder (canonical root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open `dir` as a vault.
    ///
    /// # Errors
    /// When the folder cannot be resolved or is not a folder.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        let root = std::fs::canonicalize(dir)?;
        if !root.is_dir() {
            return Err(std::io::ErrorKind::NotADirectory.into());
        }
        Ok(Self { root })
    }

    /// The canonical vault folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn join(&self, parts: &[&str]) -> PathBuf {
        parts.iter().fold(self.root.clone(), |p, part| p.join(part))
    }

    /// A real path is inside the vault.
    #[must_use]
    pub fn contains(&self, real: &Path) -> bool {
        real.starts_with(&self.root)
    }

    /// A real path is inside the vault and not hidden (a link may not lead into `.obsidian/` or another hidden place).
    #[must_use]
    pub fn reachable(&self, real: &Path) -> bool {
        real == self.root
            || self
                .relative(real)
                .is_some_and(|rel| !rel.split('/').any(is_hidden_name))
    }

    /// An existing entry named by `rel`, resolved through links that stay inside the vault and out of hidden places.
    ///
    /// # Errors
    /// `invalid_argument` for a bad path or a link out of the vault; `not_found` when nothing is there.
    pub fn existing(&self, rel: &str) -> ApiResult<Located> {
        let path = self.join(&parts(rel)?);
        std::fs::symlink_metadata(&path).map_err(|_| missing())?;
        let real = std::fs::canonicalize(&path).map_err(|_| missing())?;
        if !self.reachable(&real) {
            return Err(outside());
        }
        Ok(Located { path, real })
    }

    /// The name of the real file behind `found` (a link's target may have another name and kind).
    #[must_use]
    pub fn real_name(found: &Located) -> &str {
        found
            .real
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
    }

    /// An existing file (not a folder) named by `rel`.
    ///
    /// # Errors
    /// As [`Vault::existing`], and `invalid_argument` for a folder.
    pub fn existing_file(&self, rel: &str) -> ApiResult<Located> {
        let found = self.existing(rel)?;
        if !found.real.is_file() {
            return Err(ApiError::invalid("That is a folder, not a file."));
        }
        Ok(found)
    }

    /// Where a new file `rel` goes: its folder exists (or is created) inside the vault, and nothing is there yet.
    ///
    /// # Errors
    /// `invalid_argument` for a bad path, a folder that leads out of the vault, or a name that is taken; `io_error`
    /// when a folder cannot be created.
    pub fn new_file(&self, rel: &str) -> ApiResult<PathBuf> {
        let parts = parts(rel)?;
        let (name, folders) = parts.split_last().ok_or_else(outside)?;
        let mut dir = self.root.clone();
        for folder in folders {
            dir.push(folder);
            match std::fs::symlink_metadata(&dir) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(ApiError::invalid("A file is in the way of that folder.")),
                Err(_) => {
                    std::fs::create_dir(&dir).map_err(|e| ApiError::io("create the folder", e))?
                }
            }
        }
        let real_dir = std::fs::canonicalize(&dir).map_err(|_| outside())?;
        if !self.contains(&real_dir) {
            return Err(outside());
        }
        let path = real_dir.join(name);
        if std::fs::symlink_metadata(&path).is_ok() {
            return Err(ApiError::invalid("A file with that name is already there."));
        }
        Ok(path)
    }

    /// The vault-relative path of a real path inside the vault (`None` outside it or for a non-UTF-8 name).
    #[must_use]
    pub fn relative(&self, path: &Path) -> Option<String> {
        let rest = path.strip_prefix(&self.root).ok()?;
        let parts: Option<Vec<&str>> = rest
            .components()
            .map(|c| match c {
                Component::Normal(n) => n.to_str(),
                _ => None,
            })
            .collect();
        let parts = parts.filter(|p| !p.is_empty())?;
        Some(parts.join("/"))
    }
}

fn missing() -> ApiError {
    ApiError::new(
        crate::error::ErrorCode::NotFound,
        "That file is no longer in your vault.",
    )
}

/// The folder part of a vault-relative path (`""` for the vault folder itself).
#[must_use]
pub fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file name part of a vault-relative path.
#[must_use]
pub fn file_name(rel: &str) -> &str {
    rel.rsplit_once('/').map_or(rel, |(_, name)| name)
}

/// `folder/name`, or `name` in the vault folder.
#[must_use]
pub fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// The name without its last extension (`Session 1.md` -> `Session 1`; a name without one is unchanged).
#[must_use]
pub fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// The lower-case last extension of a name, if any.
#[must_use]
pub fn extension(name: &str) -> Option<String> {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => Some(ext.to_lowercase()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
