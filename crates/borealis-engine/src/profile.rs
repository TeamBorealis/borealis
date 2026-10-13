//! Ruleset profiles.
//!
//! The evaluation pipeline is profile agnostic; behaviour that differs between 2014 and 2024
//! is reached through the [`RulesetProfile`] trait. [`profile_for`] is the only place outside
//! tests that matches on [`Ruleset`]. [`Dnd2014`] is the shipped behaviour; [`Dnd2024`] returns
//! the design values and reports every area the engine does not implement yet as a
//! `profile.unimplemented` warning.

use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};

/// Where ability score bonuses come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityScoreSource {
    /// Race / species rules.
    Race,
    /// Background rules (2024).
    Background,
    /// Class ability score improvements and feats.
    LevelUp,
}

/// How the proficiency bonus is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProficiencySource {
    /// From stat rules on the Level elements (2014 content).
    ContentRules,
    /// From a table owned by the profile; Level element `proficiency` rules are ignored.
    Table([i32; 20]),
}

/// Rounding of half-caster levels in the multiclass spellcaster level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasterRounding {
    /// Round down (2014).
    Down,
    /// Round up (2024 multiclass rule).
    Up,
}

/// How prepared spells are limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpellPreparation {
    /// Class formula (level + modifier), 2014.
    ClassFormula,
    /// Fixed class table, 2024.
    ClassTable,
}

/// What the level-up spell swap rule looks at for one filled spell selection slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellSwapQuery {
    /// The pick is a cantrip.
    pub cantrip: bool,
    /// The select rule's own `allow_replace` setting (`None` = not set).
    pub rule_allows: Option<bool>,
    /// The spellcasting class swaps learned spells (`allow_replace` on the class).
    pub class_allows: bool,
}

impl SpellSwapQuery {
    /// The content's answer: an explicit rule setting wins over the class setting.
    #[must_use]
    pub fn content_allows(self) -> bool {
        self.rule_allows.unwrap_or(self.class_allows)
    }
}

/// Exhaustion model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExhaustionModel {
    /// Six-level table (2014).
    SixLevels,
    /// Linear penalties per level (2024).
    Linear,
}

/// Inspiration model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspirationModel {
    /// Advantage-granting flag (2014).
    Advantage,
    /// Heroic Inspiration reroll (2024).
    HeroicReroll,
}

/// Ability score generation rules of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbilityGeneration {
    /// Default point buy budget.
    pub point_buy_budget: i32,
    /// Lowest and highest base score a point buy assignment may use.
    pub point_buy_range: (i32, i32),
    /// The standard array, highest first.
    pub standard_array: [i32; 6],
}

/// Diagnostic code for an area a profile names but the engine does not implement.
pub const UNIMPLEMENTED_CODE: &str = "profile.unimplemented";

const PROFICIENCY_TABLE: [i32; 20] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6];

const XP_TABLE: [u32; 20] = [
    0, 300, 900, 2_700, 6_500, 14_000, 23_000, 34_000, 48_000, 64_000, 85_000, 100_000, 120_000,
    140_000, 165_000, 195_000, 225_000, 265_000, 305_000, 355_000,
];

const ABILITY_GENERATION: AbilityGeneration = AbilityGeneration {
    point_buy_budget: 27,
    point_buy_range: (8, 15),
    standard_array: [15, 14, 13, 12, 10, 8],
};

