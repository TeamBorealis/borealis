//! The homebrew creator's files (spec 10.1, 14.1): a creator pack folder read whole, written file by file with atomic
//! replaces in a safe order, and the `.trash` copies of deleted elements.
//!
//! **Interrupted saves.** Every file is written to a dot-named temporary file beside it, flushed, then renamed over
//! the old one, so a file on disk is always either the old or the new version, never half of each. The files of one
//! save are written `creator.json` first, then `content.json`, then `pack.json`: a save cut short leaves a pack that
//! still loads, at worst with a `creator.json` that knows an element `content.json` does not hold yet (ignored and
//! reconciled on the next open) or a retired id whose element is still there (harmless: retired ids are only never
//! given out again). Temporary files start with a dot, so a crash never leaves one that loads, syncs or bundles.

use crate::content_packs::PACK_META_FILE;
use crate::error::{ApiError, ApiResult, ErrorCode};
use borealis_authoring::pack::{CreatorFile, CREATOR_FILE};
use borealis_content::{ContentFile, Element, PackFileKind, PackManifest};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Copies of deleted elements inside the pack folder (never loaded, synced or exported: the dot keeps it out).
pub const TRASH_DIR: &str = ".trash";
const MANIFEST_FILE: &str = "pack.json";

/// Write `text` to `path` atomically: a dot-named temporary file in the same folder, flushed to disk, then renamed
/// over `path`. On failure the temporary file is removed and `path` is untouched.
///
/// # Errors
/// The I/O error of the write or the rename.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp"));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    let result = written.and_then(|()| borealis_character::format::replace_file(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// A creator-editable pack as it is on disk.
#[derive(Debug, Clone)]
pub struct PackOnDisk {
    /// The pack folder.
    pub dir: PathBuf,
    pub manifest: PackManifest,
    /// The manifest path of its one content file.
    pub content_path: String,
    pub content: ContentFile,
    /// `creator.json`, when the pack has a readable one.
    pub creator: Option<CreatorFile>,
    /// Why `creator.json` could not be read, when it exists but can't be (it is set aside on the next write).
    pub creator_problem: Option<String>,
}

fn unreadable(file: &str, cause: impl std::fmt::Display) -> ApiError {
    tracing::warn!("homebrew pack file {file} can't be read: {cause}");
    ApiError::new(
        ErrorCode::ContentError,
        format!(
            "This pack's {file} can't be read (it may have been changed outside Borealis). Nothing was changed."
        ),
    )
}

/// Read a pack folder for the creator: `pack.json`, its one content file and `creator.json`.
///
/// # Errors
/// `content_error` when `pack.json` or the content file is missing or unreadable, or the pack does not have exactly
/// one content file (the creator writes one; a pack with several is not edited in place).
pub fn read_pack(dir: &Path) -> ApiResult<PackOnDisk> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .map_err(|e| unreadable(MANIFEST_FILE, e))?;
    let manifest = PackManifest::from_json_str(&text).map_err(|e| unreadable(MANIFEST_FILE, e))?;
    let mut contents = manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content);
    let (Some(entry), None) = (contents.next(), contents.next()) else {
        return Err(ApiError::new(
            ErrorCode::ContentError,
            "This pack has more than one content file, so the creator can't edit it in place.",
        ));
    };
    let content_path = entry.path.clone();
    if !is_plain_file_name(&content_path) {
        return Err(unreadable(&content_path, "not a plain file name"));
    }
    let text = std::fs::read_to_string(dir.join(&content_path))
        .map_err(|e| unreadable(&content_path, e))?;
    let content = ContentFile::from_json_str(&text).map_err(|e| unreadable(&content_path, e))?;
    let (creator, creator_problem) = match std::fs::read_to_string(dir.join(CREATOR_FILE)) {
        Err(_) => (None, None),
        Ok(text) => match CreatorFile::parse(&text) {
            Ok(c) => (Some(c), None),
            Err(e) => {
                tracing::warn!("{}: {e}", dir.display());
                (None, Some(e.to_string()))
            }
        },
    };
    Ok(PackOnDisk {
        dir: dir.to_path_buf(),
        manifest,
        content_path,
        content,
        creator,
        creator_problem,
    })
}

