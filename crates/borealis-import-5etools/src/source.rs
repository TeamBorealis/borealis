//! Source abbreviations, edition detection and the source -> ruleset mapping.

use crate::util::flag_set;
use borealis_content::Ruleset;
use serde_json::Value;

/// Sources that were published under the 2024 rules (`dnd2024`) even when an entry carries
/// no explicit `edition` field.
const SOURCES_2024: &[&str] = &["XPHB", "XDMG", "XMM"];

/// True for the 2024 core books, whose content was 2024 before any publication date was read.
#[must_use]
pub fn is_2024_core_book(source: &str) -> bool {
    SOURCES_2024.iter().any(|s| s.eq_ignore_ascii_case(source))
}

/// Flags 5etools sets on entries that are part of a freely licensed document. `srd` is the
/// 5.1 SRD, `srd52` the 5.2 SRD. The Basic Rules flags (`basicRules`, `basicRules2024`,
/// `freeRules2024`) mark free-to-read but not openly licensed material.
pub const SRD_FLAGS: &[&str] = &["srd", "srd52"];

/// All flags that mark an entry as part of a free document (SRD or Basic/Free Rules).
pub const FREE_FLAGS: &[&str] = &[
    "srd",
    "srd52",
    "basicRules",
    "basicRules2024",
    "freeRules2024",
];

/// True if the entry is flagged as part of the CC-BY licensed SRD (5.1 or 5.2).
#[must_use]
pub fn is_srd(entry: &Value) -> bool {
    SRD_FLAGS.iter().any(|f| flag_set(entry.get(*f)))
}

/// True if the entry is flagged as part of any free document (SRD or Basic/Free Rules).
#[must_use]
pub fn is_free(entry: &Value) -> bool {
    FREE_FLAGS.iter().any(|f| flag_set(entry.get(*f)))
}

/// The edition label of an entry (`classic` or `one`), if present.
#[must_use]
pub fn edition_of(entry: &Value) -> Option<&str> {
    entry.get("edition").and_then(Value::as_str)
}

/// The ruleset an entry belongs to: `edition: "one"` and the 2024 books map to
/// [`Ruleset::Dnd2024`], everything else to [`Ruleset::Dnd2014`].
#[must_use]
pub fn ruleset_for(source: &str, edition: Option<&str>) -> Ruleset {
    match edition {
        Some("one") => Ruleset::Dnd2024,
        Some("classic") => Ruleset::Dnd2014,
        _ if SOURCES_2024.iter().any(|s| s.eq_ignore_ascii_case(source)) => Ruleset::Dnd2024,
        _ => Ruleset::Dnd2014,
    }
}

/// Ruleset of a raw entry using its own `source` and `edition`.
#[must_use]
pub fn ruleset_of_entry(entry: &Value) -> Ruleset {
    let source = entry.get("source").and_then(Value::as_str).unwrap_or("");
    ruleset_for(source, edition_of(entry))
}

/// Human readable names of well-known official sources. These are bibliographic facts, not
/// book content; `books.json` of the user's data folder takes precedence.
const SOURCE_NAMES: &[(&str, &str)] = &[
    ("PHB", "Player's Handbook (2014)"),
    ("DMG", "Dungeon Master's Guide (2014)"),
    ("MM", "Monster Manual (2014)"),
    ("XPHB", "Player's Handbook (2024)"),
    ("XDMG", "Dungeon Master's Guide (2024)"),
    ("XMM", "Monster Manual (2024)"),
    ("XGE", "Xanathar's Guide to Everything"),
    ("TCE", "Tasha's Cauldron of Everything"),
    ("MPMM", "Mordenkainen Presents: Monsters of the Multiverse"),
    ("VGM", "Volo's Guide to Monsters"),
    ("MTF", "Mordenkainen's Tome of Foes"),
    ("SCAG", "Sword Coast Adventurer's Guide"),
    ("EGW", "Explorer's Guide to Wildemount"),
    ("ERLW", "Eberron: Rising from the Last War"),
    ("FTD", "Fizban's Treasury of Dragons"),
    ("SCC", "Strixhaven: A Curriculum of Chaos"),
    ("GGR", "Guildmasters' Guide to Ravnica"),
    ("AI", "Acquisitions Incorporated"),
    ("EEPC", "Elemental Evil Player's Companion"),
    ("MOT", "Mythic Odysseys of Theros"),
    ("VRGR", "Van Richten's Guide to Ravenloft"),
    ("SATO", "Sigil and the Outlands"),
    ("BGG", "Bigby Presents: Glory of the Giants"),
    ("AAG", "Spelljammer: Adventures in Space"),
    ("SRD", "System Reference Document"),
];

/// Display name of a source abbreviation.
#[must_use]
pub fn default_source_name(abbr: &str) -> String {
    SOURCE_NAMES
        .iter()
        .find(|(a, _)| a.eq_ignore_ascii_case(abbr))
        .map_or_else(|| abbr.to_string(), |(_, n)| (*n).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ruleset_follows_edition_then_source() {
        assert_eq!(ruleset_for("PHB", None), Ruleset::Dnd2014);
        assert_eq!(ruleset_for("PHB", Some("one")), Ruleset::Dnd2024);
        assert_eq!(ruleset_for("XPHB", None), Ruleset::Dnd2024);
        assert_eq!(ruleset_for("xdmg", None), Ruleset::Dnd2024);
        assert_eq!(ruleset_for("XPHB", Some("classic")), Ruleset::Dnd2014);
        assert_eq!(ruleset_for("TCE", Some("classic")), Ruleset::Dnd2014);
    }

    #[test]
    fn flags_accept_bool_and_string() {
        assert!(is_srd(&json!({"srd": true})));
        assert!(is_srd(&json!({"srd52": "Fire Bolt"})));
        assert!(!is_srd(&json!({"srd": false})));
        assert!(!is_srd(&json!({"basicRules": true})));
        assert!(is_free(&json!({"basicRules": true})));
        assert!(is_free(&json!({"freeRules2024": true})));
        assert!(!is_free(&json!({"page": 3})));
        assert_eq!(edition_of(&json!({"edition": "one"})), Some("one"));
        assert_eq!(
            ruleset_of_entry(&json!({"source": "XDMG"})),
            Ruleset::Dnd2024
        );
    }

    #[test]
    fn names() {
        assert_eq!(default_source_name("phb"), "Player's Handbook (2014)");
        assert_eq!(default_source_name("ZZZ"), "ZZZ");
    }
}
