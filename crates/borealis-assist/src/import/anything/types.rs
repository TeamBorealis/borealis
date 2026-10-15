//! Serializable stage results of the "import anything" pipeline. Every stage returns plain data
//! so the UI can show progress, persist a draft and resume it.

use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::balance::BalanceReport;

/// Rules edition of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edition {
    /// 2014 rules.
    #[serde(rename = "2014")]
    Y2014,
    /// 2024 rules.
    #[serde(rename = "2024")]
    Y2024,
}

impl Edition {
    /// The engine ruleset.
    pub fn ruleset(self) -> Ruleset {
        match self {
            Self::Y2014 => Ruleset::Dnd2014,
            Self::Y2024 => Ruleset::Dnd2024,
        }
    }

    /// From an engine ruleset (`Any` counts as 2014).
    pub fn from_ruleset(r: Ruleset) -> Self {
        match r {
            Ruleset::Dnd2024 => Self::Y2024,
            _ => Self::Y2014,
        }
    }
}

/// What a piece of source describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// A whole class.
    Class,
    /// A subclass of an existing class.
    Subclass,
    /// A species (race).
    Species,
    /// A background.
    Background,
    /// A feat.
    Feat,
    /// A spell.
    Spell,
    /// An item.
    Item,
    /// A monster stat block.
    Monster,
    /// A weapon property the source defines (imported as a `Weapon Property` with its text).
    WeaponProperty,
    /// A subrace or lineage of a species (offered by its species' choice).
    Subrace,
    /// Rules or lore text no element kind mechanises (kept as a text-only element).
    Rules,
    /// A condition the source defines (kept as a text-only `Condition` element).
    Condition,
    /// Several kinds in one source (only valid for the document as a whole).
    Mixed,
}

impl ImportKind {
    /// Content element type of a standalone element of this kind.
    pub fn element_type(self) -> &'static str {
        match self {
            Self::Class => "Class",
            Self::Subclass => "Archetype",
            Self::Species => "Race",
            Self::Background => "Background",
            Self::Feat => "Feat",
            Self::Spell => "Spell",
            Self::Item => "Magic Item",
            Self::Monster => "Monster",
            Self::WeaponProperty => "Weapon Property",
            Self::Subrace => "Sub Race",
            Self::Rules => "Rules Glossary Entry",
            Self::Condition => "Condition",
            Self::Mixed => "Unknown",
        }
    }

    /// Element type of a standalone element of this kind, items by their [`ItemType`].
    pub fn element_type_for(self, item_type: Option<ItemType>) -> &'static str {
        match (self, item_type) {
            (Self::Item, Some(t)) => t.element_type(),
            _ => self.element_type(),
        }
    }
}

/// What kind of item an [`ImportKind::Item`] is (the bundled packs model each differently).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    /// A mundane weapon (firearms and other ranged weapons included): an attack.
    Weapon,
    /// Mundane armor or a shield.
    Armor,
    /// Ammunition.
    Ammunition,
    /// Other mundane equipment.
    Gear,
    /// A magic item.
    MagicItem,
}

impl ItemType {
    /// Content element type, as in the bundled SRD packs.
    pub fn element_type(self) -> &'static str {
        match self {
            Self::Weapon => "Weapon",
            Self::Armor => "Armor",
            Self::Ammunition | Self::Gear => "Item",
            Self::MagicItem => "Magic Item",
        }
    }
}

/// One thing found in the source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedItem {
    /// Kind.
    pub kind: ImportKind,
    /// Name found in the source.
    pub name: String,
    /// For a subclass: the parent class as named or hinted in the source.
    #[serde(default)]
    pub parent_hint: Option<String>,
    /// For an item: weapon, armor, ammunition, gear or magic item (magic item when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<ItemType>,
}

/// Something the source defines that no element kind covers (a rule, a weapon property, a
/// condition, a table): it is reported, not imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtherItem {
    /// Name as printed.
    pub name: String,
    /// What it is ("weapon property", "optional rule", ...).
    #[serde(default)]
    pub what: String,
    /// Model note.
    #[serde(default)]
    pub note: String,
}

