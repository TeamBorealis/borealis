//! B3 (GitHub #35): the builder's Add actions for the extra feats, spells, languages and
//! proficiencies a DM grants ([`borealis_engine::added_picks`]). An added pick is an equipped,
//! hidden Aurora proxy line marked `added_pick`: the snapshot lists it in `addedPicks` (for its
//! builder section) and leaves it out of the inventory.

use crate::builder::apply;
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_engine::added_picks::{AddOption, AddedKind, AddedPickSnapshot};
use borealis_engine::commands::{self, CommandError};
use borealis_engine::snapshot::CharacterSnapshot as Engine;
use serde::{Deserialize, Serialize};

/// A pick added in the builder (contract `AddedPick`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedPick {
    /// The inventory line that holds it (what `remove_pick` names).
    pub inventory_id: String,
    pub kind: AddedKind,
    pub element_id: String,
    pub name: String,
    pub source_name: String,
    /// The spellcasting class (class name) a spell was added to.
    pub spellcasting: Option<String>,
    /// Registered on the character; false while its prerequisites fail with "Ignore feat
    /// prerequisites" off (it is kept, not applied).
    pub applied: bool,
    /// HR3: applies only because "Ignore feat prerequisites" is on.
    pub prerequisites_unmet: bool,
}

/// One row of an Add dialog (contract `AddPickOption`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPickOption {
    pub id: String,
    pub name: String,
    pub source_name: String,
    /// Prerequisite text (empty for none).
    pub prerequisite: String,
    /// Spell level (spells; 0 = cantrip).
    pub level: Option<u32>,
    /// Proficiency group (`Skill`, `Saving Throw`, `Armor`, `Weapon`, `Tool`, `Other`).
    pub group: Option<String>,
    /// The character meets its prerequisites; false rows are offered only under the "Ignore
    /// feat prerequisites" house rule.
    pub qualifies: bool,
}

impl From<AddOption> for AddPickOption {
    fn from(o: AddOption) -> Self {
        Self {
            id: o.id,
            name: o.name,
            source_name: o.source,
            prerequisite: o.prerequisite,
            level: o.level,
            group: o.group,
            qualifies: o.qualifies,
        }
    }
}

fn added_pick(p: &AddedPickSnapshot) -> AddedPick {
    AddedPick {
        inventory_id: p.uid.clone(),
        kind: p.kind,
        element_id: p.element.id.clone(),
        name: p.element.name.clone(),
        source_name: p.element.source.clone(),
        spellcasting: p.spellcasting.clone(),
        applied: p.applied,
        prerequisites_unmet: p.prerequisites_unmet,
    }
}

/// The snapshot's `addedPicks`.
#[must_use]
pub fn added_picks(snap: &Engine) -> Vec<AddedPick> {
    snap.added_picks.iter().map(added_pick).collect()
}

/// Was spell `spell` added to spellcasting class `class` in the builder?
#[must_use]
pub fn is_added_spell(snap: &Engine, class: &str, spell: &str) -> bool {
    snap.added_picks.iter().any(|p| {
        p.kind == AddedKind::Spell
            && p.element.id == spell
            && p.spellcasting.as_deref() == Some(class)
    })
}

/// A spell needs its class; nothing else takes one.
fn spellcasting_for(kind: AddedKind, spellcasting: Option<&str>) -> ApiResult<Option<&str>> {
    let cast = spellcasting.map(str::trim).filter(|s| !s.is_empty());
    match (kind, cast) {
        (AddedKind::Spell, None) => Err(ApiError::invalid(
            "Choose the spellcasting class the spell is added to.",
        )),
        (AddedKind::Spell, c) => Ok(c),
        _ => Ok(None),
    }
}

fn word(kind: AddedKind) -> &'static str {
    match kind {
        AddedKind::Feat => "feat",
        AddedKind::Spell => "spell",
        AddedKind::Language => "language",
        AddedKind::Proficiency => "proficiency",
    }
}

impl AppState {
    /// What the Add action of `kind` offers (a spell: for spellcasting class `spellcasting`).
    ///
    /// # Errors
    /// `not_found` if the character is not open; `invalid_argument` for a spell without a class.
    pub fn list_added_pick_options(
        &self,
        id: &str,
        kind: AddedKind,
        spellcasting: Option<&str>,
    ) -> ApiResult<Vec<AddPickOption>> {
        let cast = spellcasting_for(kind, spellcasting)?;
        self.read(id, |ctx, s| {
            Ok(ctx
                .hub
                .engine
                .added_pick_options(&s.character.build, kind, cast)
                .into_iter()
                .map(AddPickOption::from)
                .collect())
        })
    }

    /// Add element `element_id` as a pick of `kind` (a spell to class `spellcasting`).
    ///
    /// # Errors
    /// `not_found` for an element that is not installed, `requirement_failed` for one the Add
    /// action does not offer, `invalid_argument` for a spell without a class.
    pub fn add_pick(
        &self,
        id: &str,
        kind: AddedKind,
        element_id: &str,
        spellcasting: Option<&str>,
    ) -> ApiResult<CharacterSnapshot> {
        let cast = spellcasting_for(kind, spellcasting)?;
        self.mutate(id, |ctx, d| {
            let out =
                commands::add_pick(&ctx.hub.engine, &d.character.build, kind, element_id, cast)
                    .map_err(|e| match e {
                        CommandError::NotAllowed(_) => ApiError::requirement(format!(
                            "That {} cannot be added to this character.",
                            word(kind)
                        )),
                        CommandError::UnknownElement(_) => {
                            ApiError::not_found(format!("That {} is not installed.", word(kind)))
                        }
                        other => other.into(),
                    })?;
            apply(d, out);
            Ok(())
        })
    }

    /// Remove the added pick held by inventory line `inventory_id`.
    ///
    /// # Errors
    /// `requirement_failed` when the line is not an added pick.
    pub fn remove_pick(&self, id: &str, inventory_id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::remove_pick(&ctx.hub.engine, &d.character.build, inventory_id)
                .map_err(|_| ApiError::requirement("That is not a pick added in the builder."))?;
            apply(d, out);
            Ok(())
        })
    }
}
