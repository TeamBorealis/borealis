//! The homebrew creator's id rule (spec 5.3): `ID_<CODE>_<TYPE>_<NAME>`, children `ID_<CODE>_<TYPE>_<PARENT>_<NAME>`,
//! multiclass `ID_<CODE>_MULTICLASS_<CLASS>`.
//!
//! * `<CODE>` is the pack code: 2-8 of `A-Z0-9` ([`is_pack_code`]), also the pack's Source abbreviation.
//! * `<TYPE>` is a fixed token per element type ([`type_token`]).
//! * `<NAME>` is [`slug`] of the name; a name with no ASCII letter (any other script) is `X` and the 8 upper-case hex
//!   digits of the FNV-1a hash of its normalized (NFC) text ([`name_token`]).
//! * A collision with a loaded id or a retired id of the pack gets `_2`, `_3`, ... ([`IdGenerator`]).
//!
//! Every id is `ID_` and upper-case ASCII letters, digits and `_`: never an `ID_FORBIDDEN` character, never a
//! `NonStandardId` warning. Ids are generated once (on the first save) and never change: renaming keeps the id.

use super::slug;
use borealis_content::rich::normalize_plain_text;
use std::collections::HashSet;

/// Shortest pack code.
pub const PACK_CODE_MIN: usize = 2;
/// Longest pack code.
pub const PACK_CODE_MAX: usize = 8;

/// The fixed `<TYPE>` token of each element type the creator writes. Other types use [`slug`] of the type.
const TYPE_TOKENS: &[(&str, &str)] = &[
    ("Class", "CLASS"),
    ("Class Feature", "CLASS_FEATURE"),
    ("Archetype", "SUBCLASS"),
    ("Archetype Feature", "SUBCLASS_FEATURE"),
    ("Species", "SPECIES"),
    ("Race", "RACE"),
    ("Sub Race", "SUBRACE"),
    ("Race Variant", "RACE_VARIANT"),
    ("Racial Trait", "TRAIT"),
    ("Species Trait Option", "TRAIT_OPTION"),
    ("Background", "BACKGROUND"),
    ("Background Feature", "BACKGROUND_FEATURE"),
    ("Background Characteristics", "BACKGROUND_CHARACTERISTICS"),
    ("Background Variant", "BACKGROUND_VARIANT"),
    ("Feat", "FEAT"),
    ("Feat Feature", "FEAT_FEATURE"),
    ("Spell", "SPELL"),
    ("Item", "ITEM"),
    ("Item Pack", "ITEM_PACK"),
    ("Weapon", "WEAPON"),
    ("Armor", "ARMOR"),
    ("Magic Item", "MAGIC_ITEM"),
    ("Equipment Package", "EQUIPMENT_PACKAGE"),
    ("Option", "OPTION"),
    ("Monster", "CREATURE"),
    ("Language", "LANGUAGE"),
    ("Proficiency", "PROFICIENCY"),
    ("Condition", "CONDITION"),
    ("Damage Type", "DAMAGE_TYPE"),
    ("Skill", "SKILL"),
    ("Deity", "DEITY"),
    ("Vision", "VISION"),
    ("Size", "SIZE"),
    ("Alignment", "ALIGNMENT"),
    ("Weapon Property", "WEAPON_PROPERTY"),
    ("Weapon Group", "WEAPON_GROUP"),
    ("Weapon Category", "WEAPON_CATEGORY"),
    ("Armor Group", "ARMOR_GROUP"),
    ("Mastery Property", "MASTERY"),
    ("Spellcasting Focus Group", "FOCUS_GROUP"),
    ("Dragonmark", "DRAGONMARK"),
    ("Rules Glossary Entry", "RULE"),
    ("Companion", "COMPANION"),
    ("Familiar", "FAMILIAR"),
    ("Source", "SOURCE"),
];

/// The `<TYPE>` token of an element type: the fixed token of the table above, else [`slug`] of the type.
#[must_use]
pub fn type_token(ty: &str) -> String {
    TYPE_TOKENS
        .iter()
        .find(|(t, _)| *t == ty)
        .map_or_else(|| slug(ty), |(_, token)| (*token).to_string())
}

