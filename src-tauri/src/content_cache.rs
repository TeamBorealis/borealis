//! PF1 (B-235): the startup content cache. After a full load the parsed packs (the
//! [`ContentStoreBuilder`] before IM14 editions are applied, plus the user-pack load warnings) are saved
//! as one binary file; the next load reuses it while nothing that feeds the load has changed.
//!
//! The key (compared byte for byte, never hashed) holds the cache format, the app version and the
//! running build's own file stamp, the ordered pack folders with every file below each (relative path,
//! size, modification time, and on Unix the change time and inode), the disabled-pack set and the
//! per-book editions. Any difference, a missing or unreadable file, a bad checksum or a decode error
//! means a full load, which then writes a fresh cache. The file lives in `<data>/cache/` (excluded
//! from backups and bundles), is written to a temporary file and renamed into place.

use borealis_content::{ContentStoreBuilder, Ruleset};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, UNIX_EPOCH};

/// Folder of the app data folder holding derived caches (never backed up or bundled).
pub const CACHE_DIR: &str = "cache";
/// The content cache file inside [`CACHE_DIR`].
pub const CACHE_FILE: &str = "content.bin";
/// Bump when the file layout or the meaning of the snapshot changes.
pub const CACHE_FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"BRLSCC\0\x01";
/// Folder depth walked below a pack folder for the key.
const MAX_DEPTH: usize = 8;

/// Where the content cache of a data folder lives.
#[must_use]
pub fn cache_file(data: &Path) -> PathBuf {
    data.join(CACHE_DIR).join(CACHE_FILE)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FileStamp {
    path: String,
    len: u64,
    modified_ns: Option<i128>,
    changed_ns: Option<i128>,
    inode: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct PackStamp {
    custom: bool,
    dir: String,
    files: Vec<FileStamp>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Key<'a> {
    format: u32,
    app_version: &'static str,
    build: Option<FileStamp>,
    packs: Vec<PackStamp>,
    disabled: &'a BTreeSet<String>,
    source_editions: &'a BTreeMap<String, Ruleset>,
}

/// What a load is keyed on, as bytes. Build with [`CacheKey::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey(Vec<u8>);

impl CacheKey {
    /// The key for loading `bundled` then `custom` pack folders (in that order) with these preferences.
    #[must_use]
    pub fn new(
        bundled: &[PathBuf],
        custom: &[PathBuf],
        disabled: &BTreeSet<String>,
        source_editions: &BTreeMap<String, Ruleset>,
    ) -> Self {
        let packs = bundled
            .iter()
            .map(|d| (false, d))
            .chain(custom.iter().map(|d| (true, d)))
            .map(|(custom, dir)| PackStamp {
                custom,
                dir: dir.to_string_lossy().into_owned(),
                files: stamp_tree(dir),
            })
            .collect();
        let key = Key {
            format: CACHE_FORMAT,
            app_version: env!("CARGO_PKG_VERSION"),
            build: build_stamp(),
            packs,
            disabled,
            source_editions,
        };
        // Encoding plain strings, numbers and maps cannot fail; an empty key never matches a file.
        Self(rmp_serde::to_vec_named(&key).unwrap_or_default())
    }
}

fn nanos(t: std::io::Result<std::time::SystemTime>) -> Option<i128> {
    let t = t.ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()).ok()?,
        Err(e) => -i128::try_from(e.duration().as_nanos()).ok()?,
    })
}

fn stamp(rel: String, meta: &std::fs::Metadata) -> FileStamp {
    #[cfg(unix)]
    let (changed_ns, inode) = {
        use std::os::unix::fs::MetadataExt;
        (
            Some(i128::from(meta.ctime()) * 1_000_000_000 + i128::from(meta.ctime_nsec())),
            Some(meta.ino()),
        )
    };
    #[cfg(not(unix))]
    let (changed_ns, inode) = (nanos(meta.created()), None);
    FileStamp {
        path: rel,
        len: meta.len(),
        modified_ns: nanos(meta.modified()),
        changed_ns,
        inode,
    }
}

/// Every file below `dir` (symlinks followed, as the loader reads them), sorted by relative path.
/// A folder that cannot be read is stamped as such, so it never matches a cache made when it could.
fn stamp_tree(dir: &Path) -> Vec<FileStamp> {
    let mut out = Vec::new();
    walk(dir, "", 0, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<FileStamp>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        out.push(FileStamp {
            path: format!("{rel}<unreadable>"),
            len: 0,
            modified_ns: None,
            changed_ns: None,
            inode: None,
        });
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = format!("{rel}{}", entry.file_name().to_string_lossy());
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, &format!("{name}/"), depth + 1, out);
            }
        } else {
            out.push(stamp(name, &meta));
        }
    }
}

