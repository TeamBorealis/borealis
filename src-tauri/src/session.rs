//! Open-character sessions: lookup, atomic mutation and snapshot building.

use crate::dto::{CharacterSnapshot, RollRecord};
use crate::error::{ApiError, ApiResult};
use crate::settings::AppSettings;
use crate::snapshot_map::{build_snapshot, MapInput};
use crate::state::{AppState, ContentHub, Session};
use borealis_character::Character;
use borealis_engine::{Choice, Engine};
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

/// Select every locked slot that has exactly one candidate, repeating while new slots appear.
#[must_use]
pub fn fill_locked(
    engine: &Engine,
    state: &borealis_engine::CharacterState,
) -> borealis_engine::CharacterState {
    let mut current = state.clone();
    for _ in 0..MAX_FILL_PASSES {
        let snap = engine.derive(&current);
        let pick = snap.slots.iter().find(|s| {
            s.locked
                && s.chosen.is_none()
                && s.rejected.is_none()
                && s.list_choice.is_none()
                && s.options.len() == 1
        });
        let Some(slot) = pick else { break };
        let choice = Choice::Element(slot.options[0].id.clone());
        match borealis_engine::commands::select(engine, &current, &slot.key, choice) {
            Ok(outcome) => current = outcome.state,
            Err(_) => break,
        }
    }
    current
}