/// True for a valid pack code: 2-8 of `A-Z0-9`.
#[must_use]
pub fn is_pack_code(code: &str) -> bool {
    (PACK_CODE_MIN..=PACK_CODE_MAX).contains(&code.len())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// 32-bit FNV-1a over the bytes of `text` (the hash of `borealis_content::idnorm::hashed_safe_id`).
fn fnv1a(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The `<NAME>` token: [`slug`] of the name, or `X` + the upper-case FNV-1a hex of the normalized name when the name
/// has no ASCII letter.
#[must_use]
pub fn name_token(name: &str) -> String {
    let normalized = normalize_plain_text(name);
    if normalized.chars().any(|c| c.is_ascii_alphabetic()) {
        slug(&normalized)
    } else {
        format!("X{:08X}", fnv1a(&normalized))
    }
}

/// Why an [`IdGenerator`] cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    /// The pack code is not 2-8 of `A-Z0-9`.
    BadPackCode(String),
}

impl std::fmt::Display for IdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadPackCode(code) => write!(
                f,
                "The pack code {code:?} must be {PACK_CODE_MIN} to {PACK_CODE_MAX} capital letters or digits."
            ),
        }
    }
}

impl std::error::Error for IdError {}

/// Makes new ids for one pack, unique against every loaded id and every retired id of the pack. Each id it returns
/// is reserved, so the next one never repeats it.
#[derive(Debug, Clone)]
pub struct IdGenerator {
    code: String,
    taken: HashSet<String>,
}

impl IdGenerator {
    /// A generator for the pack `code`, avoiding `loaded` ids (every loaded pack, this one included) and the pack's
    /// `retired` ids (deleted elements, `creator.json`).
    ///
    /// # Errors
    /// [`IdError::BadPackCode`] when `code` is not a pack code.
    pub fn new<L, R, S, T>(code: &str, loaded: L, retired: R) -> Result<Self, IdError>
    where
        L: IntoIterator<Item = S>,
        R: IntoIterator<Item = T>,
        S: Into<String>,
        T: Into<String>,
    {
        if !is_pack_code(code) {
            return Err(IdError::BadPackCode(code.to_string()));
        }
        let taken = loaded
            .into_iter()
            .map(Into::into)
            .chain(retired.into_iter().map(Into::into))
            .collect();
        Ok(Self {
            code: code.to_string(),
            taken,
        })
    }

    /// The pack code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// True when `id` is loaded, retired or already handed out.
    #[must_use]
    pub fn is_taken(&self, id: &str) -> bool {
        self.taken.contains(id)
    }

    /// Reserve an id that exists already (a frozen id of the draft), so no new id repeats it.
    pub fn reserve(&mut self, id: &str) {
        self.taken.insert(id.to_string());
    }

    fn unique(&mut self, base: String) -> String {
        let id = if self.taken.contains(&base) {
            (2u32..)
                .map(|n| format!("{base}_{n}"))
                .find(|c| !self.taken.contains(c))
                .unwrap_or_else(|| base.clone())
        } else {
            base
        };
        self.taken.insert(id.clone());
        id
    }

    /// `ID_<CODE>_<TYPE>_<NAME>` for an element of type `ty` called `name`.
    pub fn element_id(&mut self, ty: &str, name: &str) -> String {
        let base = format!("ID_{}_{}_{}", self.code, type_token(ty), name_token(name));
        self.unique(base)
    }

    /// `ID_<CODE>_<TYPE>_<PARENT>_<NAME>` for a child element (a class's feature) of the element called `parent`.
    pub fn child_id(&mut self, ty: &str, parent: &str, name: &str) -> String {
        let base = format!(
            "ID_{}_{}_{}_{}",
            self.code,
            type_token(ty),
            name_token(parent),
            name_token(name)
        );
        self.unique(base)
    }

    /// `ID_<CODE>_MULTICLASS_<CLASS>` for the multiclass node of the class called `class` (never the class id).
    pub fn multiclass_id(&mut self, class: &str) -> String {
        let base = format!("ID_{}_MULTICLASS_{}", self.code, name_token(class));
        self.unique(base)
    }
}

