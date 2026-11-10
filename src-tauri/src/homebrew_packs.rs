//! Homebrew creator packs (spec 14.1, 10.3): NEW PACK, pack settings, the element list, delete with its scan of the
//! local characters, and the save step every change goes through ([`AppState::hb_commit`]: check, write, fast
//! reload).

use crate::content_dto::ContentPack;
use crate::content_import::{slug, Scratch};
use crate::content_packs::{creator_editable, EventSink, InstalledPack};
use crate::dto::Ruleset;
use crate::error::{ApiError, ApiResult};
use crate::homebrew_dto::{
    HomebrewCharacterRef, HomebrewDeleteResult, HomebrewElementSummary, HomebrewPackPatch,
    HomebrewPackRequest, HomebrewProblemCounts,
};
use crate::homebrew_files::{read_pack, write_new_pack, write_pack, PackOnDisk, PackWrite};
use crate::state::{AppState, ContentHub};
use borealis_authoring::form::kind;
use borealis_authoring::ids::{is_pack_code, IdGenerator};
use borealis_authoring::pack::{
    bump_patch, content_with, manifest_with, new_content, new_manifest, pack_edition, parent_of,
    source_element, source_of, without_elements, CreatorFile, PackSettings, CONTENT_FILE,
};
use borealis_content::validate::validate_elements;
use borealis_content::{ContentStore, Issue, ParsedPack, Severity};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// The app version written to `creator.json` (`createdWith`).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The `content-reloaded` reason of a save that re-added only its pack (spec 14.1 fast reload).
pub const FAST_RELOAD: &str = "homebrew-saved";
/// The reason of a save to a pack the loaded content does not hold (turned off, or failed to load): everything is
/// read again.
pub const FULL_RELOAD: &str = "homebrew-saved-full-reload";
/// Most initials [`suggest_code`] takes from a pack name.
const SUGGESTED_CODE_MAX: usize = 4;

/// A pack the creator edits: as on disk, with its `creator.json` (made from the content when it has none yet).
#[derive(Debug, Clone)]
pub(crate) struct CreatorPack {
    pub(crate) disk: PackOnDisk,
    pub(crate) creator: CreatorFile,
}

impl CreatorPack {
    pub(crate) fn id(&self) -> &str {
        &self.disk.manifest.id
    }
}

/// Why a save did not happen.
#[derive(Debug)]
pub(crate) enum CommitError {
    /// The content checks found errors on the saved elements (nothing was written).
    Problems(Vec<Issue>),
    /// Anything else.
    Api(ApiError),
}

impl From<ApiError> for CommitError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

