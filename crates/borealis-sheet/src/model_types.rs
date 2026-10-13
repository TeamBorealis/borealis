//! Typed character-sheet model. Every value a sheet prints or pre-fills, already
//! formatted and normalised, independent of layout. Built by [`crate::build_sheet_data`] from
//! the engine snapshot; edition-specific sets live in [`EditionFields`].
//!
//! Fields the engine does not track (current/temp HP, hit dice spent, death saves, slots
//! expended, conditions) are play-time write-ins: they exist as field ids but carry no value.

use std::collections::HashMap;

/// Which rules edition the sheet follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// 2014 rules (SRD 5.1 era).
    Dnd2014,
    /// 2024 rules (SRD 5.2.1 era).
    Dnd2024,
}

/// The whole typed sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetData {
    /// Edition the field set follows.
    pub edition: Edition,
    /// Name, classes, background and progression.
    pub identity: Identity,
    /// The six abilities in the order STR..CHA.
    pub abilities: Vec<AbilityData>,
    /// The 18 skills in engine order.
    pub skills: Vec<SkillData>,
    /// Passive scores printed on the sheet (Perception plus any with a bonus).
    pub passives: Vec<PassiveData>,
    /// Proficiency bonus.
    pub proficiency_bonus: i32,
    /// AC, initiative, speed, HP, attacks, senses, defences.
    pub combat: CombatData,
    /// Normalised armour, weapon, tool and language proficiencies.
    pub proficiencies: ProficiencyData,
    /// De-duplicated, ordered, formula-evaluated features.
    pub features: Vec<FeatureData>,
    /// Limited-use pools and free casts.
    pub limited_uses: Vec<LimitedUse>,
    /// Equipment, coins, carrying and attunement.
    pub gear: GearData,
    /// Per-class casting plus pooled multiclass slots.
    pub spellcasting: SpellcastingData,
    /// Play-time state.
    pub play: PlayData,
    /// Story page fields.
    pub story: StoryData,
    /// Edition-only field sets.
    pub edition_fields: EditionFields,
    /// Borealis-only extras.
    pub extras: ExtrasData,
}

/// Edition-only field set.
#[derive(Debug, Clone, PartialEq)]
pub enum EditionFields {
    /// 2014-only fields.
    Y2014(Fields2014),
    /// 2024-only fields.
    Y2024(Fields2024),
}

/// One class of the character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassLevel {
    /// Class name.
    pub class: String,
    /// Levels in it.
    pub level: u32,
    /// Subclass names.
    pub subclass: Option<String>,
    /// Hit die size.
    pub hit_die: Option<u32>,
}

/// Name line and progression.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    /// Character name ("Unnamed character" when blank).
    pub name: String,
    /// Player name.
    pub player: String,
    /// Total level.
    pub level: u32,
    /// Classes in progression order.
    pub classes: Vec<ClassLevel>,
    /// "Cleric 3 / Wizard 2".
    pub class_label: String,
    /// Subclass names joined with ", ".
    pub subclass_label: String,
    /// Background name.
    pub background: String,
    /// Alignment name.
    pub alignment: String,
    /// Experience points.
    pub experience: u32,
    /// XP for the next level.
    pub next_level_xp: Option<u32>,
    /// Creature size (2024 prints it; Borealis prints it for both).
    pub size: Option<String>,
    /// Deity name.
    pub deity: String,
}

/// One ability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbilityData {
    /// Three-letter key: `str`, `dex`, `con`, `int`, `wis`, `cha`.
    pub key: &'static str,
    /// Display name ("Strength").
    pub name: String,
    /// Score.
    pub score: i32,
    /// Modifier.
    pub modifier: i32,
    /// Saving throw total.
    pub save: i32,
    /// Proficient in the save.
    pub save_proficient: bool,
}

/// One skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillData {
    /// Field key (`sleight_of_hand`).
    pub key: String,
    /// Display name ("Sleight of Hand").
    pub name: String,
    /// Governing ability key.
    pub ability: &'static str,
    /// Total bonus.
    pub total: i32,
    /// Proficient.
    pub proficient: bool,
    /// Expertise.
    pub expertise: bool,
    /// Passive score.
    pub passive: i32,
}

