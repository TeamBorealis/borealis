//! `expected.json` (schema 1 or 2) and `imported/report.json`: the serde model of a homebrew
//! acceptance case. Documented for case authors in `specs/homebrew-cases/README.md`.

use std::collections::BTreeMap;

use borealis_content::Ruleset;
use borealis_engine::state::EquipSlot;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The current schema version. Schema 2 only adds optional fields and checks, so schema 1
/// files keep their meaning.
pub const SCHEMA: u32 = 2;
/// Every schema version this harness reads.
pub const SCHEMAS: [u32; 2] = [1, SCHEMA];

/// One case: everything a correct import of one source must satisfy.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseFile {
    /// One of [`SCHEMAS`].
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
    /// Element counts over the imported pack (schema 2).
    #[serde(default)]
    pub counts: Vec<CountExpect>,
    /// The pack holds no element, or only of the allowed types (schema 2).
    #[serde(default)]
    pub pack_empty: Vec<PackEmptyExpect>,
    /// Elements that must NOT be in the imported pack (nothing invented; schema 2).
    #[serde(default)]
    pub absent: Vec<AbsentExpect>,
    /// Required `report.json` entries (schema 2).
    #[serde(default)]
    pub report: Vec<ReportExpect>,
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
    /// Element type, `A|B` alternatives; absent = any type (only a correction produces that).
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
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
    /// `"setter"` or `"setter@attribute"` -> a token the value must contain (schema 2).
    #[serde(default)]
    pub setters_contain: BTreeMap<String, String>,
    /// `rules_include` and `json` also look at every pack element this one grants, directly or
    /// through further grants (a source section whose sub-headings became child elements).
    #[serde(default)]
    pub follow_grants: bool,
}

/// Filter over the imported pack's elements, shared by counts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElementFilter {
    /// Element type, `A|B` alternatives; absent = any type.
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    /// `"setter"` or `"setter@attribute"` -> value (`A|B`, normalised comparison).
    #[serde(default)]
    pub setters: BTreeMap<String, String>,
    /// Every name must be a `supports` tag of the element or the name of the element it names.
    #[serde(default)]
    pub supports_names: Vec<String>,
}

/// How many pack elements match a filter.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CountExpect {
    #[serde(flatten)]
    pub head: Head,
    #[serde(flatten)]
    pub filter: ElementFilter,
    #[serde(default)]
    pub equals: Option<usize>,
    #[serde(default)]
    pub min: Option<usize>,
    #[serde(default)]
    pub max: Option<usize>,
}

/// The imported pack holds no element at all, except optionally of the listed types ("nothing
/// importable": a lore-only source).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackEmptyExpect {
    #[serde(flatten)]
    pub head: Head,
    /// Element type(s), `A|B`, that may exist; absent = none may.
    #[serde(default)]
    pub except_types: Option<String>,
}

/// No pack element has this name (and type, if given).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AbsentExpect {
    #[serde(flatten)]
    pub head: Head,
    /// Element type(s), `A|B`; absent = any type.
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    /// Name, `A|B`.
    pub name: String,
}

/// `report.json` has an entry for this source object.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportExpect {
    #[serde(flatten)]
    pub head: Head,
    /// Source name as printed, `A|B`.
    pub source: String,
    /// Accepted outcomes, `A|B` (absent = any).
    #[serde(default)]
    pub outcome: Option<String>,
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
    /// ... and at least one of these tokens (schema 2).
    #[serde(default)]
    pub contains_any: Vec<String>,
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
    /// Free text for case authors.
    #[serde(default)]
    pub note: String,
    /// Library race/species id (bundled SRD content).
    #[serde(default)]
    pub race_id: Option<String>,
    /// Race/species by name, looked up in the imported pack (schema 2).
    #[serde(default)]
    pub race: Option<String>,
    /// Subrace by name, looked up in the imported pack; picked as soon as it is offered.
    #[serde(default)]
    pub subrace: Option<String>,
    /// Library background id.
    #[serde(default)]
    pub background_id: Option<String>,
    /// Background by name, looked up in the imported pack (schema 2).
    #[serde(default)]
    pub background: Option<String>,
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
    /// Generic selections, each made as soon as a matching slot offers it (schema 2).
    #[serde(default)]
    pub picks: Vec<PickSpec>,
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
    /// Equip slot (`armor`, `primary_hand`, `secondary_hand`, `worn`); absent = the engine's pick.
    #[serde(default)]
    pub slot: Option<EquipSlot>,
}

