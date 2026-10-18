//! The character library: list, open, save, duplicate, delete, import and export.

use crate::dto::{
    CharacterSnapshot, CharacterSummary, ExportResult, ImportReport, MissingContentDto,
    RecheckEntry, RecheckReport, UnmappedSlotDto,
};
use crate::error::{ApiError, ApiResult};
use crate::extras::{portrait_data_url, portrait_from_data_url};
use crate::ids::rfc3339;
use crate::session::{new_id, validate_id};
use crate::state::{AppState, ContentHub};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_character::{
    import_dnd5e, name_hint, recheck, source_hint, Character, MissingContent, UnmappedSlot,
    UnresolvedSelection,
};
use std::path::Path;
use std::time::SystemTime;

const MAX_NAME_CHARS: usize = 120;
const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_GROUP: &str = "Characters";

fn clean_name(name: &str) -> ApiResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::invalid(format!(
            "A name must be 1 to {MAX_NAME_CHARS} characters."
        )));
    }
    Ok(trimmed.to_string())
}

fn build_label(hub: &ContentHub, character: &Character) -> String {
    let snap = hub.engine.derive_with(&character.build, false);
    let race = snap
        .elements
        .iter()
        .find(|e| matches!(e.element.ty.as_str(), "Race" | "Species"))
        .map(|e| e.element.name.clone());
    let classes: Vec<String> = snap
        .classes
        .iter()
        .filter_map(|c| c.class.as_ref().map(|e| format!("{} {}", e.name, c.level)))
        .collect();
    let class_part = if classes.is_empty() {
        format!("Level {}", snap.level)
    } else {
        classes.join(" / ")
    };
    match race {
        Some(r) => format!("{r} {class_part}"),
        None => class_part,
    }
}

fn summary(
    hub: &ContentHub,
    id: &str,
    character: &Character,
    path: &Path,
    updated: SystemTime,
) -> CharacterSummary {
    CharacterSummary {
        id: id.to_string(),
        name: character.meta.name.clone(),
        build: build_label(hub, character),
        level: character.build.level(),
        ruleset: character.ruleset(),
        portrait: portrait_data_url(character),
        favorite: character.meta.favorite,
        group: character.meta.group.clone(),
        updated_at: rfc3339(updated),
        path: Some(path.display().to_string()),
        missing_content: character.import.as_ref().map_or(0, |i| i.missing_content),
    }
}

fn missing_dto(m: &MissingContent) -> MissingContentDto {
    MissingContentDto {
        id: m.id.clone(),
        ty: m.ty.clone(),
        name: if m.name.trim().is_empty() {
            name_hint(&m.id)
        } else {
            m.name.clone()
        },
        context: m.context.clone(),
        source: source_hint(&m.id),
    }
}

fn unresolved_dto(u: &UnresolvedSelection) -> UnmappedSlotDto {
    UnmappedSlotDto {
        owner: u.owner.clone(),
        ty: u.ty.clone(),
        name: u.name.clone(),
        chosen: u.chosen.clone(),
        reason: "the content it belongs to is not installed (or no longer matches)".into(),
    }
}

fn unmapped_dto(u: &UnmappedSlot) -> UnmappedSlotDto {
    UnmappedSlotDto {
        owner: u.owner.clone(),
        ty: u.ty.clone(),
        name: u.name.clone(),
        chosen: u.chosen.clone(),
        reason: u.reason.clone(),
    }
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| SystemTime::now())
}

