//! The derived view of a character handed to the UI (all serde, camelCase-free snake_case).

use crate::diag::Diagnostic;
use crate::levels::PointBuy;
use crate::money::Coin;
use crate::state::{Choice, ClassSlot, Coins, EquipSlot, SlotKey};
use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A reference to a content element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRef {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Element type.
    pub ty: String,
    /// Source name.
    pub source: String,
}

/// One ability score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySnapshot {
    /// Lower-case ability name.
    pub name: String,
    /// Base score before bonuses.
    pub base: i32,
    /// Sum of bonuses (group `<ability>`).
    pub additional: i32,
    /// Set-score override (`<ability>:score:set`), 0 if none.
    pub override_score: i32,
    /// The cap applied.
    pub maximum: i32,
    /// Final score.
    pub score: i32,
    /// Modifier.
    pub modifier: i32,
    /// Saving throw proficiency bonus.
    pub save_proficiency: i32,
    /// Saving throw misc bonus.
    pub save_misc: i32,
    /// Saving throw total.
    pub save: i32,
    /// Proficient in the saving throw.
    pub save_proficient: bool,
}

/// One skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSnapshot {
    /// Lower-case skill name.
    pub name: String,
    /// Governing ability.
    pub ability: String,
    /// Proficiency contribution (0, once, or doubled).
    pub proficiency: i32,
    /// Misc bonus.
    pub misc: i32,
    /// Total bonus.
    pub total: i32,
    /// Proficient.
    pub proficient: bool,
    /// Expertise.
    pub expertise: bool,
    /// Not proficient, but half the proficiency bonus applies (Jack of All Trades,
    /// Remarkable Athlete).
    #[serde(default)]
    pub half_proficiency: bool,
    /// Passive score (all 18 are computed).
    pub passive: i32,
}

/// Speeds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speeds {
    /// Walking.
    pub walk: i32,
    /// Flying.
    pub fly: i32,
    /// Climbing.
    pub climb: i32,
    /// Swimming.
    pub swim: i32,
    /// Burrowing.
    pub burrow: i32,
}

/// A class progression of the character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSnapshot {
    /// Progression key.
    pub slot: ClassSlot,
    /// The class (or multiclass) element, if chosen.
    pub class: Option<ElementRef>,
    /// Levels in the class.
    pub level: u32,
    /// Hit die size.
    pub hit_die: Option<u32>,
    /// Hit points from the hit dice (no Constitution modifier).
    pub hit_points: i32,
    /// Archetype elements chosen under the class.
    pub archetypes: Vec<ElementRef>,
}

/// An option offered by a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionRef {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Source.
    pub source: String,
    /// Spell level for spells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Set for foreign-edition options: badge and adapter notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<crate::compat::EditionBadge>,
}

/// An open select slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSnapshot {
    /// Stable slot key.
    pub key: SlotKey,
    /// Owning element.
    pub owner: ElementRef,
    /// Rule name.
    pub name: String,
    /// Selected element type (`List` for text lists).
    pub ty: String,
    /// Rule level.
    pub level: u32,
    /// Number of slots in the rule.
    pub number: u32,
    /// May stay empty.
    pub optional: bool,
    /// Currently chosen element.
    pub chosen: Option<ElementRef>,
    /// Chosen entry of a `List` rule.
    pub list_choice: Option<i64>,
    /// Entries of a `List` rule.
    pub list_items: Vec<(i64, String)>,
    /// Required and still empty (a slot whose every option is already known is done, not open).
    pub requires_selection: bool,
    /// Empty, but every element the slot would offer is already known (Hobgoblin's martial
    /// weapons on a fighter): the choice counts as made.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_known: bool,
    /// The user cannot change or clear the choice: the rule has a `default`, the list has exactly
    /// one option and the slot holds a choice (engine-optional-sole-option SO-3). A sole option
    /// without a default is never locked nor chosen automatically.
    pub locked: bool,
    /// Rule default element, if any.
    pub default: Option<String>,
    /// A stored choice that is currently not allowed.
    pub rejected: Option<Choice>,
    /// A stored choice naming an element that is not installed: the slot is open (offers its
    /// options, `chosen` is empty) so a replacement can be picked; the stored choice is kept
    /// until then (it returns when the content is installed again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
    /// Spellcasting class of a spell slot.
    pub spellcasting: Option<String>,
    /// Always-prepared spell slot.
    pub prepared: bool,
    /// Options (empty when disabled in the settings).
    pub options: Vec<OptionRef>,
    /// The filled spell pick may be replaced now (level-up swap rule).
    #[serde(default)]
    pub replaceable: bool,
    /// Level the slot was gained at: the rule level, or the class level it reopened at.
    #[serde(default)]
    pub gained_level: u32,
    /// Picks replaced in this slot, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced: Vec<ReplacedRef>,
}