/// A selection: the first open slot whose name contains `slot` and that offers `option`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PickSpec {
    /// Slot name fragment (normalised `contains`); absent = any slot.
    #[serde(default)]
    pub slot: Option<String>,
    /// A pack element name (`A|B`), an option name, or `id:<id>`.
    pub option: String,
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
        /// An active `ability_check` advantage whose text names the skill (schema 2).
        #[serde(default)]
        advantage: Option<bool>,
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
    /// Element ids (and, schema 2, names `A|B`) the character has registered, or has as a
    /// spell of a spellcasting block.
    Granted {
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        absent_ids: Vec<String>,
        #[serde(default)]
        names: Vec<String>,
        #[serde(default)]
        absent_names: Vec<String>,
    },
    /// A limited-use pool: a resource (name or id contains `name`) or a stat key containing it.
    /// `name` may list alternatives as `a|b`.
    /// `absent: true` (schema 2) requires that neither exists.
    Resource {
        name: String,
        #[serde(default)]
        max: Option<i32>,
        #[serde(default)]
        absent: Option<bool>,
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
        /// No matching slot may offer these (schema 2).
        #[serde(default)]
        options_exclude_ids: Vec<String>,
        #[serde(default)]
        options_exclude_names: Vec<String>,
        /// No matching slot's rule name may contain these (schema 2).
        #[serde(default)]
        name_excludes: Vec<String>,
        /// Exact number of matching slots (a `select` with `number: N` is N slots).
        #[serde(default)]
        count: Option<usize>,
        /// Exact number of distinct matching select rules (one rule is one choice, whatever its
        /// `number`).
        #[serde(default)]
        selects: Option<usize>,
        /// Exact number of picks (slots) of the first matching select rule.
        #[serde(default)]
        picks: Option<usize>,
        /// Exact / minimum option count of the first matching slot (entries of a `List`).
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
    /// Final ability scores STR..CHA (schema 2).
    AbilityScores {
        value: [i32; 6],
    },
    /// Speeds in feet (schema 2); 0 = none.
    Speed {
        #[serde(default)]
        walk: Option<i32>,
        #[serde(default)]
        climb: Option<i32>,
        #[serde(default)]
        swim: Option<i32>,
        #[serde(default)]
        fly: Option<i32>,
        #[serde(default)]
        burrow: Option<i32>,
    },
    /// The registered Size element's name, `A|B` (schema 2).
    Size {
        value: String,
    },
    /// Sense ranges in feet (schema 2); 0 = the character lacks the sense.
    Vision {
        #[serde(default)]
        darkvision: Option<i32>,
        #[serde(default)]
        blindsight: Option<i32>,
        #[serde(default)]
        tremorsense: Option<i32>,
        #[serde(default)]
        truesight: Option<i32>,
    },
    /// Registered Language elements (schema 2). `text`: tokens found in a language name or in
    /// a feature's text (languages with no element).
    Languages {
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        names: Vec<String>,
        #[serde(default)]
        text: Vec<String>,
        #[serde(default)]
        absent_ids: Vec<String>,
    },
    /// Armor class with what is equipped (schema 2).
    ArmorClass {
        value: i32,
    },
    /// A spellcasting block (`name` `A|B`, else the first one) (schema 2).
    Spellcasting {
        #[serde(default)]
        name: Option<String>,
        /// `false`: no block (matching `name`) exists.
        #[serde(default)]
        present: Option<bool>,
        #[serde(default)]
        ability: Option<String>,
        #[serde(default)]
        dc: Option<i32>,
        #[serde(default)]
        attack: Option<i32>,
        #[serde(default)]
        prepare_limit: Option<i32>,
        /// Slots by spell level from 1st (a prefix; missing levels are not checked).
        #[serde(default)]
        slots: Option<Vec<i32>>,
        /// `true`: no slots at any level and no pact slots.
        #[serde(default)]
        no_slots: Option<bool>,
        /// Number of level-0 spells in the block's spell list.
        #[serde(default)]
        cantrips: Option<usize>,
        /// Spells listed by the block (known, prepared, or preparable from the class list).
        #[serde(default)]
        spells_include_ids: Vec<String>,
        #[serde(default)]
        spells_include_names: Vec<String>,
        #[serde(default)]
        spells_exclude_ids: Vec<String>,
        #[serde(default)]
        spells_exclude_names: Vec<String>,
    },
    /// Damage / condition defences by name, any toggle state (schema 2).
    Defenses {
        #[serde(default)]
        resistances: Vec<String>,
        #[serde(default)]
        immunities: Vec<String>,
        #[serde(default)]
        vulnerabilities: Vec<String>,
        #[serde(default)]
        condition_immunities: Vec<String>,
        /// In none of the four lists.
        #[serde(default)]
        absent: Vec<String>,
    },
    /// An advantage entry whose roll matches and whose text has every token (schema 2).
    Advantage {
        #[serde(default)]
        roll: Option<String>,
        #[serde(default)]
        text: Vec<String>,
        /// `true`: no such entry.
        #[serde(default)]
        absent: Option<bool>,
    },
    /// Attuned item count and attunement limit (schema 2).
    Attunement {
        #[serde(default)]
        attuned: Option<u32>,
        #[serde(default)]
        limit: Option<u32>,
    },
    /// The inventory line of an equip key (schema 2).
    Item {
        equip: String,
        #[serde(default)]
        equipped: Option<bool>,
        #[serde(default)]
        attuned: Option<bool>,
    },
    /// Passes when at least one alternative passes (schema 2). Alternatives carry no head.
    AnyOf {
        any_of: Vec<CharCheck>,
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
    /// Things to check by hand (an element whose source passage was not found).
    #[serde(default)]
    pub warnings: Vec<ReportIssue>,
    /// Notes about the source as a whole (a page read visually, a bad text layer), not about
    /// one element.
    #[serde(default)]
    pub source_warnings: Vec<String>,
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
    /// `element`, `merged`, `text_only`, `unsupported`, `skipped`, `dropped` (a failed element
    /// left out) or `unchecked` (a failed element kept for review).
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
pub const REPORT_OUTCOMES: [&str; 7] = [
    "element",
    "merged",
    "text_only",
    "unsupported",
    "skipped",
    "dropped",
    "unchecked",
];

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn report_accepts_source_warnings_and_still_refuses_unknown_fields() {
        let ok = r#"{"format":"borealis-import-report/1","source_warnings":["page 2 was read visually"]}"#;
        let r: ImportReport = serde_json::from_str(ok).expect("source_warnings is a report field");
        assert_eq!(r.source_warnings, vec!["page 2 was read visually"]);
        let bad = r#"{"format":"borealis-import-report/1","surprise":[]}"#;
        assert!(serde_json::from_str::<ImportReport>(bad).is_err());
    }
}
