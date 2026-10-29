//! Pack management: listing, enabling, removing and hot-reloading content.

use crate::content_dto::{ContentPack, ContentProgress, ContentSource, PackKind};
use crate::content_prefs::ContentPrefs;
use crate::elements::plain_text;
use crate::error::{ApiError, ApiResult};
use crate::state::{user_pack_dirs, AppState, ContentHub, BUNDLED_PACKS, CORE_PACK};
use borealis_content::{ContentFile, PackFileKind, PackManifest};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Name of the per-pack marker file written by Borealis installers.
pub const PACK_META_FILE: &str = "pack-meta.json";
const IMPORT_META_FILE: &str = "import.json";
/// Never listed: a restriction cannot exclude them (Aurora lists neither, see
/// [`borealis_content::UNRESTRICTABLE_SOURCES`]).
const HIDDEN_SOURCES: [&str; 2] = borealis_content::UNRESTRICTABLE_SOURCES;

/// Receives the push events of long content operations (the Tauri layer emits them to the UI).
pub trait EventSink: Send + Sync {
    /// `borealis://content-progress`.
    fn content_progress(&self, progress: &ContentProgress);
    /// `borealis://import_5etools_progress`.
    fn import_progress(&self, progress: &ContentProgress);
    /// `borealis://content-reloaded`.
    fn content_reloaded(&self, reason: &str);
}

/// A sink that drops every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

/// Build a progress payload.
#[must_use]
pub fn progress(phase: &'static str, done: usize, total: usize, message: &str) -> ContentProgress {
    ContentProgress {
        phase,
        done,
        total,
        message: message.to_string(),
    }
}

/// A pack folder on disk.
#[derive(Debug, Clone)]
pub struct InstalledPack {
    pub dir: PathBuf,
    pub manifest: PackManifest,
    pub bundled: bool,
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn kind_of(pack: &InstalledPack) -> (PackKind, Option<bool>) {
    if pack.bundled {
        return (PackKind::Bundled, None);
    }
    if read_json(&pack.dir.join(IMPORT_META_FILE)).is_some_and(|v| v["origin"] == "5etools-import")
    {
        return (PackKind::LocalImport, Some(false));
    }
    let user_import = read_json(&pack.dir.join(IMPORT_META_FILE))
        .is_some_and(|v| v["origin"] == "user-import")
        .then_some(false);
    let kind = read_json(&pack.dir.join(PACK_META_FILE))
        .and_then(|v| v["kind"].as_str().map(str::to_string))
        .unwrap_or_default();
    let kind = match kind.as_str() {
        "downloaded" => PackKind::Downloaded,
        "homebrew" => PackKind::Homebrew,
        _ => PackKind::Imported,
    };
    (kind, user_import)
}

/// Counts and sources of a pack that is not loaded (read from its files).
fn disk_stats(pack: &InstalledPack) -> (usize, BTreeSet<String>) {
    let mut count = 0;
    let mut sources = BTreeSet::new();
    for f in pack
        .manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content)
    {
        let Ok(text) = std::fs::read_to_string(pack.dir.join(&f.path)) else {
            continue;
        };
        if let Ok(file) = ContentFile::from_json_str(&text) {
            count += file.elements.len();
            sources.extend(file.elements.iter().map(|e| e.source.clone()));
        }
    }
    (count, sources)
}

impl AppState {
    /// Every pack folder: bundled first (in load order), then the user library.
    #[must_use]
    pub fn installed_packs(&self) -> Vec<InstalledPack> {
        let bundled = BUNDLED_PACKS
            .iter()
            .map(|n| (self.paths.content_root.join(n), true));
        let user = user_pack_dirs(&self.paths.library())
            .into_iter()
            .map(|d| (d, false));
        bundled
            .chain(user)
            .filter_map(|(dir, bundled)| {
                let text = std::fs::read_to_string(dir.join("pack.json")).ok()?;
                let manifest = PackManifest::from_json_str(&text).ok()?;
                Some(InstalledPack {
                    dir,
                    manifest,
                    bundled,
                })
            })
            .collect()
    }

    /// Persisted content preferences.
    #[must_use]
    pub fn content_prefs(&self) -> ContentPrefs {
        ContentPrefs::load(&self.paths.prefs_file())
    }

