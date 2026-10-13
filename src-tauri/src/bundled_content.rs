//! Extraction of bundled content packs from a read-only asset container to a real folder.
//!
//! Android keeps bundled resources inside the APK, so they are not filesystem paths and the
//! `ContentStore` (which loads from a directory) cannot read them. [`sync`] copies the packs into
//! the app data folder when the app version or the bundled bytes change, swapping the folder in
//! atomically. The asset access is abstracted by [`AssetSource`] so the logic is platform neutral.

use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};

/// Name of the stamp file stored inside the extracted folder.
const STAMP_FILE: &str = ".bundled-stamp";
/// A file that must exist in a usable extraction.
const SENTINEL: &str = "core/pack.json";

/// Read-only access to a tree of bundled assets (paths use `/`).
pub trait AssetSource {
    /// Names of the entries (files and folders) directly inside `dir`; empty for a file or a missing folder.
    ///
    /// # Errors
    /// An I/O error from the underlying container.
    fn list(&self, dir: &str) -> io::Result<Vec<String>>;

    /// The bytes of the file at `path`.
    ///
    /// # Errors
    /// An I/O error if the file cannot be read.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
}

/// What [`sync`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The target already matched the bundled content and version.
    UpToDate,
    /// The target was (re)written with this many files.
    Extracted(usize),
}

fn other(msg: String) -> io::Error {
    io::Error::other(msg)
}

fn collect(
    source: &dyn AssetSource,
    root: &str,
    rel: &str,
    out: &mut Vec<(String, Vec<u8>)>,
) -> io::Result<()> {
    let dir = format!("{root}/{rel}");
    for name in source.list(&dir)? {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(other(format!("invalid bundled entry name {name:?}")));
        }
        let child = format!("{rel}/{name}");
        if source.list(&format!("{root}/{child}"))?.is_empty() {
            out.push((child.clone(), source.read(&format!("{root}/{child}"))?));
        } else {
            collect(source, root, &child, out)?;
        }
    }
    Ok(())
}

