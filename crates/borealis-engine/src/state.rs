//! The persisted inputs of a character. Everything else is derived by the engine.

use crate::settings::HouseRules;
use borealis_content::{ElementId, Ruleset};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Which class progression owns a level (stable choice keys use the starting level of a multiclass).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClassSlot {
    /// The main class (also levels before a class is chosen).
    Main,
    /// A multiclass identified by the character level at which it was started.
    Multi {
        /// Character level of the multiclass's first level.
        starting_level: u32,
    },
}

/// Identity of a select slot: (owner element id, rule path index, ordinal).
///
/// `rule` indexes the owner's rule list (for generated Multiclass elements: the combined list
/// of copied class rules followed by the multiclass node rules). `instance` disambiguates two
/// copies of the same inventory item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SlotKey {
    /// Element id of the element that declares the select rule.
    pub owner: String,
    /// Index of the select rule in the owner's rule list.
    pub rule: u32,
    /// 1-based position among the rule's `number` slots.
    pub ordinal: u32,
    /// Inventory item uid when the owner is an inventory item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl SlotKey {
    /// A key for a content element owner.
    #[must_use]
    pub fn new(owner: impl Into<String>, rule: u32, ordinal: u32) -> Self {
        Self {
            owner: owner.into(),
            rule,
            ordinal,
            instance: None,
        }
    }
}

/// What was chosen in a slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// An element id.
    Element(String),
    /// The integer id of a `List` select entry.
    ListItem(i64),
}

/// One stored choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// The slot.
    pub slot: SlotKey,
    /// The choice.
    pub choice: Choice,
    /// Character level at which a spell was swapped (retrain marker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_level: Option<u32>,
}

/// A pick that was swapped out of a selection slot (spell retraining).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacedPick {
    /// Element id of the replaced pick.
    pub id: String,
    /// Class level at which it was replaced (the slot reopened at this level).
    pub level: u32,
}

/// History of one selection slot: the picks replaced in it, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotHistory {
    /// The slot.
    pub slot: SlotKey,
    /// Replaced picks, oldest first.
    #[serde(default)]
    pub replaced: Vec<ReplacedPick>,
}

/// Ability score generation mode (the engine only sees the resulting base scores).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityMethod {
    /// 3d6 per ability.
    Roll3d6,
    /// 4d6 drop lowest (the default).
    #[default]
    Roll4d6,
    /// The standard array assigned per ability.
    StandardArray,
    /// Point buy with a budget.
    PointBuy,
    /// Free entry.
    Manual,
}

/// Base ability scores and how they were generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityInputs {
    /// Generation method.
    pub method: AbilityMethod,
    /// Base scores in STR, DEX, CON, INT, WIS, CHA order (before bonuses).
    pub base: [i32; 6],
}

impl Default for AbilityInputs {
    fn default() -> Self {
        Self {
            method: AbilityMethod::default(),
            base: [10; 6],
        }
    }
}

/// Persisted hit point entries of a class (index = class level - 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HpRolls {
    /// The class progression.
    pub class: ClassSlot,
    /// Entries (at most 20).
    pub rolls: Vec<u32>,
}

/// Prepared spell ids of a spellcasting class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedSpells {
    /// Spellcasting name (class name).
    pub class: String,
    /// Prepared spell ids.
    pub spells: Vec<String>,
}

/// Inventory slot an item is equipped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipSlot {
    /// Body armor.
    Armor,
    /// Primary hand.
    PrimaryHand,
    /// Secondary hand (shield / off-hand).
    SecondaryHand,
    /// A worn slot (ring, cloak, ...) with no hand logic.
    Worn,
}

