//! Bundle reader. `open` validates the container and the manifest without extracting anything;
//! file bytes are read (capped, hashed) one item at a time.

use crate::error::{BundleError, Result};
use crate::exclude::{is_excluded, is_portable_only, is_safe_rel_path, is_safe_segment};
use crate::limits::{Limits, RATIO_FLOOR_BYTES};
use crate::manifest::{
    item_digest, sha256_hex, FileEntry, Item, ItemKind, Manifest, FORMAT, FORMAT_PREFIX,
    MANIFEST_PATH, SETTINGS_PATH,
};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::io::{Read, Seek};
use zip::ZipArchive;

/// An opened, structurally validated bundle.
pub struct Bundle<R: Read + Seek> {
    archive: ZipArchive<R>,
    manifest: Manifest,
    limits: Limits,
}

/// The verified bytes of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemData {
    /// `(path inside the zip, bytes)` in manifest order, each checked against its SHA-256.
    pub files: Vec<(String, Vec<u8>)>,
}

impl<R: Read + Seek> Bundle<R> {
    /// Open with the default [`Limits`].
    ///
    /// # Errors
    /// See [`Bundle::open_with`].
    pub fn open(reader: R) -> Result<Self> {
        Self::open_with(reader, Limits::default())
    }

    /// Open a bundle: parse the zip directory, check every entry name and size against the caps,
    /// parse the manifest, refuse unknown format versions, and cross-check the manifest against the
    /// zip (no missing, unlisted, duplicate, excluded or oversized entries). No file body except the
    /// manifest is read here.
    ///
    /// # Errors
    /// Any violation, as a [`BundleError`].
    pub fn open_with(reader: R, limits: Limits) -> Result<Self> {
        let mut archive = ZipArchive::new(reader)
            .map_err(|e| BundleError::NotABundle(format!("not a readable zip ({e})")))?;
        let names = scan_directory(&mut archive, &limits)?;
        let manifest = read_manifest(&mut archive, &limits)?;
        validate_manifest(&manifest, &names, &limits)?;
        Ok(Self {
            archive,
            manifest,
            limits,
        })
    }

    /// The validated manifest.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Read one file, capped at its declared size plus one byte and checked against its SHA-256.
    fn read_file(&mut self, entry: &FileEntry) -> Result<Vec<u8>> {
        let mut file = self.archive.by_name(&entry.path)?;
        if file.size() != entry.size {
            return Err(BundleError::Inconsistent(format!(
                "`{}` is {} bytes in the zip but {} in the manifest",
                entry.path,
                file.size(),
                entry.size
            )));
        }
        let mut buf = Vec::with_capacity(usize::try_from(entry.size).unwrap_or(0));
        // Never trust the declared size: stop one byte past it.
        (&mut file).take(entry.size + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 != entry.size || sha256_hex(&buf) != entry.sha256 {
            return Err(BundleError::HashMismatch(entry.path.clone()));
        }
        Ok(buf)
    }

    /// Read and verify every file of an item, and re-derive the item digest.
    ///
    /// # Errors
    /// Unknown item, hash mismatch, or invalid character JSON.
    pub fn read_item(&mut self, kind: ItemKind, key: &str) -> Result<ItemData> {
        let item = self
            .manifest
            .items
            .iter()
            .find(|i| i.kind == kind && i.key == key)
            .cloned()
            .ok_or_else(|| BundleError::Inconsistent(format!("no such item `{key}`")))?;
        let mut files = Vec::with_capacity(item.files.len());
        for entry in &item.files {
            let bytes = self.read_file(entry)?;
            if kind == ItemKind::Character
                && !matches!(parse_json(&entry.path, &bytes)?, Value::Object(_))
            {
                return Err(BundleError::Inconsistent(format!(
                    "`{}` is not an object",
                    entry.path
                )));
            }
            files.push((entry.path.clone(), bytes));
        }
        if item_digest(&item.files) != item.sha256 {
            return Err(BundleError::HashMismatch(format!("item `{key}`")));
        }
        Ok(ItemData { files })
    }

    /// Read the portable settings, if the bundle has them.
    ///
    /// # Errors
    /// Hash mismatch, invalid JSON, or a non-portable key.
    pub fn read_settings(&mut self) -> Result<Option<Value>> {
        let Some(entry) = self.manifest.settings.clone() else {
            return Ok(None);
        };
        let bytes = self.read_file(&entry)?;
        let value = parse_json(&entry.path, &bytes)?;
        if !is_portable_only(&value) {
            return Err(BundleError::Excluded(SETTINGS_PATH.to_owned()));
        }
        Ok(Some(value))
    }

    /// Read and check everything (all items and the settings). Total bytes are bounded by the
    /// limits already enforced at open.
    ///
    /// # Errors
    /// The first failure found.
    pub fn verify_all(&mut self) -> Result<()> {
        let items: Vec<(ItemKind, String)> = self
            .manifest
            .items
            .iter()
            .map(|i| (i.kind, i.key.clone()))
            .collect();
        for (kind, key) in items {
            self.read_item(kind, &key)?;
        }
        self.read_settings()?;
        Ok(())
    }

    /// The limits this bundle was opened with.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }
}

fn parse_json(path: &str, bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|source| BundleError::Json {
        path: path.to_owned(),
        source,
    })
}