/// Edition specific behaviour. Implemented by [`Dnd2014`] and [`Dnd2024`]; obtain one with
/// [`profile_for`].
pub trait RulesetProfile: std::fmt::Debug + Send + Sync {
    /// The ruleset this profile implements.
    fn ruleset(&self) -> Ruleset;
    /// Display label.
    fn label(&self) -> &'static str;
    /// Index of the element universe the engine keeps for this profile.
    fn universe_index(&self) -> usize;
    /// Ordered sources of ability score bonuses.
    fn ability_score_sources(&self) -> &'static [AbilityScoreSource];
    /// Where the proficiency bonus comes from.
    fn proficiency_source(&self) -> ProficiencySource;
    /// True if characters take an (origin) feat at level 1.
    fn feat_at_level_1(&self) -> bool;
    /// Level at which subclasses are chosen when the class does not say (`None` = class decides).
    fn default_subclass_level(&self) -> Option<u32>;
    /// Rounding applied to half-caster levels when pooling multiclass spell slots.
    fn half_caster_rounding(&self) -> CasterRounding;
    /// Spell preparation model.
    fn spell_preparation(&self) -> SpellPreparation;
    /// True if the profile has weapon mastery.
    fn weapon_mastery(&self) -> bool;
    /// Exhaustion model.
    fn exhaustion(&self) -> ExhaustionModel;
    /// Inspiration model.
    fn inspiration(&self) -> InspirationModel;
    /// True if feats are grouped in categories (origin, general, epic boon).
    fn feat_categories(&self) -> bool;
    /// Ability score generation rules.
    fn ability_generation(&self) -> AbilityGeneration {
        ABILITY_GENERATION
    }
    /// XP thresholds for levels 1..=20 (used when the Level elements carry none).
    fn xp_thresholds(&self) -> [u32; 20] {
        XP_TABLE
    }
    /// Id of the multiclass spellcasting slot table element.
    fn multiclass_slot_table(&self) -> &'static str {
        crate::ids::MULTICLASS_SPELLCASTING
    }
    /// Proficiency bonus for a character level (both editions share the progression).
    fn proficiency_bonus(&self, level: u32) -> i32 {
        PROFICIENCY_TABLE[(level.clamp(1, 20) - 1) as usize]
    }
    /// Applies the profile's lower bound to a raw prepared-spell limit. Returns the limit and
    /// whether it was raised (2014 only).
    fn prepared_limit(&self, raw: i32) -> (i32, bool);
    /// May the spell in a filled selection slot be replaced on level-up?
    fn spell_swap_on_level_up(&self, q: SpellSwapQuery) -> bool;
    /// Whether foreign-edition content is allowed by default (strict mode off).
    fn default_allow_cross_edition(&self) -> bool;
    /// May an element of type `ty` authored for `from` be used by this profile at all?
    fn accepts_foreign(&self, ty: &str, from: Ruleset) -> bool;
    /// Standard languages chosen at character creation besides Common (0 = languages come from
    /// content, 2014).
    fn creation_languages(&self) -> u32 {
        0
    }
    /// Areas the profile names but the engine does not implement yet.
    fn unimplemented_areas(&self) -> &'static [&'static str] {
        &[]
    }
}

/// D&D 2014 (5e): the shipped behaviour.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dnd2014;

/// D&D 2024 (5.5): design values; areas in [`RulesetProfile::unimplemented_areas`] are stubs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dnd2024;

impl RulesetProfile for Dnd2014 {
    fn ruleset(&self) -> Ruleset {
        Ruleset::Dnd2014
    }
    fn label(&self) -> &'static str {
        "D&D 2014 (5e)"
    }
    fn universe_index(&self) -> usize {
        0
    }
    fn ability_score_sources(&self) -> &'static [AbilityScoreSource] {
        &[AbilityScoreSource::Race, AbilityScoreSource::LevelUp]
    }
    fn proficiency_source(&self) -> ProficiencySource {
        ProficiencySource::ContentRules
    }
    fn feat_at_level_1(&self) -> bool {
        false
    }
    fn default_subclass_level(&self) -> Option<u32> {
        None
    }
    fn half_caster_rounding(&self) -> CasterRounding {
        CasterRounding::Down
    }
    fn spell_preparation(&self) -> SpellPreparation {
        SpellPreparation::ClassFormula
    }
    fn weapon_mastery(&self) -> bool {
        false
    }
    fn exhaustion(&self) -> ExhaustionModel {
        ExhaustionModel::SixLevels
    }
    fn inspiration(&self) -> InspirationModel {
        InspirationModel::Advantage
    }
    fn feat_categories(&self) -> bool {
        false
    }
    fn prepared_limit(&self, raw: i32) -> (i32, bool) {
        (raw.max(1), raw < 1)
    }
    /// 2014: known casters swap one learned spell on level-up; cantrips are never swapped.
    fn spell_swap_on_level_up(&self, q: SpellSwapQuery) -> bool {
        !q.cantrip && q.content_allows()
    }
    fn default_allow_cross_edition(&self) -> bool {
        false
    }
    fn accepts_foreign(&self, ty: &str, from: Ruleset) -> bool {
        // In 2014 characters 2024 content is limited to spells and items.
        from != Ruleset::Dnd2024
            || matches!(ty, "Spell" | "Magic Item" | "Weapon" | "Armor" | "Item")
    }
}

