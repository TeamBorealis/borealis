//! Ids of the Borealis core pack (`app/content/core`, see `docs/core-pack-ids.md`) that
//! imported rules reference, plus the name -> id mappings.

use crate::util::id_fragment;

/// The six abilities as `(abbreviation, full name)`.
pub const ABILITIES: [(&str, &str); 6] = [
    ("str", "Strength"),
    ("dex", "Dexterity"),
    ("con", "Constitution"),
    ("int", "Intelligence"),
    ("wis", "Wisdom"),
    ("cha", "Charisma"),
];

/// Full ability name for an abbreviation (`str` -> `Strength`).
#[must_use]
pub fn ability_name(abbr: &str) -> Option<&'static str> {
    ABILITIES
        .iter()
        .find(|(a, _)| a.eq_ignore_ascii_case(abbr))
        .map(|(_, n)| *n)
}

/// Statistic group name of an ability (`str` -> `strength`).
#[must_use]
pub fn ability_stat(abbr: &str) -> Option<String> {
    ability_name(abbr).map(str::to_lowercase)
}

/// Saving throw proficiency id.
#[must_use]
pub fn saving_throw_id(abbr: &str) -> Option<String> {
    ability_name(abbr).map(|n| format!("ID_PROFICIENCY_SAVINGTHROW_{}", n.to_uppercase()))
}

const SKILLS: [&str; 18] = [
    "acrobatics",
    "animal handling",
    "arcana",
    "athletics",
    "deception",
    "history",
    "insight",
    "intimidation",
    "investigation",
    "medicine",
    "nature",
    "perception",
    "performance",
    "persuasion",
    "religion",
    "sleight of hand",
    "stealth",
    "survival",
];

/// Skill proficiency id for a lower-case skill name.
#[must_use]
pub fn skill_id(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    SKILLS.contains(&lower.as_str()).then(|| {
        format!(
            "ID_PROFICIENCY_SKILL_{}",
            lower.replace(' ', "").to_uppercase()
        )
    })
}

/// True for a recognised skill name.
#[must_use]
pub fn is_skill(name: &str) -> bool {
    skill_id(name).is_some()
}

/// Armor proficiency id for `light`, `medium`, `heavy` or `shield`.
#[must_use]
pub fn armor_prof_id(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "light" => "ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR",
        "medium" => "ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR",
        "heavy" => "ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR",
        "shield" => "ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS",
        _ => return None,
    })
}

const WEAPON_PROFS: &[&str] = &[
    "BATTLEAXE",
    "BLOWGUN",
    "CLUB",
    "CROSSBOW_HAND",
    "CROSSBOW_HEAVY",
    "CROSSBOW_LIGHT",
    "DAGGER",
    "DART",
    "FLAIL",
    "GLAIVE",
    "GREATAXE",
    "GREATCLUB",
    "GREATSWORD",
    "HALBERD",
    "HANDAXE",
    "JAVELIN",
    "LANCE",
    "LIGHT_HAMMER",
    "LONGBOW",
    "LONGSWORD",
    "MACE",
    "MAUL",
    "MORNINGSTAR",
    "NET",
    "PIKE",
    "QUARTERSTAFF",
    "RAPIER",
    "SCIMITAR",
    "SHORTBOW",
    "SHORTSWORD",
    "SICKLE",
    "SLING",
    "SPEAR",
    "TRIDENT",
    "UNARMED_STRIKE",
    "WARHAMMER",
    "WAR_PICK",
    "WHIP",
];

/// Weapon proficiency id for a weapon name (`simple`, `martial` and single weapons).
#[must_use]
pub fn weapon_prof_id(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    let suffix = match lower.as_str() {
        "simple" => "SIMPLE_WEAPONS".to_string(),
        "martial" => "MARTIAL_WEAPONS".to_string(),
        "light crossbow" => "CROSSBOW_LIGHT".to_string(),
        "hand crossbow" => "CROSSBOW_HAND".to_string(),
        "heavy crossbow" => "CROSSBOW_HEAVY".to_string(),
        other => id_fragment(other),
    };
    let known = suffix == "SIMPLE_WEAPONS"
        || suffix == "MARTIAL_WEAPONS"
        || WEAPON_PROFS.contains(&suffix.as_str());
    known.then(|| format!("ID_PROFICIENCY_WEAPON_PROFICIENCY_{suffix}"))
}