/// Walk the zip directory: names, sizes, ratios, symlinks. Returns the set of file entry names.
fn scan_directory<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: &Limits,
) -> Result<HashSet<String>> {
    if archive.len() > limits.max_entries {
        return Err(BundleError::LimitExceeded(format!(
            "{} entries (cap {})",
            archive.len(),
            limits.max_entries
        )));
    }
    let mut names = HashSet::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i)?;
        // zip 9 decodes the name per entry (UTF-8 flag or CP437) and fails on undecodable bytes.
        let name = file
            .name()
            .map_err(|_| BundleError::UnsafePath(format!("entry {i} (undecodable name)")))?
            .into_owned();
        check_entry_name(&name)?;
        if file.is_dir() {
            continue;
        }
        if file.is_symlink() || file.encrypted() {
            return Err(BundleError::UnsafePath(format!(
                "{name} (symlink or encrypted)"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(BundleError::Inconsistent(format!(
                "duplicate entry `{name}`"
            )));
        }
        let size = file.size();
        let max = if name == MANIFEST_PATH {
            limits.max_manifest_bytes
        } else {
            limits.max_file_bytes
        };
        if size > max {
            return Err(BundleError::LimitExceeded(format!(
                "`{name}` is {size} bytes (cap {max})"
            )));
        }
        let packed = file.compressed_size().max(1);
        if size > RATIO_FLOOR_BYTES && size / packed > limits.max_ratio {
            return Err(BundleError::LimitExceeded(format!(
                "`{name}` expands {}x (cap {}x)",
                size / packed,
                limits.max_ratio
            )));
        }
        total = total.saturating_add(size);
        if total > limits.max_total_bytes {
            return Err(BundleError::LimitExceeded(format!(
                "bundle expands past {} bytes",
                limits.max_total_bytes
            )));
        }
    }
    Ok(names)
}

/// Zip entry names must be plain relative `/` paths: no absolute paths, backslashes, drive letters,
/// NUL, `.`/`..` segments or empty segments.
fn check_entry_name(name: &str) -> Result<()> {
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    let bad = trimmed.is_empty()
        || name.contains('\\')
        || name.contains('\0')
        || name.starts_with('/')
        || trimmed.split('/').any(|s| matches!(s, "" | "." | ".."))
        || trimmed.split('/').next().is_some_and(|s| s.contains(':'));
    if bad {
        return Err(BundleError::UnsafePath(name.escape_debug().to_string()));
    }
    Ok(())
}

fn read_manifest<R: Read + Seek>(archive: &mut ZipArchive<R>, limits: &Limits) -> Result<Manifest> {
    let file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|_| BundleError::NotABundle("no manifest.json".into()))?;
    let mut buf = Vec::new();
    file.take(limits.max_manifest_bytes + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > limits.max_manifest_bytes {
        return Err(BundleError::LimitExceeded("manifest.json too large".into()));
    }
    // Check the version before the full parse so a newer layout gets the right message.
    let raw = parse_json(MANIFEST_PATH, &buf)?;
    let format = raw
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| BundleError::NotABundle("manifest has no format".into()))?;
    if !format.starts_with(FORMAT_PREFIX) {
        return Err(BundleError::NotABundle(format!("format `{format}`")));
    }
    if format != FORMAT {
        return Err(BundleError::UnsupportedFormat(format.to_owned()));
    }
    serde_json::from_value(raw).map_err(|source| BundleError::Json {
        path: MANIFEST_PATH.to_owned(),
        source,
    })
}