impl RulesetProfile for Dnd2024 {
    fn ruleset(&self) -> Ruleset {
        Ruleset::Dnd2024
    }
    fn label(&self) -> &'static str {
        "D&D 2024 (5.5)"
    }
    fn universe_index(&self) -> usize {
        1
    }
    fn ability_score_sources(&self) -> &'static [AbilityScoreSource] {
        &[AbilityScoreSource::Background, AbilityScoreSource::LevelUp]
    }
    fn proficiency_source(&self) -> ProficiencySource {
        ProficiencySource::Table(PROFICIENCY_TABLE)
    }
    fn feat_at_level_1(&self) -> bool {
        true
    }
    fn default_subclass_level(&self) -> Option<u32> {
        Some(3)
    }
    fn half_caster_rounding(&self) -> CasterRounding {
        CasterRounding::Up
    }
    fn spell_preparation(&self) -> SpellPreparation {
        SpellPreparation::ClassTable
    }
    fn weapon_mastery(&self) -> bool {
        true
    }
    fn exhaustion(&self) -> ExhaustionModel {
        ExhaustionModel::Linear
    }
    fn inspiration(&self) -> InspirationModel {
        InspirationModel::HeroicReroll
    }
    fn feat_categories(&self) -> bool {
        true
    }
    fn prepared_limit(&self, raw: i32) -> (i32, bool) {
        (raw.max(0), false)
    }
    /// 2024: a class level gain also lets a caster replace a cantrip.
    fn spell_swap_on_level_up(&self, q: SpellSwapQuery) -> bool {
        q.content_allows()
    }
    fn default_allow_cross_edition(&self) -> bool {
        true
    }
    fn accepts_foreign(&self, ty: &str, from: Ruleset) -> bool {
        // 2014 races, backgrounds, subclasses, feats, spells and items adapt into 2024.
        // Classes never cross editions.
        from != Ruleset::Dnd2014 || ty != "Class"
    }
    fn creation_languages(&self) -> u32 {
        2
    }
    fn unimplemented_areas(&self) -> &'static [&'static str] {
        &[]
    }
}

static DND2014: Dnd2014 = Dnd2014;
static DND2024: Dnd2024 = Dnd2024;

/// Every profile, in `universe_index` order.
#[must_use]
pub fn all_profiles() -> [&'static dyn RulesetProfile; 2] {
    [&DND2014, &DND2024]
}

/// The profile for a ruleset (`Any` behaves as 2014). The only `match` on [`Ruleset`] outside tests.
#[must_use]
pub fn profile_for(ruleset: Ruleset) -> &'static dyn RulesetProfile {
    match ruleset {
        Ruleset::Dnd2024 => &DND2024,
        Ruleset::Dnd2014 | Ruleset::Any => &DND2014,
    }
}