/// A file name with no folder part (the creator writes its files at the top of the pack folder).
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// What one save writes.
#[derive(Debug, Clone)]
pub struct PackWrite {
    pub manifest: PackManifest,
    pub content: ContentFile,
    pub creator: CreatorFile,
    /// Elements deleted by this save, copied to `.trash` (spec 10.2: Undo restores from it).
    pub trashed: Vec<Element>,
}

/// Write a save into the pack folder, file by file in the safe order (module doc). An unreadable `creator.json` is
/// kept beside as `creator.json.bad` before the new one replaces it.
///
/// # Errors
/// `io_error` when a file cannot be written (the files written before it stay, each whole).
pub fn write_pack(pack: &PackOnDisk, next: &PackWrite, now_ms: u128) -> ApiResult<()> {
    let dir = &pack.dir;
    if !next.trashed.is_empty() {
        let trash = dir.join(TRASH_DIR);
        std::fs::create_dir_all(&trash)
            .map_err(|e| ApiError::io("keep a copy of the deleted elements", e))?;
        let first = next.trashed[0].id.clone();
        let file = ContentFile::new(next.trashed.clone());
        write_atomic(
            &trash.join(format!("{now_ms}-{}.json", safe_name(&first))),
            &file.to_json_pretty()?,
        )
        .map_err(|e| ApiError::io("keep a copy of the deleted elements", e))?;
    }
    if pack.creator_problem.is_some() {
        let _ = std::fs::rename(
            dir.join(CREATOR_FILE),
            dir.join(format!("{CREATOR_FILE}.bad")),
        );
    }
    write_atomic(&dir.join(CREATOR_FILE), &next.creator.to_json_pretty())
        .map_err(|e| ApiError::io("save the homebrew pack", e))?;
    write_atomic(
        &dir.join(&pack.content_path),
        &next.content.to_json_pretty()?,
    )
    .map_err(|e| ApiError::io("save the homebrew pack", e))?;
    write_atomic(&dir.join(MANIFEST_FILE), &next.manifest.to_json_pretty()?)
        .map_err(|e| ApiError::io("save the homebrew pack", e))?;
    Ok(())
}

/// Write a new pack folder (NEW PACK): the files of [`write_pack`] and `pack-meta.json` (`kind: homebrew`).
///
/// # Errors
/// `io_error`.
pub fn write_new_pack(dir: &Path, next: &PackWrite, content_path: &str) -> ApiResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| ApiError::io("create the homebrew pack", e))?;
    write_atomic(&dir.join(CREATOR_FILE), &next.creator.to_json_pretty())
        .and_then(|()| {
            write_atomic(
                &dir.join(content_path),
                &next.content.to_json_pretty().unwrap_or_default(),
            )
        })
        .and_then(|()| {
            write_atomic(
                &dir.join(PACK_META_FILE),
                &serde_json::to_string_pretty(&serde_json::json!({ "kind": "homebrew" }))
                    .unwrap_or_default(),
            )
        })
        .and_then(|()| {
            write_atomic(
                &dir.join(MANIFEST_FILE),
                &next.manifest.to_json_pretty().unwrap_or_default(),
            )
        })
        .map_err(|e| ApiError::io("create the homebrew pack", e))
}

/// An id as a file name part (ids are `A-Z0-9_`; anything else becomes `_`).
fn safe_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_whole_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("content.json");
        std::fs::write(&p, "old").unwrap();
        write_atomic(&p, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["content.json"]);
    }

    #[test]
    fn a_failed_atomic_write_keeps_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        // the target is a folder: the rename fails, the "file" is untouched and no temporary file stays
        let p = dir.path().join("content.json");
        std::fs::create_dir_all(p.join("inner")).unwrap();
        assert!(write_atomic(&p, "new").is_err());
        assert!(p.join("inner").is_dir());
        assert!(!dir.path().join(".content.json.tmp").exists());
    }

    #[test]
    fn plain_file_names_only() {
        assert!(is_plain_file_name("content.json"));
        assert!(!is_plain_file_name("a/content.json"));
        assert!(!is_plain_file_name(".."));
        assert_eq!(safe_name("ID_A/../B"), "ID_A____B");
    }
}