/// One inventory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    /// Per-instance identifier.
    pub uid: String,
    /// Item element id.
    pub item: String,
    /// Count (stackable items).
    #[serde(default = "one")]
    pub amount: u32,
    /// Optional adorner (magic item layered on the base item).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adorner: Option<String>,
    /// Slot the item is equipped in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped: Option<EquipSlot>,
    /// A versatile weapon held in both hands.
    #[serde(default)]
    pub versatile: bool,
    /// Attuned.
    #[serde(default)]
    pub attuned: bool,
    /// Storage container name when stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Name of the element that granted the item (starting equipment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_from: Option<String>,
    /// Hidden from the inventory table.
    #[serde(default)]
    pub hidden: bool,
    /// Description printed in the equipment sidebar.
    #[serde(default)]
    pub sidebar: bool,
    /// Print an item card ("show card"; on by default for a new line).
    #[serde(default = "yes")]
    pub card: bool,
    /// Alternative display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_name: Option<String>,
    /// Free notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Rolled (or entered) maximum of each dice-valued resource of the item, by resource id
    /// ("1d6 + 1 charges"): rolled once when the item is gained, see [`crate::rolled`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rolled_max: BTreeMap<String, u32>,
    /// U12: the item's grants apply although the granted elements' own requirements fail (an
    /// epic boon added through Additional Feat before level 19). Off, as in Aurora, by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub override_requirements: bool,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

impl InventoryItem {
    /// A fresh unequipped item.
    #[must_use]
    pub fn new(uid: impl Into<String>, item: impl Into<String>) -> Self {
        Self {
            uid: uid.into(),
            item: item.into(),
            amount: 1,
            adorner: None,
            equipped: None,
            versatile: false,
            attuned: false,
            storage: None,
            acquired_from: None,
            hidden: false,
            sidebar: false,
            card: true,
            alt_name: None,
            notes: None,
            rolled_max: BTreeMap::new(),
            override_requirements: false,
        }
    }
}

/// Coins by denomination.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coins {
    /// Copper.
    pub cp: u64,
    /// Silver.
    pub sp: u64,
    /// Electrum.
    pub ep: u64,
    /// Gold.
    pub gp: u64,
    /// Platinum.
    pub pp: u64,
}

/// Inventory and money.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    /// Items in inventory order.
    pub items: Vec<InventoryItem>,
    /// Storage container labels (at most two in the legacy format).
    #[serde(default)]
    pub storages: Vec<String>,
    /// Coins.
    #[serde(default)]
    pub coins: Coins,
    /// Inventory uids of weapons whose attack line stays in the attacks table while the weapon
    /// is not wielded. Aurora's attacks section is a list the user fills ("Add to Attacks"); it
    /// recomputes and prints every listed weapon, equipped or not. The `.dnd5e` import keeps
    /// that list here (B-10).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub listed_attacks: Vec<String>,
}

/// Mutable play-time state: resource uses, toggles, exhaustion, Heroic Inspiration, spell slot
/// uses, concentration and custom trackers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayState {
    /// Expended uses per resource id (zero entries are not stored).
    #[serde(default)]
    pub resources_used: BTreeMap<String, u32>,
    /// Switched-on state ids (Rage).
    #[serde(default)]
    pub toggles: BTreeSet<String>,
    /// Exhaustion level.
    #[serde(default)]
    pub exhaustion: u32,
    /// The character holds (Heroic) Inspiration.
    #[serde(default)]
    pub heroic_inspiration: bool,
    /// Wild Shape: ids of the Beast stat blocks the character has learned.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub wild_shape_forms: BTreeSet<String>,
    /// Starting-equipment choices: package element id to the chosen option label.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub equipment_choices: BTreeMap<String, String>,
    /// Expended spell slots per spell level 1..=9 (zero entries are not stored; Pact Magic slots
    /// are counted in `pact_slots_used`). Files written before the tracker have none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spell_slots_used: BTreeMap<u32, u32>,
    /// Expended Pact Magic slots.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pact_slots_used: u32,
    /// Id of the spell the character is concentrating on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concentration: Option<String>,
    /// The user's own trackers (Play tab), in the order they were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trackers: Vec<CustomTracker>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// When a custom tracker gets its uses back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackerReset {
    /// On a short or a long rest.
    ShortRest,
    /// On a long rest.
    LongRest,
    /// Never: the user counts it down and up by hand.
    #[default]
    Never,
}