const TOOLS: &[&str] = &[
    "ALCHEMISTS_SUPPLIES",
    "ARTISANS_TOOLS",
    "BAGPIPES",
    "BREWERS_SUPPLIES",
    "CALLIGRAPHERS_SUPPLIES",
    "CARPENTERS_TOOLS",
    "CARTOGRAPHERS_TOOLS",
    "COBBLERS_TOOLS",
    "COOKS_UTENSILS",
    "DISGUISE_KIT",
    "DRUM",
    "DULCIMER",
    "FLUTE",
    "FORGERY_KIT",
    "GLASSBLOWERS_TOOLS",
    "HERBALISM_KIT",
    "HORN",
    "JEWELERS_TOOLS",
    "LEATHERWORKERS_TOOLS",
    "LUTE",
    "LYRE",
    "MASONS_TOOLS",
    "MUSICAL_INSTRUMENT",
    "NAVIGATORS_TOOLS",
    "PAINTERS_SUPPLIES",
    "PAN_FLUTE",
    "POISONERS_KIT",
    "POTTERS_TOOLS",
    "SHAWM",
    "SMITHS_TOOLS",
    "THIEVES_TOOLS",
    "TINKERS_TOOLS",
    "VEHICLES_LAND",
    "VEHICLES_WATER",
    "VIOL",
    "WEAVERS_TOOLS",
    "WOODCARVERS_TOOLS",
];

const GAMING_SETS: &[&str] = &[
    "DICE_SET",
    "DRAGONCHESS_SET",
    "GAMING_SET",
    "PLAYING_CARD_SET",
    "THREE_DRAGON_ANTE_SET",
];

/// Tool / instrument / gaming set proficiency id for a lower-case tool name.
#[must_use]
pub fn tool_prof_id(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    let lower = lower
        .replace("vehicles (land)", "vehicles land")
        .replace("vehicles (water)", "vehicles water");
    let frag = id_fragment(&lower);
    if GAMING_SETS.contains(&frag.as_str()) {
        return Some(format!("ID_PROFICIENCY_GAMING_SET_PROFICIENCY_{frag}"));
    }
    TOOLS
        .contains(&frag.as_str())
        .then(|| format!("ID_PROFICIENCY_TOOL_PROFICIENCY_{frag}"))
}

const CORE_LANGUAGES: &[&str] = &[
    "ABYSSAL",
    "CELESTIAL",
    "COMMON",
    "DEEP_SPEECH",
    "DRACONIC",
    "DWARVISH",
    "ELVISH",
    "GIANT",
    "GNOMISH",
    "GOBLIN",
    "HALFLING",
    "INFERNAL",
    "ORC",
    "PRIMORDIAL",
    "SYLVAN",
    "UNDERCOMMON",
    "DRUIDIC",
];

/// Core language id for a lower-case language name, if the core pack defines it.
#[must_use]
pub fn core_language_id(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    if lower == "thieves' cant" || lower == "thieves\u{2019} cant" {
        return Some("ID_WOTC_PHB_LANGUAGE_THIEVES_CANT".to_string());
    }
    // The Primordial dialects of the 2014/2024 rules have no element of their own.
    if matches!(lower.as_str(), "aquan" | "auran" | "ignan" | "terran") {
        return Some("ID_LANGUAGE_PRIMORDIAL".to_string());
    }
    let frag = id_fragment(&lower);
    CORE_LANGUAGES
        .contains(&frag.as_str())
        .then(|| format!("ID_LANGUAGE_{frag}"))
}

const DAMAGE_TYPES: [&str; 13] = [
    "bludgeoning",
    "piercing",
    "slashing",
    "acid",
    "cold",
    "fire",
    "force",
    "lightning",
    "necrotic",
    "poison",
    "psychic",
    "radiant",
    "thunder",
];

/// Damage type name normalised (`fire`), if recognised.
#[must_use]
pub fn damage_type(name: &str) -> Option<&'static str> {
    let lower = name.trim().to_lowercase();
    DAMAGE_TYPES.iter().find(|d| **d == lower).copied()
}

/// Core condition id for damage resistance / immunity / vulnerability.
#[must_use]
pub fn damage_condition_id(kind: DamageRelation, damage: &str) -> Option<String> {
    damage_type(damage).map(|d| {
        format!(
            "ID_INTERNAL_CONDITION_DAMAGE_{}_{}",
            kind.token(),
            d.to_uppercase()
        )
    })
}

/// Which relation a damage condition expresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageRelation {
    /// Resistance.
    Resistance,
    /// Immunity.
    Immunity,
    /// Vulnerability.
    Vulnerability,
}

