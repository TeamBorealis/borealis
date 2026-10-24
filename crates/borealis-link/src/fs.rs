//! Private, atomic file writes (non-secret device state such as the registry).

use std::io::Write as _;
use std::path::Path;

use crate::error::LinkError;

/// Atomically replace `path` with `bytes`: write a sibling temp file (mode 0600 on unix), fsync,
/// rename over the target, then fsync the directory (best effort).
///
/// # Errors
/// Any I/O failure; the target is left untouched in that case.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), LinkError> {
    let dir = path
        .parent()
        .ok_or_else(|| LinkError::Invalid("path has no parent directory".into()))?;
    let mut tmp = tempfile::Builder::new().prefix(".tmp-").tempfile_in(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| LinkError::io(e.error))?;
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_atomically_with_private_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.json");
        write_private_file(&p, b"one").unwrap();
        write_private_file(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
