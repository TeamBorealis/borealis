//! Typed data model for `type: "Monster"` elements.
//!
//! A Monster element carries its stat block in the element `data` map (see `docs/monster-model.md`).
//! [`MonsterData::from_element`] / [`MonsterData::to_element`] convert between the two, and
//! [`MonsterData::validate`] returns the hard consistency problems the content store reports,
//! [`MonsterData::warnings`] the printed numbers it keeps but reports (a non-standard XP).
//! Softer checks that may legitimately differ in the printed source live in [`MonsterData::soft_checks`].

use crate::element::Element;
use crate::ruleset::Ruleset;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

pub use crate::monster_types::{
    Abilities, Ability, ArmorClass, Attack, AttackKind, Cr, CreatureSize, CreatureType,
    DamageClause, DamagePart, DamageType, Feature, HitPoints, Initiative, LairOption, LairSection,
    LegendaryActions, RestKind, SaveEffect, Senses, Skill, Speeds, SpellFrequency, SpellList,
    SpellRef, SpellcastingBlock, Usage,
};

/// What a sheet shows for a creature with neither a challenge rating nor a note for it (also
/// for an armor class or hit points with neither a number nor a note).
pub const NO_CHALLENGE_RATING: &str = "\u{2014}";

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
    /// Armor classes (more than one when the AC depends on a form). Empty only when the armor
    /// class is printed as a formula (decision IM12a), kept as text in `ac_note`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ac: Vec<ArmorClass>,
    /// The armor class as display text when it is a formula, not a number (a base plus the
    /// summoning spell's level). Only with no `ac`; nothing computes with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac_note: Option<String>,
    /// Hit points. Absent only when they are printed as a formula (decision IM12a), kept as
    /// text in `hp_note`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp: Option<HitPoints>,
    /// The hit points as display text when they are a formula, not a number (a base plus so many
    /// per spell level). Only with no `hp`; nothing computes with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp_note: Option<String>,
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
    /// Challenge rating. Absent for a creature printed without one (decision IM12: a summoned
    /// spirit whose numbers follow the spell, a level-scaled companion); such a creature has no
    /// XP and no select or filter that compares challenge ratings offers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cr: Option<Cr>,
    /// What stands in for a missing challenge rating, as display text ("Scales with spell
    /// level (Summon Beast)", "Level 5"). Only with no `cr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cr_note: Option<String>,
    /// Experience points (absent exactly when there is no `cr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xp: Option<u32>,
    /// Experience points when fought in its lair ("XP 18,000, or 20,000 in lair").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xp_lair: Option<u32>,
    /// Proficiency bonus (derived from the CR, stored for convenience). With no `cr` it may be
    /// absent (a summoned creature uses its summoner's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proficiency_bonus: Option<u8>,
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
    /// Lair actions (printed with the creature's lair).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lair_actions: Option<LairSection>,
    /// Regional effects (printed with the creature's lair).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regional_effects: Option<LairSection>,
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

    /// Numbers kept as given that differ from the standard tables (the content store reports these
    /// as warnings): an XP other than the standard XP of the challenge rating. Nothing recomputes
    /// or replaces the stored value.
    #[must_use]
    pub fn warnings(&self) -> Vec<MonsterIssue> {
        crate::monster_validate::warnings(self)
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

    /// The challenge rating as shown on a sheet or card: the CR (`1/4`), else the note that
    /// stands in for it, else an em dash.
    #[must_use]
    pub fn challenge_text(&self) -> String {
        match &self.cr {
            Some(cr) => cr.as_text(),
            None => note_or_dash(self.cr_note.as_deref()),
        }
    }

    /// The first armor class number, or `None` when the AC is a formula (decision IM12a).
    #[must_use]
    pub fn armor_class(&self) -> Option<u32> {
        self.ac.first().map(|a| a.value)
    }

    /// The average hit points, or `None` when they are a formula (decision IM12a).
    #[must_use]
    pub fn hit_points(&self) -> Option<u32> {
        self.hp.as_ref().map(|hp| hp.average)
    }

    /// The armor class as shown on a sheet or card (`13 (natural armor)`, several forms joined
    /// with `, `), else the formula text that stands in for it, else an em dash.
    #[must_use]
    pub fn armor_class_text(&self) -> String {
        if self.ac.is_empty() {
            return note_or_dash(self.ac_note.as_deref());
        }
        self.ac
            .iter()
            .map(|a| {
                let mut text = a.value.to_string();
                if let Some(source) = a.source.as_deref().filter(|s| !s.trim().is_empty()) {
                    text.push_str(&format!(" ({})", source.trim()));
                }
                if let Some(condition) = a.condition.as_deref().filter(|c| !c.trim().is_empty()) {
                    text.push_str(&format!(" {}", condition.trim()));
                }
                text
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The hit points as shown on a sheet or card (`11 (2d8+2)`), else the formula text that
    /// stands in for them, else an em dash.
    #[must_use]
    pub fn hit_points_text(&self) -> String {
        match &self.hp {
            Some(HitPoints {
                average,
                formula: Some(f),
            }) => format!("{average} ({f})"),
            Some(hp) => hp.average.to_string(),
            None => note_or_dash(self.hp_note.as_deref()),
        }
    }

    /// Ability modifier of `ability`.
    #[must_use]
    pub fn modifier(&self, ability: Ability) -> i32 {
        self.abilities.modifier(ability)
    }
}

/// A trimmed display note, or an em dash when there is none.
fn note_or_dash(note: Option<&str>) -> String {
    note.map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or_else(|| NO_CHALLENGE_RATING.to_string(), str::to_string)
}