/// A pick that was replaced in a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacedRef {
    /// The replaced element (name falls back to the id when the content is gone).
    pub pick: ElementRef,
    /// Class level at which it was replaced.
    pub level: u32,
}

/// A registered element instance, for the feature tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredElement {
    /// The element.
    pub element: ElementRef,
    /// Instance index.
    pub index: usize,
    /// Parent instance index.
    pub parent: Option<usize>,
    /// How it was acquired: `root`, `granted` or `selected`.
    pub acquired: String,
}

/// A feature, trait or feat the character has, with its text (sheet and AI display).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDetail {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Element type (`Class Feature`, `Racial Trait`, `Feat`, ...).
    pub ty: String,
    /// Content source name.
    pub source: String,
    /// Level the granting rule activates it at (1 when unrestricted).
    pub level: u32,
    /// Name of the element that granted or offered it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    /// Sanitized HTML description from the content.
    pub description: String,
    /// Sheet text for the current level, placeholders resolved from the statistics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_summary: Option<String>,
    /// Shown in the sheet's feature list (`sheet display`, default true).
    pub displayed: bool,
}

/// A statistic group with its sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatSnapshot {
    /// Value.
    pub value: i32,
    /// Contributions `(label, value)`.
    pub sources: Vec<(String, i32)>,
}

/// A spell the character has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellEntry {
    /// Spell id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Level (0 = cantrip).
    pub level: u32,
    /// Prepared (always-prepared spells included).
    pub prepared: bool,
    /// Always prepared (does not count against the limit).
    pub always_prepared: bool,
    /// Learned through a select or grant (as opposed to merely available).
    pub known: bool,
    /// Cantrips only: damage at the character's level (`2d10 Fire`), from the spell text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<String>,
    /// The element that granted the spell (none for spells prepared from a class list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<SpellGrant>,
}

/// The granting element of a spell (feeds the spell card footer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellGrant {
    /// Granting element name.
    pub name: String,
    /// Granting element type (`Racial Trait`, `Feat`, ...).
    pub ty: String,
    /// Name of the element that granted the granting element, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    /// Class progression that owns the granting element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

/// One spellcasting class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellcastingSnapshot {
    /// Spellcasting name (class).
    pub name: String,
    /// Ability name.
    pub ability: String,
    /// Prepares spells.
    pub prepare: bool,
    /// May swap spells on level-up.
    pub allow_replace: bool,
    /// Spell save DC.
    pub dc: i32,
    /// Spell attack bonus (proficiency + ability + misc, counted once).
    pub attack: i32,
    /// Slots by spell level 1..=9 (class table). Never holds Pact Magic slots.
    pub slots: [i32; 9],
    /// Pact Magic: (slot count, slot level). A separate pool: every slot is of that level, they
    /// recover on a short or long rest, and they never join the multiclass slot pool.
    pub pact: Option<(i32, i32)>,
    /// Prepared limit (clamped to at least 1), when the class prepares.
    pub prepare_limit: Option<i32>,
    /// Number of prepared spells that count against the limit.
    pub prepared_count: u32,
    /// Spells known / granted / preparable.
    pub spells: Vec<SpellEntry>,
    /// The whole class list may be prepared.
    pub prepare_from_list: bool,
    /// Where prepared spells come from: `class_list`, `spellbook` or empty (known spells only).
    #[serde(default)]
    pub prepare_source: String,
    /// The wizard-style spellbook (2024 `spellbook` rule).
    #[serde(default)]
    pub spellbook: Option<SpellbookSnapshot>,
    /// Mystic Arcanum picks by spell level (2024 `mystic_arcanum` rule).
    #[serde(default)]
    pub arcanum: Vec<ArcanumSnapshot>,
    /// How the prepared list is edited (`when`, `count`), from `prepared_spell_change`.
    #[serde(default)]
    pub prepared_change: Option<(String, String)>,
    /// Ritual casting without preparing, from this source (`ritual_casting`).
    #[serde(default)]
    pub ritual_source: Option<String>,
    /// Extra class lists prepared spells may come from (`spell_list_extend`).
    #[serde(default)]
    pub extended_lists: Vec<String>,
}

