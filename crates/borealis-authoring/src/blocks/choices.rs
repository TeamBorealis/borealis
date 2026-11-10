//! Closed value sets of block fields, with their plain words. Values are spelled as the engine readers read them and
//! as the bundled packs write them (SRD rules terms only, CS1).

use serde::Serialize;

/// One value of a combo or chip field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Choice {
    /// What the rule stores.
    pub value: &'static str,
    /// What the dialog shows.
    pub label: &'static str,
}

const fn c(value: &'static str, label: &'static str) -> Choice {
    Choice { value, label }
}

/// The abilities, as rule fields write them.
pub const ABILITIES: &[Choice] = &[
    c("strength", "Strength"),
    c("dexterity", "Dexterity"),
    c("constitution", "Constitution"),
    c("intelligence", "Intelligence"),
    c("wisdom", "Wisdom"),
    c("charisma", "Charisma"),
];

/// `resource.recharge` tokens (`rule_kinds::RESOURCE_RECHARGE`, same order).
pub const RECHARGE: &[Choice] = &[
    c("short_rest", "Short rest"),
    c("long_rest", "Long rest"),
    c("turn_start", "Start of each turn"),
    c("initiative", "Rolling initiative"),
    c("none", "Never (by other means only)"),
];

/// The two rests (free casts, slot recovery, regains).
pub const RESTS: &[Choice] = &[c("short_rest", "Short rest"), c("long_rest", "Long rest")];

/// When a pick may be changed (`swap_on`, `change_on`, `when`).
pub const CHANGE_ON: &[Choice] = &[
    c("long_rest", "After a long rest"),
    c("short_rest", "After a short rest"),
    c("gain_level", "On gaining a level"),
];

/// `grant_spell.mode` (`rule_kinds::GRANT_SPELL_MODES`, same order).
pub const SPELL_MODES: &[Choice] = &[
    c("always_prepared", "Always prepared"),
    c("known", "Known"),
    c("known_cantrip", "Known cantrip"),
    c("at_will_no_slot", "At will, without a slot"),
    c("ritual_only", "Only as a ritual"),
];

/// Actions a feature takes.
pub const ACTIONS: &[Choice] = &[
    c("action", "Action"),
    c("bonus_action", "Bonus action"),
    c("reaction", "Reaction"),
    c("none", "No action"),
];

/// Conditions, lower case as the readers compare them.
pub const CONDITIONS: &[Choice] = &[
    c("blinded", "Blinded"),
    c("charmed", "Charmed"),
    c("deafened", "Deafened"),
    c("exhaustion", "Exhaustion"),
    c("frightened", "Frightened"),
    c("grappled", "Grappled"),
    c("incapacitated", "Incapacitated"),
    c("invisible", "Invisible"),
    c("paralyzed", "Paralyzed"),
    c("petrified", "Petrified"),
    c("poisoned", "Poisoned"),
    c("prone", "Prone"),
    c("restrained", "Restrained"),
    c("stunned", "Stunned"),
    c("unconscious", "Unconscious"),
];

/// Damage types, lower case.
pub const DAMAGE_TYPES: &[Choice] = &[
    c("acid", "Acid"),
    c("bludgeoning", "Bludgeoning"),
    c("cold", "Cold"),
    c("fire", "Fire"),
    c("force", "Force"),
    c("lightning", "Lightning"),
    c("necrotic", "Necrotic"),
    c("piercing", "Piercing"),
    c("poison", "Poison"),
    c("psychic", "Psychic"),
    c("radiant", "Radiant"),
    c("slashing", "Slashing"),
    c("thunder", "Thunder"),
];

/// Skills as `skill_bonus.skills` writes them.
pub const SKILLS: &[Choice] = &[
    c("Acrobatics", "Acrobatics"),
    c("Animal Handling", "Animal Handling"),
    c("Arcana", "Arcana"),
    c("Athletics", "Athletics"),
    c("Deception", "Deception"),
    c("History", "History"),
    c("Insight", "Insight"),
    c("Intimidation", "Intimidation"),
    c("Investigation", "Investigation"),
    c("Medicine", "Medicine"),
    c("Nature", "Nature"),
    c("Perception", "Perception"),
    c("Performance", "Performance"),
    c("Persuasion", "Persuasion"),
    c("Religion", "Religion"),
    c("Sleight of Hand", "Sleight of Hand"),
    c("Stealth", "Stealth"),
    c("Survival", "Survival"),
];

