//! Contract DTOs: the JSON shapes the UI expects (`ui/src/api/types`). All camelCase.

use borealis_engine::state::AbilityMethod;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use borealis_content::Ruleset;

/// Ability score generation method as the UI names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityMethodDto {
    /// Point buy.
    #[serde(rename = "point_buy")]
    PointBuy,
    /// Standard array.
    #[serde(rename = "standard_array")]
    StandardArray,
    /// 3d6.
    #[serde(rename = "roll_3d6")]
    Roll3d6,
    /// 4d6 drop lowest.
    #[serde(rename = "roll_4d6")]
    Roll4d6,
    /// Free entry.
    #[serde(rename = "manual")]
    Manual,
}

impl From<AbilityMethodDto> for AbilityMethod {
    fn from(m: AbilityMethodDto) -> Self {
        match m {
            AbilityMethodDto::PointBuy => Self::PointBuy,
            AbilityMethodDto::StandardArray => Self::StandardArray,
            AbilityMethodDto::Roll3d6 => Self::Roll3d6,
            AbilityMethodDto::Roll4d6 => Self::Roll4d6,
            AbilityMethodDto::Manual => Self::Manual,
        }
    }
}

impl From<AbilityMethod> for AbilityMethodDto {
    fn from(m: AbilityMethod) -> Self {
        match m {
            AbilityMethod::PointBuy => Self::PointBuy,
            AbilityMethod::StandardArray => Self::StandardArray,
            AbilityMethod::Roll3d6 => Self::Roll3d6,
            AbilityMethod::Roll4d6 => Self::Roll4d6,
            AbilityMethod::Manual => Self::Manual,
        }
    }
}

/// The six abilities, serialised as `{ str, dex, con, int, wis, cha }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AbilityRecord<T> {
    pub str: T,
    pub dex: T,
    pub con: T,
    pub int: T,
    pub wis: T,
    pub cha: T,
}

impl<T> AbilityRecord<T> {
    /// Build from a function of the ability index (0 = STR .. 5 = CHA).
    pub fn from_fn(mut f: impl FnMut(usize) -> T) -> Self {
        Self {
            str: f(0),
            dex: f(1),
            con: f(2),
            int: f(3),
            wis: f(4),
            cha: f(5),
        }
    }
}

/// Reference to a content element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementRef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub source: String,
}

/// Element row for pickers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub source: String,
    pub source_name: String,
    pub short_description: String,
    pub prerequisites: Option<String>,
    pub rulesets: Vec<Ruleset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub school: Option<String>,
    /// Casting facts (spells).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spell: Option<crate::gear_facts::SpellFacts>,
    /// Armor columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armor: Option<crate::gear_facts::ArmorFacts>,
    /// Weapon columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon: Option<crate::gear_facts::WeaponFacts>,
}

/// A slot option (an element row, optionally disabled).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotOption {
    #[serde(flatten)]
    pub summary: ElementSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

/// A page of results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page<T> {
    pub total: usize,
    pub items: Vec<T>,
}

/// Library row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSummary {
    pub id: String,
    pub name: String,
    pub build: String,
    pub level: u32,
    pub ruleset: Ruleset,
    pub portrait: Option<String>,
    pub favorite: bool,
    pub group: String,
    pub updated_at: String,
    pub path: Option<String>,
    /// Content references from a reference-app import that are still not installed.
    pub missing_content: u32,
}

/// Character options as the UI edits them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CharacterOptions {
    pub multiclassing: bool,
    pub feats: bool,
    pub average_hp: bool,
    pub point_buy_unrestricted: bool,
    pub attunement_limit: u32,
    pub custom: BTreeMap<String, bool>,
}

/// Partial options (all fields optional).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OptionsPatch {
    pub multiclassing: Option<bool>,
    pub feats: Option<bool>,
    pub average_hp: Option<bool>,
    pub point_buy_unrestricted: Option<bool>,
    pub attunement_limit: Option<u32>,
    pub custom: Option<BTreeMap<String, bool>>,
}