/// A spellbook (spells copied into it, capacity from the class level).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpellbookSnapshot {
    /// Spells the book can hold at this class level.
    pub capacity: u32,
    /// Spells in the book.
    pub spells: Vec<SpellEntry>,
    /// Class-list spells that could be copied in (level 1+, castable level, not yet in the book).
    pub candidates: Vec<SpellEntry>,
    /// Copying cost in gp per spell level.
    pub copy_cost_gp_per_level: u32,
    /// Copying time in hours per spell level.
    pub copy_hours_per_level: u32,
}

/// One Mystic Arcanum entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArcanumSnapshot {
    /// Spell level of the arcanum.
    pub spell_level: u32,
    /// Class level at which it is gained.
    pub class_level: u32,
    /// Gained at the current class level.
    pub unlocked: bool,
    /// Chosen spell id.
    pub spell: Option<String>,
    /// One free cast per `recharge` (`long_rest`).
    pub recharge: String,
}

/// A spell that can be cast without a slot a limited number of times (`grant_spell.free_cast`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeCastSnapshot {
    /// Spell id.
    pub spell: String,
    /// Spell name.
    pub name: String,
    /// Uses per recharge; `None` when it comes from a table the engine does not resolve.
    pub uses: Option<i32>,
    /// `1`, `proficiency` or `table:<key>` as authored.
    pub uses_rule: String,
    /// `long_rest`, `short_rest`, ...
    pub recharge: String,
    /// The spell can still be cast with slots as well.
    pub also_with_slots: bool,
    /// Slot level the free casts are cast at, when above the spell's own (`cast_level`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_level: Option<u32>,
    /// Element that grants it.
    pub source: String,
}

/// A named save DC (`save_dc`), evaluated when the formula is understood.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSaveDc {
    /// Owning element id.
    pub owner: String,
    /// DC id (for example `channel divinity`).
    pub id: String,
    /// Formula as authored.
    pub formula: String,
    /// Value, or `None` when the formula is not understood.
    pub value: Option<i32>,
}

/// Pooled multiclass spell slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MulticlassSlots {
    /// Combined caster level.
    pub caster_level: u32,
    /// Slots by spell level 1..=9.
    pub slots: [i32; 9],
}

/// An inventory line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemSnapshot {
    /// Item uid.
    pub uid: String,
    /// Item element.
    pub item: ElementRef,
    /// Display name.
    pub name: String,
    /// Amount.
    pub amount: u32,
    /// Equipped slot.
    pub equipped: Option<EquipSlot>,
    /// Effective attunement.
    pub attuned: bool,
    /// Needs attunement.
    pub requires_attunement: bool,
    /// Printed attunement restriction ("by a cleric, druid, or warlock").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attunement_restriction: Option<String>,
    /// The character does not meet the (enforced) attunement restriction: equipping does not
    /// attune it and attuning is refused.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attunement_barred: bool,
    /// The item makes the character proficient with it (setter `proficient`, see
    /// [`crate::item_prof`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proficient_by_item: bool,
    /// The item can be equipped ([`crate::inventory::ItemFacts::equippable`]); one that cannot is
    /// used, and attuned, while carried.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub equippable: bool,
    /// Weight of the line in pounds.
    pub weight: f64,
    /// Price of one piece in gold pieces.
    pub cost_gp: f64,
    /// Stored in a container.
    pub storage: Option<String>,
    /// Effective "show card" flag (the line's flag, forced off when the line is hidden).
    #[serde(default = "default_true")]
    pub show_card: bool,
    /// U12: "Override requirements" is on for this line.
    #[serde(default)]
    pub override_requirements: bool,
    /// Elements the equipped line grants that are withheld because their own requirements fail
    /// (what "Override requirements" would apply).
    #[serde(default)]
    pub withheld_grants: Vec<ElementRef>,
}

fn default_true() -> bool {
    true
}

