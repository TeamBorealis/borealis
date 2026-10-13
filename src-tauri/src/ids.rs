//! Identifier codecs and small shared helpers (slot ids, class slot ids, type names, time).

use crate::error::{ApiError, ApiResult};
use borealis_engine::{ClassSlot, SlotKey};
use std::time::SystemTime;

/// Encode a slot key as the stable string the UI uses: `owner:rule:ordinal[@instance]`.
#[must_use]
pub fn encode_slot(key: &SlotKey) -> String {
    let base = format!("{}:{}:{}", key.owner, key.rule, key.ordinal);
    match &key.instance {
        Some(i) => format!("{base}@{i}"),
        None => base,
    }
}

/// Decode a slot id produced by [`encode_slot`].
///
/// # Errors
/// `invalid_argument` if the id is malformed.
pub fn decode_slot(id: &str) -> ApiResult<SlotKey> {
    let bad = || ApiError::invalid("Malformed slot id.");
    let (head, instance) = match id.split_once('@') {
        Some((h, i)) => (h, Some(i.to_string())),
        None => (id, None),
    };
    let mut parts = head.rsplitn(3, ':');
    let ordinal = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
    let rule = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
    let owner = parts.next().filter(|o| !o.is_empty()).ok_or_else(bad)?;
    Ok(SlotKey {
        owner: owner.to_string(),
        rule,
        ordinal,
        instance,
    })
}

/// Encode a class progression: `main` or `multi-<starting level>`.
#[must_use]
pub fn encode_class_slot(slot: &ClassSlot) -> String {
    match slot {
        ClassSlot::Main => "main".to_string(),
        ClassSlot::Multi { starting_level } => format!("multi-{starting_level}"),
    }
}

/// Decode a class slot id.
///
/// # Errors
/// `invalid_argument` if the id is malformed.
pub fn decode_class_slot(id: &str) -> ApiResult<ClassSlot> {
    if id == "main" {
        return Ok(ClassSlot::Main);
    }
    id.strip_prefix("multi-")
        .and_then(|n| n.parse().ok())
        .map(|starting_level| ClassSlot::Multi { starting_level })
        .ok_or_else(|| ApiError::invalid("Malformed class slot id."))
}

/// Contract element type for a content element type name.
#[must_use]
pub fn ui_type(content_type: &str) -> &'static str {
    match content_type {
        "Race" | "Species" => "race",
        "Sub Race" => "subrace",
        "Racial Trait" | "Species Trait Option" => "racial-trait",
        "Class" => "class",
        "Multiclass" => "multiclass",
        "Archetype" => "archetype",
        "Class Feature" | "Archetype Feature" => "class-feature",
        "Fighting Style" => "fighting-style",
        "Mastery Property" => "weapon-mastery",
        "Background" => "background",
        "Background Feature" => "background-feature",
        "Feat" | "Feat Feature" => "feat",
        "Ability Score Improvement" => "asi",
        "Language" => "language",
        "Skill" => "skill",
        "Proficiency" => "proficiency",
        "Spell" => "spell",
        "Item" | "Equipment Package" => "item",
        "Weapon" => "weapon",
        "Armor" => "armor",
        "Magic Item" => "magic-item",
        "Companion" => "companion",
        "Deity" => "deity",
        "Source" => "source",
        "List" => "list",
        _ => "option",
    }
}

/// Builder tab that hosts slots selecting the given content type.
#[must_use]
pub fn tab_for(content_type: &str) -> &'static str {
    match content_type {
        "Race" | "Sub Race" | "Species" | "Racial Trait" | "Species Trait Option" => "race",
        "Background" | "Background Feature" => "background",
        "Ability Score Improvement" => "abilities",
        "Feat" => "feats",
        "Language" => "languages",
        "Proficiency" | "Skill" => "proficiencies",
        "Companion" => "companion",
        "Spell" => "spells",
        _ => "class",
    }
}

/// All builder tabs, in the order of the UI's `BuilderTab`.
pub const TABS: [&str; 9] = [
    "race",
    "class",
    "background",
    "abilities",
    "feats",
    "languages",
    "proficiencies",
    "companion",
    "spells",
];