/// The running build's own file: a rebuilt or updated app never reads a cache an older build wrote,
/// even under the same version number. Desktop and iOS: the executable. Android: the app's native
/// library (its install folder changes on every install and update).
fn build_stamp() -> Option<FileStamp> {
    let path = build_file()?;
    let meta = std::fs::metadata(&path).ok()?;
    Some(stamp(path.to_string_lossy().into_owned(), &meta))
}

#[cfg(not(target_os = "android"))]
fn build_file() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(target_os = "android")]
fn build_file() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .find(|p| p.contains("libborealis_lib.so"))
        .map(|p| PathBuf::from(p.split("!/").next().unwrap_or(p)))
}

/// A loaded cache: the builder before editions and the user-pack warnings of the load that wrote it.
#[derive(Debug)]
pub struct Cached {
    /// Every enabled pack, in load order.
    pub builder: ContentStoreBuilder,
    /// Problems found while loading user packs.
    pub load_warnings: Vec<String>,
}

/// Why a cache was not used (logged).
#[derive(Debug, PartialEq, Eq)]
pub enum Miss {
    /// No cache file (first launch, cleared, or unreadable).
    Absent,
    /// The file is not a complete cache of this format (truncated, corrupt, older format).
    Corrupt(&'static str),
    /// The content, preferences or app changed since it was written.
    Stale,
    /// The snapshot did not decode (the decoder's message is not logged: it can quote content).
    Undecodable,
}

impl Miss {
    /// A short reason for the log.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::Absent => "no cache".into(),
            Self::Corrupt(why) => format!("unusable cache ({why})"),
            Self::Stale => "content or settings changed".into(),
            Self::Undecodable => "undecodable cache".into(),
        }
    }
}

fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    if bytes.len() < n {
        return None;
    }
    let (head, rest) = bytes.split_at(n);
    *bytes = rest;
    Some(head)
}

fn take_part<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    let len = u64::from_le_bytes(take(bytes, 8)?.try_into().ok()?);
    take(bytes, usize::try_from(len).ok()?)
}

/// Read the cache at `file` if it was written for `key`.
///
/// # Errors
/// [`Miss`] saying why it cannot be used; the caller then loads from the files.
pub fn load(file: &Path, key: &CacheKey) -> Result<Cached, Miss> {
    let bytes = std::fs::read(file).map_err(|_| Miss::Absent)?;
    let (body, sum) = bytes
        .split_last_chunk::<4>()
        .ok_or(Miss::Corrupt("too short"))?;
    if crc32fast::hash(body) != u32::from_le_bytes(*sum) {
        return Err(Miss::Corrupt("checksum"));
    }
    let mut rest = body;
    if take(&mut rest, MAGIC.len()) != Some(MAGIC.as_slice()) {
        return Err(Miss::Corrupt("not a content cache"));
    }
    let format = take(&mut rest, 4).and_then(|b| b.try_into().ok());
    if format.map(u32::from_le_bytes) != Some(CACHE_FORMAT) {
        return Err(Miss::Corrupt("other format"));
    }
    let stored_key = take_part(&mut rest).ok_or(Miss::Corrupt("truncated"))?;
    if stored_key != key.0.as_slice() || key.0.is_empty() {
        return Err(Miss::Stale);
    }
    let warnings = take_part(&mut rest).ok_or(Miss::Corrupt("truncated"))?;
    let snapshot = take_part(&mut rest).ok_or(Miss::Corrupt("truncated"))?;
    if !rest.is_empty() {
        return Err(Miss::Corrupt("trailing bytes"));
    }
    let load_warnings: Vec<String> =
        rmp_serde::from_slice(warnings).map_err(|_| Miss::Undecodable)?;
    let builder = ContentStoreBuilder::from_snapshot(snapshot).map_err(|_| Miss::Undecodable)?;
    Ok(Cached {
        builder,
        load_warnings,
    })
}

fn push_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

/// Encode a cache for `key`.
///
/// # Errors
/// The snapshot encoding error, as text.
pub fn encode(
    key: &CacheKey,
    builder: &ContentStoreBuilder,
    load_warnings: &[String],
) -> Result<Vec<u8>, String> {
    let snapshot = builder.to_snapshot().map_err(|e| e.to_string())?;
    let warnings = rmp_serde::to_vec(load_warnings).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(snapshot.len() + key.0.len() + warnings.len() + 64);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CACHE_FORMAT.to_le_bytes());
    push_part(&mut out, &key.0);
    push_part(&mut out, &warnings);
    push_part(&mut out, &snapshot);
    let sum = crc32fast::hash(&out);
    out.extend_from_slice(&sum.to_le_bytes());
    Ok(out)
}

