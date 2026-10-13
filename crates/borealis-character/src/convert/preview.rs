//! The reviewable result of a 2014 -> 2024 conversion.

use crate::model::Character;
use borealis_content::MapKind;
use borealis_engine::CharacterSnapshot;
use serde::{Deserialize, Serialize};

/// What a converted item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Race -> Species (including folded subraces).
    Species,
    /// Background.
    Background,
    /// Class.
    Class,
    /// Subclass / archetype.
    Subclass,
    /// Feat or ability score improvement.
    Feat,
    /// Spell.
    Spell,
    /// Inventory item.
    Item,
    /// Language.
    Language,
    /// Ability score sources.
    AbilityScore,
    /// Any other stored choice.
    Choice,
}

/// What happened to an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// Carried over unchanged.
    Kept {
        /// Element id.
        id: String,
        /// Kept as a foreign-edition pick.
        cross_edition: bool,
    },
    /// Replaced by its 2024 counterpart.
    Mapped {
        /// 2014 id.
        from: String,
        /// 2024 id.
        to: String,
        /// How close the counterpart is.
        kind: Option<MapKind>,
    },
    /// Removed, with the reason.
    Dropped {
        /// Element id (or a description).
        id: String,
        /// Why it could not be carried over.
        reason: String,
    },
    /// An empty slot the user must fill.
    NeedsChoice {
        /// What to choose.
        what: String,
    },
}

/// One line of the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionItem {
    /// What kind of item.
    pub category: Category,
    /// Display label (the 2014 name where there is one).
    pub label: String,
    /// What happened.
    pub outcome: Outcome,
}

/// Key stats shown before and after.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStats {
    /// Character level.
    pub level: u32,
    /// Maximum hit points.
    pub hit_points: i32,
    /// Armor class.
    pub armor_class: i32,
    /// Initiative modifier.
    pub initiative: i32,
    /// Proficiency bonus.
    pub proficiency_bonus: i32,
    /// Walking speed.
    pub speed: i32,
    /// Ability scores in STR, DEX, CON, INT, WIS, CHA order.
    pub ability_scores: [i32; 6],
    /// Spell save DC of the first spellcasting class.
    pub spell_save_dc: Option<i32>,
}

impl KeyStats {
    /// Extract the key stats from an engine snapshot.
    #[must_use]
    pub fn from_snapshot(s: &CharacterSnapshot) -> Self {
        let mut ability_scores = [0; 6];
        for (slot, a) in ability_scores.iter_mut().zip(&s.abilities) {
            *slot = a.score;
        }
        Self {
            level: s.level,
            hit_points: s.hit_points,
            armor_class: s.armor_class,
            initiative: s.initiative,
            proficiency_bonus: s.proficiency_bonus,
            speed: s.speed.walk,
            ability_scores,
            spell_save_dc: s.spellcasting.first().map(|c| c.dc),
        }
    }
}

/// The diff of a conversion plus the converted copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionPreview {
    /// Every item, in review order.
    pub items: Vec<ConversionItem>,
    /// Stats of the original.
    pub before: KeyStats,
    /// Stats of the converted copy (unresolved choices still empty).
    pub after: KeyStats,
    pub(crate) converted: Character,
}

impl ConversionPreview {
    /// Items that need a user decision.
    pub fn pending(&self) -> impl Iterator<Item = &ConversionItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.outcome, Outcome::NeedsChoice { .. }))
    }

    /// Items that were dropped.
    pub fn dropped(&self) -> impl Iterator<Item = &ConversionItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.outcome, Outcome::Dropped { .. }))
    }

    /// The converted copy this preview would apply.
    #[must_use]
    pub fn converted(&self) -> &Character {
        &self.converted
    }
}