/// Stage 1 result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    /// Overall kind (`Mixed` when `items` has several kinds).
    pub kind: ImportKind,
    /// Edition.
    pub edition: Edition,
    /// Model confidence 0..1.
    pub confidence: f64,
    /// Everything found.
    pub items: Vec<DetectedItem>,
    /// Model notes.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Source objects that are none of the import kinds (reported, not imported).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other: Vec<OtherItem>,
}

/// Role of an element inside its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The `Class` element.
    Class,
    /// A class feature at a level.
    ClassFeature,
    /// The class feature that offers the subclass choice (built without the model).
    SubclassGate,
    /// The `Archetype` element.
    Subclass,
    /// A subclass feature at a level.
    SubclassFeature,
    /// A single element (species, feat, spell, ...).
    Standalone,
    /// A named part its `parent` element grants (a species trait, a background feature, a
    /// sub-feature of a feature).
    Part,
    /// One option of a pick-N list its `parent` element offers through a select.
    Option,
    /// Rules or lore text kept as a text-only element (built from the source passage).
    Text,
}

/// Where in the source an element came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceExcerpt {
    /// 1-based first line.
    pub start_line: usize,
    /// 1-based last line.
    pub end_line: usize,
    /// First characters of the passage.
    pub snippet: String,
}

/// One element the plan wants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedElement {
    /// Final deterministic id.
    pub id: String,
    /// Element type.
    pub ty: String,
    /// Name.
    pub name: String,
    /// Role.
    pub role: Role,
    /// Class or subclass level (features).
    pub level: Option<u32>,
    /// Plan id of the owning class or subclass.
    pub owner: Option<String>,
    /// What the source says, in the model's words.
    pub brief: String,
    /// Source passage.
    pub excerpt: Option<SourceExcerpt>,
    /// Plan id of the element that grants ([`Role::Part`]) or offers ([`Role::Option`]) it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The pick-N list this element offers (its options are [`Role::Option`] children).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picks: Option<OptionPicks>,
    /// An option's prerequisite level (above the level the list is first offered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerequisite_level: Option<u32>,
    /// Spells the feature grants by class level (a subclass or feature spell table).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spells: Vec<SpellGrant>,
}

/// A spell a planned feature grants at a class level, resolved against the import and the
/// library when the plan is built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellGrant {
    /// Class level the spell is gained at.
    pub level: u32,
    /// Spell name as printed.
    pub name: String,
    /// Element id of the spell (`None` when neither the import nor the library has it).
    #[serde(default)]
    pub id: Option<String>,
    /// The class whose spell it counts as (`spellcasting` / `spell_list` of the grant).
    pub class: String,
}

/// A pick-N list: the select(s) an element carries and the `supports` key its options share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionPicks {
    /// The `supports` key of every option (and of the selects).
    pub key: String,
    /// Element type of the options.
    pub ty: String,
    /// Picks gained, by level.
    pub choose: Vec<PickCount>,
    /// A spell select's `spellcasting` name (options that are existing spells).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<String>,
}

/// `count` picks gained at `level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickCount {
    /// Level the picks are gained at (class level for class features, 1 otherwise).
    #[serde(default)]
    pub level: u32,
    /// Number of picks.
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

/// A named sub-item of a feature or trait in an outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlinePart {
    /// Name as printed.
    pub name: String,
    /// What it does.
    #[serde(default)]
    pub summary: String,
    /// A short verbatim quote from the source.
    #[serde(default)]
    pub quote: String,
    /// For an option: its prerequisite level, if any.
    #[serde(default)]
    pub level: Option<u32>,
}

/// A feature in an outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineFeature {
    /// Class (or subclass) level (1 or unset for species and background traits).
    #[serde(default)]
    pub level: u32,
    /// Feature name.
    pub name: String,
    /// What it does.
    #[serde(default)]
    pub summary: String,
    /// A short verbatim quote from the source (used to locate the passage).
    #[serde(default)]
    pub quote: String,
    /// Named sub-features it grants (each its own element).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<OutlinePart>,
    /// Named options to pick from (each its own element, offered by a select).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OutlinePart>,
    /// Picks of `options` gained by level (one pick at the feature's level when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choose: Vec<PickCount>,
    /// Spells the feature grants, by class level (a spell table such as subclass spells).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spells: Vec<LeveledSpells>,
}

/// The spells a feature grants at one class level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeveledSpells {
    /// Class level.
    #[serde(default)]
    pub level: u32,
    /// Spell names as printed.
    #[serde(default)]
    pub spells: Vec<String>,
}

