//! Spelling variants of the terms custom rules carry (conditions, skills, abilities, roll
//! kinds), read the way the bundled content writes them. Homebrew packs (hand-written or
//! imported) name a condition by its element id (`ID_SRD51_CONDITION_BLINDED`), a skill check
//! as `skill_check`, or an ability by its abbreviation; the sheet shows them like the bundled
//! words (`blinded`, `ability_check`, `Strength`).

use crate::ids;
use crate::universe::Universe;

/// Roll kinds the readers understand, with the other spellings that mean them.
const ROLL_SYNONYMS: [(&str, &[&str]); 2] = [
    (
        "ability_check",
        &[
            "skill_check",
            "skill_checks",
            "skill",
            "check",
            "checks",
            "ability_checks",
            "ability check",
            "ability checks",
            "skill check",
            "skill checks",
        ],
    ),
    (
        "saving_throw",
        &[
            "saving_throws",
            "save",
            "saves",
            "saving throw",
            "saving throws",
        ],
    ),
];

/// The reader's roll kind for a `roll` value (`skill_check` is an `ability_check`).
#[must_use]
pub fn roll_kind(raw: &str) -> String {
    let r = raw.trim().to_lowercase();
    ROLL_SYNONYMS
        .iter()
        .find(|(_, alts)| alts.contains(&r.as_str()))
        .map_or(r.clone(), |(kind, _)| (*kind).to_string())
}

/// The words after the last `marker` of an id-like `ID_..._<MARKER>_<WORDS>`, lower case.
fn id_words(raw: &str, marker: &str) -> Option<String> {
    let upper = raw.to_uppercase();
    let at = upper.rfind(marker)?;
    let words = raw[at + marker.len()..].replace('_', " ").to_lowercase();
    let words = words.trim();
    (!words.is_empty()).then(|| words.to_string())
}

/// A term that may be an element id: the element's name, else the id's trailing words after
/// `marker` (lower case), else the text itself.
#[must_use]
pub fn term(uni: &Universe, raw: &str, marker: &str) -> String {
    let t = raw.trim();
    if let Some(e) = uni.get(t) {
        return e.el.name.clone();
    }
    if t.starts_with("ID_") {
        if let Some(w) = id_words(t, marker) {
            return w;
        }
    }
    t.to_string()
}

/// A condition name (`blinded`, lower case) from a name or a condition element id.
#[must_use]
pub fn condition(uni: &Universe, raw: &str) -> String {
    term(uni, raw, "CONDITION_").to_lowercase()
}

/// A skill name (`stealth`) from a name or a skill element id.
#[must_use]
pub fn skill(uni: &Universe, raw: &str) -> String {
    term(uni, raw, "SKILL_")
}

/// The full ability name (`strength`) of a name or an abbreviation; other text as written.
#[must_use]
pub fn ability(raw: &str) -> String {
    ids::ability_index(raw)
        .map_or_else(|| raw.trim().to_string(), |i| ids::ABILITIES[i].to_string())
}

#[cfg(test)]
mod tests {
    use super::{ability, id_words, roll_kind};

    #[test]
    fn roll_spellings_read_as_the_reader_kinds() {
        assert_eq!(roll_kind("skill_check"), "ability_check");
        assert_eq!(roll_kind("Ability Check"), "ability_check");
        assert_eq!(roll_kind("saves"), "saving_throw");
        assert_eq!(roll_kind("initiative"), "initiative");
    }

    #[test]
    fn id_words_take_the_text_after_the_marker() {
        assert_eq!(
            id_words("ID_SRD51_CONDITION_BLINDED", "CONDITION_").as_deref(),
            Some("blinded")
        );
        assert_eq!(
            id_words("ID_X_SKILL_SLEIGHT_OF_HAND", "SKILL_").as_deref(),
            Some("sleight of hand")
        );
        assert_eq!(id_words("ID_X_CONDITION_", "CONDITION_"), None);
    }

    #[test]
    fn abbreviations_name_the_ability() {
        assert_eq!(ability("str"), "strength");
        assert_eq!(ability("Wisdom"), "wisdom");
        assert_eq!(ability("Luck"), "Luck");
    }
}