impl DamageRelation {
    fn token(self) -> &'static str {
        match self {
            Self::Resistance => "RESISTANCE",
            Self::Immunity => "IMMUNITY",
            Self::Vulnerability => "VULNERABILITY",
        }
    }

    /// Support tag of the core condition elements (`Resistance`, ...).
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Resistance => "Resistance",
            Self::Immunity => "Immunity",
            Self::Vulnerability => "Vulnerability",
        }
    }
}

/// Size id for a 5etools size code.
#[must_use]
pub fn size_id(code: &str) -> Option<&'static str> {
    Some(match code {
        "T" => "ID_SIZE_TINY",
        "S" => "ID_SIZE_SMALL",
        "M" => "ID_SIZE_MEDIUM",
        "L" => "ID_SIZE_LARGE",
        "H" => "ID_SIZE_HUGE",
        "G" => "ID_SIZE_GARGANTUAN",
        "V" => "ID_SIZE_COLOSSAL",
        _ => return None,
    })
}

/// Vision element id and the statistic group holding its range.
#[must_use]
pub fn vision(sense: &str) -> Option<(&'static str, &'static str)> {
    Some(match sense {
        "darkvision" => ("ID_VISION_DARKVISION", "darkvision:range"),
        "blindsight" => ("ID_VISION_BLINDSIGHT", "blindsight:range"),
        "truesight" => ("ID_VISION_TRUESIGHT", "truesight:range"),
        _ => return None,
    })
}

/// Spell save "Class Feature" ids that the engine generates; never authored here.
pub const ASI_TEMPLATE_SUPPORT: &str = "Improvement Option";

/// Multiclass spellcasting slot grant for a caster progression.
#[must_use]
pub fn multiclass_slot_grant(progression: &str, is_2024: bool) -> Option<&'static str> {
    Some(match (progression, is_2024) {
        ("full", _) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL",
        ("1/2", false) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF",
        ("1/2", true) | ("artificer", _) => {
            "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP"
        }
        ("1/3", false) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_THIRD",
        ("1/3", true) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_THIRD_UP",
        ("pact", _) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO",
        _ => return None,
    })
}