/// Inventory summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventorySnapshot {
    /// Lines.
    pub items: Vec<ItemSnapshot>,
    /// Coins.
    pub coins: Coins,
    /// Total value in gold pieces.
    pub gold_value: f64,
    /// Carried weight in pounds (storage excluded).
    pub weight: f64,
    /// Carry capacity in pounds.
    pub capacity: f64,
    /// Attuned items (after the limit).
    pub attuned: u32,
    /// Attunement limit.
    pub attunement_limit: u32,
    /// Coin denominations (for display order).
    pub coin_order: Vec<Coin>,
}

/// An attack line derived from an equipped weapon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackSnapshot {
    /// Item uid.
    pub uid: String,
    /// Weapon name.
    pub name: String,
    /// Range text.
    pub range: String,
    /// Attack bonus.
    pub attack: i32,
    /// Damage text such as `1d8+3 slashing`.
    pub damage: String,
    /// Linked ability.
    pub ability: String,
    /// Proficient with the weapon.
    pub proficient: bool,
    /// The weapon's Mastery property when the character has mastered this kind of weapon
    /// (2024); `Some` with `mastered: false` is informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mastery: Option<MasteryRef>,
    /// Lowest d20 roll that is a critical hit (20 normally, 19 for Improved Critical).
    #[serde(default = "default_crit_range")]
    pub crit_range: u8,
    /// Damage dice below this value count as it (Great Weapon Fighting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub die_floor: Option<u8>,
    /// Damage of the extra attack of a Light weapon (2024; includes the modifier only with
    /// Two-Weapon Fighting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offhand_damage: Option<String>,
    /// Carried `+N` ammunition the weapon fires (its bonus is in `attack` and `damage`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ammunition: Option<String>,
    /// A cantrip that calls for a saving throw instead of an attack roll (`attack` is 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<AttackSave>,
}

/// The saving throw of a save-based attack row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackSave {
    /// Save DC.
    pub dc: i32,
    /// Ability of the save (`dexterity`).
    pub ability: String,
}

fn default_crit_range() -> u8 {
    20
}

/// A Mastery property shown on an attack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasteryRef {
    /// Property name such as `Graze`.
    pub name: String,
    /// Plain text of the property.
    pub text: String,
    /// The character chose this weapon kind for a Weapon Mastery slot.
    pub mastered: bool,
}

/// One limited-use pool (`resource` rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    /// Resource id (`rage`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Maximum uses now.
    pub max: i32,
    /// Remaining uses.
    pub current: i32,
    /// Name of the feature that declares it.
    pub source: String,
    /// Recharge triggers (`short_rest`, `long_rest`, `initiative`, ...).
    pub recharge: Vec<String>,
    /// Uses regained by a Short Rest: `Some(max)` = all, `None` = none.
    pub short_rest_regain: Option<i32>,
    /// A Long Rest restores everything.
    pub long_rest_regain_all: bool,
    /// Free text about other ways to regain uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_regain: Vec<String>,
    /// The maximum is rolled with dice once, when the item is gained ("1d6 + 1 charges").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled: Option<RolledMaxSnapshot>,
}

/// A dice-valued resource maximum ([`crate::rolled`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolledMaxSnapshot {
    /// The dice as the content prints them (`1d6+1`).
    pub dice: String,
    /// Inventory uid of the item that holds the roll (re-roll / edit target).
    pub item_uid: Option<String>,
    /// A roll is stored; `false` = none yet, `max` shows the average.
    pub rolled: bool,
    /// Lowest possible maximum.
    pub min: u32,
    /// Highest possible maximum.
    pub max: u32,
}

/// A switchable state such as Rage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToggleSnapshot {
    /// Toggle id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The user switched it on.
    pub on: bool,
    /// Its effects apply (on and not blocked).
    pub active: bool,
    /// Why an enabled toggle has no effect (worn armor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<String>,
    /// Activation action type.
    pub activation: String,
    /// Duration text.
    pub duration: String,
    /// Things that stop while it is on (`concentration`, `spellcasting`).
    pub blocks: Vec<String>,
    /// Human readable effects.
    pub effects: Vec<String>,
    /// Declaring feature.
    pub source: String,
}

/// Exhaustion as the profile models it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExhaustionSnapshot {
    /// Linear (2024) numbers are computed.
    pub linear: bool,
    /// Current level (0 to `max_levels`).
    pub level: u32,
    /// Highest level.
    pub max_levels: u32,
    /// Penalty to every D20 Test (negative or zero).
    pub d20_penalty: i32,
    /// Speed reduction in feet (negative or zero).
    pub speed_penalty: i32,
    /// The character is dead of exhaustion.
    pub dead: bool,
}

