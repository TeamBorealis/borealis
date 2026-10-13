//! Known element types. `Element::ty` stays an open string; this enum is a helper for
//! callers that want to branch on the well-known ones.

macro_rules! known_types {
    ($( $(#[$m:meta])* $variant:ident => $text:literal : $cat:ident ),+ $(,)?) => {
        /// A type string the loaders recognise.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum KnownType { $( $(#[$m])* $variant ),+ }

        impl KnownType {
            /// Every known type.
            pub const ALL: &'static [KnownType] = &[ $( KnownType::$variant ),+ ];

            /// The exact (case-sensitive) type string.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $( KnownType::$variant => $text ),+ }
            }

            /// Which loader family the type belongs to.
            #[must_use]
            pub fn category(self) -> TypeCategory {
                match self { $( KnownType::$variant => TypeCategory::$cat ),+ }
            }

            /// Look a type string up (exact match).
            #[must_use]
            pub fn parse(text: &str) -> Option<KnownType> {
                match text { $( $text => Some(KnownType::$variant), )+ _ => None }
            }
        }
    };
}

/// How the original loader treats a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeCategory {
    /// Has a dedicated parser that reads type specific setters.
    Dedicated,
    /// Loaded by the default parser.
    Default,
    /// Proposed for the 2024 ruleset.
    Proposed2024,
}

known_types! {
    AbilityScoreImprovement => "Ability Score Improvement": Dedicated,
    Archetype => "Archetype": Dedicated,
    ArchetypeFeature => "Archetype Feature": Dedicated,
    Armor => "Armor": Dedicated,
    BackgroundFeature => "Background Feature": Dedicated,
    Class => "Class": Dedicated,
    ClassFeature => "Class Feature": Dedicated,
    Companion => "Companion": Dedicated,
    Deity => "Deity": Dedicated,
    Dragonmark => "Dragonmark": Dedicated,
    Familiar => "Familiar": Dedicated,
    FamiliarAction => "Familiar Action": Dedicated,
    FamiliarFeature => "Familiar Feature": Dedicated,
    FamiliarTrait => "Familiar Trait": Dedicated,
    Feat => "Feat": Dedicated,
    FeatFeature => "Feat Feature": Dedicated,
    Grants => "Grants": Dedicated,
    Item => "Item": Dedicated,
    Language => "Language": Dedicated,
    LanguageFeature => "Language Feature": Dedicated,
    Level => "Level": Dedicated,
    MagicItem => "Magic Item": Dedicated,
    Monster => "Monster": Dedicated,
    Multiclass => "Multiclass": Dedicated,
    Option => "Option": Dedicated,
    Proficiency => "Proficiency": Dedicated,
    Race => "Race": Dedicated,
    RacialTrait => "Racial Trait": Dedicated,
    Source => "Source": Dedicated,
    Spell => "Spell": Dedicated,
    Weapon => "Weapon": Dedicated,
    Alignment => "Alignment": Default,
    ArmorGroup => "Armor Group": Default,
    Background => "Background": Default,
    BackgroundCharacteristics => "Background Characteristics": Default,
    BackgroundVariant => "Background Variant": Default,
    CompanionAction => "Companion Action": Default,
    CompanionFeature => "Companion Feature": Default,
    CompanionReaction => "Companion Reaction": Default,
    CompanionTrait => "Companion Trait": Default,
    Condition => "Condition": Default,
    Core => "Core": Default,
    DamageType => "Damage Type": Default,
    Ignore => "Ignore": Default,
    Internal => "Internal": Default,
    ItemPack => "Item Pack": Default,
    MonsterAction => "Monster Action": Default,
    MonsterTrait => "Monster Trait": Default,
    Property => "Property": Default,
    RaceVariant => "Race Variant": Default,
    Size => "Size": Default,
    Skill => "Skill": Default,
    SpellcastingFocusGroup => "Spellcasting Focus Group": Default,
    SubRace => "Sub Race": Default,
    Support => "Support": Default,
    Vision => "Vision": Default,
    WeaponCategory => "Weapon Category": Default,
    WeaponGroup => "Weapon Group": Default,
    WeaponProperty => "Weapon Property": Default,
    Species => "Species": Proposed2024,
    SpeciesTraitOption => "Species Trait Option": Proposed2024,
    MasteryProperty => "Mastery Property": Proposed2024,
    EquipmentPackage => "Equipment Package": Proposed2024,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_known_type() {
        for t in KnownType::ALL {
            assert_eq!(KnownType::parse(t.as_str()), Some(*t));
        }
        assert_eq!(KnownType::ALL.len(), 63);
    }

    #[test]
    fn unknown_and_case_sensitive() {
        assert_eq!(KnownType::parse("Homebrew Thing"), None);
        assert_eq!(KnownType::parse("class"), None);
        assert_eq!(KnownType::SubRace.as_str(), "Sub Race");
        assert_eq!(KnownType::Species.category(), TypeCategory::Proposed2024);
        assert_eq!(KnownType::Condition.category(), TypeCategory::Default);
    }
}