/// A column of a class table other than the level, proficiency bonus, features and spell
/// slots (a points pool, uses, a die size, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    /// Column heading as printed.
    pub name: String,
    /// One value per level 1-20, as printed.
    #[serde(default)]
    pub values: Vec<Value>,
}

/// Spellcasting as the source describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpellcastingOutline {
    /// Casting ability (`intelligence`, ...).
    pub ability: String,
    /// `full`, `half`, `third`, `pact` or `none`.
    #[serde(alias = "kind")]
    pub progression: String,
    /// Spell list name, e.g. `Wizard`.
    #[serde(default, alias = "spell_list_name")]
    pub list: Option<String>,
    /// The class prepares spells.
    #[serde(default)]
    pub prepared: Option<bool>,
    /// The class knows a fixed number of spells.
    #[serde(default)]
    pub known: Option<bool>,
    /// The class can cast rituals.
    #[serde(default)]
    pub ritual: Option<bool>,
    /// Cantrips known at levels 1-20 (cumulative, 20 entries).
    #[serde(default)]
    pub cantrips_by_level: Option<Vec<u32>>,
    /// Spells known (or prepared) at levels 1-20 (cumulative, 20 entries).
    #[serde(default)]
    pub spells_known_by_level: Option<Vec<u32>>,
    /// 2014 prepared-count formula, e.g. `mod + level`, `mod + half`.
    #[serde(default)]
    pub prepared_formula: Option<String>,
    /// Names of the spells on the class's spell list as the source prints it (existing spells
    /// get the list by an append; see `derived.rs`).
    #[serde(default, alias = "spell_list", skip_serializing_if = "Option::is_none")]
    pub spells: Option<Vec<String>>,
}

impl Outline {
    /// The caster profile, unless the class does not cast.
    pub fn caster(&self) -> Option<&SpellcastingOutline> {
        self.spellcasting
            .as_ref()
            .filter(|s| super::caster::caster_kind(s) != super::caster::CasterKind::None)
    }
}

/// What the model found in a class or subclass text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    /// Name.
    pub name: String,
    /// One-line summary.
    #[serde(default)]
    pub summary: String,
    /// Class hit die.
    #[serde(default)]
    pub hit_die: Option<u32>,
    /// Level of the subclass choice (class).
    #[serde(default)]
    pub subclass_level: Option<u32>,
    /// Spellcasting, if any.
    #[serde(default)]
    pub spellcasting: Option<SpellcastingOutline>,
    /// Features by level.
    pub features: Vec<OutlineFeature>,
    /// Quote locating the class/subclass header.
    #[serde(default)]
    pub quote: String,
    /// The class table's other columns (class outlines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<TableColumn>,
}

/// The parent class of a subclass (for a subrace: the parent species, `supports` = the key of
/// the species' subrace choice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentLink {
    /// Class element id.
    pub class_id: String,
    /// Class name.
    pub class_name: String,
    /// The `supports` key subclasses use, e.g. `Fighter Subclass`.
    pub supports: String,
    /// Levels where the parent's subclasses gain features.
    pub subclass_levels: Vec<u32>,
    /// The parent is a class imported from the same source (its id and levels come from the
    /// plan, not the library).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_source: bool,
}

/// A parent class candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentCandidate {
    /// The candidate link.
    pub link: ParentLink,
    /// Match score (higher is better).
    pub score: u32,
}

/// The user must pick a parent class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsChoice {
    /// Group (subclass name) the choice is for.
    pub group: String,
    /// Why.
    pub reason: String,
    /// Candidates, best first.
    pub candidates: Vec<ParentCandidate>,
}

/// One class or subclass (or standalone) the plan builds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanGroup {
    /// What it is.
    pub kind: ImportKind,
    /// Name.
    pub name: String,
    /// Id of the group's root element (class or subclass) once planned.
    #[serde(default)]
    pub root_id: Option<String>,
    /// Outline (classes and subclasses).
    pub outline: Option<Outline>,
    /// Resolved parent (subclasses).
    pub parent: Option<ParentLink>,
    /// Open parent choice (subclasses).
    pub choice: Option<NeedsChoice>,
    /// Item type (items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<ItemType>,
}

