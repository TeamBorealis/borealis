//! `expected.json` (schema 1) and `imported/report.json`: the serde model of a homebrew
//! acceptance case. Documented for case authors in `specs/homebrew-cases/README.md`.

use std::collections::BTreeMap;

use borealis_content::Ruleset;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The schema version this harness reads.
pub const SCHEMA: u32 = 1;

/// One case: everything a correct import of one source must satisfy.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseFile {
    /// Must be [`SCHEMA`].
    pub schema: u32,
    /// Case id (the folder name, by convention).
    pub case: String,
    /// Human readable title.
    #[serde(default)]
    pub title: String,
    /// Ruleset the reference characters are built in.
    pub ruleset: Ruleset,
    /// Bundled packs (folder names under `app/content`) loaded before the import.
    #[serde(default = "default_base_packs")]
    pub base_packs: Vec<String>,
    /// Every object the source defines; each must map to a pack element or a report entry.
    #[serde(default)]
    pub source_objects: Vec<SourceObject>,
    /// Checks on single elements of the imported pack.
    #[serde(default)]
    pub elements: Vec<ElementExpect>,
    /// Checks that some pack element carries a piece of text (TEXT-ONLY-OK sub-items).
    #[serde(default)]
    pub mentions: Vec<MentionExpect>,
    /// Reference characters built with the real engine.
    #[serde(default)]
    pub characters: Vec<CharacterSpec>,
}

fn default_base_packs() -> Vec<String> {
    ["core", "srd-5.1", "srd-5.2.1"]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// `must` failures fail the test; `should` failures are reported only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Must,
    Should,
}

/// How the engine is expected to treat the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mech {
    /// Must be engine-effective.
    Mechanised,
    /// A description is acceptable: the element must exist and carry text.
    TextOnlyOk,
    /// The engine cannot model it today; an element with text or a report entry is expected.
    Unsupported,
}

impl Mech {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Mechanised => "MECH",
            Self::TextOnlyOk => "TEXT",
            Self::Unsupported => "UNSUP",
        }
    }
}

/// Header shared by every expectation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Head {
    /// Unique id inside the case (characters prefix it with the character id).
    pub id: String,
    pub level: Level,
    pub class: Mech,
    /// Why, or which reading of the source was chosen.
    #[serde(default)]
    pub note: String,
}

/// A thing the source defines.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObject {
    /// Name as printed; `A|B` lists accepted alternatives.
    pub name: String,
    /// Expected element type(s), `A|B`; absent = any type.
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    pub class: Mech,
    /// Level of the "not silently dropped" row (default `must`).
    #[serde(default = "must")]
    pub level: Level,
}

fn must() -> Level {
    Level::Must
}

/// Checks on the pack element selected by `type` + `name`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElementExpect {
    #[serde(flatten)]
    pub head: Head,
    /// Element type, `A|B` alternatives.
    #[serde(rename = "type")]
    pub ty: String,
    /// Element name, `A|B` alternatives (case, quotes and spacing are normalised).
    pub name: String,
    /// `"setter"` or `"setter@attribute"` -> expected value (normalised comparison).
    #[serde(default)]
    pub setters: BTreeMap<String, String>,
    /// Every name must be the name of an element the element `supports` (or the tag itself).
    #[serde(default)]
    pub supports_names: Vec<String>,
    /// Every id must be among the element's `supports` tags.
    #[serde(default)]
    pub supports_ids: Vec<String>,
    /// Each value must be a JSON subset of one of the element's rules (own or multiclass).
    #[serde(default)]
    pub rules_include: Vec<Value>,
    /// JSON pointer checks on the serialized element.
    #[serde(default)]
    pub json: Vec<JsonCheck>,
    /// Every token must occur in the element's text (description, sheet text, setter values).
    #[serde(default)]
    pub text_contains: Vec<String>,
}

/// A JSON pointer check (RFC 6901) on the serialized element.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JsonCheck {
    pub pointer: String,
    /// The value at the pointer must be a JSON subset of this.
    #[serde(default)]
    pub equals: Option<Value>,
    /// The value at the pointer, as text, must contain every token (case-insensitive).
    #[serde(default)]
    pub contains: Vec<String>,
}

/// Some pack element (name or text) mentions every token.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MentionExpect {
    #[serde(flatten)]
    pub head: Head,
    pub text: Vec<String>,
}