/// A printed passive score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassiveData {
    /// Skill key.
    pub key: String,
    /// Label ("Passive Perception").
    pub label: String,
    /// Value.
    pub value: i32,
}

/// One die type of the hit dice pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitDice {
    /// Die size.
    pub die: u32,
    /// How many dice (levels in classes using it).
    pub count: u32,
}

/// Kind of attack line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackKind {
    /// Equipped weapon.
    Weapon,
    /// Manually entered line.
    Manual,
    /// Damage cantrip.
    Cantrip,
}

/// One attack line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackRow {
    /// Name.
    pub name: String,
    /// Where it comes from.
    pub kind: AttackKind,
    /// Hit / DC cell: attack bonus with sign ("+5"), or "DC 14 DEX" for a save cantrip.
    pub bonus: String,
    /// Damage text.
    pub damage: String,
    /// Range text.
    pub range: String,
    /// Weapon mastery property (2024).
    pub mastery: Option<String>,
    /// Extra notes (casting class of cantrips, offhand damage).
    pub notes: String,
}

/// Combat block.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatData {
    /// Armour class.
    pub armor_class: i32,
    /// Conditional AC note.
    pub ac_note: String,
    /// A shield is equipped.
    pub shield_equipped: bool,
    /// Initiative modifier.
    pub initiative: i32,
    /// Advantage on initiative.
    pub initiative_advantage: bool,
    /// Speeds: walk first, then non-zero modes.
    pub speeds: Vec<(String, i32)>,
    /// Maximum hit points.
    pub hp_max: i32,
    /// Hit dice by die size, largest first.
    pub hit_dice: Vec<HitDice>,
    /// Attack lines.
    pub attacks: Vec<AttackRow>,
}

/// Normalised proficiencies (no duplicates, no category-plus-member repeats, no internal labels).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProficiencyData {
    /// Armour: categories plus uncovered extras, "Shields" once.
    pub armor: Vec<String>,
    /// Weapons: categories plus uncovered extras.
    pub weapons: Vec<String>,
    /// Tools, instruments, gaming sets, vehicles.
    pub tools: Vec<String>,
    /// Languages.
    pub languages: Vec<String>,
}

/// Where a feature comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureGroup {
    /// Class feature.
    Class,
    /// Subclass feature.
    Subclass,
    /// Species or race trait.
    Species,
    /// Feat.
    Feat,
    /// Background feature.
    Background,
    /// Anything else.
    Other,
}

/// One feature, trait or feat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureData {
    /// Element id of the first occurrence.
    pub id: String,
    /// Unique display name.
    pub name: String,
    /// Group.
    pub group: FeatureGroup,
    /// Class or subclass the feature is listed under, when any.
    pub class: Option<String>,
    /// Level it is gained at.
    pub level: u32,
    /// Granting elements (several when the same feature comes from two classes).
    pub sources: Vec<String>,
    /// Full text: no repeated name, formulas evaluated.
    pub text: String,
    /// Short text (first sentence, at most 180 characters).
    pub summary: String,
}

/// A limited-use pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitedUse {
    /// Name.
    pub name: String,
    /// Maximum uses.
    pub max: i32,
    /// Recharge ("Long Rest", "Short or Long Rest").
    pub recharge: String,
}

/// One equipment line.
#[derive(Debug, Clone, PartialEq)]
pub struct GearItem {
    /// Name.
    pub name: String,
    /// Quantity.
    pub quantity: u32,
    /// Weight of the line in pounds.
    pub weight: f64,
    /// "worn", "main hand", "off hand" or empty.
    pub equipped: String,
    /// Attuned.
    pub attuned: bool,
    /// Needs attunement.
    pub requires_attunement: bool,
}