/// Selection Flow priority of a selected content type (lower = earlier).
#[must_use]
pub fn flow_priority(content_type: &str) -> u8 {
    match content_type {
        "Race" | "Species" => 0,
        "Sub Race" => 1,
        "Class" => 2,
        "Archetype" => 3,
        "Multiclass" => 4,
        "Background" => 5,
        "Ability Score Improvement" => 6,
        "Language" => 7,
        "Proficiency" | "Skill" => 8,
        "Feat" => 9,
        "Companion" => 10,
        "Spell" => 11,
        _ => 12,
    }
}

/// Ability keys in engine order.
pub const ABILITY_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

/// Three-letter ability key (`str`..`cha`) for a full ability name or abbreviation.
#[must_use]
pub fn ability_key(name: &str) -> String {
    name.trim().to_lowercase().chars().take(3).collect()
}

/// Index 0..6 of an ability key.
#[must_use]
pub fn ability_index(key: &str) -> Option<usize> {
    ["str", "dex", "con", "int", "wis", "cha"]
        .iter()
        .position(|k| *k == ability_key(key))
}

/// Title-case a lower-case name (`animal handling` -> `Animal Handling`).
#[must_use]
pub fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format a time as RFC 3339 UTC (seconds precision).
#[must_use]
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Longest stem [`file_stem`] returns (in chars), well under every file system's name limit.
const MAX_STEM_CHARS: usize = 120;

/// Device names Windows reserves in any folder, with or without an extension (case-insensitive).
const WINDOWS_RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

fn is_windows_reserved(stem: &str) -> bool {
    let upper = stem.to_uppercase();
    if WINDOWS_RESERVED.contains(&upper.as_str()) {
        return true;
    }
    // COM0-9 / LPT0-9, including the superscript digits Windows also treats as devices.
    let mut chars = upper.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let rest: Vec<char> = chars.collect();
    (prefix == "COM" || prefix == "LPT")
        && rest.len() == 1
        && (rest[0].is_ascii_digit() || matches!(rest[0], '\u{b9}' | '\u{b2}' | '\u{b3}'))
}

/// A file-name stem from a character name: letters, digits and `-` stay, the rest becomes `_`.
/// Portable everywhere: Windows device names (`CON`, `nul`, `COM1` ...) get a `_` suffix and the
/// stem is capped at [`MAX_STEM_CHARS`].
#[must_use]
pub fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(MAX_STEM_CHARS)
        .collect();
    if stem.trim_matches('_').is_empty() {
        "character".to_string()
    } else if is_windows_reserved(&stem) {
        format!("{stem}_")
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn file_stem_keeps_letters_digits_and_dashes() {
        assert_eq!(file_stem("Mira Stone-Heart"), "Mira_Stone-Heart");
        assert_eq!(file_stem("C:\\x/y*?<>|\""), "C__x_y______");
        assert_eq!(file_stem("..."), "character");
    }

    #[test]
    fn file_stem_avoids_windows_device_names() {
        for name in [
            "CON",
            "con",
            "Nul",
            "aux",
            "PRN",
            "COM1",
            "lpt9",
            "COM\u{b9}",
        ] {
            assert_eq!(file_stem(name), format!("{name}_"), "{name}");
        }
        for name in ["Conan", "COM10", "LPT", "Nulla", "COMA"] {
            assert_eq!(file_stem(name), name, "{name}");
        }
    }

    #[test]
    fn file_stem_is_capped() {
        assert_eq!(file_stem(&"a".repeat(500)).chars().count(), MAX_STEM_CHARS);
    }

    #[test]
    fn slot_ids_round_trip() {
        let key = SlotKey::new("ID_SRD_CLASS_WIZARD", 3, 2);
        assert_eq!(decode_slot(&encode_slot(&key)).unwrap(), key);
        let inst = SlotKey {
            instance: Some("u1".into()),
            ..key
        };
        assert_eq!(decode_slot(&encode_slot(&inst)).unwrap(), inst);
        assert!(decode_slot("nonsense").is_err());
    }

    #[test]
    fn class_slot_ids_round_trip() {
        for s in [ClassSlot::Main, ClassSlot::Multi { starting_level: 4 }] {
            assert_eq!(decode_class_slot(&encode_class_slot(&s)).unwrap(), s);
        }
        assert!(decode_class_slot("multi-x").is_err());
    }

    #[test]
    fn rfc3339_formats_known_instants() {
        assert_eq!(rfc3339(SystemTime::UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_709_210_096);
        assert_eq!(rfc3339(t), "2024-02-29T12:34:56Z");
    }
}
