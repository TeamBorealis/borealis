//! One row per name (B-55, `specs/version-rows-and-filters.md` 1.1): the options of a slot that
//! share a group key are versions of one picker row.
//!
//! Group key = type family + normalized name. The type family is the engine's own
//! ([`crate::universe::type_family`]: `Race` and `Species` fill one slot). The normalized name
//! is Unicode NFC, trimmed, inner whitespace collapsed to one space, lower-cased, with the
//! typographic apostrophes and quotes folded to ASCII `'` and `"`. Nothing else is folded: a
//! different dash, an accent or a "(Legacy)" suffix keeps two names apart.

use unicode_normalization::UnicodeNormalization;

/// Separates the family from the name in a [`group_key`] (never part of a normalized name).
const KEY_SEPARATOR: char = '\u{1f}';

/// The normalized form of a display name (module docs).
#[must_use]
pub fn name_key(name: &str) -> String {
    let folded: String = name
        .nfc()
        .map(fold_quote)
        .collect::<String>()
        .to_lowercase();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The type family of an element type: the first type of [`crate::universe::type_family`], or
/// the type itself when the engine pairs it with no other.
#[must_use]
pub fn type_family_key(ty: &str) -> &str {
    crate::universe::type_family(ty)
        .first()
        .copied()
        .unwrap_or(ty)
}

/// The group key of an element of type `ty` named `name`: options with equal keys are versions
/// of one row.
#[must_use]
pub fn group_key(ty: &str, name: &str) -> String {
    format!("{}{KEY_SEPARATOR}{}", type_family_key(ty), name_key(name))
}

/// Typographic apostrophes and quotation marks as their ASCII forms.
fn fold_quote(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{02bc}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => '"',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_case_and_quotes_fold() {
        assert_eq!(name_key("  Half-Elf "), "half-elf");
        assert_eq!(name_key("half-elf"), "half-elf");
        assert_eq!(name_key("Storm\u{a0}  Herald"), "storm herald");
        assert_eq!(
            name_key("Tasha\u{2019}s Hideous Laughter"),
            "tasha's hideous laughter"
        );
        assert_eq!(name_key("\u{201c}Lucky\u{201d}"), "\"lucky\"");
    }

    #[test]
    fn nfc_joins_a_decomposed_accent_but_no_accent_is_stripped() {
        assert_eq!(name_key("Se\u{301}ance"), name_key("S\u{e9}ance"));
        assert_ne!(name_key("S\u{e9}ance"), name_key("Seance"));
    }

    #[test]
    fn a_different_dash_or_a_suffix_keeps_names_apart() {
        assert_ne!(name_key("Half-Elf"), name_key("Half\u{2010}Elf"));
        assert_ne!(name_key("Half-Elf"), name_key("Half\u{2013}Elf"));
        assert_ne!(name_key("Elf"), name_key("Elf (Legacy)"));
    }

    #[test]
    fn race_and_species_are_one_family_other_types_their_own() {
        assert_eq!(
            group_key("Race", "Dragonborn"),
            group_key("Species", "dragonborn")
        );
        assert_ne!(
            group_key("Feat", "Alert"),
            group_key("Class Feature", "Alert")
        );
        assert_eq!(type_family_key("Spell"), "Spell");
    }
}
