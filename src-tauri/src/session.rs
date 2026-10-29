//! Open-character sessions: lookup, atomic mutation and snapshot building.

use crate::dto::{CharacterSnapshot, RollRecord};
use crate::error::{ApiError, ApiResult};
use crate::settings::AppSettings;
use crate::snapshot_map::{build_snapshot, MapInput};
use crate::state::{AppState, ContentHub, Session};
use borealis_character::Character;
use borealis_engine::{Choice, SlotKey};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

const MAX_ID_LEN: usize = 64;
const MAX_FILL_PASSES: usize = 24;

/// Everything a mutation closure may need besides the character itself.
pub struct Ctx<'a> {
    /// Content and engine.
    pub hub: &'a ContentHub,
    /// Settings in effect.
    pub settings: &'a AppSettings,
    /// The application state (random numbers).
    pub app: &'a AppState,
}

/// The editable parts of a session handed to a mutation.
pub struct Draft {
    /// The character (a working copy: dropped if the mutation fails).
    pub character: Character,
    /// Ability roll history.
    pub rolls: Vec<RollRecord>,
    /// Non-fatal warnings shown only with the resulting snapshot.
    pub warnings: Vec<String>,
}

/// A fresh library id.
#[must_use]
pub fn new_id() -> String {
    format!("c_{}", uuid::Uuid::new_v4().simple())
}

/// Reject ids that could escape the characters folder.
///
/// # Errors
/// `invalid_argument` for an empty, long or non-alphanumeric id.
pub fn validate_id(id: &str) -> ApiResult<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid("Invalid character id."))
    }
}

impl AppState {
    /// File of a library character.
    ///
    /// # Errors
    /// `invalid_argument` for a bad id.
    pub fn character_path(&self, id: &str) -> ApiResult<PathBuf> {
        validate_id(id)?;
        Ok(self.characters_dir().join(format!("{id}.json")))
    }