/// Stage 2 result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPlan {
    /// Edition.
    pub edition: Edition,
    /// Groups.
    pub groups: Vec<PlanGroup>,
    /// Elements to generate (empty for groups with an open choice).
    pub elements: Vec<PlannedElement>,
    /// Plan notes (levels snapped, levels filled).
    pub notes: Vec<String>,
}

impl ImportPlan {
    /// Open parent choices.
    pub fn open_choices(&self) -> Vec<&NeedsChoice> {
        self.groups
            .iter()
            .filter_map(|g| g.choice.as_ref())
            .collect()
    }
}

/// State of one draft element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementStatus {
    /// Not generated yet (resume point).
    Pending,
    /// Valid on the first try.
    Ok,
    /// Valid after the repair loop.
    Repaired,
    /// Valid, but the user should look (see `issues`).
    NeedsReview,
    /// No usable element.
    Failed,
}

/// One element of the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftElement {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Type.
    pub ty: String,
    /// Role.
    pub role: Role,
    /// Level (features).
    pub level: Option<u32>,
    /// The element as content JSON (`Null` while pending or failed to generate).
    pub json: Value,
    /// Status.
    pub status: ElementStatus,
    /// Review notes and failure reasons.
    pub issues: Vec<String>,
    /// Source passage.
    pub excerpt: Option<SourceExcerpt>,
    /// Balance flags (homebrew balance baselines).
    pub balance: Option<BalanceReport>,
    /// Repair rounds used.
    pub repair_rounds: u32,
    /// User revisions applied.
    pub revisions: u32,
    /// Rules or fields removed because the engine would ignore them after every repair (also in
    /// `issues`; they go to the import report as errors).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_errors: Vec<String>,
    /// Deterministic rule fixes applied (resource ids, `max`, rest names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalised: Vec<String>,
    /// Source-derived acceptance checks run on this element (see [`ElementCheck`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<ElementCheck>,
}

/// One acceptance check derived from the source itself (a heading, table row, number or
/// reference the element must carry). Feeds the element's review state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementCheck {
    /// Stable machine code, e.g. `table_row_missing`, `number_not_in_source`.
    pub code: String,
    /// One line for the user, naming what is missing or wrong.
    pub message: String,
    /// Whether the check failed.
    pub failed: bool,
}

/// Where the import text comes from.
#[derive(Debug, Clone)]
pub enum ImportSource {
    /// Pasted text.
    Text(String),
    /// Text already extracted from a PDF text layer (the fallback for providers without native PDF).
    PdfText(String),
    /// Images and PDFs sent to the model as they are (the model reads scans natively). The model
    /// also transcribes them; later stages work from that transcript.
    Attachments(Vec<borealis_ai::Attachment>),
}

/// Stage names, for progress events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Detect.
    Detect,
    /// Plan.
    Plan,
    /// Generate.
    Generate,
    /// Validate and repair.
    Validate,
    /// Revise.
    Revise,
}

/// A progress event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportProgress {
    /// Stage.
    pub stage: Stage,
    /// Items done.
    pub done: usize,
    /// Items in this stage.
    pub total: usize,
    /// Short message.
    pub message: String,
}

/// Stage 5: the reviewable result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportDraft {
    /// Edition.
    pub edition: Edition,
    /// Id prefix.
    pub id_prefix: String,
    /// `source` stamped on elements.
    pub source_name: String,
    /// Pack id.
    pub pack_id: String,
    /// Pack name.
    pub pack_name: String,
    /// `text`, `pdf_text` or `attachments`.
    pub source_kind: String,
    /// The source text (transcript for attachments).
    pub source_text: String,
    /// Stage 1.
    pub detection: Detection,
    /// Stage 2.
    pub plan: ImportPlan,
    /// Elements in generation order.
    pub elements: Vec<DraftElement>,
    /// Unresolved engine sweep issues.
    pub sweep_issues: Vec<super::sweep::SweepIssue>,
    /// Source-derived checks not tied to one element (e.g. a heading no element covers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_checks: Vec<ElementCheck>,
}

impl ImportDraft {
    /// An element by id.
    pub fn element(&self, id: &str) -> Option<&DraftElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Number of elements with `status`.
    pub fn count(&self, status: ElementStatus) -> usize {
        self.elements.iter().filter(|e| e.status == status).count()
    }
}