/// Skills as `roll_advantage.skill` writes them (lower case).
pub const SKILLS_LOWER: &[Choice] = &[
    c("acrobatics", "Acrobatics"),
    c("animal handling", "Animal Handling"),
    c("arcana", "Arcana"),
    c("athletics", "Athletics"),
    c("deception", "Deception"),
    c("history", "History"),
    c("insight", "Insight"),
    c("intimidation", "Intimidation"),
    c("investigation", "Investigation"),
    c("medicine", "Medicine"),
    c("nature", "Nature"),
    c("perception", "Perception"),
    c("performance", "Performance"),
    c("persuasion", "Persuasion"),
    c("religion", "Religion"),
    c("sleight of hand", "Sleight of Hand"),
    c("stealth", "Stealth"),
    c("survival", "Survival"),
];

/// Rolls `roll_advantage` reads (`rule_terms::roll_kind`, `combat::roll_text`).
pub const ROLLS: &[Choice] = &[
    c("ability_check", "Ability checks"),
    c("saving_throw", "Saving throws"),
    c("initiative", "Initiative"),
    c("spell_attack", "Spell attack rolls"),
];

/// Creature sizes.
pub const SIZES: &[Choice] = &[
    c("Tiny", "Tiny"),
    c("Small", "Small"),
    c("Medium", "Medium"),
    c("Large", "Large"),
    c("Huge", "Huge"),
    c("Gargantuan", "Gargantuan"),
];

/// Weapon properties (`proficiency_filter.properties_any`).
pub const WEAPON_PROPERTIES: &[Choice] = &[
    c("Ammunition", "Ammunition"),
    c("Finesse", "Finesse"),
    c("Heavy", "Heavy"),
    c("Light", "Light"),
    c("Loading", "Loading"),
    c("Reach", "Reach"),
    c("Thrown", "Thrown"),
    c("Two-Handed", "Two-Handed"),
    c("Versatile", "Versatile"),
];

/// Weapon categories.
pub const WEAPON_CATEGORIES: &[Choice] = &[c("Simple", "Simple"), c("Martial", "Martial")];

/// Weapon kinds of a pact weapon.
pub const WEAPON_KINDS: &[Choice] = &[
    c("Simple Melee", "Simple melee"),
    c("Martial Melee", "Martial melee"),
    c("Simple Ranged", "Simple ranged"),
    c("Martial Ranged", "Martial ranged"),
];

/// `attack_bonus.weapon` (`attack_rules::weapon_filter`; anything else applies to every weapon).
pub const ATTACK_WEAPONS: &[Choice] = &[
    c("any", "Any weapon"),
    c("melee", "Melee weapons"),
    c("ranged", "Ranged weapons"),
];

/// `damage_die_floor.weapon` (`weapon_filter`: "two-handed" or "versatile" = melee held in two hands).
pub const DIE_FLOOR_WEAPONS: &[Choice] = &[
    c(
        "melee two-handed or versatile held in two hands",
        "Melee weapons held in two hands",
    ),
    c("melee", "Melee weapons"),
    c("ranged", "Ranged weapons"),
    c("any", "Any weapon"),
];

/// `critical_range.applies_to` (`attack_rules::targets` looks for "weapon" and "unarmed").
pub const CRIT_APPLIES: &[Choice] = &[
    c("weapon attacks", "Weapon attacks"),
    c("unarmed strikes", "Unarmed strikes"),
];

/// `weapon_mastery.filter.weapon_kind`.
pub const MASTERY_KINDS: &[Choice] = &[c("any", "Any weapon"), c("melee", "Melee weapons")];

/// Dice sizes.
pub const DIE_SIZES: &[Choice] = &[
    c("d4", "d4"),
    c("d6", "d6"),
    c("d8", "d8"),
    c("d10", "d10"),
    c("d12", "d12"),
];

/// Senses with an engine meaning.
pub const SENSES: &[Choice] = &[
    c("darkvision", "Darkvision"),
    c("blindsight", "Blindsight"),
    c("tremorsense", "Tremorsense"),
    c("truesight", "Truesight"),
];

/// Movement modes.
pub const MOVEMENT_MODES: &[Choice] = &[
    c("fly", "Fly"),
    c("swim", "Swim"),
    c("climb", "Climb"),
    c("burrow", "Burrow"),
];

/// Speeds a toggle multiplies (`speed_toggles`).
pub const SPEEDS: &[Choice] = &[
    c("walk", "Walking"),
    c("fly", "Flying"),
    c("swim", "Swimming"),
    c("climb", "Climbing"),
    c("burrow", "Burrowing"),
];

