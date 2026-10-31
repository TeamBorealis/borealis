//! U14 (GitHub #10): what the New Character dialog and an edition-emptied Class list need: the
//! classes each edition offers, and "Start over as 2014".

use crate::builder::MAX_LEVEL;
use crate::dto::{CharacterSnapshot, ClassCounts, NewCharacterRequest, OptionsPatch, Ruleset};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_engine::commands;

impl AppState {
    /// How many classes a new character of each edition is offered with every source on (the New
    /// Character dialog warns when the chosen edition has none, U14).
    ///
    /// # Errors
    /// `internal` if the content lock is poisoned.
    pub fn class_counts_by_edition(&self) -> ApiResult<ClassCounts> {
        let hub = self.hub()?;
        let count = |ruleset: Ruleset| {
            let state = commands::new_character(&hub.engine, ruleset);
            let snap = hub.engine.derive(&state);
            snap.slots
                .iter()
                .find(|s| s.ty == "Class" && s.chosen.is_none())
                .map_or(0, |s| u32::try_from(s.options.len()).unwrap_or(u32::MAX))
        };
        Ok(ClassCounts {
            dnd2014: count(Ruleset::Dnd2014),
            dnd2024: count(Ruleset::Dnd2024),
        })
    }

    /// "Start over as 2014" (U14): a NEW character of `ruleset` with the old one's name, details,
    /// group, portrait (copied as stored), level, ability method and base scores, character
    /// options and source restrictions, saved into the library and opened. The old character is
    /// not changed: its file stays as it is (classes never cross editions, R4, so nothing else
    /// carries over). Nothing is saved until the new character is complete, so a failure leaves
    /// no half-made character behind.
    ///
    /// # Errors
    /// `not_found` for an unknown character; `invalid_argument` for `any` or the character's own
    /// edition; `io_error` if saving fails.
    pub fn start_over_in_edition(
        &self,
        id: &str,
        ruleset: Ruleset,
    ) -> ApiResult<CharacterSnapshot> {
        let old = self.get_snapshot(id)?;
        if ruleset == Ruleset::Any || ruleset == old.ruleset {
            return Err(ApiError::invalid(
                "Choose the other edition to start over in.",
            ));
        }
        let (base, level, restricted, portrait) = self.read(id, |_, session| {
            let c = &session.character;
            Ok((
                c.build.abilities.base,
                c.build.level().max(1),
                c.build.restricted_sources.clone(),
                c.portrait.clone(),
            ))
        })?;
        let offered: std::collections::BTreeSet<String> = self
            .list_character_options(ruleset)?
            .into_iter()
            .map(|o| o.id)
            .collect();
        let o = &old.options;
        let request = NewCharacterRequest {
            name: old.name.clone(),
            ruleset,
            starting_level: level.min(MAX_LEVEL),
            gender: old.details.gender.clone(),
            ability_method: old.abilities.method,
            options: OptionsPatch {
                multiclassing: Some(o.multiclassing),
                feats: Some(o.feats),
                average_hp: Some(o.average_hp),
                point_buy_unrestricted: Some(o.point_buy_unrestricted),
                attunement_limit: Some(o.attunement_limit),
                custom: Some(
                    o.custom
                        .iter()
                        .filter(|(k, _)| offered.contains(*k))
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                ),
            },
            // copied as stored below: an imported portrait need not survive a data-URL round
            // trip (review M1)
            portrait: None,
            group: Some(old.group.clone()),
        };
        let mut draft = self.new_character_draft(ruleset, &request)?;
        draft.character.portrait = portrait;
        draft.character.build.abilities.base = base;
        draft.character.build.restricted_sources = restricted;
        self.save_new_character(draft)
    }
}