impl AppState {
    /// All saved characters, newest first (open characters show their unsaved state).
    ///
    /// # Errors
    /// `io_error` if the characters folder cannot be read.
    pub fn list_characters(&self) -> ApiResult<Vec<CharacterSummary>> {
        let hub = self.hub()?;
        let entries = std::fs::read_dir(self.characters_dir())
            .map_err(|e| ApiError::io("read the character library", e))?;
        let sessions = self.sessions()?;
        let mut rows = Vec::new();
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "json"))
                .filter(|id| validate_id(id).is_ok())
            else {
                continue;
            };
            let updated = modified(&path);
            if let Some(open) = sessions.get(id) {
                rows.push(summary(&hub, id, &open.character, &path, updated));
                continue;
            }
            match Character::load(&path) {
                Ok(c) => rows.push(summary(&hub, id, &c, &path, updated)),
                Err(e) => tracing::warn!("skipping unreadable character {id}: {e}"),
            }
        }
        rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.name.cmp(&b.name)));
        Ok(rows)
    }

    /// Open a saved character (or return the already open one).
    ///
    /// # Errors
    /// `not_found` if there is no such file, `invalid_argument` if it cannot be parsed.
    pub fn load_character(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        let path = self.character_path(id)?;
        if self.sessions()?.contains_key(id) {
            return self.get_snapshot(id);
        }
        if !path.is_file() {
            return Err(ApiError::not_found("That character no longer exists."));
        }
        let character = Character::load(&path)?;
        self.insert_session(id, character, path, false)
    }

    /// Save to the character's own file (silent save, also used by autosave).
    ///
    /// # Errors
    /// `not_found` if not open, `io_error` if writing fails.
    pub fn save_character(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        {
            let mut sessions = self.sessions()?;
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| ApiError::not_found("That character is not open."))?;
            session.character.save(&session.path)?;
            session.dirty = false;
        }
        self.get_snapshot(id)
    }

    /// Save a copy. With `path` the copy is exported to that file and the session is unchanged;
    /// otherwise a new library character is created and the session moves to it.
    ///
    /// # Errors
    /// `not_found`, `invalid_argument` (bad name or path) or `io_error`.
    pub fn save_character_as(
        &self,
        id: &str,
        name: Option<&str>,
        path: Option<&Path>,
    ) -> ApiResult<CharacterSnapshot> {
        let mut copy = self.read(id, |_, s| Ok(s.character.clone()))?;
        if let Some(n) = name {
            copy.meta.name = clean_name(n)?;
        }
        if let Some(p) = path {
            if p.as_os_str().is_empty() || p.is_dir() {
                return Err(ApiError::invalid("Choose a file to save to."));
            }
            copy.save(p)?;
            return self.get_snapshot(id);
        }
        let new = new_id();
        let new_path = self.character_path(&new)?;
        copy.save(&new_path)?;
        let mut sessions = self.sessions()?;
        sessions.remove(id);
        drop(sessions);
        self.insert_session(&new, copy, new_path, false)
    }

    /// Release an open character; unsaved edits are saved unless `discard` is set.
    ///
    /// # Errors
    /// `io_error` if the final save fails (the character then stays open).
    pub fn close_character(&self, id: &str, discard: bool) -> ApiResult<()> {
        let mut sessions = self.sessions()?;
        if let Some(session) = sessions.get(id) {
            if session.dirty && !discard {
                session.character.save(&session.path)?;
            }
            sessions.remove(id);
        }
        Ok(())
    }

    /// Permanently delete a character (file and open session).
    ///
    /// # Errors
    /// `not_found` if nothing exists with that id, `io_error` if the file cannot be removed.
    pub fn delete_character(&self, id: &str) -> ApiResult<()> {
        let path = self.character_path(id)?;
        let was_open = self.sessions()?.remove(id).is_some();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && was_open => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ApiError::not_found("That character no longer exists."))
            }
            Err(e) => Err(ApiError::io("delete the character", e)),
        }
    }

    /// Copy a character into a new library entry (not opened).
    ///
    /// # Errors
    /// `not_found` or `io_error`.
    pub fn duplicate_character(&self, id: &str) -> ApiResult<CharacterSummary> {
        let source = match self.read(id, |_, s| Ok(s.character.clone())) {
            Ok(c) => c,
            Err(_) => {
                let path = self.character_path(id)?;
                if !path.is_file() {
                    return Err(ApiError::not_found("That character no longer exists."));
                }
                Character::load(&path)?
            }
        };
        let mut copy = source;
        copy.meta.name = format!("{} (copy)", copy.meta.name);
        let new = new_id();
        let path = self.character_path(&new)?;
        copy.save(&path)?;
        let hub = self.hub()?;
        Ok(summary(&hub, &new, &copy, &path, modified(&path)))
    }

    /// Rename, regroup or (un)favourite a character.
    ///
    /// # Errors
    /// `not_found` or `invalid_argument`.
    pub fn update_character_meta(
        &self,
        id: &str,
        name: Option<&str>,
        group: Option<&str>,
        favorite: Option<bool>,
    ) -> ApiResult<CharacterSnapshot> {
        let name = name.map(clean_name).transpose()?;
        let group = group
            .map(|g| clean_name(g).map_err(|_| ApiError::invalid("A group name cannot be empty.")))
            .transpose()?;
        self.mutate(id, |_, d| {
            if let Some(n) = name {
                d.character.meta.name = n;
            }
            if let Some(g) = group {
                d.character.meta.group = g;
            }
            if let Some(f) = favorite {
                d.character.meta.favorite = f;
            }
            Ok(())
        })
    }

    /// Set or clear the portrait from a `data:` URL.
    ///
    /// # Errors
    /// `not_found` or `invalid_argument`.
    pub fn set_portrait(&self, id: &str, data_url: Option<&str>) -> ApiResult<CharacterSnapshot> {
        let portrait = data_url
            .map(|u| portrait_from_data_url(u).map_err(ApiError::invalid))
            .transpose()?;
        self.mutate(id, |_, d| {
            d.character.portrait = portrait;
            Ok(())
        })
    }

    /// Import a legacy `.dnd5e` file or a native character JSON file from bytes.
    ///
    /// # Errors
    /// `invalid_argument` for undecodable or unsupported files, `io_error` on write failure.
    pub fn import_character(
        &self,
        file_name: &str,
        content_base64: &str,
    ) -> ApiResult<ImportReport> {
        let bytes = STANDARD
            .decode(content_base64.trim())
            .map_err(|_| ApiError::invalid("The file content is not valid base64."))?;
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err(ApiError::invalid("That file is too large to import."));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| ApiError::invalid("The file is not valid UTF-8 text."))?;
        let hub = self.hub()?;
        let text = text.trim_start_matches('\u{feff}');
        let (mut character, divergences, mut warnings, selections_imported, invalid, unmapped) =
            if text.trim_start().starts_with('<') {
                let outcome = import_dnd5e(&hub.engine, text)?;
                let r = outcome.report;
                let divergences = r
                    .legacy_divergences
                    .into_iter()
                    .map(|d| d.message)
                    .collect();
                (
                    outcome.character,
                    divergences,
                    r.warnings,
                    r.selections_imported,
                    r.invalid_selections,
                    r.unmapped_slots.iter().map(unmapped_dto).collect(),
                )
            } else {
                {
                    let c = Character::from_json_str(text)?;
                    let unmapped = c.unresolved.iter().map(unresolved_dto).collect();
                    (c, Vec::new(), Vec::new(), 0, Vec::new(), unmapped)
                }
            };
        warnings.dedup();
        if character.meta.name.trim().is_empty() {
            character.meta.name = Path::new(file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .map_or_else(|| "Imported character".to_string(), str::to_string);
        }
        if character.meta.group.trim().is_empty() {
            character.meta.group = DEFAULT_GROUP.to_string();
        }
        let id = new_id();
        let path = self.character_path(&id)?;
        character.save(&path)?;
        let missing_content = character
            .import
            .as_ref()
            .map(|i| i.missing.iter().map(missing_dto).collect())
            .unwrap_or_default();
        Ok(ImportReport {
            summary: summary(&hub, &id, &character, &path, modified(&path)),
            divergences,
            warnings,
            missing_content,
            unmapped_slots: unmapped,
            invalid_selections: invalid,
            selections_imported,
        })
    }

    /// Export a character as native JSON (base64 for the UI to download).
    ///
    /// # Errors
    /// `not_found` or `internal`.
    pub fn export_character(&self, id: &str) -> ApiResult<ExportResult> {
        let (name, json) = self.read(id, |_, s| {
            Ok((s.character.meta.name.clone(), s.character.to_json_string()?))
        })?;
        let stem = crate::ids::file_stem(&name);
        Ok(ExportResult {
            file_name: format!("{stem}.borealis.json"),
            mime: "application/json".into(),
            path: None,
            data_base64: Some(STANDARD.encode(json)),
        })
    }
}