/// A counter the user adds on the Play tab (Drakkenheim contamination, Splicer points).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTracker {
    /// Stable id within the character (`tracker-<n>`).
    pub id: String,
    /// Name the user gave it.
    pub name: String,
    /// Maximum (at least 1).
    pub max: u32,
    /// Marked (used) pips, at most `max`.
    #[serde(default)]
    pub used: u32,
    /// When it resets.
    #[serde(default)]
    pub reset: TrackerReset,
}

impl PlayState {
    /// True when nothing is recorded (skipped on save).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Everything the engine needs to derive a character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterState {
    /// Ruleset profile.
    pub ruleset: Ruleset,
    /// Registered Option element ids.
    #[serde(default)]
    pub options: Vec<String>,
    /// Class progression of each character level (index 0 = level 1; always `Main`).
    pub levels: Vec<ClassSlot>,
    /// Experience points.
    #[serde(default)]
    pub experience: u32,
    /// Ability scores.
    #[serde(default)]
    pub abilities: AbilityInputs,
    /// Stored choices.
    #[serde(default)]
    pub selections: Vec<Selection>,
    /// Persisted hit point entries.
    #[serde(default)]
    pub hp_rolls: Vec<HpRolls>,
    /// Prepared spells.
    #[serde(default)]
    pub prepared: Vec<PreparedSpells>,
    /// Inventory.
    #[serde(default)]
    pub inventory: Inventory,
    /// Names of restricted (unchecked) content sources.
    #[serde(default)]
    pub restricted_sources: Vec<String>,
    /// House rule overrides.
    #[serde(default)]
    pub house_rules: HouseRules,
    /// Allow foreign-edition content in new picks (`None` = the profile default: strict for 2014,
    /// open for 2024). Strict mode never unregisters stored choices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_cross_edition: Option<bool>,
    /// Spellbook contents per class name (Wizard).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spellbooks: BTreeMap<String, Vec<ElementId>>,
    /// Play-time state.
    #[serde(default, skip_serializing_if = "PlayState::is_empty")]
    pub play: PlayState,
    /// Per-slot replacement history (absent in files written before it existed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slot_history: Vec<SlotHistory>,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            ruleset: crate::profile::default_ruleset(),
            options: Vec::new(),
            levels: vec![ClassSlot::Main],
            experience: 0,
            abilities: AbilityInputs::default(),
            selections: Vec::new(),
            hp_rolls: Vec::new(),
            prepared: Vec::new(),
            inventory: Inventory::default(),
            restricted_sources: Vec::new(),
            house_rules: HouseRules::default(),
            allow_cross_edition: None,
            spellbooks: BTreeMap::new(),
            play: PlayState::default(),
            slot_history: Vec::new(),
        }
    }
}

impl CharacterState {
    /// Picks replaced in a slot, oldest first (empty when never replaced).
    #[must_use]
    pub fn replaced_in(&self, key: &SlotKey) -> &[ReplacedPick] {
        self.slot_history
            .iter()
            .find(|h| &h.slot == key)
            .map_or(&[], |h| h.replaced.as_slice())
    }

    /// Total character level.
    #[must_use]
    pub fn level(&self) -> u32 {
        u32::try_from(self.levels.len()).unwrap_or(u32::MAX)
    }

    /// Levels owned by a class progression.
    #[must_use]
    pub fn class_level(&self, slot: &ClassSlot) -> u32 {
        u32::try_from(self.levels.iter().filter(|l| *l == slot).count()).unwrap_or(u32::MAX)
    }

    /// The stored choice for a slot.
    #[must_use]
    pub fn selection(&self, slot: &SlotKey) -> Option<&Selection> {
        self.selections.iter().find(|s| &s.slot == slot)
    }

    /// True if an Option element is registered.
    #[must_use]
    pub fn has_option(&self, id: &str) -> bool {
        self.options.iter().any(|o| o == id)
    }

    /// The spellbook spell ids of a class.
    #[must_use]
    pub fn spellbook_for(&self, class: &str) -> &[ElementId] {
        self.spellbooks.get(class).map_or(&[], Vec::as_slice)
    }

    /// The prepared spell ids of a class.
    #[must_use]
    pub fn prepared_for(&self, class: &str) -> &[String] {
        self.prepared
            .iter()
            .find(|p| p.class == class)
            .map_or(&[], |p| p.spells.as_slice())
    }
}