fn stamp_of(app_version: &str, files: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(app_version.as_bytes());
    for (path, bytes) in files {
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{app_version}\n{digest}")
}

fn sibling(target: &Path, suffix: &str) -> PathBuf {
    let name = target
        .file_name()
        .map_or_else(|| "bundled".into(), |n| n.to_string_lossy().into_owned());
    target.with_file_name(format!(".{name}{suffix}"))
}

fn write_tree(dir: &Path, files: &[(String, Vec<u8>)], stamp: &str) -> io::Result<()> {
    for (rel, bytes) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    std::fs::write(dir.join(STAMP_FILE), stamp)
}

/// Make `target` hold exactly the bundled `packs` found under `asset_root` for `app_version`.
///
/// The packs are read first; if `target` already carries the same stamp (version + content hash)
/// and its sentinel file, nothing is written. Otherwise they are written to a temporary sibling
/// folder which then replaces `target` (previous folder renamed aside, then removed).
///
/// # Errors
/// An I/O error if a pack is missing/empty, or the extraction cannot be written.
pub fn sync(
    source: &dyn AssetSource,
    asset_root: &str,
    packs: &[&str],
    target: &Path,
    app_version: &str,
) -> io::Result<SyncOutcome> {
    let mut files = Vec::new();
    for pack in packs {
        let before = files.len();
        collect(source, asset_root, pack, &mut files)?;
        if files.len() == before {
            return Err(other(format!("bundled pack {pack} has no files")));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let stamp = stamp_of(app_version, &files);
    let current = std::fs::read_to_string(target.join(STAMP_FILE)).ok();
    if current.as_deref() == Some(stamp.as_str()) && target.join(SENTINEL).is_file() {
        return Ok(SyncOutcome::UpToDate);
    }
    let tmp = sibling(target, ".tmp");
    let old = sibling(target, ".old");
    for leftover in [&tmp, &old] {
        if leftover.exists() {
            std::fs::remove_dir_all(leftover)?;
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(e) = write_tree(&tmp, &files, &stamp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if target.exists() {
        std::fs::rename(target, &old)?;
    }
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(SyncOutcome::Extracted(files.len()))
}

/// The APK's `assets/` folder (Android only).
#[cfg(target_os = "android")]
#[derive(Debug, Default, Clone, Copy)]
pub struct ApkAssets;

#[cfg(target_os = "android")]
impl AssetSource for ApkAssets {
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        borealis_android_assets::list(dir)
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        borealis_android_assets::read(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fake(BTreeMap<String, Vec<u8>>);

    impl Fake {
        fn new(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(p, c)| ((*p).to_string(), c.as_bytes().to_vec()))
                    .collect(),
            )
        }
    }

    impl AssetSource for Fake {
        fn list(&self, dir: &str) -> io::Result<Vec<String>> {
            let prefix = format!("{dir}/");
            let mut names: Vec<String> = self
                .0
                .keys()
                .filter_map(|k| k.strip_prefix(&prefix))
                .map(|rest| rest.split('/').next().unwrap_or_default().to_string())
                .collect();
            names.dedup();
            Ok(names)
        }
        fn read(&self, path: &str) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
    }

    const FILES: [(&str, &str); 3] = [
        ("content/core/pack.json", "{}"),
        ("content/core/a/b.json", "[1]"),
        ("content/srd/pack.json", "{}"),
    ];

    #[test]
    fn extracts_all_packs_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bundled-content");
        let out = sync(
            &Fake::new(&FILES),
            "content",
            &["core", "srd"],
            &target,
            "1",
        )
        .unwrap();
        assert_eq!(out, SyncOutcome::Extracted(3));
        assert_eq!(std::fs::read(target.join("core/a/b.json")).unwrap(), b"[1]");
        assert!(target.join("srd/pack.json").is_file());
        assert!(!sibling(&target, ".tmp").exists());
    }

    #[test]
    fn second_run_is_up_to_date_and_changes_reextract() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bc");
        let packs = ["core", "srd"];
        sync(&Fake::new(&FILES), "content", &packs, &target, "1").unwrap();
        let again = sync(&Fake::new(&FILES), "content", &packs, &target, "1").unwrap();
        assert_eq!(again, SyncOutcome::UpToDate);
        let bumped = sync(&Fake::new(&FILES), "content", &packs, &target, "2").unwrap();
        assert_eq!(bumped, SyncOutcome::Extracted(3));
        let mut changed = FILES;
        changed[1].1 = "[2]";
        let out = sync(&Fake::new(&changed), "content", &packs, &target, "2").unwrap();
        assert_eq!(out, SyncOutcome::Extracted(3));
        assert_eq!(std::fs::read(target.join("core/a/b.json")).unwrap(), b"[2]");
    }

    #[test]
    fn replacement_drops_stale_files_and_leaves_no_temp_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bc");
        sync(
            &Fake::new(&FILES),
            "content",
            &["core", "srd"],
            &target,
            "1",
        )
        .unwrap();
        std::fs::write(target.join("core/stale.json"), "x").unwrap();
        sync(
            &Fake::new(&FILES),
            "content",
            &["core", "srd"],
            &target,
            "2",
        )
        .unwrap();
        assert!(!target.join("core/stale.json").exists());
        assert!(!sibling(&target, ".old").exists());
        assert!(!sibling(&target, ".tmp").exists());
    }

    #[test]
    fn missing_pack_fails_and_keeps_previous_extraction() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bc");
        sync(&Fake::new(&FILES), "content", &["core"], &target, "1").unwrap();
        let err = sync(
            &Fake::new(&FILES),
            "content",
            &["core", "nope"],
            &target,
            "2",
        );
        assert!(err.is_err());
        assert!(target.join("core/pack.json").is_file());
    }

    #[test]
    fn damaged_target_is_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("bc");
        sync(&Fake::new(&FILES), "content", &["core"], &target, "1").unwrap();
        std::fs::remove_file(target.join("core/pack.json")).unwrap();
        let out = sync(&Fake::new(&FILES), "content", &["core"], &target, "1").unwrap();
        assert_eq!(out, SyncOutcome::Extracted(2));
    }
}