/// Option definition to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub locked: bool,
}

/// `create_character` request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCharacterRequest {
    pub name: String,
    pub ruleset: Ruleset,
    #[serde(default = "one")]
    pub starting_level: u32,
    #[serde(default)]
    pub gender: String,
    pub ability_method: AbilityMethodDto,
    #[serde(default)]
    pub options: OptionsPatch,
    #[serde(default)]
    pub portrait: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

const fn one() -> u32 {
    1
}

/// A selection slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSlot {
    pub id: String,
    pub tab: String,
    pub group_id: Option<String>,
    pub group_label: Option<String>,
    /// Spell slots: `ClassSpellcasting.classSlotId` of the class the spell is learned for.
    pub class_slot_id: Option<String>,
    pub title: String,
    pub element_type: String,
    pub ordinal: u32,
    pub of: u32,
    pub required: bool,
    pub locked: bool,
    pub chosen: Option<ElementRef>,
    pub required_level: Option<u32>,
    pub allow_replace: bool,
    pub hint: Option<String>,
    /// Level the slot was gained at (rule level, or the class level it reopened at).
    pub gained_on_level: u32,
    /// Picks replaced in this slot, oldest first.
    pub replaced: Vec<ReplacedPick>,
}

/// A pick that was replaced in a selection slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacedPick {
    pub id: String,
    pub name: String,
    pub level: u32,
}

/// Progress through the required slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionProgress {
    pub total: usize,
    pub remaining: usize,
    pub next_slot_id: Option<String>,
    pub by_tab: BTreeMap<String, usize>,
}

/// One class progression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassLevelInfo {
    pub slot_id: String,
    pub class_id: Option<String>,
    pub name: Option<String>,
    pub level: u32,
    pub hit_die: u32,
    pub is_main: bool,
    pub subclass: Option<ElementRef>,
    pub hp_rolls: Vec<u32>,
    pub multiclass_prerequisite: Option<String>,
}

/// Level state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelState {
    pub total: u32,
    pub xp: u32,
    pub next_level_xp: Option<u32>,
    pub current_level_xp: u32,
    pub level_up_available: bool,
    pub can_level_up: bool,
    pub can_level_down: bool,
    pub can_multiclass: bool,
    pub multiclass_blocked_reason: Option<String>,
    pub classes: Vec<ClassLevelInfo>,
}

/// One dice roll of the ability generator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollRecord {
    pub ability: String,
    pub dice: Vec<u32>,
    pub total: u32,
}

/// Point buy state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointBuyState {
    pub budget: i32,
    pub spent: i32,
    pub remaining: i32,
    pub costs: BTreeMap<i32, i32>,
    pub min: i32,
    pub max: i32,
    pub unrestricted: bool,
}

/// Standard array state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StandardArrayState {
    pub values: Vec<i32>,
    pub assigned: AbilityRecord<Option<i32>>,
    pub unassigned: Vec<i32>,
}

/// Saving throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct SaveEntry {
    pub bonus: i32,
    pub proficient: bool,
}

/// Ability scores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbilityState {
    pub method: AbilityMethodDto,
    pub base: AbilityRecord<i32>,
    pub bonus: AbilityRecord<i32>,
    pub bonus_sources: AbilityRecord<Vec<String>>,
    #[serde(rename = "final")]
    pub final_score: AbilityRecord<i32>,
    pub modifier: AbilityRecord<i32>,
    pub save: AbilityRecord<SaveEntry>,
    pub cap: i32,
    pub over_cap: Vec<String>,
    pub point_buy: Option<PointBuyState>,
    pub standard_array: Option<StandardArrayState>,
    pub rolls: Vec<RollRecord>,
}

/// One skill row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillEntry {
    pub key: String,
    pub name: String,
    pub ability: String,
    pub bonus: i32,
    pub proficiency: String,
}

/// Group of proficiencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProficiencyGroup {
    pub label: String,
    pub values: Vec<String>,
}

/// A named feature with its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureEntry {
    pub name: String,
    pub source: String,
}