    /// All installed packs with their enabled state.
    ///
    /// # Errors
    /// `internal` if a lock is poisoned.
    pub fn list_packs(&self) -> ApiResult<Vec<ContentPack>> {
        let hub = self.hub()?;
        let prefs = self.content_prefs();
        let mut loaded: BTreeMap<&str, (usize, BTreeSet<String>)> = BTreeMap::new();
        for stored in hub.store.iter() {
            let pack = stored.file.split('/').next().unwrap_or_default();
            let entry = loaded.entry(pack).or_default();
            entry.0 += 1;
            entry.1.insert(stored.element.source.clone());
        }
        Ok(self
            .installed_packs()
            .iter()
            .map(|p| {
                let id = &p.manifest.id;
                let enabled = id == CORE_PACK || !prefs.disabled_packs.contains(id);
                let (element_count, sources) = match loaded.get(id.as_str()) {
                    Some((n, s)) if enabled => (*n, s.clone()),
                    _ => disk_stats(p),
                };
                let (kind, redistributable) = kind_of(p);
                ContentPack {
                    id: id.clone(),
                    name: p.manifest.name.clone(),
                    version: p.manifest.version.clone(),
                    kind,
                    rulesets: p.manifest.rulesets.clone(),
                    element_count,
                    enabled,
                    license: p.manifest.license.clone().unwrap_or_default(),
                    attribution: p.manifest.attribution.clone().unwrap_or_default(),
                    source_ids: sources.into_iter().collect(),
                    redistributable,
                }
            })
            .collect())
    }

    /// Content sources (books) of the enabled packs.
    ///
    /// # Errors
    /// `internal` if a lock is poisoned.
    pub fn list_sources(&self) -> ApiResult<Vec<ContentSource>> {
        let hub = self.hub()?;
        let names: BTreeMap<String, String> = self
            .installed_packs()
            .into_iter()
            .map(|p| (p.manifest.id, p.manifest.name))
            .collect();
        let mut out = Vec::new();
        for source in hub.store.source_names() {
            if HIDDEN_SOURCES.contains(&source) {
                continue;
            }
            let elements: Vec<_> = hub.store.by_source(source).collect();
            let pack_id = elements
                .first()
                .and_then(|e| e.file.split('/').next())
                .unwrap_or_default()
                .to_string();
            let description = elements
                .iter()
                .find(|e| e.element.ty == "Source")
                .map(|e| plain_text(&e.element.description))
                .unwrap_or_default();
            let flags = source_flags(elements.iter().map(|e| &e.element));
            out.push(ContentSource {
                id: source.to_string(),
                name: source.to_string(),
                group: names.get(&pack_id).cloned().unwrap_or_default(),
                pack_id,
                element_count: elements.len(),
                description,
                playtest: flags.playtest,
                incomplete: flags.incomplete,
                info: flags.info,
            });
        }
        Ok(out)
    }

    /// Load a fresh content snapshot for `prefs`, persist them, and swap it in (hot reload).
    /// Nothing changes if the content fails to load.
    ///
    /// # Errors
    /// `content_error` / `io_error`.
    pub fn reload_content(
        &self,
        prefs: &ContentPrefs,
        sink: &dyn EventSink,
        reason: &str,
    ) -> ApiResult<()> {
        let settings = self.settings()?;
        let hub = ContentHub::load_with(&self.paths, &settings, &prefs.disabled_packs)?;
        prefs.save(&self.paths.prefs_file())?;
        self.set_hub(hub)?;
        // Characters imported earlier pick up newly available content without a re-import.
        if let Err(e) = self.recheck_missing_content() {
            tracing::warn!("re-checking imported characters failed: {e}");
        }
        sink.content_reloaded(reason);
        Ok(())
    }

    /// Switch a pack on or off and hot-reload.
    ///
    /// # Errors
    /// `not_found` for an unknown pack, `requirement_failed` for the core pack.
    pub fn set_pack_enabled(
        &self,
        pack_id: &str,
        enabled: bool,
        sink: &dyn EventSink,
    ) -> ApiResult<Vec<ContentPack>> {
        let _guard = self.content_lock()?;
        if !self
            .installed_packs()
            .iter()
            .any(|p| p.manifest.id == pack_id)
        {
            return Err(ApiError::not_found("That content pack is not installed."));
        }
        if pack_id == CORE_PACK && !enabled {
            return Err(ApiError::requirement(
                "The core pack is required and cannot be turned off.",
            ));
        }
        let mut prefs = self.content_prefs();
        if enabled {
            prefs.disabled_packs.remove(pack_id);
        } else {
            prefs.disabled_packs.insert(pack_id.to_string());
        }
        self.reload_content(&prefs, sink, "pack-enabled")?;
        self.list_packs()
    }