/// Every core id this module can produce for the given name lists (used by tests that check
/// the mappings against the shipped core pack).
#[must_use]
pub fn all_mapped_ids() -> Vec<String> {
    let mut ids = Vec::new();
    for (a, _) in ABILITIES {
        ids.extend(saving_throw_id(a));
    }
    ids.extend(SKILLS.iter().filter_map(|s| skill_id(s)));
    for k in ["light", "medium", "heavy", "shield"] {
        ids.extend(armor_prof_id(k).map(str::to_string));
    }
    ids.extend(
        WEAPON_PROFS
            .iter()
            .map(|w| format!("ID_PROFICIENCY_WEAPON_PROFICIENCY_{w}")),
    );
    ids.push("ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS".into());
    ids.push("ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS".into());
    ids.extend(
        TOOLS
            .iter()
            .map(|t| format!("ID_PROFICIENCY_TOOL_PROFICIENCY_{t}")),
    );
    ids.extend(
        GAMING_SETS
            .iter()
            .map(|t| format!("ID_PROFICIENCY_GAMING_SET_PROFICIENCY_{t}")),
    );
    ids.extend(CORE_LANGUAGES.iter().map(|l| format!("ID_LANGUAGE_{l}")));
    ids.push("ID_WOTC_PHB_LANGUAGE_THIEVES_CANT".into());
    for rel in [
        DamageRelation::Resistance,
        DamageRelation::Immunity,
        DamageRelation::Vulnerability,
    ] {
        ids.extend(
            DAMAGE_TYPES
                .iter()
                .filter_map(|d| damage_condition_id(rel, d)),
        );
    }
    for c in ["T", "S", "M", "L", "H", "G", "V"] {
        ids.extend(size_id(c).map(str::to_string));
    }
    for s in ["darkvision", "blindsight", "truesight"] {
        ids.extend(vision(s).map(|(i, _)| i.to_string()));
    }
    ids.push("ID_VISION_SUPERIORDARKVISION".into());
    for p in ["full", "1/2", "1/3", "pact", "artificer"] {
        for y in [false, true] {
            ids.extend(multiclass_slot_grant(p, y).map(str::to_string));
        }
    }
    for id in [
        "ID_INTERNAL_GRANT_MULTICLASS",
        "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE",
        "ID_INTERNAL_RITUAL_CASTING",
        "ID_INTERNAL_GRANTS_MULTICLASSING_PREREQUISITE",
    ] {
        ids.push(id.to_string());
    }
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_saves_and_armor() {
        assert_eq!(
            skill_id("Sleight of Hand").unwrap(),
            "ID_PROFICIENCY_SKILL_SLEIGHTOFHAND"
        );
        assert_eq!(
            skill_id("animal handling").unwrap(),
            "ID_PROFICIENCY_SKILL_ANIMALHANDLING"
        );
        assert!(skill_id("juggling").is_none());
        assert_eq!(
            saving_throw_id("con").unwrap(),
            "ID_PROFICIENCY_SAVINGTHROW_CONSTITUTION"
        );
        assert_eq!(ability_stat("wis").unwrap(), "wisdom");
        assert_eq!(
            armor_prof_id("shield").unwrap(),
            "ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS"
        );
        assert!(armor_prof_id("tower").is_none());
        assert!(ability_name("xyz").is_none());
    }

    #[test]
    fn weapons_tools_languages() {
        assert_eq!(
            weapon_prof_id("simple").unwrap(),
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS"
        );
        assert_eq!(
            weapon_prof_id("light hammer").unwrap(),
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_LIGHT_HAMMER"
        );
        assert_eq!(
            weapon_prof_id("Light Crossbow").unwrap(),
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_CROSSBOW_LIGHT"
        );
        assert_eq!(
            weapon_prof_id("war pick").unwrap(),
            "ID_PROFICIENCY_WEAPON_PROFICIENCY_WAR_PICK"
        );
        assert!(weapon_prof_id("laser pistol").is_none());
        assert_eq!(
            tool_prof_id("smith's tools").unwrap(),
            "ID_PROFICIENCY_TOOL_PROFICIENCY_SMITHS_TOOLS"
        );
        assert_eq!(
            tool_prof_id("thieves' tools").unwrap(),
            "ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS"
        );
        assert_eq!(
            tool_prof_id("dice set").unwrap(),
            "ID_PROFICIENCY_GAMING_SET_PROFICIENCY_DICE_SET"
        );
        assert_eq!(
            tool_prof_id("three-dragon ante set").unwrap(),
            "ID_PROFICIENCY_GAMING_SET_PROFICIENCY_THREE_DRAGON_ANTE_SET"
        );
        assert_eq!(
            tool_prof_id("vehicles (land)").unwrap(),
            "ID_PROFICIENCY_TOOL_PROFICIENCY_VEHICLES_LAND"
        );
        assert_eq!(
            tool_prof_id("pan flute").unwrap(),
            "ID_PROFICIENCY_TOOL_PROFICIENCY_PAN_FLUTE"
        );
        assert!(tool_prof_id("vehicles (air)").is_none());
        assert_eq!(
            core_language_id("Deep Speech").unwrap(),
            "ID_LANGUAGE_DEEP_SPEECH"
        );
        assert_eq!(
            core_language_id("thieves' cant").unwrap(),
            "ID_WOTC_PHB_LANGUAGE_THIEVES_CANT"
        );
        assert!(core_language_id("gith").is_none());
        assert_eq!(
            core_language_id("Aquan").as_deref(),
            Some("ID_LANGUAGE_PRIMORDIAL")
        );
    }

    #[test]
    fn damage_size_vision_and_slots() {
        assert_eq!(
            damage_condition_id(DamageRelation::Resistance, "Fire").unwrap(),
            "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE"
        );
        assert_eq!(
            damage_condition_id(DamageRelation::Immunity, "poison").unwrap(),
            "ID_INTERNAL_CONDITION_DAMAGE_IMMUNITY_POISON"
        );
        assert!(damage_condition_id(
            DamageRelation::Vulnerability,
            "bludgeoning, piercing, and slashing from nonmagical"
        )
        .is_none());
        assert_eq!(DamageRelation::Vulnerability.tag(), "Vulnerability");
        assert_eq!(size_id("M"), Some("ID_SIZE_MEDIUM"));
        assert_eq!(size_id("Q"), None);
        assert_eq!(vision("darkvision").unwrap().1, "darkvision:range");
        assert!(vision("tremorsense").is_none());
        assert_eq!(
            multiclass_slot_grant("1/2", false),
            Some("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF")
        );
        assert_eq!(
            multiclass_slot_grant("1/2", true),
            Some("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP")
        );
        assert_eq!(
            multiclass_slot_grant("artificer", false),
            Some("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP")
        );
        assert_eq!(
            multiclass_slot_grant("pact", true),
            Some("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO")
        );
        assert_eq!(multiclass_slot_grant("none", false), None);
    }

    #[test]
    fn mapped_ids_list_is_sorted_and_unique() {
        let ids = all_mapped_ids();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids, sorted);
        assert!(ids.len() > 150);
    }
}