static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);
const TEMP_PREFIX: &str = "content.bin.tmp-";

/// Temporary files another process left (the app quit while writing); this process's own are kept.
fn remove_abandoned_temps(dir: &Path) {
    let mine = format!("{TEMP_PREFIX}{}-", std::process::id());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(TEMP_PREFIX) && !name.starts_with(&mine) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Write a cache for `key` to `file`: a temporary file in the same folder, then a rename.
///
/// # Errors
/// I/O or encoding failures, as text (the cache is optional: callers log and go on).
pub fn save(
    file: &Path,
    key: &CacheKey,
    builder: &ContentStoreBuilder,
    load_warnings: &[String],
) -> Result<usize, String> {
    let bytes = encode(key, builder, load_warnings)?;
    let dir = file.parent().ok_or("no cache folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    remove_abandoned_temps(dir);
    let temp = dir.join(format!(
        "{TEMP_PREFIX}{}-{}",
        std::process::id(),
        TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::write(&temp, &bytes).and_then(|()| std::fs::rename(&temp, file));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e.to_string());
    }
    Ok(bytes.len())
}

/// Remove the cache (Settings > Clear caches). A missing file is fine.
pub fn remove(data: &Path) {
    let file = cache_file(data);
    if let Err(e) = std::fs::remove_file(&file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("content cache: not removed: {e}");
        }
    }
}

/// [`save`] on a background thread, logging the outcome; the returned handle joins it.
pub fn save_in_background(
    file: PathBuf,
    key: CacheKey,
    builder: std::sync::Arc<ContentStoreBuilder>,
    load_warnings: Vec<String>,
) -> Option<std::thread::JoinHandle<()>> {
    let spawned = std::thread::Builder::new()
        .name("borealis-content-cache".into())
        .spawn(move || {
            let began = Instant::now();
            match save(&file, &key, &builder, &load_warnings) {
                Ok(len) => tracing::info!(
                    "content cache: written {} KB in {} ms",
                    len / 1024,
                    began.elapsed().as_millis()
                ),
                Err(e) => tracing::warn!("content cache: not written: {e}"),
            }
        });
    match spawned {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::warn!("content cache: not written: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, Element, PackOrigin};

    fn tiny_builder() -> ContentStoreBuilder {
        let file = ContentFile::new(vec![Element::new("ID_X", "X", "Feat", "Core")]);
        ContentStoreBuilder::new().add_file("a.json", PackOrigin::Custom, file)
    }

    fn key_for(dir: &Path, disabled: &[&str]) -> CacheKey {
        let disabled: BTreeSet<String> = disabled.iter().map(|s| (*s).to_string()).collect();
        CacheKey::new(&[dir.to_path_buf()], &[], &disabled, &BTreeMap::new())
    }

    #[test]
    fn a_saved_cache_loads_only_for_its_own_key() {
        let dir = tempfile::tempdir().unwrap();
        let pack = dir.path().join("pack");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("a.json"), "{}").unwrap();
        let file = cache_file(dir.path());
        let key = key_for(&pack, &[]);
        assert_eq!(load(&file, &key).unwrap_err(), Miss::Absent);
        save(&file, &key, &tiny_builder(), &["w".into()]).unwrap();
        let cached = load(&file, &key).unwrap();
        assert!(cached.builder.same_content(&tiny_builder()));
        assert_eq!(cached.load_warnings, vec!["w".to_string()]);
        assert_eq!(key_for(&pack, &[]), key, "the key is stable");
        assert_eq!(
            load(&file, &key_for(&pack, &["x"])).unwrap_err(),
            Miss::Stale
        );
        std::fs::write(pack.join("b.json"), "{}").unwrap();
        assert_eq!(load(&file, &key_for(&pack, &[])).unwrap_err(), Miss::Stale);
    }

    #[test]
    fn the_key_carries_the_running_build() {
        assert!(build_stamp().is_some(), "tests run from an executable");
    }

    #[test]
    fn abandoned_temporary_files_are_removed_on_the_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let file = cache_file(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let other_pid = std::process::id().wrapping_add(1);
        let stale = file.with_file_name(format!("{TEMP_PREFIX}{other_pid}-0"));
        std::fs::write(&stale, b"half").unwrap();
        save(&file, &key_for(dir.path(), &[]), &tiny_builder(), &[]).unwrap();
        let left: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec![CACHE_FILE.to_string()]);
    }
}