    /// Delete a user pack and hot-reload.
    ///
    /// # Errors
    /// `not_found` for an unknown pack, `requirement_failed` for bundled packs.
    pub fn remove_pack(&self, pack_id: &str, sink: &dyn EventSink) -> ApiResult<Vec<ContentPack>> {
        let _guard = self.content_lock()?;
        let pack = self
            .installed_packs()
            .into_iter()
            .find(|p| p.manifest.id == pack_id)
            .ok_or_else(|| ApiError::not_found("That content pack is not installed."))?;
        if pack.bundled {
            return Err(ApiError::requirement(
                "Bundled packs cannot be removed; turn them off instead.",
            ));
        }
        std::fs::remove_dir_all(&pack.dir).map_err(|e| ApiError::io("remove the pack", e))?;
        let mut prefs = self.content_prefs();
        prefs.disabled_packs.remove(pack_id);
        self.reload_content(&prefs, sink, "pack-removed")?;
        self.list_packs()
    }

    /// Delete every pack installed from an update index and hot-reload.
    ///
    /// # Errors
    /// `io_error` if a pack cannot be deleted.
    pub fn clear_downloaded_content(&self, sink: &dyn EventSink) -> ApiResult<Vec<ContentPack>> {
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        for pack in self.installed_packs().iter().filter(|p| !p.bundled) {
            if kind_of(pack).0 == PackKind::Downloaded {
                std::fs::remove_dir_all(&pack.dir)
                    .map_err(|e| ApiError::io("remove downloaded content", e))?;
                prefs.disabled_packs.remove(&pack.manifest.id);
            }
        }
        for index in &mut prefs.indexes {
            index.updates_available = index.file_count;
        }
        self.reload_content(&prefs, sink, "downloads-cleared")?;
        self.list_packs()
    }
}

/// The content-warning setters of a source's `Source` element.
#[derive(Debug, Default, PartialEq, Eq)]
struct SourceFlags {
    playtest: bool,
    incomplete: bool,
    info: String,
}

/// Reads `playtest`/`beta`, `incomplete`/`wip` and `information` from the first `Source` element.
fn source_flags<'a>(elements: impl Iterator<Item = &'a borealis_content::Element>) -> SourceFlags {
    let Some(src) = elements.into_iter().find(|e| e.ty == "Source") else {
        return SourceFlags::default();
    };
    let flag = |names: &[&str]| {
        names
            .iter()
            .any(|n| src.setter(n).and_then(|s| s.as_bool()) == Some(true))
    };
    SourceFlags {
        playtest: flag(&["playtest", "beta"]),
        incomplete: flag(&["incomplete", "wip"]),
        info: src
            .setter_value("information")
            .map(|v| v.trim().to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod source_flag_tests {
    use super::*;
    use borealis_content::{Element, Setter};

    fn source(setters: &[(&str, &str)]) -> Element {
        let mut e = Element::new(
            "ID_SYN_SOURCE_X",
            "Synthetic Source",
            "Source",
            "Synthetic Source",
        );
        for (k, v) in setters {
            e.setters.push(Setter::new(*k, *v));
        }
        e
    }

    #[test]
    fn flags_come_from_the_source_element_setters() {
        let e = source(&[
            ("wip", "true"),
            ("beta", "true"),
            ("information", " Draft. "),
        ]);
        let f = source_flags(std::iter::once(&e));
        assert_eq!(
            f,
            SourceFlags {
                playtest: true,
                incomplete: true,
                info: "Draft.".into()
            }
        );
    }

    #[test]
    fn a_plain_source_has_no_flags() {
        let e = source(&[("incomplete", "false")]);
        assert_eq!(source_flags(std::iter::once(&e)), SourceFlags::default());
        assert_eq!(source_flags(std::iter::empty()), SourceFlags::default());
    }
}
