//! Safe writes. A note is written only when what is on disk is still what the editor loaded (same content hash), so
//! a change another app made (Obsidian, a sync service) is never overwritten silently. Every write is atomic: the new
//! bytes go to a hidden temporary file in the same folder, which is flushed and then renamed over the note, so a crash
//! or a full disk leaves either the old note or the new one, never half of it.

use super::text;
use std::io::Write as _;
use std::path::Path;

/// Permissions of a new note on Unix (owner read-write, others read).
#[cfg(unix)]
const NEW_FILE_MODE: u32 = 0o644;

/// What a guarded write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Written; the new content hash.
    Saved { hash: String },
    /// The file changed since it was loaded; nothing was written. `disk` is what is there now.
    Changed { disk: Vec<u8> },
    /// The file was deleted or moved since it was loaded; nothing was written.
    Missing,
}

/// Write `bytes` to `path` with a temporary file and a rename in the same folder. An existing file keeps its
/// permissions.
///
/// # Errors
/// When the temporary file cannot be written or renamed.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".borealis-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    if let Ok(meta) = std::fs::metadata(path) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Create a new file at `path` atomically; fails when something appeared there meanwhile (it is never replaced).
///
/// # Errors
/// When the file exists or cannot be written.
pub fn create_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".borealis-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    // The temporary file is private (0600); a note is an ordinary file like the ones Obsidian creates.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(NEW_FILE_MODE))?;
    }
    tmp.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}

/// Copy the file `src` to the new file `dest` atomically (a temporary file in `dest`'s folder, renamed only if nothing
/// appeared at `dest` meanwhile), so a failed copy leaves nothing behind.
///
/// # Errors
/// When `src` cannot be read, the copy fails, or `dest` is taken.
pub fn copy_new(src: &Path, dest: &Path) -> std::io::Result<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut from = std::fs::File::open(src)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".borealis-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    std::io::copy(&mut from, tmp.as_file_mut())?;
    tmp.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(NEW_FILE_MODE))?;
    }
    tmp.persist_noclobber(dest).map_err(|e| e.error)?;
    Ok(())
}

/// Rename `from` to `to` without replacing a file at `to` (one Obsidian or a sync service may have just created): a
/// hard link first, which fails when the name is taken, then the old name goes. A case-only rename, a symbolic link, or
/// a file system without hard links falls back to a plain rename after `to` is checked free.
///
/// # Errors
/// `AlreadyExists` when `to` is taken; otherwise the rename's error.
pub fn rename_no_clobber(from: &Path, to: &Path, case_only: bool) -> std::io::Result<()> {
    let is_link = std::fs::symlink_metadata(from)?.file_type().is_symlink();
    if !case_only && !is_link {
        match std::fs::hard_link(from, to) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_file(from) {
                    let _ = std::fs::remove_file(to);
                    return Err(e);
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(e),
            Err(_) => {}
        }
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(std::io::ErrorKind::AlreadyExists.into());
        }
    }
    std::fs::rename(from, to)
}

/// Write `bytes` to the existing file `path` only if its content still hashes to `base_hash`.
///
/// # Errors
/// When the file cannot be read (other than missing) or written.
pub fn guarded_write(path: &Path, base_hash: &str, bytes: &[u8]) -> std::io::Result<Outcome> {
    let current = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Outcome::Missing),
        Err(e) => return Err(e),
    };
    if text::hash(&current) != base_hash {
        return Ok(Outcome::Changed { disk: current });
    }
    if current == bytes {
        return Ok(Outcome::Saved {
            hash: base_hash.to_string(),
        });
    }
    atomic_write(path, bytes)?;
    Ok(Outcome::Saved {
        hash: text::hash(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(dir: &Path, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join("Note.md");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn visible_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn an_unchanged_file_is_written_and_no_temporary_file_is_left() {
        let dir = tempfile::tempdir().unwrap();
        let p = note(dir.path(), b"old");
        let out = guarded_write(&p, &text::hash(b"old"), b"new").unwrap();
        assert_eq!(
            out,
            Outcome::Saved {
                hash: text::hash(b"new")
            }
        );
        assert_eq!(std::fs::read(&p).unwrap(), b"new");
        assert_eq!(visible_files(dir.path()), ["Note.md"]);
    }

    #[test]
    fn a_file_changed_by_another_app_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = note(dir.path(), b"theirs");
        let out = guarded_write(&p, &text::hash(b"loaded"), b"mine").unwrap();
        assert_eq!(
            out,
            Outcome::Changed {
                disk: b"theirs".to_vec()
            }
        );
        assert_eq!(std::fs::read(&p).unwrap(), b"theirs");
    }

    #[test]
    fn a_deleted_file_is_reported_and_not_recreated() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Gone.md");
        assert_eq!(
            guarded_write(&p, &text::hash(b"x"), b"y").unwrap(),
            Outcome::Missing
        );
        assert!(!p.exists());
    }

    #[test]
    fn writing_the_same_bytes_does_not_touch_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = note(dir.path(), b"same");
        let before = std::fs::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        guarded_write(&p, &text::hash(b"same"), b"same").unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn the_atomic_write_keeps_the_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let p = note(dir.path(), b"a");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
        atomic_write(&p, b"b").unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn rename_never_replaces_a_file_that_appeared_at_the_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let from = note(dir.path(), b"mine");
        let to = dir.path().join("Taken.md");
        std::fs::write(&to, b"theirs").unwrap();
        let err = rename_no_clobber(&from, &to, false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&to).unwrap(), b"theirs");
        assert_eq!(std::fs::read(&from).unwrap(), b"mine");
        let free = dir.path().join("Free.md");
        rename_no_clobber(&from, &free, false).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read(&free).unwrap(), b"mine");
    }

    #[test]
    fn a_copy_lands_whole_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        std::fs::write(&src, b"attachment").unwrap();
        let dest = dir.path().join("sub.bin");
        copy_new(&src, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"attachment");
        assert!(copy_new(&src, &dest).is_err(), "never over a file");
        assert!(copy_new(&dir.path().join("missing"), &dir.path().join("x.bin")).is_err());
        assert_eq!(visible_files(dir.path()), ["src.bin", "sub.bin"]);
    }

    #[test]
    fn create_new_never_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = note(dir.path(), b"first");
        assert!(create_new(&p, b"second").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"first");
        let fresh = dir.path().join("Fresh.md");
        create_new(&fresh, b"hello").unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"hello");
        assert_eq!(visible_files(dir.path()), ["Fresh.md", "Note.md"]);
    }
}