impl AppState {
    /// Re-resolve every saved character against the content installed now: the unresolved parts
    /// of imports, and anything a character names (choices, options, items) that is not
    /// installed, built or imported. Saved characters are rewritten when something resolved; an open character
    /// is updated in its session (and only written to disk if it has no unsaved edits).
    ///
    /// # Errors
    /// `io_error` if the characters folder cannot be read or a changed character cannot be saved.
    pub fn recheck_missing_content(&self) -> ApiResult<RecheckReport> {
        let hub = self.hub()?;
        let entries = std::fs::read_dir(self.characters_dir())
            .map_err(|e| ApiError::io("read the character library", e))?;
        let mut sessions = self.sessions()?;
        let mut report = RecheckReport {
            checked: 0,
            updated: Vec::new(),
            outstanding: Vec::new(),
        };
        let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "json"))
                .filter(|id| validate_id(id).is_ok())
                .map(str::to_string)
            else {
                continue;
            };
            let open = sessions.get_mut(&id);
            let mut character = match &open {
                Some(s) => s.character.clone(),
                None => match Character::load(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("skipping unreadable character {id}: {e}");
                        continue;
                    }
                },
            };
            // the ids a session rewrites when the character is opened (renamed 5etools
            // elements): they are not missing
            let _ = crate::renamed_ids::migrate(&mut character.build, &hub.store);
            // every character: one made in the builder loses content too when a pack is
            // removed or updated (recheck lists what it names that is not installed)
            report.checked += 1;
            let outcome = recheck(&hub.engine, &mut character);
            let entry = RecheckEntry {
                character_id: id.clone(),
                name: character.meta.name.clone(),
                resolved: outcome.resolved,
                still_missing: outcome.still_missing.iter().map(missing_dto).collect(),
                still_unmapped: outcome.still_unmapped.iter().map(unmapped_dto).collect(),
            };
            if outcome.changed() {
                match open {
                    Some(session) => {
                        session.character = character;
                        // Never commit the user's unsaved edits behind their back.
                        if !session.dirty {
                            session.character.save(&session.path)?;
                        }
                    }
                    None => character.save(&path)?,
                }
                report.updated.push(entry.clone());
            }
            if !entry.still_missing.is_empty() || !entry.still_unmapped.is_empty() {
                report.outstanding.push(entry);
            }
        }
        Ok(report)
    }
}