/// Equipment block.
#[derive(Debug, Clone, PartialEq)]
pub struct GearData {
    /// Lines.
    pub items: Vec<GearItem>,
    /// Coins CP SP EP GP PP.
    pub coins: [u64; 5],
    /// Carried weight.
    pub weight: f64,
    /// Carrying capacity.
    pub capacity: f64,
    /// Attunement slots available.
    pub attunement_limit: u32,
    /// Names of attuned items.
    pub attuned: Vec<String>,
}

/// Casting-time and similar facts of a spell, supplied from content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpellFacts {
    /// "1 action".
    pub casting_time: Option<String>,
    /// "120 feet".
    pub range: Option<String>,
    /// "V, S, M".
    pub components: Option<String>,
    /// "Instantaneous".
    pub duration: Option<String>,
    /// Requires concentration.
    pub concentration: bool,
    /// Can be cast as a ritual.
    pub ritual: bool,
    /// Short effect text from SRD facts.
    pub effect: Option<String>,
    /// The spell is resolved with a spell attack roll.
    pub attack_roll: bool,
    /// Three-letter ability of the target's saving throw ("DEX"), when the spell has one.
    pub save: Option<String>,
}

/// Spell facts by spell element id.
pub type SpellFactsMap = HashMap<String, SpellFacts>;

/// One spell row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellRow {
    /// Spell element id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Level (0 = cantrip).
    pub level: u32,
    /// Prepared (always-prepared included).
    pub prepared: bool,
    /// Always prepared.
    pub always_prepared: bool,
    /// Known or granted (as opposed to merely preparable).
    pub known: bool,
    /// Granting feature, when not from the class list.
    pub source: Option<String>,
    /// Cantrip damage at this level.
    pub damage: Option<String>,
    /// Content facts; `None` until the content lookup supplies them.
    pub facts: Option<SpellFacts>,
}

/// Pact magic slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PactSlots {
    /// Number of slots.
    pub count: i32,
    /// Level they are cast at.
    pub level: i32,
}

/// One spellcasting class.
#[derive(Debug, Clone, PartialEq)]
pub struct CasterData {
    /// Class name.
    pub class: String,
    /// Ability key.
    pub ability: &'static str,
    /// Ability display name.
    pub ability_name: String,
    /// Ability modifier.
    pub ability_modifier: i32,
    /// Save DC.
    pub dc: i32,
    /// Spell attack bonus.
    pub attack: i32,
    /// Prepares spells.
    pub prepares: bool,
    /// Prepared limit.
    pub prepare_limit: Option<i32>,
    /// Prepared count.
    pub prepared_count: u32,
    /// Class slot table for spell levels 1 to 9 (always nine entries).
    pub slots: [i32; 9],
    /// The character casts with the pooled multiclass table instead.
    pub slots_pooled: bool,
    /// Pact magic.
    pub pact: Option<PactSlots>,
    /// Cantrips (level 0), sorted by name.
    pub cantrips: Vec<SpellRow>,
    /// Leveled spells, sorted by level then name.
    pub spells: Vec<SpellRow>,
}

impl CasterData {
    /// Slots of one spell level (1..=9) for a per-level heading: the class table, plus the Pact
    /// Magic slots at their level (a separate pool, never in `slots`); 0 for any other level.
    #[must_use]
    pub fn level_slots(&self, level: usize) -> i32 {
        let table = level
            .checked_sub(1)
            .and_then(|i| self.slots.get(i))
            .copied()
            .unwrap_or(0);
        let pact = self
            .pact
            .filter(|p| usize::try_from(p.level).ok() == Some(level))
            .map_or(0, |p| p.count);
        table + pact
    }

    /// Leveled spells to print: all of them, or only prepared / always-prepared / non-preparing
    /// casters' spells when `include_unprepared` is false.
    #[must_use]
    pub fn listed(&self, include_unprepared: bool) -> Vec<&SpellRow> {
        self.spells
            .iter()
            .filter(|s| !self.prepares || include_unprepared || s.prepared || s.always_prepared)
            .collect()
    }
}

/// Pooled multiclass slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PooledSlots {
    /// Combined caster level.
    pub caster_level: u32,
    /// Slots by level 1 to 9.
    pub slots: [i32; 9],
}