/// A way of earning Heroic Inspiration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspirationSource {
    /// Feature name.
    pub source: String,
    /// `long_rest` or `start_of_turn_in_combat`.
    pub trigger: String,
}

/// Monk Martial Arts as computed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MartialArtsSnapshot {
    /// Damage die such as `1d8`.
    pub die: String,
    /// The rule applies right now (no armor, no shield).
    pub active: bool,
    /// Bonus Action Unarmed Strike is available.
    pub bonus_unarmed_strike: bool,
}

/// A Weapon Mastery pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasteryPick {
    /// Weapon element id.
    pub weapon: String,
    /// Weapon name.
    pub name: String,
    /// Mastery property name.
    pub property: String,
    /// Mastery property text.
    pub text: String,
}

/// A cost in resource points of an option (Metamagic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCostSnapshot {
    /// Option name.
    pub option: String,
    /// Resource id.
    pub resource: String,
    /// Points.
    pub cost: i32,
}

/// A stored selection that no longer applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidSelection {
    /// The slot.
    pub slot: SlotKey,
    /// The stored choice.
    pub choice: Choice,
}

/// Profile-selected behaviour surfaced to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileInfo {
    /// Where ability score bonuses come from, in order.
    pub ability_score_sources: Vec<crate::profile::AbilityScoreSource>,
    /// An (origin) feat is taken at level 1.
    pub feat_at_level_1: bool,
    /// Half-caster rounding for pooled multiclass slots.
    pub half_caster_rounding: crate::profile::CasterRounding,
    /// Default subclass level (`None` = class decides).
    pub default_subclass_level: Option<u32>,
    /// The standard array (content tables over the profile default).
    #[serde(default)]
    pub standard_array: Vec<i32>,
    /// Point buy cost per base score from the content tables (empty = built-in table).
    #[serde(default)]
    pub point_buy_costs: BTreeMap<i32, i32>,
    /// Standard languages chosen at creation besides Common.
    #[serde(default)]
    pub languages_at_creation: u32,
}

