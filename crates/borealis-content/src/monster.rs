//! Typed data model for `type: "Monster"` elements.
//!
//! A Monster element carries its stat block in the element `data` map (see `docs/monster-model.md`).
//! [`MonsterData::from_element`] / [`MonsterData::to_element`] convert between the two, and
//! [`MonsterData::validate`] returns the hard consistency problems the content store reports.
//! Softer checks that may legitimately differ in the printed source live in [`MonsterData::soft_checks`].

use crate::element::Element;
use crate::ruleset::Ruleset;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

pub use crate::monster_types::{
    Abilities, Ability, ArmorClass, Attack, AttackKind, Cr, CreatureSize, CreatureType,
    DamageClause, DamagePart, DamageType, Feature, HitPoints, Initiative, LegendaryActions,
    RestKind, SaveEffect, Senses, Skill, Speeds, SpellFrequency, SpellList, SpellRef,
    SpellcastingBlock, Usage,
};

/// A problem found in a [`MonsterData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterIssue {
    /// Field the problem is about (`cr`, `actions[2].attack`, ...).
    pub field: String,
    /// What is wrong.
    pub message: String,
}

impl MonsterIssue {
    pub(crate) fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for MonsterIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Why an element could not be read as a monster.
#[derive(Debug, thiserror::Error)]
pub enum MonsterError {
    /// The element type is not `Monster`.
    #[error("element {0} is not a Monster")]
    NotAMonster(String),
    /// The `data` map does not match the model.
    #[error("monster data of {id}: {source}")]
    Data {
        /// Element id.
        id: String,
        /// The serde error.
        source: serde_json::Error,
    },
}

/// A full monster stat block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterData {
    /// The ruleset whose stat block conventions this follows.
    pub ruleset: Ruleset,
    /// Size (several for "Medium or Small").
    pub size: Vec<CreatureSize>,
    /// For swarms: the size of each member ("Medium swarm of Tiny beasts" -> `tiny`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swarm_of: Option<CreatureSize>,
    /// Creature type.
    pub creature_type: CreatureType,
    /// Parenthetical tags: `goblinoid`, `any race`, `human, shapechanger`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Alignment as printed (`neutral evil`, `unaligned`, `any non-good alignment`).
    pub alignment: String,
    /// Armor classes (more than one when the AC depends on a form).
    pub ac: Vec<ArmorClass>,
    /// Hit points.
    pub hp: HitPoints,
    /// Movement.
    pub speed: Speeds,
    /// Initiative modifier and score (SRD 5.2.1 stat blocks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiative: Option<Initiative>,
    /// Ability scores.
    pub abilities: Abilities,
    /// Saving throw bonuses. Only saves that differ from the ability modifier (proficient saves) are listed;
    /// [`MonsterData::save_bonus`] returns the printed value of any ability.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub saves: BTreeMap<Ability, i32>,
    /// Skill bonuses.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<Skill, i32>,
    /// Printed remarks on skills ("Stealth: +6 in dim light").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_notes: Vec<String>,
    /// Damage vulnerabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage_vulnerabilities: Vec<DamageClause>,
    /// Damage resistances.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage_resistances: Vec<DamageClause>,
    /// Damage immunities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage_immunities: Vec<DamageClause>,
    /// Condition immunities (lowercase condition names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub condition_immunities: Vec<String>,
    /// Equipment printed in the `Gear` line (SRD 5.2.1), one entry per item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gear: Vec<String>,
    /// Senses and passive Perception.
    pub senses: Senses,
    /// Languages as printed, one entry per comma separated item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Telepathy range in feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telepathy: Option<u32>,
    /// Challenge rating.
    pub cr: Cr,
    /// Experience points.
    pub xp: u32,
    /// Experience points when fought in its lair ("XP 18,000, or 20,000 in lair").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xp_lair: Option<u32>,
    /// Proficiency bonus (derived from the CR, stored for convenience).
    pub proficiency_bonus: u8,
    /// Passive traits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<Feature>,
    /// Actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Feature>,
    /// Bonus actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bonus_actions: Vec<Feature>,
    /// Reactions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<Feature>,
    /// Legendary actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legendary: Option<LegendaryActions>,
    /// Flavour text printed with the stat block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl MonsterData {
    /// Read the stat block out of a `Monster` element's `data` map.
    ///
    /// # Errors
    /// [`MonsterError::NotAMonster`] for other types, [`MonsterError::Data`] when `data` does not fit.
    pub fn from_element(element: &Element) -> Result<Self, MonsterError> {
        if element.ty != "Monster" {
            return Err(MonsterError::NotAMonster(element.id.clone()));
        }
        let map: serde_json::Map<String, Value> = element
            .data
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        serde_json::from_value(Value::Object(map)).map_err(|source| MonsterError::Data {
            id: element.id.clone(),
            source,
        })
    }

    /// Build a `Monster` element carrying this stat block in `data` (and the ruleset tag).
    ///
    /// # Panics
    /// Never in practice: the model always serializes to a JSON object.
    #[must_use]
    pub fn to_element(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Element {
        let mut e = Element::new(id, name, "Monster", source);
        e.ruleset = Some(self.ruleset);
        e.data = match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m.into_iter().collect(),
            other => panic!("monster data is not an object: {other:?}"),
        };
        e
    }

    /// Hard consistency problems (the content store reports these as errors).
    #[must_use]
    pub fn validate(&self) -> Vec<MonsterIssue> {
        crate::monster_validate::validate(self)
    }

    /// Checks the printed source may legitimately violate (HP average vs formula, passive Perception,
    /// attack damage averages). Used by the extraction QA tests, not by the store.
    #[must_use]
    pub fn soft_checks(&self) -> Vec<MonsterIssue> {
        crate::monster_validate::soft_checks(self)
    }

    /// Saving throw bonus as printed: the listed proficient bonus, else the ability modifier.
    #[must_use]
    pub fn save_bonus(&self, ability: Ability) -> i32 {
        self.saves
            .get(&ability)
            .copied()
            .unwrap_or_else(|| self.modifier(ability))
    }

    /// Ability modifier of `ability`.
    #[must_use]
    pub fn modifier(&self, ability: Ability) -> i32 {
        self.abilities.modifier(ability)
    }
}