/// The ruleset new characters and unit-test fixtures default to.
#[must_use]
pub fn default_ruleset() -> Ruleset {
    Ruleset::Dnd2014
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spell_swap_rule_differs_for_cantrips_and_honours_the_rule_flag() {
        let q = |cantrip, rule_allows, class_allows| SpellSwapQuery {
            cantrip,
            rule_allows,
            class_allows,
        };
        let (p14, p24) = (profile_for(Ruleset::Dnd2014), profile_for(Ruleset::Dnd2024));
        // known caster, leveled spell: both editions swap
        assert!(p14.spell_swap_on_level_up(q(false, None, true)));
        assert!(p24.spell_swap_on_level_up(q(false, None, true)));
        // cantrips: 2014 never, 2024 on a class level gain
        assert!(!p14.spell_swap_on_level_up(q(true, Some(true), true)));
        assert!(p24.spell_swap_on_level_up(q(true, Some(true), true)));
        // the rule's allow-replace setting wins over the class
        assert!(!p14.spell_swap_on_level_up(q(false, Some(false), true)));
        assert!(!p24.spell_swap_on_level_up(q(true, Some(false), true)));
        assert!(p14.spell_swap_on_level_up(q(false, Some(true), false)));
        // classes that do not swap (2014 wizard)
        assert!(!p14.spell_swap_on_level_up(q(false, None, false)));
    }

    #[test]
    fn profile_for_maps_every_ruleset() {
        assert_eq!(profile_for(Ruleset::Dnd2014).ruleset(), Ruleset::Dnd2014);
        assert_eq!(profile_for(Ruleset::Dnd2024).ruleset(), Ruleset::Dnd2024);
        assert_eq!(profile_for(Ruleset::Any).ruleset(), Ruleset::Dnd2014);
        for (i, p) in all_profiles().iter().enumerate() {
            assert_eq!(p.universe_index(), i);
        }
    }

    #[test]
    fn profiles_differ_only_where_designed() {
        let a = profile_for(Ruleset::Dnd2014);
        let b = profile_for(Ruleset::Dnd2024);
        assert_eq!(a.proficiency_source(), ProficiencySource::ContentRules);
        assert!(matches!(
            b.proficiency_source(),
            ProficiencySource::Table(_)
        ));
        assert!(!a.feat_at_level_1() && b.feat_at_level_1());
        assert_eq!(a.half_caster_rounding(), CasterRounding::Down);
        assert_eq!(b.half_caster_rounding(), CasterRounding::Up);
        assert_eq!(a.ability_score_sources()[0], AbilityScoreSource::Race);
        assert_eq!(b.ability_score_sources()[0], AbilityScoreSource::Background);
        assert_eq!(a.proficiency_bonus(1), 2);
        assert_eq!(a.proficiency_bonus(5), 3);
        assert_eq!(a.proficiency_bonus(17), 6);
        assert_eq!(a.default_subclass_level(), None);
        assert_eq!(b.default_subclass_level(), Some(3));
        assert_eq!(a.prepared_limit(-2), (1, true));
        assert_eq!(a.prepared_limit(4), (4, false));
        assert_eq!(b.prepared_limit(-2), (0, false));
        assert!(!a.weapon_mastery() && b.weapon_mastery());
        assert_eq!(a.exhaustion(), ExhaustionModel::SixLevels);
        assert_eq!(b.inspiration(), InspirationModel::HeroicReroll);
        assert!(!a.default_allow_cross_edition() && b.default_allow_cross_edition());
        assert!(!b.accepts_foreign("Class", Ruleset::Dnd2014));
        assert!(b.accepts_foreign("Race", Ruleset::Dnd2014));
        assert!(!a.accepts_foreign("Class", Ruleset::Dnd2024));
        assert!(a.accepts_foreign("Spell", Ruleset::Dnd2024));
        assert!(a.unimplemented_areas().is_empty());
        assert!(b.unimplemented_areas().is_empty());
    }

    #[test]
    fn ability_generation_values() {
        let g = profile_for(Ruleset::Dnd2014).ability_generation();
        assert_eq!(g.point_buy_budget, 27);
        assert_eq!(g.point_buy_range, (8, 15));
        assert_eq!(g.standard_array, [15, 14, 13, 12, 10, 8]);
        assert_eq!(profile_for(Ruleset::Dnd2024).xp_thresholds()[19], 355_000);
    }
}
