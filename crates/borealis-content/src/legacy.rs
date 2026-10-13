//! Legacy statistic name mapping.

const ABILITIES: [&str; 6] = [
    "Strength",
    "Dexterity",
    "Constitution",
    "Intelligence",
    "Wisdom",
    "Charisma",
];

const SKILLS: [&str; 18] = [
    "Acrobatics",
    "Animal Handling",
    "Arcana",
    "Athletics",
    "Deception",
    "History",
    "Insight",
    "Intimidation",
    "Investigation",
    "Medicine",
    "Nature",
    "Perception",
    "Performance",
    "Persuasion",
    "Religion",
    "Sleight of Hand",
    "Stealth",
    "Survival",
];

fn strip_suffix_in<'a>(name: &'a str, set: &[&'a str], suffix: &str) -> Option<&'a str> {
    let base = name.strip_suffix(suffix)?;
    set.iter().copied().find(|s| *s == base)
}

/// Map a legacy statistic name to its normalized lower-case form. Matching of the legacy
/// forms is exact (case-sensitive); every other name is simply lower-cased.
#[must_use]
pub fn map_stat_name(name: &str) -> String {
    let mapped = if let Some(a) = strip_suffix_in(name, &ABILITIES, " Maximum") {
        format!("{a}:max")
    } else if let Some(a) = strip_suffix_in(name, &ABILITIES, " Modifier") {
        format!("{a}:modifier")
    } else if let Some(a) = strip_suffix_in(name, &ABILITIES, " Saving Throw Proficiency") {
        format!("{a}:save:proficiency")
    } else if let Some(a) = strip_suffix_in(name, &ABILITIES, " Saving Throw Misc") {
        format!("{a}:save:misc")
    } else if let Some(s) = strip_suffix_in(name, &SKILLS, " Proficiency") {
        format!("{s}:proficiency")
    } else if let Some(s) = strip_suffix_in(name, &SKILLS, " Misc") {
        format!("{s}:misc")
    } else if let Some(s) = strip_suffix_in(name, &SKILLS, " Passive") {
        format!("{s}:passive")
    } else {
        match name {
            "Proficiency Half" | "Proficiency Half Rounded Down" => "proficiency:half".into(),
            "Proficiency Half Rounded Up" => "proficiency:half:up".into(),
            "ac:dexterity:mod:cap:armor:medium" => "ac:armored:dexterity:cap".into(),
            other => other.to_string(),
        }
    };
    mapped.to_lowercase()
}

/// Normalize a stat rule `value`: one leading `+` is dropped; a leading `-` is kept and
/// the remainder mapped like a name.
#[must_use]
pub fn map_stat_value(value: &str) -> String {
    let v = value.trim();
    let v = v.strip_prefix('+').unwrap_or(v);
    match v.strip_prefix('-') {
        Some(rest) => format!("-{}", map_stat_name(rest)),
        None => map_stat_name(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abilities_and_skills() {
        assert_eq!(map_stat_name("Strength Maximum"), "strength:max");
        assert_eq!(map_stat_name("Dexterity Modifier"), "dexterity:modifier");
        assert_eq!(
            map_stat_name("Wisdom Saving Throw Proficiency"),
            "wisdom:save:proficiency"
        );
        assert_eq!(
            map_stat_name("Charisma Saving Throw Misc"),
            "charisma:save:misc"
        );
        assert_eq!(
            map_stat_name("Animal Handling Proficiency"),
            "animal handling:proficiency"
        );
        assert_eq!(
            map_stat_name("Sleight of Hand Misc"),
            "sleight of hand:misc"
        );
        assert_eq!(map_stat_name("Stealth Passive"), "stealth:passive");
    }

    #[test]
    fn proficiency_and_ac() {
        assert_eq!(map_stat_name("Proficiency Half"), "proficiency:half");
        assert_eq!(
            map_stat_name("Proficiency Half Rounded Down"),
            "proficiency:half"
        );
        assert_eq!(
            map_stat_name("Proficiency Half Rounded Up"),
            "proficiency:half:up"
        );
        assert_eq!(
            map_stat_name("ac:dexterity:mod:cap:armor:medium"),
            "ac:armored:dexterity:cap"
        );
    }

    #[test]
    fn unmapped_names_are_lowercased() {
        assert_eq!(map_stat_name("Proficiency"), "proficiency");
        assert_eq!(map_stat_name("Innate Speed"), "innate speed");
        assert_eq!(map_stat_name("Strength Foo"), "strength foo");
    }

    #[test]
    fn values() {
        assert_eq!(map_stat_value("+1"), "1");
        assert_eq!(map_stat_value("-10"), "-10");
        assert_eq!(
            map_stat_value("-Warlock:Spellcasting:Slots:Count"),
            "-warlock:spellcasting:slots:count"
        );
        assert_eq!(map_stat_value("Strength Modifier"), "strength:modifier");
        assert_eq!(map_stat_value("-Strength Modifier"), "-strength:modifier");
        assert_eq!(map_stat_value(" 3 "), "3");
    }
}