/// A reference character.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterSpec {
    pub id: String,
    /// Library race/species id (bundled SRD content).
    #[serde(default)]
    pub race_id: Option<String>,
    /// Library background id.
    #[serde(default)]
    pub background_id: Option<String>,
    /// Class by name, looked up in the imported pack.
    #[serde(default)]
    pub class: Option<String>,
    /// Class by id, looked up in the whole store (a bundled class).
    #[serde(default)]
    pub class_id: Option<String>,
    /// Subclass by name, looked up in the imported pack; picked as soon as it is offered.
    #[serde(default)]
    pub subclass: Option<String>,
    pub level: u32,
    /// Base scores STR, DEX, CON, INT, WIS, CHA (before race bonuses).
    pub scores: [i32; 6],
    /// Items added and equipped in a hand, in order.
    #[serde(default)]
    pub equip: Vec<EquipSpec>,
    /// Checks on the derived character.
    #[serde(default)]
    pub expect: Vec<CharExpect>,
}

/// An item to add and equip.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EquipSpec {
    /// Label the `attack` checks refer to.
    pub key: String,
    /// Item name in the imported pack, or `id:<library id>`.
    pub item: String,
    /// Optional magic item (adorner) name in the pack, or `id:<library id>`.
    #[serde(default)]
    pub adorner: Option<String>,
}

/// One check on a derived character.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharExpect {
    #[serde(flatten)]
    pub head: Head,
    #[serde(flatten)]
    pub check: CharCheck,
}

/// What to check on the derived character.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum CharCheck {
    /// No engine error diagnostics and no invalid selections.
    Clean {},
    HitPoints {
        value: i32,
    },
    ProficiencyBonus {
        value: i32,
    },
    Save {
        ability: String,
        #[serde(default)]
        proficient: Option<bool>,
        #[serde(default)]
        total: Option<i32>,
    },
    Skill {
        skill: String,
        #[serde(default)]
        proficient: Option<bool>,
        #[serde(default)]
        expertise: Option<bool>,
        #[serde(default)]
        total: Option<i32>,
    },
    Initiative {
        #[serde(default)]
        total: Option<i32>,
        #[serde(default)]
        advantage: Option<bool>,
    },
    /// Feature names (`A|B` alternatives) present / absent in the feature list.
    Features {
        #[serde(default)]
        present: Vec<String>,
        #[serde(default)]
        absent: Vec<String>,
    },
    /// Library element ids the character has (granted proficiencies and the like).
    Granted {
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        absent_ids: Vec<String>,
    },
    /// A limited-use pool: a resource (name or id contains `name`) or a stat key containing it.
    Resource {
        name: String,
        max: i32,
    },
    /// A named save DC whose owner element name or id contains `name`.
    SaveDc {
        name: String,
        value: i32,
    },
    /// Choice slots matching the filters.
    Choice {
        #[serde(default)]
        name_contains: Option<String>,
        #[serde(default, rename = "type")]
        ty: Option<String>,
        /// Owning element name (`A|B`).
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        options_include_ids: Vec<String>,
        #[serde(default)]
        options_include_names: Vec<String>,
        /// Exact number of matching slots.
        #[serde(default)]
        count: Option<usize>,
        /// Exact / minimum option count of the first matching slot.
        #[serde(default)]
        options: Option<usize>,
        #[serde(default)]
        min_options: Option<usize>,
    },
    /// The attack line of an equipped item.
    Attack {
        equip: String,
        #[serde(default)]
        attack: Option<i32>,
        #[serde(default)]
        damage: Option<String>,
        #[serde(default)]
        range: Option<String>,
        #[serde(default)]
        proficient: Option<bool>,
    },
}

/// Optional `imported/report.json` written by the import step: what happened to each source
/// object that did not simply become an element of the same name.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportReport {
    /// `borealis-import-report/1`.
    pub format: String,
    #[serde(default)]
    pub entries: Vec<ReportEntry>,
    /// Rules the engine would ignore that survived every repair and were removed.
    #[serde(default)]
    pub errors: Vec<ReportIssue>,
    /// Deterministic rule fixes the importer applied.
    #[serde(default)]
    pub normalised: Vec<ReportIssue>,
}

/// One message about one pack element.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportIssue {
    /// Element id in the pack.
    pub element: String,
    /// What happened.
    pub message: String,
}

/// One report entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportEntry {
    /// The source object's name as printed.
    pub source: String,
    /// `element`, `merged`, `text_only`, `unsupported` or `skipped`.
    pub outcome: String,
    /// Element ids that carry it.
    #[serde(default)]
    pub elements: Vec<String>,
    #[serde(default)]
    pub note: String,
}

/// Format tag of [`ImportReport`].
pub const REPORT_FORMAT: &str = "borealis-import-report/1";
/// Allowed [`ReportEntry::outcome`] values.
pub const REPORT_OUTCOMES: [&str; 5] = ["element", "merged", "text_only", "unsupported", "skipped"];
