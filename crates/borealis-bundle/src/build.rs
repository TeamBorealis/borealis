//! Build a bundle straight from the on-disk layout (`<documents>/characters`, `<data>/library`).

use crate::error::{BundleError, Result};
use crate::exclude::portable_settings;
use crate::inventory::{list_characters, list_packs, mtime_ms, read_pack_dir, PackIndex};
use crate::limits::Limits;
use crate::manifest::{ItemKind, Manifest};
use crate::writer::{BundleMeta, BundleWriter};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

/// Which characters to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterSelection {
    /// Every readable character file.
    All,
    /// These keys; a missing or unreadable one is an error.
    Keys(Vec<String>),
}

/// Which user packs to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackSelection {
    /// Only the user packs the selected characters need (SY2: packs must travel with them).
    Required,
    /// Every user pack.
    All,
    /// No packs.
    None,
}

/// Inputs for [`build_bundle`].
#[derive(Debug, Clone)]
pub struct BuildRequest<'a> {
    /// `<documents>/characters`.
    pub characters_dir: &'a Path,
    /// `<data>/library`.
    pub library_dir: &'a Path,
    /// `<data>/settings.json`; its portable subset is bundled. `None` = no settings.
    pub settings_file: Option<&'a Path>,
    /// Header data.
    pub meta: BundleMeta,
    /// Characters to send.
    pub characters: CharacterSelection,
    /// Packs to send.
    pub packs: PackSelection,
    /// Index of bundled packs (so `required_packs` can name them); user packs are indexed here.
    pub bundled_index: PackIndex,
    /// Caps.
    pub limits: Limits,
}

/// What was left out and why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildReport {
    /// Character files skipped because they were not valid JSON objects (`All` selection only).
    pub skipped_characters: Vec<String>,
    /// Pack files skipped by the exclusion list or for unsafe names, as `pack/path`.
    pub skipped_pack_files: Vec<String>,
}

fn read_character(path: &Path) -> Result<(Vec<u8>, Value)> {
    let bytes = fs::read(path).map_err(|e| BundleError::io_at(path, e))?;
    let value = serde_json::from_slice(&bytes).map_err(|source| BundleError::Json {
        path: path.display().to_string(),
        source,
    })?;
    Ok((bytes, value))
}

/// Write a bundle for the requested characters and packs into `out`.
///
/// # Errors
/// Unknown/unreadable selected characters, I/O failure, or a cap.
pub fn build_bundle<W: Write + Seek>(
    out: W,
    req: &BuildRequest<'_>,
) -> Result<(W, Manifest, BuildReport)> {
    let mut report = BuildReport::default();
    let mut index = req.bundled_index.clone();
    index.add_user_library(req.library_dir, &req.limits)?;
    let mut writer = BundleWriter::with_limits(out, req.meta.clone(), req.limits);
    let mut wanted_packs: BTreeSet<String> = BTreeSet::new();

    let available = list_characters(req.characters_dir)?;
    let chosen: Vec<_> = match &req.characters {
        CharacterSelection::All => available,
        CharacterSelection::Keys(keys) => keys
            .iter()
            .map(|k| {
                available
                    .iter()
                    .find(|c| &c.key == k)
                    .cloned()
                    .ok_or_else(|| BundleError::Inconsistent(format!("no character `{k}`")))
            })
            .collect::<Result<_>>()?,
    };
    let strict = matches!(req.characters, CharacterSelection::Keys(_));
    for c in chosen {
        let (bytes, value) = match read_character(&c.path) {
            Ok(ok) => ok,
            Err(_) if !strict => {
                report.skipped_characters.push(c.key);
                continue;
            }
            Err(e) => return Err(e),
        };
        let required = index.required_for(&value);
        wanted_packs.extend(required.iter().filter(|p| !p.bundled).map(|p| p.id.clone()));
        writer.add_character(&c.key, &bytes, mtime_ms(&c.path), required)?;
    }

    for (key, dir) in list_packs(req.library_dir)? {
        let include = match req.packs {
            PackSelection::All => true,
            PackSelection::Required => wanted_packs.contains(&key),
            PackSelection::None => false,
        };
        if !include {
            continue;
        }
        let pack = read_pack_dir(&dir, &req.limits)?;
        report
            .skipped_pack_files
            .extend(pack.skipped.iter().map(|s| format!("{key}/{s}")));
        let files = pack
            .files
            .iter()
            .map(|(rel, path)| {
                fs::read(path)
                    .map(|b| (rel.clone(), b))
                    .map_err(|e| BundleError::io_at(path, e))
            })
            .collect::<Result<Vec<_>>>()?;
        writer.add_pack(&key, &files, pack.edited_at_ms)?;
    }

    if let Some(settings) = req.settings_file {
        if settings.is_file() {
            let bytes = fs::read(settings).map_err(|e| BundleError::io_at(settings, e))?;
            let full: Value =
                serde_json::from_slice(&bytes).map_err(|source| BundleError::Json {
                    path: settings.display().to_string(),
                    source,
                })?;
            writer.set_portable_settings(&portable_settings(&full))?;
        }
    }
    let (out, manifest) = writer.finish()?;
    Ok((out, manifest, report))
}

/// `<data>/sync/trash`, where overwritten local files go for 30 days.
#[must_use]
pub fn trash_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sync").join("trash")
}

/// Number of days a trashed file is kept.
pub const TRASH_RETENTION_DAYS: u64 = 30;

/// Convenience: item kinds present in a manifest.
#[must_use]
pub fn count_kind(manifest: &Manifest, kind: ItemKind) -> usize {
    manifest.items.iter().filter(|i| i.kind == kind).count()
}