/// Derived statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterStats {
    pub proficiency_bonus: i32,
    pub armor_class: i32,
    pub initiative: i32,
    pub speed: i32,
    pub hp_max: i32,
    pub passive_perception: i32,
    pub skills: Vec<SkillEntry>,
    pub languages: Vec<String>,
    pub proficiencies: Vec<ProficiencyGroup>,
    pub features: Vec<FeatureEntry>,
}

/// A spell row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellEntry {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub school: String,
    pub prepared: bool,
    pub always_prepared: bool,
    pub origin: String,
    pub slot_id: Option<String>,
    pub can_replace: bool,
    pub concentration: bool,
    pub ritual: bool,
    /// Casting time, range, duration and V/S/M components.
    pub casting: Option<crate::gear_facts::SpellFacts>,
    /// Name of the source book the spell comes from (empty when unknown).
    pub source_name: String,
}

/// Spellcasting of one class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassSpellcasting {
    pub class_slot_id: String,
    pub class_name: String,
    pub ability: String,
    pub save_dc: i32,
    pub attack_bonus: i32,
    pub model: String,
    pub cantrips_max: u32,
    pub cantrips_known: u32,
    pub prepared_max: u32,
    pub prepared_count: u32,
    pub max_spell_level: u32,
    pub spells: Vec<SpellEntry>,
}

/// Slots per spell level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpellSlotRow {
    pub level: u32,
    pub total: i32,
}

/// Spellcasting overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellcastingState {
    pub classes: Vec<ClassSpellcasting>,
    pub slots: Vec<SpellSlotRow>,
    pub multiclass_slots: Option<Vec<SpellSlotRow>>,
}

/// Weapon data of an inventory row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponInfo {
    pub damage: String,
    pub damage_type: String,
    pub properties: String,
    pub ability: String,
    /// Weapon Mastery property.
    pub mastery: Option<String>,
    /// Normal/long range in feet.
    pub range: Option<String>,
}

/// Inventory row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    pub id: String,
    pub item_id: String,
    pub name: String,
    pub display_name: String,
    pub category: String,
    pub quantity: u32,
    pub weight: f64,
    pub value_cp: u64,
    pub equipped: Option<String>,
    pub attuned: bool,
    pub requires_attunement: bool,
    pub storage: u8,
    pub notes: String,
    pub list_in_inventory: bool,
    /// Effective "show card" flag (the line's flag, off when the line is hidden).
    pub show_card: Option<bool>,
    pub is_pack: bool,
    pub pack_contents: Vec<String>,
    pub is_weapon: bool,
    pub weapon: Option<WeaponInfo>,
    /// Armor columns.
    pub armor: Option<crate::gear_facts::ArmorFacts>,
}

/// Coin purse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct Coins {
    pub cp: u64,
    pub sp: u64,
    pub ep: u64,
    pub gp: u64,
    pub pp: u64,
}

/// Attunement usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Attunement {
    pub used: u32,
    pub max: u32,
}

/// Inventory overview.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryState {
    pub items: Vec<InventoryItem>,
    pub coins: Coins,
    pub total_gp: f64,
    pub carried_weight: f64,
    pub capacity: f64,
    pub attunement: Attunement,
    pub storage_names: [String; 2],
    pub warnings: Vec<String>,
}

/// Free-text character details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDetails {
    pub gender: String,
    pub player_name: String,
    pub alignment: String,
    pub deity: String,
    pub age: String,
    pub height: String,
    pub weight: String,
    pub eyes: String,
    pub skin: String,
    pub hair: String,
    pub features: String,
    pub backstory: String,
    pub personality_traits: String,
    pub ideal: String,
    pub bond: String,
    pub flaw: String,
    pub trinket: String,
    pub notes_left: String,
    pub notes_right: String,
    pub organization: String,
    pub allies: String,
    pub combat_notes: String,
    pub sheet_ac_note: String,
    /// Additional treasure (free text).
    pub treasure: String,
    /// Quest items (free text).
    pub quest_items: String,
}