/// A select's name (its label and persistence key): `"<Label> (<Owner>)"`, as the SRD writes them ("Skill
/// Proficiency (Paladin)"), made unique among the element's other select names with ` 2`, ` 3`, ...
#[must_use]
pub fn select_name<'a, I>(label: &str, owner: &str, existing: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let label = normalize_plain_text(label);
    let owner = normalize_plain_text(owner);
    let base = if owner.is_empty() {
        label
    } else {
        format!("{label} ({owner})")
    };
    let existing: HashSet<&str> = existing.into_iter().collect();
    if !existing.contains(base.as_str()) {
        return base;
    }
    (2u32..)
        .map(|n| format!("{base} {n}"))
        .find(|c| !existing.contains(c.as_str()))
        .unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::validate::ID_FORBIDDEN;

    fn generator() -> IdGenerator {
        IdGenerator::new("GL", ["ID_GL_SPELL_EMBER"], ["ID_GL_FEAT_OLD"]).expect("code")
    }

    fn is_standard(id: &str) -> bool {
        id.starts_with("ID_")
            && id
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && !id.contains(ID_FORBIDDEN)
    }

    #[test]
    fn deterministic_shapes() {
        let mut a = generator();
        let mut b = generator();
        assert_eq!(
            a.element_id("Spell", "Fire Lance"),
            "ID_GL_SPELL_FIRE_LANCE"
        );
        assert_eq!(
            b.element_id("Spell", "Fire Lance"),
            "ID_GL_SPELL_FIRE_LANCE"
        );
        assert_eq!(
            a.child_id("Class Feature", "Gloom Knight", "Dark Step"),
            "ID_GL_CLASS_FEATURE_GLOOM_KNIGHT_DARK_STEP"
        );
        assert_eq!(
            a.multiclass_id("Gloom Knight"),
            "ID_GL_MULTICLASS_GLOOM_KNIGHT"
        );
        assert_eq!(
            a.element_id("Archetype", "Oath of Ash"),
            "ID_GL_SUBCLASS_OATH_OF_ASH"
        );
        assert_eq!(
            a.element_id("Homebrew Thing", "X"),
            "ID_GL_HOMEBREW_THING_X"
        );
    }

    #[test]
    fn collisions_with_loaded_retired_and_given_ids() {
        let mut g = generator();
        assert_eq!(g.element_id("Spell", "Ember"), "ID_GL_SPELL_EMBER_2");
        assert_eq!(g.element_id("Spell", "Ember"), "ID_GL_SPELL_EMBER_3");
        assert_eq!(g.element_id("Feat", "Old"), "ID_GL_FEAT_OLD_2");
        g.reserve("ID_GL_FEAT_NEW");
        assert!(g.is_taken("ID_GL_FEAT_NEW"));
        assert_eq!(g.element_id("Feat", "New"), "ID_GL_FEAT_NEW_2");
    }

    #[test]
    fn names_without_ascii_letters_hash() {
        let mut g = generator();
        let fire = g.element_id("Spell", "火球");
        assert!(fire.starts_with("ID_GL_SPELL_X"), "{fire}");
        assert_eq!(fire.len(), "ID_GL_SPELL_X".len() + 8);
        // NFC and white space do not change the hash
        assert_eq!(name_token("Ö"), name_token("O\u{308}"));
        assert_eq!(name_token(" 火球 "), name_token("火球"));
        assert_ne!(name_token("火球"), name_token("水"));
        assert_eq!(name_token("1984"), format!("X{:08X}", fnv1a("1984")));
        // a name with some ASCII letters keeps its slug
        assert_eq!(name_token("Été 2"), "T_2");
        assert!(is_standard(&fire));
    }

    #[test]
    fn every_id_is_standard() {
        let mut g = generator();
        for name in [
            "Ember's Ward!",
            "(a)|[b]:c,&d",
            "  ",
            "",
            "ñandú",
            "Æther",
            "Ω",
            "a’b",
        ] {
            for ty in ["Spell", "Class Feature", "Weird type: x"] {
                let id = g.element_id(ty, name);
                assert!(is_standard(&id), "{id}");
                let child = g.child_id(ty, name, name);
                assert!(is_standard(&child), "{child}");
            }
        }
    }

    #[test]
    fn pack_codes() {
        assert!(is_pack_code("GL"));
        assert!(is_pack_code("ABCD1234"));
        assert!(!is_pack_code("G"));
        assert!(!is_pack_code("ABCDE12345"));
        assert!(!is_pack_code("gl"));
        assert!(!is_pack_code("G_L"));
        assert_eq!(
            IdGenerator::new("g", Vec::<String>::new(), Vec::<String>::new()).unwrap_err(),
            IdError::BadPackCode("g".into())
        );
        assert!(IdError::BadPackCode("g".into())
            .to_string()
            .contains("2 to 8"));
    }

    #[test]
    fn type_tokens_are_unique_and_standard() {
        let mut seen = HashSet::new();
        for (ty, token) in TYPE_TOKENS {
            assert!(seen.insert(*token), "{ty}");
            assert_eq!(slug(token), *token);
        }
        assert_eq!(type_token("Magic Item"), "MAGIC_ITEM");
        assert_eq!(type_token("Sub Race"), "SUBRACE");
    }

    #[test]
    fn select_names_are_unique_per_element() {
        assert_eq!(
            select_name("Skill Proficiency", "Paladin", []),
            "Skill Proficiency (Paladin)"
        );
        assert_eq!(
            select_name(
                "Skill Proficiency",
                "Paladin",
                [
                    "Skill Proficiency (Paladin)",
                    "Skill Proficiency (Paladin) 2"
                ]
            ),
            "Skill Proficiency (Paladin) 3"
        );
        assert_eq!(select_name(" Ideal ", "", []), "Ideal");
    }
}