/// A code for a pack the creator opens for the first time: up to four initials of its name, made unique with a
/// number, else `HB`.
#[must_use]
pub(crate) fn suggest_code(name: &str, taken: &HashSet<String>) -> String {
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(char::is_ascii_alphanumeric))
        .map(|c| c.to_ascii_uppercase())
        .take(SUGGESTED_CODE_MAX)
        .collect();
    let base = if is_pack_code(&initials) {
        initials
    } else {
        "HB".to_string()
    };
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|c| is_pack_code(c) && !taken.contains(c))
        .unwrap_or(base)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Today as `YYYYMMDD` (the pack's `release`).
fn today() -> String {
    crate::ids::rfc3339(SystemTime::now())
        .get(..10)
        .unwrap_or_default()
        .replace('-', "")
}

fn not_editable(name: &str) -> ApiError {
    ApiError::requirement(format!(
        "\"{name}\" can't be changed in place. Copy its elements to one of your homebrew packs instead."
    ))
}

/// Upper-cased `abbreviation`s of the loaded books, except the ones in `except_pack`.
fn book_codes(store: &ContentStore, except_pack: Option<&str>) -> HashSet<String> {
    store
        .by_type("Source")
        .filter(|s| except_pack.is_none_or(|p| !s.file.starts_with(&format!("{p}/"))))
        .filter_map(|s| s.element.setter_value("abbreviation"))
        .map(|a| a.trim().to_ascii_uppercase())
        .filter(|a| !a.is_empty())
        .collect()
}

impl AppState {
    fn user_pack(&self, pack_id: &str) -> ApiResult<InstalledPack> {
        self.installed_packs()
            .into_iter()
            .find(|p| !p.bundled && p.manifest.id == pack_id)
            .ok_or_else(|| ApiError::not_found("That homebrew pack isn't installed."))
    }

    /// Codes in use: loaded books' abbreviations and other creator packs' codes (`except_pack` excluded).
    fn hb_taken_codes(&self, hub: &ContentHub, except_pack: Option<&str>) -> HashSet<String> {
        let mut taken = book_codes(&hub.store, except_pack);
        for p in self.installed_packs() {
            if p.bundled || Some(p.manifest.id.as_str()) == except_pack {
                continue;
            }
            if let Ok(text) =
                std::fs::read_to_string(p.dir.join(borealis_authoring::pack::CREATOR_FILE))
            {
                if let Ok(c) = CreatorFile::parse(&text) {
                    taken.insert(c.code);
                }
            }
        }
        taken
    }

    /// The pack the creator edits, read from disk. A pack without a readable `creator.json` gets one in memory (its
    /// book's abbreviation as the code when it has one, else one from its name); it is written on the next save.
    ///
    /// # Errors
    /// `not_found` for an unknown pack, `requirement_failed` for a pack that is not the user's own (HC5),
    /// `content_error` when its files can't be read.
    pub(crate) fn hb_pack(&self, pack_id: &str) -> ApiResult<CreatorPack> {
        let installed = self.user_pack(pack_id)?;
        if !creator_editable(&installed) {
            return Err(not_editable(&installed.manifest.name));
        }
        let disk = read_pack(&installed.dir)?;
        let hub = self.hub()?;
        let creator = match &disk.creator {
            Some(c) if is_pack_code(&c.code) => c.reconciled(&disk.content),
            _ => {
                let taken = self.hb_taken_codes(&hub, Some(pack_id));
                let own = source_of(&disk.content)
                    .and_then(|s| s.setter_value("abbreviation"))
                    .map(|a| a.trim().to_ascii_uppercase())
                    .filter(|a| is_pack_code(a) && !taken.contains(a));
                let code = own.unwrap_or_else(|| suggest_code(&disk.manifest.name, &taken));
                CreatorFile::adopt(&code, APP_VERSION, &disk.content)
            }
        };
        Ok(CreatorPack { disk, creator })
    }

    /// The pack as the Content page lists it.
    pub(crate) fn hb_pack_dto(&self, pack_id: &str) -> ApiResult<ContentPack> {
        self.list_packs()?
            .into_iter()
            .find(|p| p.id == pack_id)
            .ok_or_else(|| ApiError::internal("the homebrew pack is missing after a save"))
    }

    /// An id generator for the pack: avoids every loaded id, every id of the pack, and its retired ids.
    pub(crate) fn hb_ids(&self, hub: &ContentHub, pack: &CreatorPack) -> ApiResult<IdGenerator> {
        let loaded = hub
            .store
            .iter()
            .map(|s| s.element.id.as_str())
            .chain(pack.disk.content.elements.iter().map(|e| e.id.as_str()));
        IdGenerator::new(&pack.creator.code, loaded, pack.creator.retired_ids.iter())
            .map_err(|e| ApiError::invalid(e.to_string()))
    }

    /// Check a save against the loaded content, write it, and swap the content in (spec 14.1). The elements
    /// `check_ids` are checked with the content validator in a store that holds the new pack; an error stops the save
    /// before anything is written. The reload re-adds only this pack to the cached builder (fast reload); a pack the
    /// loaded content does not hold (turned off, or failed to load) reloads everything.
    pub(crate) fn hb_commit(
        &self,
        pack: &CreatorPack,
        next: &PackWrite,
        check_ids: &[String],
        sink: &dyn EventSink,
    ) -> Result<(), CommitError> {
        let hub = self.hub()?;
        let parsed = ParsedPack::from_parts(
            next.manifest.clone(),
            vec![(pack.disk.content_path.clone(), next.content.clone())],
        );
        let fast = hub.builder_with_pack(parsed.clone());
        let store = match &fast {
            Some(b) => b.clone().build(),
            None => hub.builder.as_ref().clone().add_parsed_pack(parsed).build(),
        };
        let ids: HashSet<&str> = check_ids.iter().map(String::as_str).collect();
        let errors: Vec<Issue> = validate_elements(&store, &ids)
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(CommitError::Problems(errors));
        }
        write_pack(&pack.disk, next, now_ms())?;
        let prefs = self.content_prefs();
        match fast {
            Some(builder) => {
                let settings = self.settings()?;
                let next_hub =
                    ContentHub::from_built(builder, store, hub.load_warnings.clone(), &settings);
                self.set_hub(next_hub)?;
                if let Err(e) = self.recheck_missing_content() {
                    tracing::warn!("re-checking imported characters failed: {e}");
                }
                sink.content_reloaded(FAST_RELOAD);
                self.hub()?.save_cache(&self.paths, &prefs);
            }
            None => self.reload_content(&prefs, sink, FULL_RELOAD)?,
        }
        Ok(())
    }

    /// NEW PACK (spec 14.1): a pack folder with `pack.json`, `content.json` (its book element), `pack-meta.json`
    /// (`kind: homebrew`) and `creator.json`, installed and loaded.
    ///
    /// # Errors
    /// `invalid_argument` for a missing name, a bad code or a code in use.
    pub fn homebrew_create_pack(
        &self,
        request: &HomebrewPackRequest,
        sink: &dyn EventSink,
    ) -> ApiResult<ContentPack> {
        let _guard = self.content_lock()?;
        let hub = self.hub()?;
        let settings = PackSettings {
            name: request.name.clone(),
            code: request.code.trim().to_string(),
            edition: request.edition,
            author: request.author.clone(),
            description: request.description.clone(),
            license: request.license.clone(),
        }
        .checked(&self.hb_taken_codes(&hub, None))
        .map_err(|p| ApiError::invalid(p.0))?;
        let pack_id = settings.pack_id();
        if self
            .installed_packs()
            .iter()
            .any(|p| p.manifest.id == pack_id)
            || self.paths.library().join(slug(&pack_id)).exists()
        {
            return Err(ApiError::invalid(format!(
                "Another pack already uses the code {}.",
                settings.code
            )));
        }
        let loaded = hub.store.iter().map(|s| s.element.id.as_str());
        let mut ids = IdGenerator::new(&settings.code, loaded, std::iter::empty::<&str>())
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        let next = PackWrite {
            manifest: new_manifest(&settings),
            content: new_content(&settings, source_element(&settings, &mut ids, &today())),
            creator: CreatorFile::new(&settings.code, APP_VERSION),
            trashed: Vec::new(),
        };
        let scratch = Scratch::new(&self.paths.data)?;
        let staged = scratch.0.join("pack");
        write_new_pack(&staged, &next, CONTENT_FILE)?;
        self.install_packs(&[(staged, pack_id.clone())], sink, "homebrew-pack-created")?;
        self.hb_pack_dto(&pack_id)
    }

    /// Pack settings (spec 13.10): name, edition, author, description, licence. The code never changes. The pack's
    /// book element and the elements that name it follow a new name; the version's patch number goes up.
    ///
    /// # Errors
    /// As [`AppState::hb_pack`]; `invalid_argument` for an empty name.
    pub fn homebrew_update_pack(
        &self,
        pack_id: &str,
        patch: &HomebrewPackPatch,
        sink: &dyn EventSink,
    ) -> ApiResult<ContentPack> {
        let _guard = self.content_lock()?;
        let pack = self.hb_pack(pack_id)?;
        let m = &pack.disk.manifest;
        let pick = |new: &Option<String>, old: &Option<String>| new.clone().or_else(|| old.clone());
        let settings = PackSettings {
            name: patch.name.clone().unwrap_or_else(|| m.name.clone()),
            code: pack.creator.code.clone(),
            edition: patch
                .edition
                .unwrap_or_else(|| pack_edition(m, &pack.disk.content)),
            author: pick(&patch.author, &m.attribution),
            description: pick(&patch.description, &m.description),
            license: pick(&patch.license, &m.license),
        }
        .checked(&HashSet::new())
        .map_err(|p| ApiError::invalid(p.0))?;
        let mut manifest = manifest_with(m, &settings);
        manifest.version = bump_patch(&m.version);
        let next = PackWrite {
            manifest,
            content: content_with(&pack.disk.content, &settings),
            creator: pack.creator.clone(),
            trashed: Vec::new(),
        };
        match self.hb_commit(&pack, &next, &[], sink) {
            Ok(()) => self.hb_pack_dto(pack_id),
            Err(CommitError::Api(e)) => Err(e),
            Err(CommitError::Problems(_)) => {
                Err(ApiError::internal("pack settings failed a check"))
            }
        }
    }

    /// The element list of a pack (spec 13.2): every element but the pack's book, with its form, its parent (for a
    /// feature or trait), its edition, its content problems and whether it has an edited draft.
    ///
    /// # Errors
    /// As [`AppState::hb_pack`].
    pub fn homebrew_list_elements(&self, pack_id: &str) -> ApiResult<Vec<HomebrewElementSummary>> {
        let pack = self.hb_pack(pack_id)?;
        let hub = self.hub()?;
        let elements = &pack.disk.content.elements;
        let ids: HashSet<&str> = elements.iter().map(|e| e.id.as_str()).collect();
        let mut counts: HashMap<String, HomebrewProblemCounts> = HashMap::new();
        for issue in validate_elements(&hub.store, &ids) {
            let Some(id) = issue.element.clone() else {
                continue;
            };
            let c = counts.entry(id).or_default();
            match issue.severity {
                Severity::Error => c.errors += 1,
                Severity::Warning => c.warnings += 1,
                Severity::Info => {}
            }
        }
        let drafted: HashSet<String> = self
            .hb_drafts
            .list(&self.paths.data)?
            .into_iter()
            .filter(|d| d.pack_id == pack_id)
            .filter_map(|d| d.element_id)
            .collect();
        Ok(elements
            .iter()
            .filter(|e| e.ty != borealis_authoring::pack::SOURCE_TYPE)
            .map(|e| HomebrewElementSummary {
                id: e.id.clone(),
                name: e.name.clone(),
                kind: pack
                    .creator
                    .elements
                    .get(&e.id)
                    .map(|c| c.kind.clone())
                    .filter(|k| kind(k).is_some())
                    .unwrap_or_else(|| {
                        borealis_authoring::form::kind_for_type(&e.ty)
                            .id
                            .to_string()
                    }),
                element_type: e.ty.clone(),
                parent_id: parent_of(e, elements).map(|p| p.id.clone()),
                edition: e.ruleset.filter(|r| *r != Ruleset::Any),
                problems: counts.get(&e.id).copied().unwrap_or_default(),
                has_draft: drafted.contains(&e.id),
            })
            .collect())
    }

    /// Local characters that use any of `ids` (spec 10.3): the files of the characters folder, and the open
    /// characters as they are in memory (unsaved changes included).
    ///
    /// # Errors
    /// `internal` when a lock is poisoned.
    pub(crate) fn hb_characters_using(
        &self,
        ids: &HashSet<&str>,
    ) -> ApiResult<Vec<HomebrewCharacterRef>> {
        let mut out = Vec::new();
        let sessions = self.sessions()?;
        for (id, s) in sessions.iter() {
            if let Ok(v) = serde_json::to_value(&s.character) {
                if mentions_any(&v, ids) {
                    out.push(HomebrewCharacterRef {
                        character_id: id.clone(),
                        name: s.character.meta.name.clone(),
                    });
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(self.characters_dir()) {
            for path in entries.filter_map(Result::ok).map(|e| e.path()) {
                let Some(id) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|_| path.extension().is_some_and(|e| e == "json"))
                    .filter(|id| {
                        crate::session::validate_id(id).is_ok() && !sessions.contains_key(*id)
                    })
                else {
                    continue;
                };
                let Some(v) = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                else {
                    continue;
                };
                if mentions_any(&v, ids) {
                    out.push(HomebrewCharacterRef {
                        character_id: id.to_string(),
                        name: v["meta"]["name"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then(a.character_id.cmp(&b.character_id))
        });
        Ok(out)
    }

    /// Delete elements of a pack (spec 10.3). Without `confirm`, a scan first: when local characters use any of
    /// them, nothing is deleted and they are listed. Deleting retires the ids (never given out again), keeps a copy
    /// in the pack's `.trash`, drops the elements' drafts and reloads.
    ///
    /// # Errors
    /// As [`AppState::hb_pack`]; `not_found` when an id is not in the pack.
    pub fn homebrew_delete_elements(
        &self,
        pack_id: &str,
        element_ids: &[String],
        confirm: bool,
        sink: &dyn EventSink,
    ) -> ApiResult<HomebrewDeleteResult> {
        let _guard = self.content_lock()?;
        let pack = self.hb_pack(pack_id)?;
        let ids: HashSet<&str> = element_ids.iter().map(String::as_str).collect();
        let held: HashSet<&str> = pack
            .disk
            .content
            .elements
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        if ids.is_empty() || !ids.is_subset(&held) {
            return Err(ApiError::not_found(
                "That element isn't in this pack any more.",
            ));
        }
        if source_of(&pack.disk.content).is_some_and(|s| ids.contains(s.id.as_str())) {
            return Err(ApiError::invalid("The pack's book can't be deleted."));
        }
        let used_by = self.hb_characters_using(&ids)?;
        if !confirm && !used_by.is_empty() {
            return Ok(HomebrewDeleteResult {
                deleted: false,
                used_by,
                pack: None,
            });
        }
        let (content, trashed) = without_elements(&pack.disk.content, &ids);
        let mut manifest = pack.disk.manifest.clone();
        manifest.version = bump_patch(&manifest.version);
        let next = PackWrite {
            manifest,
            content,
            creator: pack.creator.with_retired(ids.iter().copied()),
            trashed,
        };
        match self.hb_commit(&pack, &next, &[], sink) {
            Ok(()) => {}
            Err(CommitError::Api(e)) => return Err(e),
            Err(CommitError::Problems(_)) => {
                return Err(ApiError::internal("a delete failed a check"))
            }
        }
        self.hb_drafts
            .remove_for_elements(&self.paths.data, pack_id, element_ids)?;
        Ok(HomebrewDeleteResult {
            deleted: true,
            used_by,
            pack: Some(self.hb_pack_dto(pack_id)?),
        })
    }
}

/// True when any string anywhere in `v` is one of `ids`.
fn mentions_any(v: &Value, ids: &HashSet<&str>) -> bool {
    match v {
        Value::String(s) => ids.contains(s.as_str()),
        Value::Array(a) => a.iter().any(|x| mentions_any(x, ids)),
        Value::Object(o) => o
            .iter()
            .any(|(k, x)| ids.contains(k.as_str()) || mentions_any(x, ids)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_suggested_from_initials_and_kept_unique() {
        let none = HashSet::new();
        assert_eq!(suggest_code("My Homebrew", &none), "MH");
        assert_eq!(suggest_code("x", &none), "HB");
        assert_eq!(suggest_code("a b c d e f", &none), "ABCD");
        let taken: HashSet<String> = ["MH".to_string(), "MH2".to_string()].into();
        assert_eq!(suggest_code("My Homebrew", &taken), "MH3");
    }

    #[test]
    fn mentions_are_found_at_any_depth_and_in_keys() {
        let ids: HashSet<&str> = ["ID_X"].into();
        assert!(mentions_any(
            &serde_json::json!({"a": [{"b": "ID_X"}]}),
            &ids
        ));
        assert!(mentions_any(
            &serde_json::json!({"choices": {"ID_X": 1}}),
            &ids
        ));
        assert!(!mentions_any(&serde_json::json!({"a": "ID_XY"}), &ids));
    }
}