/// Attack row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attack {
    pub id: String,
    pub name: String,
    pub range: String,
    pub attack_bonus: String,
    pub damage: String,
    pub description: String,
    pub ability: Option<String>,
    pub auto: bool,
    pub show_in_combat: bool,
    pub card: bool,
    /// The weapon's mastery property (2024), with `mastered` when chosen.
    pub mastery: Option<crate::play_dto::MasteryDto>,
    /// Lowest d20 roll that is a critical hit.
    pub crit_range: u8,
    /// Damage dice never roll below this (Great Weapon Fighting).
    pub die_floor: Option<u8>,
    /// Damage of the off-hand attack when it differs (Light weapons).
    pub offhand_damage: Option<String>,
}

/// The complete character view, returned by every mutation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSnapshot {
    pub id: String,
    pub name: String,
    pub ruleset: Ruleset,
    pub portrait: Option<String>,
    pub group: String,
    pub favorite: bool,
    pub dirty: bool,
    /// Display-only path of the file this session saves to (never fed back into a command).
    pub display_path: String,
    pub level: LevelState,
    pub abilities: AbilityState,
    pub stats: CharacterStats,
    pub slots: Vec<SelectionSlot>,
    pub progress: SelectionProgress,
    pub spellcasting: SpellcastingState,
    pub inventory: InventoryState,
    pub details: CharacterDetails,
    pub attacks: Vec<Attack>,
    pub companion: Option<crate::content_dto::CompanionState>,
    pub options: CharacterOptions,
    pub available_options: Vec<OptionDef>,
    pub source_restrictions: Option<Vec<String>>,
    pub notices: Vec<String>,
    /// Resources, toggles, defences, senses, notes, Wild Shape, packages, compatibility, features.
    pub play: crate::play_dto::PlayStateDto,
}

/// `get_ruleset_profile` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesetProfileDto {
    pub id: Ruleset,
    pub label: String,
    pub srd_source: String,
    pub terminology: Terminology,
    pub ability_methods: Vec<AbilityMethodDto>,
    pub ability_cap: i32,
    pub point_buy_budget: i32,
    pub weapon_mastery: bool,
    pub origin_feats: bool,
    pub background_asi: bool,
}

/// Edition-specific words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Terminology {
    pub species: String,
    pub armor_training: String,
    pub inspiration: String,
}

/// `import_character` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub summary: CharacterSummary,
    pub divergences: Vec<String>,
    pub warnings: Vec<String>,
    /// Content the file references that is not installed.
    pub missing_content: Vec<MissingContentDto>,
    /// Selections that could not be placed (kept for `recheck_missing_content`).
    pub unmapped_slots: Vec<UnmappedSlotDto>,
    /// Stored choices the current rules reject.
    pub invalid_selections: Vec<String>,
    /// Selections placed on the character.
    pub selections_imported: u32,
}

/// An unresolved content reference in an import report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingContentDto {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub name: String,
    /// Where the file referenced it (`option`, `selection`, `inventory`, `dependency`, ...).
    pub context: String,
    /// The source book it most likely belongs to, derived from the id (may be empty).
    pub source: String,
}

/// A selection slot that could not be mapped onto a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmappedSlotDto {
    pub owner: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub name: String,
    pub chosen: String,
    pub reason: String,
}

/// One character in a `recheck_missing_content` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecheckEntry {
    pub character_id: String,
    pub name: String,
    /// References and slots that resolved this time.
    pub resolved: u32,
    pub still_missing: Vec<MissingContentDto>,
    pub still_unmapped: Vec<UnmappedSlotDto>,
}

/// `recheck_missing_content` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecheckReport {
    /// Characters that had something outstanding and were looked at.
    pub checked: u32,
    /// Characters that picked up content (the others are not listed unless `all` was asked for).
    pub updated: Vec<RecheckEntry>,
    /// Characters that still have missing content afterwards.
    pub outstanding: Vec<RecheckEntry>,
}

/// `export_character` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub file_name: String,
    pub mime: String,
    pub path: Option<String>,
    pub data_base64: Option<String>,
}
