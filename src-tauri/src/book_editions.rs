//! IM14 (GitHub #11): the edition (2014 or 2024) of each imported book.
//!
//! The importer guesses it (`borealis_migrate::edition`: the `D&D 5.5e` marker, then the name and
//! date). The user can set it on the Sources page or in the import preview; the choice is a
//! content preference keyed by the book's name ([`borealis_content::source_key`]) and applied when
//! content loads ([`borealis_content::ContentStoreBuilder::with_source_editions`]), so it survives
//! re-import and restart and never rewrites the imported files. A character that already holds
//! content of a re-tagged book keeps it (the engine replays stored choices of user-set books
//! whatever their edition); new picks follow the book's edition (R4).

use crate::content_dto::{ContentSource, PreviewBook};
use crate::content_packs::EventSink;
use crate::content_prefs::ContentPrefs;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_content::{source_key, ContentFile, PackFileKind, PackManifest, PackOrigin, Ruleset};
use std::collections::BTreeMap;
use std::path::Path;

/// The content files of the pack written to `dir` (none if it cannot be read).
pub(crate) fn pack_content_files(dir: &Path) -> Vec<ContentFile> {
    let Some(manifest) = std::fs::read_to_string(dir.join("pack.json"))
        .ok()
        .and_then(|t| PackManifest::from_json_str(&t).ok())
    else {
        return Vec::new();
    };
    manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content)
        .filter_map(|f| std::fs::read_to_string(dir.join(&f.path)).ok())
        .filter_map(|t| ContentFile::from_json_str(&t).ok())
        .collect()
}

/// The books (`Source` elements) of the pack in `dir` with the edition the importer wrote for
/// each (no edition or `any` reads as 2014), in file order, each name once.
pub(crate) fn pack_books(dir: &Path) -> Vec<(String, Ruleset)> {
    let mut out: Vec<(String, Ruleset)> = Vec::new();
    for file in pack_content_files(dir) {
        for el in file.elements.iter().filter(|e| e.ty == "Source") {
            if out
                .iter()
                .any(|(n, _)| source_key(n) == source_key(&el.name))
            {
                continue;
            }
            let edition = match el.ruleset.or(file.ruleset) {
                Some(Ruleset::Dnd2024) => Ruleset::Dnd2024,
                _ => Ruleset::Dnd2014,
            };
            out.push((el.name.clone(), edition));
        }
    }
    out
}

/// The import preview's book list: what the importer detected, and the edition that will apply:
/// the user's stored choice, else the edition the book has in the installed pack this import
/// replaces (`installed`, review M2: a re-import keeps it), else the detected one.
pub(crate) fn preview_books(
    dir: &Path,
    installed: &Path,
    prefs: &ContentPrefs,
) -> Vec<PreviewBook> {
    let chosen = prefs.source_editions();
    let had: BTreeMap<String, Ruleset> = pack_books(installed)
        .into_iter()
        .map(|(n, r)| (source_key(&n), r))
        .collect();
    pack_books(dir)
        .into_iter()
        .map(|(name, detected)| {
            let key = source_key(&name);
            let set = chosen.get(&key).copied();
            PreviewBook {
                edition: set.or_else(|| had.get(&key).copied()).unwrap_or(detected),
                name,
                detected,
                set_by_user: set.is_some(),
            }
        })
        .collect()
}

/// Only the two editions a book can have.
fn check_edition(edition: Ruleset) -> ApiResult<()> {
    if edition == Ruleset::Any {
        return Err(ApiError::invalid("A book is either 2014 or 2024 rules."));
    }
    Ok(())
}

/// The user's editions for books of a preview: every name must be one of `books`.
pub(crate) fn check_preview_editions(
    books: &[PreviewBook],
    editions: &BTreeMap<String, Ruleset>,
) -> ApiResult<()> {
    for (name, edition) in editions {
        check_edition(*edition)?;
        if !books
            .iter()
            .any(|b| source_key(&b.name) == source_key(name))
        {
            return Err(ApiError::invalid(format!(
                "\"{name}\" is not a book of this import."
            )));
        }
    }
    Ok(())
}

/// A re-import (index update, file drop, or the preview without a choice): an installed book
/// whose detected edition would change, and that the user never set, keeps the edition it had
/// (stored as a choice the user can change on the Sources page), so a character using it never
/// loses it because the importer guesses differently now. `packs` pairs each new pack folder
/// with the installed folder it replaces; both are read from disk, so a pack that is switched
/// off counts too (review M3).
pub(crate) fn keep_installed_editions(
    prefs: &mut ContentPrefs,
    packs: &[(std::path::PathBuf, std::path::PathBuf)],
) {
    let chosen = prefs.source_editions();
    for (new_dir, installed_dir) in packs {
        let installed: BTreeMap<String, Ruleset> = pack_books(installed_dir)
            .into_iter()
            .map(|(n, r)| (source_key(&n), r))
            .collect();
        if installed.is_empty() {
            continue;
        }
        for (name, detected) in pack_books(new_dir) {
            let key = source_key(&name);
            if chosen.contains_key(&key) {
                continue;
            }
            if let Some(&old) = installed.get(&key).filter(|old| **old != detected) {
                tracing::info!(book = %name, kept = old.as_str(), "re-import keeps a book's edition");
                prefs.set_source_edition(&name, old);
            }
        }
    }
}

/// A book's edition as loaded: its first element with an edition (`null` when none has one).
pub(crate) fn loaded_edition<'a>(
    elements: impl Iterator<Item = &'a borealis_content::StoredElement>,
) -> (Option<Ruleset>, bool) {
    let mut edition = None;
    let mut imported = false;
    for e in elements {
        imported |= e.origin == PackOrigin::Custom;
        if edition.is_none() {
            edition = e.element.ruleset.filter(|r| *r != Ruleset::Any);
        }
    }
    (edition, imported)
}

impl AppState {
    /// IM14: set an imported book's edition (Sources page) and hot-reload. Characters that already
    /// use the book keep its content; new picks follow the new edition.
    ///
    /// # Errors
    /// `invalid_argument` for `any` or a bundled book; `not_found` for an unknown book.
    pub fn set_source_edition(
        &self,
        source_id: &str,
        edition: Ruleset,
        sink: &dyn EventSink,
    ) -> ApiResult<Vec<ContentSource>> {
        check_edition(edition)?;
        let _guard = self.content_lock()?;
        let source = self
            .list_sources()?
            .into_iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| ApiError::not_found("That book is not installed."))?;
        if !source.edition_editable {
            return Err(ApiError::invalid(
                "Bundled books keep their edition; only imported books can be set.",
            ));
        }
        let mut prefs = self.content_prefs();
        prefs.set_source_edition(&source.name, edition);
        self.reload_content(&prefs, sink, "source-edition")?;
        self.list_sources()
    }
}