fn validate_manifest(m: &Manifest, names: &HashSet<String>, limits: &Limits) -> Result<()> {
    if !m.campaigns.is_empty() {
        return Err(BundleError::UnsupportedFeature("campaigns"));
    }
    if m.items.len() > limits.max_items {
        return Err(BundleError::LimitExceeded(format!(
            "{} items",
            m.items.len()
        )));
    }
    let mut listed: BTreeSet<&str> = BTreeSet::new();
    listed.insert(MANIFEST_PATH);
    let mut keys = BTreeSet::new();
    for item in &m.items {
        validate_item(item, limits)?;
        if !keys.insert((item.kind, item.key.as_str())) {
            return Err(BundleError::Inconsistent(format!(
                "duplicate item `{}`",
                item.key
            )));
        }
        for f in &item.files {
            if !listed.insert(f.path.as_str()) {
                return Err(BundleError::Inconsistent(format!(
                    "`{}` listed twice",
                    f.path
                )));
            }
        }
    }
    if let Some(s) = &m.settings {
        if s.path != SETTINGS_PATH {
            return Err(BundleError::UnsafePath(s.path.clone()));
        }
        listed.insert(SETTINGS_PATH);
    }
    for n in names {
        if !listed.contains(n.as_str()) {
            return Err(BundleError::Inconsistent(format!("unlisted entry `{n}`")));
        }
    }
    for l in &listed {
        if !names.contains(*l) {
            return Err(BundleError::Inconsistent(format!(
                "`{l}` is listed but missing"
            )));
        }
    }
    Ok(())
}

fn validate_item(item: &Item, limits: &Limits) -> Result<()> {
    if !is_safe_segment(&item.key) {
        return Err(BundleError::UnsafePath(item.key.clone()));
    }
    if is_excluded(&item.key) {
        return Err(BundleError::Excluded(item.key.clone()));
    }
    let prefix = match item.kind {
        ItemKind::Character => "characters/".to_owned(),
        ItemKind::Pack => format!("packs/{}/", item.key),
    };
    if item.files.is_empty() || (item.kind == ItemKind::Character && item.files.len() != 1) {
        return Err(BundleError::Inconsistent(format!(
            "item `{}` has a bad file list",
            item.key
        )));
    }
    for f in &item.files {
        let rel = f
            .path
            .strip_prefix(&prefix)
            .ok_or_else(|| BundleError::UnsafePath(f.path.clone()))?;
        let ok = match item.kind {
            ItemKind::Character => rel == format!("{}.json", item.key),
            ItemKind::Pack => is_safe_rel_path(rel, limits.max_depth),
        };
        if !ok {
            return Err(BundleError::UnsafePath(f.path.clone()));
        }
        if is_excluded(rel) {
            return Err(BundleError::Excluded(f.path.clone()));
        }
        if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(BundleError::Inconsistent(format!(
                "bad sha256 for `{}`",
                f.path
            )));
        }
    }
    let total: u64 = item.files.iter().map(|f| f.size).sum();
    if total != item.size {
        return Err(BundleError::Inconsistent(format!(
            "item `{}` size does not add up",
            item.key
        )));
    }
    Ok(())
}