/// Feat categories (`select_feat.category`).
pub const FEAT_CATEGORIES: &[Choice] = &[
    c("Origin", "Origin"),
    c("General", "General"),
    c("Fighting Style", "Fighting Style"),
    c("Epic Boon", "Epic Boon"),
];

/// Challenge ratings a Wild Shape form may have.
pub const CHALLENGE_RATINGS: &[Choice] = &[
    c("0", "0"),
    c("1/8", "1/8"),
    c("1/4", "1/4"),
    c("1/2", "1/2"),
    c("1", "1"),
    c("2", "2"),
    c("3", "3"),
    c("4", "4"),
    c("5", "5"),
    c("6", "6"),
];

/// `heroic_inspiration.trigger` (`snapshot.rs`).
pub const INSPIRATION_TRIGGERS: &[Choice] = &[
    c("long_rest", "After a long rest"),
    c(
        "start_of_turn_in_combat",
        "At the start of each turn in combat",
    ),
];

/// `regeneration.timing`.
pub const TURN_TIMINGS: &[Choice] = &[
    c("start_of_turn", "At the start of each turn"),
    c("end_of_turn", "At the end of each turn"),
];

/// `ritual_casting.source`.
pub const RITUAL_SOURCES: &[Choice] = &[
    c("spellbook", "Spells in the spellbook"),
    c("prepared", "Prepared spells"),
];

/// `prepared_spell_change.when`.
pub const PREPARE_WHEN: &[Choice] = &[
    c("long_rest", "After a long rest"),
    c("gain_level", "On gaining a level"),
];

/// `prepared_spell_change.count`.
pub const PREPARE_COUNT: &[Choice] = &[c("any", "Any number"), c("one", "One spell")];

/// `save_advantage.scope`.
pub const SAVE_SCOPES: &[Choice] = &[c("avoid_or_end", "To avoid or end the condition")];

/// `toggle.requires_not_wearing` (`play_rules::worn_blocker` reads "heavy" and "armor").
pub const NOT_WEARING: &[Choice] = &[c("heavy armor", "Heavy armor"), c("armor", "Any armor")];

/// `toggle.blocks`.
pub const TOGGLE_BLOCKS: &[Choice] = &[
    c("concentration", "Concentration"),
    c("spellcasting", "Casting spells"),
];

/// `damage_bonus_spell.once_per`, `extra_damage.once_per`.
pub const ONCE_PER: &[Choice] = &[c("turn", "Turn"), c("spell", "Spell")];

/// Stacking groups of a `stat` (`bonus`) the bundled packs use most.
pub const STAT_BONUSES: &[Choice] = &[
    c("base", "Base value (the highest counts)"),
    c("calculation", "Calculation (the highest counts)"),
    c("modifier", "Modifier"),
    c("ability", "Ability"),
    c("proficiency", "Proficiency"),
];

/// "all" (a regain of every use).
pub const ALL_WORD: &[Choice] = &[c("all", "All uses")];
/// "any" (any spell level).
pub const ANY_WORD: &[Choice] = &[c("any", "Any level")];
/// "speed" (equal to the walking speed).
pub const SPEED_WORD: &[Choice] = &[c("speed", "Equal to the walking speed")];

/// The label of `value` in `choices`.
#[must_use]
pub fn label_of(choices: &[Choice], value: &str) -> Option<&'static str> {
    choices.iter().find(|c| c.value == value).map(|c| c.label)
}

/// The values of `choices`.
#[cfg(test)]
fn values(choices: &[Choice]) -> Vec<&'static str> {
    choices.iter().map(|c| c.value).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::rule_kinds::{GRANT_SPELL_MODES, RESOURCE_RECHARGE};

    #[test]
    fn registry_value_sets_match_their_choices() {
        assert_eq!(values(RECHARGE), RESOURCE_RECHARGE);
        assert_eq!(values(SPELL_MODES), GRANT_SPELL_MODES);
    }

    #[test]
    fn values_are_unique_and_labelled() {
        for set in [
            ABILITIES,
            RECHARGE,
            CHANGE_ON,
            ACTIONS,
            CONDITIONS,
            DAMAGE_TYPES,
            SKILLS,
            SKILLS_LOWER,
            ROLLS,
            SIZES,
            WEAPON_PROPERTIES,
            SENSES,
            MOVEMENT_MODES,
            SPEEDS,
            FEAT_CATEGORIES,
            CHALLENGE_RATINGS,
        ] {
            let mut v = values(set);
            v.sort_unstable();
            v.dedup();
            assert_eq!(v.len(), set.len());
            assert!(set
                .iter()
                .all(|c| !c.label.is_empty() && !c.label.contains('_')));
        }
    }
}