/// Everything derived from a [`crate::CharacterState`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterSnapshot {
    /// Ruleset profile.
    pub ruleset: Ruleset,
    /// What the ruleset profile changes (seams for the 2024 ruleset).
    pub profile: ProfileInfo,
    /// Character level.
    pub level: u32,
    /// Experience points.
    pub experience: u32,
    /// XP needed for the next level.
    pub next_level_xp: Option<u32>,
    /// Experience crossed the next threshold.
    pub level_up_available: bool,
    /// Proficiency bonus.
    pub proficiency_bonus: i32,
    /// Abilities.
    pub abilities: Vec<AbilitySnapshot>,
    /// Skills.
    pub skills: Vec<SkillSnapshot>,
    /// Armor class.
    pub armor_class: i32,
    /// Initiative.
    pub initiative: i32,
    /// Constitution saving throw to keep concentration (the save plus `concentration:save`).
    #[serde(default)]
    pub concentration_save: i32,
    /// Speeds.
    pub speed: Speeds,
    /// Maximum hit points.
    pub hit_points: i32,
    /// Class progressions.
    pub classes: Vec<ClassSnapshot>,
    /// Chosen race / background / alignment / deity etc. by type.
    pub origin: BTreeMap<String, ElementRef>,
    /// Open select slots.
    pub slots: Vec<SlotSnapshot>,
    /// Registered element tree (pre-order).
    pub elements: Vec<RegisteredElement>,
    /// Statistic groups.
    pub stats: BTreeMap<String, StatSnapshot>,
    /// Spellcasting classes.
    pub spellcasting: Vec<SpellcastingSnapshot>,
    /// Pooled multiclass slots when two or more caster classes exist.
    pub multiclass_slots: Option<MulticlassSlots>,
    /// Limited free casts of granted spells (2024 `grant_spell.free_cast`).
    #[serde(default)]
    pub free_casts: Vec<FreeCastSnapshot>,
    /// Named save DCs (2024 `save_dc`).
    #[serde(default)]
    pub save_dcs: Vec<NamedSaveDc>,
    /// Limited-use pools (Rage, Focus Points, Channel Divinity, ...).
    #[serde(default)]
    pub resources: Vec<ResourceSnapshot>,
    /// Costs of options in resource points.
    #[serde(default)]
    pub resource_costs: Vec<ResourceCostSnapshot>,
    /// Switchable states.
    #[serde(default)]
    pub toggles: Vec<ToggleSnapshot>,
    /// Exhaustion.
    #[serde(default)]
    pub exhaustion: ExhaustionSnapshot,
    /// The character holds (Heroic) Inspiration.
    #[serde(default)]
    pub inspiration: bool,
    /// Features that grant Heroic Inspiration.
    #[serde(default)]
    pub inspiration_sources: Vec<InspirationSource>,
    /// Martial Arts, when the character has it.
    #[serde(default)]
    pub martial_arts: Option<MartialArtsSnapshot>,
    /// Chosen Weapon Mastery weapons.
    #[serde(default)]
    pub masteries: Vec<MasteryPick>,
    /// Inventory.
    pub inventory: InventorySnapshot,
    /// Attacks of equipped weapons.
    pub attacks: Vec<AttackSnapshot>,
    /// Point buy summary when that method is used.
    pub point_buy: Option<PointBuy>,
    /// A new multiclass level may be started.
    pub can_multiclass: bool,
    /// Wearing armor that imposes stealth disadvantage.
    pub stealth_disadvantage: bool,
    /// Advantage on initiative.
    pub initiative_advantage: bool,
    /// Resistances, immunities, vulnerabilities and condition immunities (2024 rules).
    #[serde(default)]
    pub defenses: crate::combat::Defenses,
    /// Senses with ranges.
    #[serde(default)]
    pub senses: Vec<crate::combat::SenseSnapshot>,
    /// Structured advantages (saves, checks, initiative, death saves).
    #[serde(default)]
    pub advantages: Vec<crate::combat::AdvantageEntry>,
    /// Surfaced rules (extra damage, Evasion, check floors, flight, ...).
    #[serde(default)]
    pub notes: Vec<crate::combat::RuleNote>,
    /// Carrying capacity with size steps (only when a feature grants steps).
    #[serde(default)]
    pub carry: Option<crate::combat::CarrySnapshot>,
    /// Wild Shape limits, learned forms and the Beasts that qualify.
    #[serde(default)]
    pub wild_shape: Option<crate::wild_shape::WildShapeSnapshot>,
    /// Starting-equipment packages offered by the class and background.
    #[serde(default)]
    pub equipment_packages: Vec<crate::packages::EquipmentPackageSnapshot>,
    /// Selections that no longer apply.
    pub invalid_selections: Vec<InvalidSelection>,
    /// Diagnostics (content problems, legacy divergences, warnings).
    pub diagnostics: Vec<Diagnostic>,
    /// Foreign-edition content in use and how it was adapted.
    #[serde(default)]
    pub compatibility: crate::compat::CompatibilityReport,
    /// Features, traits and feats with descriptions, in registration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<FeatureDetail>,
    /// The displayed features' sheet text, one `Name: text` line each.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sheet_summary: String,
    /// Stat provenance per statistic (only when [`crate::EvaluateOptions::provenance`] is set).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provenance: BTreeMap<crate::provenance::StatName, crate::provenance::StatTrace>,
}

impl CharacterSnapshot {
    /// Find an ability by name.
    #[must_use]
    pub fn ability(&self, name: &str) -> Option<&AbilitySnapshot> {
        self.abilities.iter().find(|a| a.name == name)
    }

    /// Find a skill by name.
    #[must_use]
    pub fn skill(&self, name: &str) -> Option<&SkillSnapshot> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// Statistic value (0 if absent).
    #[must_use]
    pub fn stat(&self, name: &str) -> i32 {
        self.stats.get(name).map_or(0, |s| s.value)
    }

    /// Spellcasting block by name.
    #[must_use]
    pub fn caster(&self, name: &str) -> Option<&SpellcastingSnapshot> {
        self.spellcasting.iter().find(|s| s.name == name)
    }

    /// Does the character have a registered element?
    #[must_use]
    pub fn has_element(&self, id: &str) -> bool {
        self.elements.iter().any(|e| e.element.id == id)
    }

    /// Open slot lookup.
    #[must_use]
    pub fn slot(&self, owner: &str, name: &str) -> Option<&SlotSnapshot> {
        self.slots
            .iter()
            .find(|s| s.owner.id == owner && s.name == name)
    }
}