    /// Run `f` against an open character. On success the working copy replaces the session's
    /// character and the session becomes dirty; on failure nothing changes.
    ///
    /// # Errors
    /// `not_found` if the character is not open, or whatever `f` returns.
    pub fn mutate<F>(&self, id: &str, f: F) -> ApiResult<CharacterSnapshot>
    where
        F: FnOnce(&Ctx<'_>, &mut Draft) -> ApiResult<()>,
    {
        let hub = self.hub()?;
        let settings = self.settings()?;
        let mut sessions = self.sessions()?;
        let session = sessions.get_mut(id).ok_or_else(not_open)?;
        let mut draft = Draft {
            character: session.character.clone(),
            rolls: session.rolls.clone(),
            warnings: Vec::new(),
        };
        let ctx = Ctx {
            hub: &hub,
            settings: &settings,
            app: self,
        };
        f(&ctx, &mut draft)?;
        clamp_tracker(&hub, &mut draft.character.build);
        session.character = draft.character;
        session.rolls = draft.rolls;
        session.dirty = true;
        Ok(snapshot_for(id, session, &hub, &settings, &draft.warnings))
    }

    /// Read an open character's snapshot.
    ///
    /// # Errors
    /// `not_found` if the character is not open.
    pub fn get_snapshot(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        let hub = self.hub()?;
        let settings = self.settings()?;
        let sessions = self.sessions()?;
        let session = sessions.get(id).ok_or_else(not_open)?;
        Ok(snapshot_for(id, session, &hub, &settings, &[]))
    }

    /// Read-only access to an open character plus the content.
    ///
    /// # Errors
    /// `not_found` if the character is not open, or whatever `f` returns.
    pub fn read<T>(
        &self,
        id: &str,
        f: impl FnOnce(&Ctx<'_>, &Session) -> ApiResult<T>,
    ) -> ApiResult<T> {
        let hub = self.hub()?;
        let settings = self.settings()?;
        let sessions = self.sessions()?;
        let session = sessions.get(id).ok_or_else(not_open)?;
        let ctx = Ctx {
            hub: &hub,
            settings: &settings,
            app: self,
        };
        f(&ctx, session)
    }

    /// Register a new open session (replacing any with the same id).
    ///
    /// # Errors
    /// `internal` if the session lock is poisoned.
    pub fn insert_session(
        &self,
        id: &str,
        mut character: Character,
        path: PathBuf,
        dirty: bool,
    ) -> ApiResult<CharacterSnapshot> {
        let hub: Arc<ContentHub> = self.hub()?;
        let _ = crate::renamed_ids::migrate(&mut character.build, &hub.store);
        let settings = self.settings()?;
        let session = Session {
            character,
            dirty,
            path,
            rolls: Vec::new(),
        };
        let snap = snapshot_for(id, &session, &hub, &settings, &[]);
        self.sessions()?.insert(id.to_string(), session);
        Ok(snap)
    }
}

/// Write the spell tracker's clamp back (GitHub #2): a change that lowered a maximum (level down,
/// respec) lowers the saved used count too, and a concentration spell no longer ready ends. The
/// derive is the hub's cached one that the snapshot reads next.
fn clamp_tracker(hub: &ContentHub, build: &mut borealis_engine::CharacterState) {
    use borealis_engine::spell_tracker::{has_tracking, normalized};
    if !has_tracking(&build.play) {
        return;
    }
    let snap = hub.derive(build);
    if let Some(next) = normalized(&hub.engine, build, &snap) {
        *build = next;
    }
}

fn not_open() -> ApiError {
    ApiError::not_found("That character is not open.")
}

fn snapshot_for(
    id: &str,
    session: &Session,
    hub: &ContentHub,
    settings: &AppSettings,
    warnings: &[String],
) -> CharacterSnapshot {
    build_snapshot(&MapInput {
        id,
        character: &session.character,
        dirty: session.dirty,
        display_path: session.path.to_string_lossy().into_owned(),
        hub,
        settings,
        rolls: &session.rolls,
        warnings,
    })
}

/// Choose the rule `default` of every slot that is first populated by a command (engine-optional-
/// sole-option SO-2): a slot whose key is not in `existing` (the slots of the state before the
/// command) and whose list holds its default. Nothing else is ever chosen automatically (SO-1):
/// a sole option without a default stays open for the user. Repeats while chosen defaults bring
/// new slots. Its derives go through the hub's cache: the last one is the state's snapshot that
/// follows.
#[must_use]
pub fn fill_defaults(
    hub: &ContentHub,
    existing: &HashSet<SlotKey>,
    state: &borealis_engine::CharacterState,
) -> borealis_engine::CharacterState {
    let engine = &hub.engine;
    let mut done = existing.clone();
    let mut current = state.clone();
    for _ in 0..MAX_FILL_PASSES {
        let snap = hub.derive(&current);
        let pick = snap.slots.iter().find_map(|s| {
            let default = s
                .default
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())?;
            (!done.contains(&s.key)
                && s.chosen.is_none()
                && s.rejected.is_none()
                && s.missing.is_none()
                && s.list_choice.is_none()
                && s.options.iter().any(|o| o.id == default))
            .then(|| (s.key.clone(), default.to_string()))
        });
        let Some((key, default)) = pick else { break };
        done.insert(key.clone());
        let choice = Choice::Element(default);
        match borealis_engine::commands::select_derived(engine, &current, &snap, &key, choice) {
            Ok(outcome) => {
                hub.keep_derived(&outcome);
                current = outcome.state;
            }
            Err(_) => break,
        }
    }
    current
}

/// Keys of every slot of `state` (rejected ones included): the slots that already exist before
/// a command, whose `default` is therefore never applied again ([`fill_defaults`]).
#[must_use]
pub fn slot_keys(hub: &ContentHub, state: &borealis_engine::CharacterState) -> HashSet<SlotKey> {
    hub.derive(state)
        .slots
        .iter()
        .map(|s| s.key.clone())
        .collect()
}
