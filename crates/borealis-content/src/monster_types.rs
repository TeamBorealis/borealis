//! Building blocks of the monster stat block model (see [`crate::monster`]).

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! lowercase_enum {
    ($(#[$m:meta])* $name:ident { $( $(#[$vm:meta])* $variant:ident => $text:literal ),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name { $( $(#[$vm])* #[serde(rename = $text)] $variant ),+ }

        impl $name {
            /// Every value in declaration order.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// The serialized name.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $( $name::$variant => $text ),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
    };
}

lowercase_enum! {
    /// Creature size category.
    CreatureSize {
        /// Tiny.
        Tiny => "tiny",
        /// Small.
        Small => "small",
        /// Medium.
        Medium => "medium",
        /// Large.
        Large => "large",
        /// Huge.
        Huge => "huge",
        /// Gargantuan.
        Gargantuan => "gargantuan",
    }
}

lowercase_enum! {
    /// Creature type.
    CreatureType {
        /// Aberration.
        Aberration => "aberration",
        /// Beast.
        Beast => "beast",
        /// Celestial.
        Celestial => "celestial",
        /// Construct.
        Construct => "construct",
        /// Dragon.
        Dragon => "dragon",
        /// Elemental.
        Elemental => "elemental",
        /// Fey.
        Fey => "fey",
        /// Fiend.
        Fiend => "fiend",
        /// Giant.
        Giant => "giant",
        /// Humanoid.
        Humanoid => "humanoid",
        /// Monstrosity.
        Monstrosity => "monstrosity",
        /// Ooze.
        Ooze => "ooze",
        /// Plant.
        Plant => "plant",
        /// Undead.
        Undead => "undead",
    }
}

lowercase_enum! {
    /// Ability (short keys as used in saves and ability tables).
    Ability {
        /// Strength.
        Str => "str",
        /// Dexterity.
        Dex => "dex",
        /// Constitution.
        Con => "con",
        /// Intelligence.
        Int => "int",
        /// Wisdom.
        Wis => "wis",
        /// Charisma.
        Cha => "cha",
    }
}

lowercase_enum! {
    /// Skill.
    Skill {
        /// Acrobatics.
        Acrobatics => "acrobatics",
        /// Animal Handling.
        AnimalHandling => "animal handling",
        /// Arcana.
        Arcana => "arcana",
        /// Athletics.
        Athletics => "athletics",
        /// Deception.
        Deception => "deception",
        /// History.
        History => "history",
        /// Insight.
        Insight => "insight",
        /// Intimidation.
        Intimidation => "intimidation",
        /// Investigation.
        Investigation => "investigation",
        /// Medicine.
        Medicine => "medicine",
        /// Nature.
        Nature => "nature",
        /// Perception.
        Perception => "perception",
        /// Performance.
        Performance => "performance",
        /// Persuasion.
        Persuasion => "persuasion",
        /// Religion.
        Religion => "religion",
        /// Sleight of Hand.
        SleightOfHand => "sleight of hand",
        /// Stealth.
        Stealth => "stealth",
        /// Survival.
        Survival => "survival",
    }
}

lowercase_enum! {
    /// Damage type.
    DamageType {
        /// Acid.
        Acid => "acid",
        /// Bludgeoning.
        Bludgeoning => "bludgeoning",
        /// Cold.
        Cold => "cold",
        /// Fire.
        Fire => "fire",
        /// Force.
        Force => "force",
        /// Lightning.
        Lightning => "lightning",
        /// Necrotic.
        Necrotic => "necrotic",
        /// Piercing.
        Piercing => "piercing",
        /// Poison.
        Poison => "poison",
        /// Psychic.
        Psychic => "psychic",
        /// Radiant.
        Radiant => "radiant",
        /// Slashing.
        Slashing => "slashing",
        /// Thunder.
        Thunder => "thunder",
    }
}

/// Ability scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Abilities {
    /// Strength.
    pub str: u8,
    /// Dexterity.
    pub dex: u8,
    /// Constitution.
    pub con: u8,
    /// Intelligence.
    pub int: u8,
    /// Wisdom.
    pub wis: u8,
    /// Charisma.
    pub cha: u8,
}

impl Abilities {
    /// Score of `ability`.
    #[must_use]
    pub fn score(&self, ability: Ability) -> u8 {
        match ability {
            Ability::Str => self.str,
            Ability::Dex => self.dex,
            Ability::Con => self.con,
            Ability::Int => self.int,
            Ability::Wis => self.wis,
            Ability::Cha => self.cha,
        }
    }

    /// Modifier of `ability` (floor((score - 10) / 2)).
    #[must_use]
    pub fn modifier(&self, ability: Ability) -> i32 {
        (i32::from(self.score(ability)) - 10).div_euclid(2)
    }
}

/// One armor class value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmorClass {
    /// The AC.
    pub value: u32,
    /// What provides it ("natural armor", "leather armor, shield", "15 with mage armor").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// When it applies ("in humanoid form").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Hit points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitPoints {
    /// The average value.
    pub average: u32,
    /// Hit dice formula without spaces (`19d12+133`); absent for fixed HP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
}

