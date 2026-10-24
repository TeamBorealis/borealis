//! Bundle writer. Every add validates keys, paths, exclusions and caps, so a bundle that cannot be
//! read back is never produced.

use crate::error::{BundleError, Result};
use crate::exclude::{is_excluded, is_portable_only, is_safe_rel_path, is_safe_segment};
use crate::limits::Limits;
use crate::manifest::{
    item_digest, sha256_hex, FileEntry, Item, ItemKind, Manifest, PackRef, FORMAT, MANIFEST_PATH,
    SETTINGS_PATH,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Seek, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Who made the bundle and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleMeta {
    /// Borealis version.
    pub app_version: String,
    /// Display name of the sending device.
    pub source_device: String,
    /// Creation time, Unix milliseconds.
    pub created_ms: u64,
}

/// Streams items into a zip and finishes with the manifest.
pub struct BundleWriter<W: Write + Seek> {
    zip: ZipWriter<W>,
    meta: BundleMeta,
    limits: Limits,
    items: Vec<Item>,
    settings: Option<FileEntry>,
    seen: BTreeSet<(ItemKind, String)>,
    total_bytes: u64,
    entries: usize,
}

impl<W: Write + Seek> BundleWriter<W> {
    /// Start a bundle with the default [`Limits`].
    #[must_use]
    pub fn new(out: W, meta: BundleMeta) -> Self {
        Self::with_limits(out, meta, Limits::default())
    }

    /// Start a bundle with explicit limits.
    #[must_use]
    pub fn with_limits(out: W, meta: BundleMeta, limits: Limits) -> Self {
        Self {
            zip: ZipWriter::new(out),
            meta,
            limits,
            items: Vec::new(),
            settings: None,
            seen: BTreeSet::new(),
            total_bytes: 0,
            entries: 1, // the manifest
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<FileEntry> {
        let size = bytes.len() as u64;
        if size > self.limits.max_file_bytes {
            return Err(BundleError::LimitExceeded(format!(
                "`{path}` is {size} bytes (cap {})",
                self.limits.max_file_bytes
            )));
        }
        self.total_bytes += size;
        self.entries += 1;
        if self.total_bytes > self.limits.max_total_bytes {
            return Err(BundleError::LimitExceeded(format!(
                "bundle exceeds {} bytes",
                self.limits.max_total_bytes
            )));
        }
        if self.entries > self.limits.max_entries {
            return Err(BundleError::LimitExceeded(format!(
                "more than {} files",
                self.limits.max_entries
            )));
        }
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        self.zip.start_file(path, opts)?;
        self.zip.write_all(bytes)?;
        Ok(FileEntry {
            path: path.to_owned(),
            sha256: sha256_hex(bytes),
            size,
        })
    }

    fn push_item(
        &mut self,
        kind: ItemKind,
        key: &str,
        files: Vec<FileEntry>,
        edited_at_ms: u64,
        required_packs: Vec<PackRef>,
    ) -> Result<()> {
        if self.items.len() >= self.limits.max_items {
            return Err(BundleError::LimitExceeded(format!(
                "more than {} items",
                self.limits.max_items
            )));
        }
        let size = files.iter().map(|f| f.size).sum();
        self.items.push(Item {
            kind,
            key: key.to_owned(),
            sha256: item_digest(&files),
            size,
            edited_at_ms,
            files,
            required_packs,
        });
        Ok(())
    }

    fn claim(&mut self, kind: ItemKind, key: &str) -> Result<()> {
        if !is_safe_segment(key) {
            return Err(BundleError::UnsafePath(key.to_owned()));
        }
        if is_excluded(key) {
            return Err(BundleError::Excluded(key.to_owned()));
        }
        if !self.seen.insert((kind, key.to_owned())) {
            return Err(BundleError::Inconsistent(format!("duplicate item `{key}`")));
        }
        Ok(())
    }

    /// Add a character file. `bytes` must be a JSON object. `required_packs` are the packs it needs
    /// (see [`crate::PackIndex::required_for`]).
    ///
    /// # Errors
    /// Unsafe key, non-JSON content, duplicate, or a cap.
    pub fn add_character(
        &mut self,
        key: &str,
        bytes: &[u8],
        edited_at_ms: u64,
        mut required_packs: Vec<PackRef>,
    ) -> Result<()> {
        self.claim(ItemKind::Character, key)?;
        let path = format!("characters/{key}.json");
        match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Object(_)) => {}
            Ok(_) => {
                return Err(BundleError::NotABundle(format!(
                    "`{path}` is not an object"
                )))
            }
            Err(source) => return Err(BundleError::Json { path, source }),
        }
        required_packs.sort();
        required_packs.dedup();
        let file = self.put(&path, bytes)?;
        self.push_item(
            ItemKind::Character,
            key,
            vec![file],
            edited_at_ms,
            required_packs,
        )
    }

    /// Add a user content pack: `files` are `(path relative to the pack folder, bytes)`.
    ///
    /// # Errors
    /// Unsafe or excluded paths, an empty pack, duplicates, or a cap.
    pub fn add_pack(
        &mut self,
        key: &str,
        files: &[(String, Vec<u8>)],
        edited_at_ms: u64,
    ) -> Result<()> {
        self.claim(ItemKind::Pack, key)?;
        if files.is_empty() {
            return Err(BundleError::Inconsistent(format!(
                "pack `{key}` has no files"
            )));
        }
        let mut entries = Vec::with_capacity(files.len());
        let mut rels = BTreeSet::new();
        for (rel, bytes) in files {
            if !is_safe_rel_path(rel, self.limits.max_depth) {
                return Err(BundleError::UnsafePath(format!("{key}/{rel}")));
            }
            if is_excluded(rel) {
                return Err(BundleError::Excluded(format!("{key}/{rel}")));
            }
            if !rels.insert(rel.to_lowercase()) {
                return Err(BundleError::Inconsistent(format!(
                    "duplicate `{key}/{rel}`"
                )));
            }
            entries.push(self.put(&format!("packs/{key}/{rel}"), bytes)?);
        }
        self.push_item(ItemKind::Pack, key, entries, edited_at_ms, Vec::new())
    }

    /// Add `settings.portable.json`. The value must already be the portable subset (use
    /// [`crate::portable_settings`]); anything else is refused rather than silently trimmed.
    ///
    /// # Errors
    /// Non-portable keys, or a cap.
    pub fn set_portable_settings(&mut self, settings: &Value) -> Result<()> {
        if !is_portable_only(settings) {
            return Err(BundleError::Excluded(SETTINGS_PATH.to_owned()));
        }
        let bytes = serde_json::to_vec_pretty(settings).map_err(|source| BundleError::Json {
            path: SETTINGS_PATH.to_owned(),
            source,
        })?;
        self.settings = Some(self.put(SETTINGS_PATH, &bytes)?);
        Ok(())
    }

    /// Write the manifest and close the zip. Returns the sink and the manifest written.
    ///
    /// # Errors
    /// Zip or I/O failure.
    pub fn finish(mut self) -> Result<(W, Manifest)> {
        self.items
            .sort_by(|a, b| (a.kind, &a.key).cmp(&(b.kind, &b.key)));
        let manifest = Manifest {
            format: FORMAT.to_owned(),
            app_version: self.meta.app_version.clone(),
            created_ms: self.meta.created_ms,
            source_device: self.meta.source_device.clone(),
            items: std::mem::take(&mut self.items),
            settings: self.settings.take(),
            campaigns: Vec::new(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|source| BundleError::Json {
            path: MANIFEST_PATH.to_owned(),
            source,
        })?;
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        self.zip.start_file(MANIFEST_PATH, opts)?;
        self.zip.write_all(&bytes)?;
        Ok((self.zip.finish()?, manifest))
    }
}
