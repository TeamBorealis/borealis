//! Local state as the planner and the writer see it: characters and user packs on disk, with the
//! same digests and edit stamps a bundle would carry.

use crate::error::{BundleError, Result};
use crate::exclude::{is_excluded, is_safe_rel_path, is_safe_segment};
use crate::limits::Limits;
use crate::manifest::{item_digest, sha256_hex, FileEntry, ItemKind, PackRef};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// A character or pack that exists on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalItem {
    /// Kind.
    pub kind: ItemKind,
    /// Character file stem or pack folder name.
    pub key: String,
    /// Same digest as [`crate::Item::sha256`].
    pub sha256: String,
    /// Edit stamp, Unix milliseconds: file modification time (newest file for a pack).
    pub edited_at_ms: u64,
}

/// Modification time of a path in Unix milliseconds (0 when unavailable).
#[must_use]
pub fn mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| BundleError::io_at(dir, e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .collect();
    out.sort();
    Ok(out)
}

fn is_real_file(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
}

fn is_real_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

/// A character file found on disk.
#[derive(Debug, Clone)]
pub struct CharacterFile {
    /// File stem.
    pub key: String,
    /// Full path.
    pub path: PathBuf,
}

/// `*.json` files directly inside `dir` with a safe stem. A missing directory is empty.
///
/// # Errors
/// I/O failure listing an existing directory.
pub fn list_characters(dir: &Path) -> Result<Vec<CharacterFile>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    Ok(read_dir_sorted(dir)?
        .into_iter()
        .filter(|p| is_real_file(p) && p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let key = p.file_stem()?.to_str()?.to_owned();
            (is_safe_segment(&key) && !is_excluded(&key)).then_some(CharacterFile { key, path: p })
        })
        .collect())
}

/// Digests of every character file in `dir`.
///
/// # Errors
/// I/O failure.
pub fn scan_characters(dir: &Path) -> Result<Vec<LocalItem>> {
    list_characters(dir)?
        .into_iter()
        .map(|c| {
            let bytes = fs::read(&c.path).map_err(|e| BundleError::io_at(&c.path, e))?;
            Ok(LocalItem {
                kind: ItemKind::Character,
                key: c.key,
                sha256: sha256_hex(&bytes),
                edited_at_ms: mtime_ms(&c.path),
            })
        })
        .collect()
}

/// The files of one user pack, after the exclusion list.
#[derive(Debug, Clone, Default)]
pub struct PackFiles {
    /// `(path relative to the pack folder, absolute path)`, sorted.
    pub files: Vec<(String, PathBuf)>,
    /// Newest modification time among the files, Unix milliseconds.
    pub edited_at_ms: u64,
    /// Relative paths left out by the exclusion list, unsafe names, symlinks or depth.
    pub skipped: Vec<String>,
}

/// Collect a pack folder's bundle-eligible files.
///
/// # Errors
/// I/O failure.
pub fn read_pack_dir(dir: &Path, limits: &Limits) -> Result<PackFiles> {
    let mut out = PackFiles::default();
    walk(dir, "", limits, &mut out)?;
    out.files.sort();
    Ok(out)
}

fn walk(dir: &Path, rel: &str, limits: &Limits, out: &mut PackFiles) -> Result<()> {
    for path in read_dir_sorted(dir)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let child = if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{rel}/{name}")
        };
        if is_excluded(&child) || !is_safe_rel_path(&child, limits.max_depth) {
            out.skipped.push(child);
        } else if is_real_dir(&path) {
            walk(&path, &child, limits, out)?;
        } else if is_real_file(&path) {
            out.edited_at_ms = out.edited_at_ms.max(mtime_ms(&path));
            out.files.push((child, path));
        } else {
            out.skipped.push(child);
        }
    }
    Ok(())
}

/// User pack folders (those with a `pack.json`) in `library`, as `(key, path)`.
///
/// # Errors
/// I/O failure.
pub fn list_packs(library: &Path) -> Result<Vec<(String, PathBuf)>> {
    if !library.is_dir() {
        return Ok(Vec::new());
    }
    Ok(read_dir_sorted(library)?
        .into_iter()
        .filter(|p| is_real_dir(p) && is_real_file(&p.join("pack.json")))
        .filter_map(|p| {
            let key = p.file_name()?.to_str()?.to_owned();
            (is_safe_segment(&key) && !is_excluded(&key)).then_some((key, p))
        })
        .collect())
}

/// Digests of every user pack in `library`, computed over the same file set a bundle would carry.
///
/// # Errors
/// I/O failure.
pub fn scan_packs(library: &Path, limits: &Limits) -> Result<Vec<LocalItem>> {
    list_packs(library)?
        .into_iter()
        .map(|(key, dir)| {
            let pack = read_pack_dir(&dir, limits)?;
            let entries = pack
                .files
                .iter()
                .map(|(rel, path)| {
                    let bytes = fs::read(path).map_err(|e| BundleError::io_at(path, e))?;
                    Ok(FileEntry {
                        path: format!("packs/{key}/{rel}"),
                        sha256: sha256_hex(&bytes),
                        size: bytes.len() as u64,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LocalItem {
                kind: ItemKind::Pack,
                key,
                sha256: item_digest(&entries),
                edited_at_ms: pack.edited_at_ms,
            })
        })
        .collect()
}

/// Which pack defines which element id, to work out the packs a character needs.
#[derive(Debug, Clone, Default)]
pub struct PackIndex {
    by_element: BTreeMap<String, BTreeSet<PackRef>>,
}

impl PackIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pack and the element ids it defines. The app registers bundled packs with
    /// `bundled = true` so characters that use them list them (they are never shipped).
    pub fn add_pack<I: IntoIterator<Item = String>>(&mut self, id: &str, bundled: bool, ids: I) {
        for element in ids {
            self.by_element.entry(element).or_default().insert(PackRef {
                id: id.to_owned(),
                bundled,
            });
        }
    }

    /// Index every user pack in `library` by reading the `"id"` strings of its `.json` files.
    ///
    /// # Errors
    /// I/O failure. Unparseable JSON files are skipped (the pack validator owns that complaint).
    pub fn add_user_library(&mut self, library: &Path, limits: &Limits) -> Result<()> {
        for (key, dir) in list_packs(library)? {
            let pack = read_pack_dir(&dir, limits)?;
            let mut ids = Vec::new();
            for (rel, path) in &pack.files {
                if !rel.ends_with(".json") {
                    continue;
                }
                let bytes = fs::read(path).map_err(|e| BundleError::io_at(path, e))?;
                if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                    collect_ids(&v, &mut ids);
                }
            }
            self.add_pack(&key, false, ids);
        }
        Ok(())
    }

    /// Packs a character document refers to: every pack defining an element id that appears as a
    /// string anywhere in the document. Sorted, without duplicates.
    #[must_use]
    pub fn required_for(&self, character: &Value) -> Vec<PackRef> {
        let mut found = BTreeSet::new();
        visit_strings(character, &mut |s| {
            if let Some(packs) = self.by_element.get(s) {
                found.extend(packs.iter().cloned());
            }
        });
        found.into_iter().collect()
    }
}

fn collect_ids(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                match (k.as_str(), child) {
                    ("id", Value::String(s)) => out.push(s.clone()),
                    _ => collect_ids(child, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|c| collect_ids(c, out)),
        _ => {}
    }
}

fn visit_strings(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(items) => items.iter().for_each(|c| visit_strings(c, f)),
        Value::Object(map) => map.values().for_each(|c| visit_strings(c, f)),
        _ => {}
    }
}