/// Spellcasting block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpellcastingData {
    /// One entry per casting class.
    pub casters: Vec<CasterData>,
    /// Pooled slots when two or more classes cast with slots.
    pub pooled: Option<PooledSlots>,
}

/// Exhaustion state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExhaustionData {
    /// Current level.
    pub level: u32,
    /// Highest level (6 in both editions).
    pub max_levels: u32,
    /// Linear 2024 track (d20 penalty and speed penalty) rather than the 2014 table.
    pub linear: bool,
}

/// Play-time state. Values the engine does not track are `None` and print as write-ins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayData {
    /// Current hit points (not tracked).
    pub current_hp: Option<i32>,
    /// Temporary hit points (not tracked).
    pub temp_hp: Option<i32>,
    /// Inspiration (2014) / Heroic Inspiration (2024).
    pub inspiration: bool,
    /// Exhaustion.
    pub exhaustion: ExhaustionData,
    /// Condition names with a tick box each.
    pub conditions: &'static [&'static str],
}

/// Story page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoryData {
    /// Gender.
    pub gender: String,
    /// Age.
    pub age: String,
    /// Height.
    pub height: String,
    /// Weight.
    pub weight: String,
    /// Eyes.
    pub eyes: String,
    /// Skin.
    pub skin: String,
    /// Hair.
    pub hair: String,
    /// A portrait is attached.
    pub has_portrait: bool,
    /// Personality traits.
    pub traits: String,
    /// Ideals.
    pub ideals: String,
    /// Bonds.
    pub bonds: String,
    /// Flaws.
    pub flaws: String,
    /// Trinket.
    pub trinket: String,
    /// Backstory.
    pub backstory: String,
    /// Organisation name.
    pub organization: String,
    /// Allies.
    pub allies: String,
    /// Treasure.
    pub treasure: String,
    /// Quest items.
    pub quest: String,
    /// Equipment notes.
    pub equipment_notes: String,
    /// Additional features text.
    pub additional_features: String,
    /// Notes (both columns, joined).
    pub notes: String,
}

impl StoryData {
    /// Whether the Story page has anything to print.
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.has_portrait
            || [
                &self.age,
                &self.height,
                &self.weight,
                &self.eyes,
                &self.skin,
                &self.hair,
                &self.traits,
                &self.ideals,
                &self.bonds,
                &self.flaws,
                &self.backstory,
                &self.organization,
                &self.allies,
                &self.treasure,
                &self.quest,
                &self.additional_features,
                &self.notes,
            ]
            .iter()
            .any(|s| !s.trim().is_empty())
    }
}

/// 2014-only fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields2014 {
    /// Race.
    pub race: String,
    /// Sub-race.
    pub subrace: Option<String>,
}

/// Armour training ticks (2024).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArmorTraining {
    /// Light armour.
    pub light: bool,
    /// Medium armour.
    pub medium: bool,
    /// Heavy armour.
    pub heavy: bool,
    /// Shields.
    pub shields: bool,
}

/// 2024-only fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields2024 {
    /// Species.
    pub species: String,
    /// Lineage or sub-choice of the species.
    pub lineage: Option<String>,
    /// Origin feat granted by the background.
    pub origin_feat: Option<String>,
    /// Armour training ticks.
    pub armor_training: ArmorTraining,
    /// Chosen weapon mastery weapons.
    pub weapon_masteries: Vec<String>,
}

/// Borealis-only extras.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtrasData {
    /// "Darkvision 60 ft".
    pub senses: Vec<String>,
    /// Damage resistances in effect.
    pub resistances: Vec<String>,
    /// Damage immunities in effect.
    pub immunities: Vec<String>,
    /// Damage vulnerabilities in effect.
    pub vulnerabilities: Vec<String>,
    /// Condition immunities in effect.
    pub condition_immunities: Vec<String>,
    /// Advantage notes.
    pub advantages: Vec<String>,
    /// Companion name (no stat block in the model yet).
    pub companion_name: String,
    /// The character has Wild Shape.
    pub has_wild_shape: bool,
}