/// Movement speeds in feet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Speeds {
    /// Walking speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk: Option<u32>,
    /// Burrow speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burrow: Option<u32>,
    /// Climb speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climb: Option<u32>,
    /// Fly speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fly: Option<u32>,
    /// Swim speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swim: Option<u32>,
    /// True if it can hover.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hover: bool,
    /// Printed form-dependent remarks ("walk: 40 ft. in boar form").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One clause of a damage vulnerability/resistance/immunity list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageClause {
    /// The damage types the clause names.
    #[serde(default)]
    pub types: Vec<DamageType>,
    /// The clause verbatim when it is more than a plain list of types (qualifiers such as
    /// "from nonmagical attacks"); absent for a plain list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Senses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Senses {
    /// Blindsight range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blindsight: Option<u32>,
    /// Darkvision range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub darkvision: Option<u32>,
    /// Tremorsense range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tremorsense: Option<u32>,
    /// Truesight range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truesight: Option<u32>,
    /// Reduced range while deafened (grimlock).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deafened_range: Option<u32>,
    /// "blind beyond this radius".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blind_beyond: bool,
    /// Passive Perception.
    pub passive_perception: u32,
    /// Printed remarks ("darkvision: rat form only").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Challenge rating, stored as a reduced fraction so `1/8` never becomes a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cr {
    numer: u8,
    denom: u8,
}

impl Cr {
    /// Every legal CR with its XP award, in ascending order (identical in SRD 5.1 and 5.2.1).
    pub const XP_TABLE: &'static [(&'static str, u32)] = &[
        ("0", 10),
        ("1/8", 25),
        ("1/4", 50),
        ("1/2", 100),
        ("1", 200),
        ("2", 450),
        ("3", 700),
        ("4", 1_100),
        ("5", 1_800),
        ("6", 2_300),
        ("7", 2_900),
        ("8", 3_900),
        ("9", 5_000),
        ("10", 5_900),
        ("11", 7_200),
        ("12", 8_400),
        ("13", 10_000),
        ("14", 11_500),
        ("15", 13_000),
        ("16", 15_000),
        ("17", 18_000),
        ("18", 20_000),
        ("19", 22_000),
        ("20", 25_000),
        ("21", 33_000),
        ("22", 41_000),
        ("23", 50_000),
        ("24", 62_000),
        ("25", 75_000),
        ("26", 90_000),
        ("27", 105_000),
        ("28", 120_000),
        ("29", 135_000),
        ("30", 155_000),
    ];

    /// Parse `0`, `1/8`, `1/4`, `1/2` or an integer 1..=30.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if !Self::XP_TABLE.iter().any(|(t, _)| *t == text) {
            return None;
        }
        match text.split_once('/') {
            Some((n, d)) => Some(Self {
                numer: n.parse().ok()?,
                denom: d.parse().ok()?,
            }),
            None => Some(Self {
                numer: text.parse().ok()?,
                denom: 1,
            }),
        }
    }

    /// Canonical text (`1/8`, `17`).
    #[must_use]
    pub fn as_text(self) -> String {
        if self.denom == 1 {
            self.numer.to_string()
        } else {
            format!("{}/{}", self.numer, self.denom)
        }
    }

    /// The value as a float (only for ordering/display; never compare CRs with it).
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.numer) / f64::from(self.denom)
    }

    /// XP award from the standard table.
    #[must_use]
    pub fn standard_xp(self) -> u32 {
        let text = self.as_text();
        Self::XP_TABLE
            .iter()
            .find(|(t, _)| *t == text)
            .map_or(0, |(_, xp)| *xp)
    }

    /// True if `xp` is acceptable for this CR (CR 0 is printed as either 0 or 10 XP).
    #[must_use]
    pub fn accepts_xp(self, xp: u32) -> bool {
        xp == self.standard_xp() || (self.numer == 0 && xp == 0)
    }

    /// Proficiency bonus for this CR: 2 up to CR 4, then +1 every four levels.
    #[must_use]
    pub fn proficiency_bonus(self) -> u8 {
        if self.denom != 1 || self.numer == 0 {
            return 2;
        }
        2 + (self.numer.saturating_sub(1)) / 4
    }
}

impl fmt::Display for Cr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

impl Serialize for Cr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_text())
    }
}

impl<'de> Deserialize<'de> for Cr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Int(u8),
            Text(String),
        }
        let text = match Repr::deserialize(d)? {
            Repr::Int(n) => n.to_string(),
            Repr::Text(t) => t,
        };
        Cr::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid challenge rating {text:?}")))
    }
}

/// Which rest restores a limited-use feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestKind {
    /// "Recharges after a Short or Long Rest".
    ShortOrLong,
    /// Long rest only.
    Long,
}

/// How often a feature can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Usage {
    /// "Recharge 5-6": regained on a d6 roll in `min..=max`.
    Recharge {
        /// Lowest recharging roll.
        min: u8,
        /// Highest recharging roll.
        max: u8,
    },
    /// "3/Day".
    PerDay(u8),
    /// "3/Day, or 4/Day in Lair" (SRD 5.2.1 Legendary Resistance).
    PerDayInLair {
        /// Uses per day outside the lair.
        normal: u8,
        /// Uses per day in the creature's lair.
        lair: u8,
    },
    /// "Recharges after a Short or Long Rest".
    OnRest(RestKind),
}

/// Kind of attack roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackKind {
    /// Melee weapon attack.
    MeleeWeapon,
    /// Ranged weapon attack.
    RangedWeapon,
    /// Melee or ranged weapon attack.
    MeleeOrRangedWeapon,
    /// Melee spell attack.
    MeleeSpell,
    /// Ranged spell attack.
    RangedSpell,
    /// "Melee Attack Roll" (SRD 5.2.1 no longer splits weapon and spell attacks).
    Melee,
    /// "Ranged Attack Roll".
    Ranged,
    /// "Melee or Ranged Attack Roll".
    MeleeOrRanged,
}

/// One damage part of an attack ("7 (2d6) fire").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamagePart {
    /// Printed average.
    pub average: u32,
    /// Dice expression without spaces (`2d10+8`); absent for flat damage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    /// Damage type.
    #[serde(rename = "type")]
    pub damage_type: DamageType,
}

/// Structured view of a weapon/spell attack action (the `text` stays authoritative).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attack {
    /// Attack kind.
    pub kind: AttackKind,
    /// To-hit bonus.
    pub to_hit: i32,
    /// Melee reach in feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<u32>,
    /// Normal range in feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<u32>,
    /// Long range in feet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_range: Option<u32>,
    /// Damage parts in printed order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage: Vec<DamagePart>,
}

/// A trait, action, bonus action, reaction or legendary action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    /// Name without parenthetical usage notes.
    pub name: String,
    /// Full text. Spell lists keep one list line per `\n`.
    pub text: String,
    /// Recharge / per-day usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Legendary action cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<u8>,
    /// Other parenthetical on the name ("Humanoid Form Only").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Parsed attack, when the text is a standard attack line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack: Option<Attack>,
    /// First saving throw the text calls for (SRD 5.2.1 "Dexterity Saving Throw: DC 21 ... Failure: ...").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<SaveEffect>,
    /// Parsed spell lists of a `Spellcasting` feature (the `text` stays authoritative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<SpellcastingBlock>,
}

/// Legendary actions block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegendaryActions {
    /// Legendary actions per round (SRD 5.1) or "Legendary Action Uses" (SRD 5.2.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_round: Option<u8>,
    /// Uses when the creature is in its lair ("Legendary Action Uses: 3 (4 in Lair)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lair_uses: Option<u8>,
    /// Introductory paragraph.
    #[serde(default)]
    pub preamble: String,
    /// The options.
    #[serde(default)]
    pub actions: Vec<Feature>,
}

/// Lair actions or regional effects: a section printed with a creature's lair (5etools keeps
/// them in the creature's legendary group).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LairSection {
    /// Text before the options (paragraphs separated by a blank line).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preamble: String,
    /// The options or effects, in printed order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<LairOption>,
    /// Text printed after the options (paragraphs separated by a blank line).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub closing: String,
}

/// One lair action or regional effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LairOption {
    /// The option's name, when it is printed with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Full text.
    pub text: String,
}

/// Initiative entry of a 2024 stat block: "Initiative +7 (17)".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initiative {
    /// Modifier added to the Initiative roll.
    pub modifier: i32,
    /// Fixed Initiative score (10 + modifier) for tables that do not roll.
    pub score: u8,
}

/// Structured view of a saving throw effect ("Constitution Saving Throw: DC 20 ... Failure: 31 (9d6) Necrotic damage.
/// Success: Half damage.").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveEffect {
    /// Ability of the saving throw.
    pub ability: Ability,
    /// Save DC.
    pub dc: u8,
    /// Damage parts printed after "Failure:" (empty when the failure is a condition or other effect).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage: Vec<DamagePart>,
    /// True if "Success: Half damage."
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub half_on_success: bool,
}

/// How often a spell list entry can be cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpellFrequency {
    /// "At Will".
    AtWill,
    /// "2/Day Each" (`each`) or "1/Day" (a single use shared by the list).
    PerDay {
        /// Uses per day.
        uses: u8,
        /// True if every spell of the list gets that many uses.
        each: bool,
    },
}

/// One spell named in a stat block spell list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellRef {
    /// Spell name as printed.
    pub name: String,
    /// Content id of the spell when it exists in the bundled SRD 5.2.1 spells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Printed parenthetical ("level 5 version", "included in AC").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One "At Will: ..." or "1/Day Each: ..." line of a spell list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellList {
    /// Frequency.
    pub frequency: SpellFrequency,
    /// The spells.
    pub spells: Vec<SpellRef>,
}

/// Spellcasting details parsed from a `Spellcasting` feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellcastingBlock {
    /// Spellcasting ability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ability: Option<Ability>,
    /// Spell save DC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dc: Option<u8>,
    /// Spell attack bonus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_bonus: Option<i32>,
    /// The spell lists.
    pub lists: Vec<SpellList>,
}
