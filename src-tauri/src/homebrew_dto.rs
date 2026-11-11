//! Wire types of the homebrew creator (1.5.0; `specs/homebrew-creator-spec.md` section 15.2). They mirror
//! `ui/src/api/types/homebrew.ts` field for field (camelCase on the wire); `tests/contract_homebrew.rs` checks each
//! against the generated contract. The form itself is `borealis_authoring::form::FormState` (WB-06); inner parts owned
//! by later work items travel as JSON values until those items type them.

use crate::content_dto::ContentPack;
use crate::dto::{CharacterSnapshot, Ruleset};
use crate::dto_ext::ElementDetail;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// One editor sub-tab (`general`, `levels`, `rules`...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewTab {
    pub id: String,
    pub label: String,
}

/// One choice of a combo field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewChoice {
    pub value: String,
    pub label: String,
}

/// What a reference picker offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewPickerTarget {
    pub types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
}

/// One form field and the model locations it writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewFieldSpec {
    pub key: String,
    pub label: String,
    pub tab: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// `text`, `number`, `combo`, `reference`, `condition`, `richText`, `blocks`... (`HomebrewControl`).
    pub control: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editions: Option<Vec<Ruleset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<HomebrewChoice>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_custom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picker: Option<HomebrewPickerTarget>,
    pub bindings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert: Option<bool>,
}

/// One creator form.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewKindDescriptor {
    pub id: String,
    pub label: String,
    pub plural_label: String,
    pub description: String,
    /// `main`, `small`, `child` or `expert`.
    pub group: String,
    pub element_types: Vec<String>,
    pub parent_kinds: Vec<String>,
    pub editions: Vec<Ruleset>,
    pub tabs: Vec<HomebrewTab>,
    pub fields: Vec<HomebrewFieldSpec>,
}

/// One shape of a `variant` block field (`resource.max`: fixed, statistic, class table, formula, rolled).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewBlockVariant {
    pub value: String,
    pub label: String,
    pub fields: Vec<HomebrewBlockField>,
}

/// One field of a block dialog: a form field plus what the block controls need.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewBlockField {
    #[serde(flatten)]
    pub base: HomebrewFieldSpec,
    /// `formula` / `formulas`: the dialect of the field's engine reader.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Free text the engine shows verbatim ("Shown as a note; Borealis does not apply it").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shown_as_note: Option<bool>,
    /// `name`: what the name names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// `name`: several names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    /// `group`, `rows`: the parts; `map`: the key and the value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<HomebrewBlockField>>,
    /// `variant`: the shapes, chosen by `type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<HomebrewBlockVariant>>,
    /// `blocks`: the block kinds the nested list holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
}

/// One rule block kind of the catalog.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewBlockSpec {
    pub kind: String,
    pub label: String,
    /// Add menu group id (`HomebrewBlockGroup`).
    pub group: String,
    pub group_label: String,
    pub common: bool,
    pub core_only: bool,
    pub effect_only: bool,
    pub level_gated: bool,
    pub only_if: bool,
    pub fields: Vec<HomebrewBlockField>,
}

/// One statistic of the statistic picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewStatEntry {
    pub name: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_scoped: Option<bool>,
}

/// A group of the statistic picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewStatGroup {
    pub id: String,
    pub label: String,
    pub stats: Vec<HomebrewStatEntry>,
}

/// NEW PACK.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewPackRequest {
    pub name: String,
    pub code: String,
    /// `dnd2014`, `dnd2024` or `any` (Both editions).
    pub edition: Ruleset,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
}

/// Pack settings change (the code never changes).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomebrewPackPatch {
    pub name: Option<String>,
    pub edition: Option<Ruleset>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
}

/// Problem counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewProblemCounts {
    pub errors: usize,
    pub warnings: usize,
}

/// One row of the element list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewElementSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub element_type: String,
    pub parent_id: Option<String>,
    pub edition: Option<Ruleset>,
    pub problems: HomebrewProblemCounts,
    pub has_draft: bool,
}

/// One rule block of a form: `borealis_authoring::form::Block` (its `memo` never travels: `to_ui` strips it).
pub type HomebrewBlock = borealis_authoring::form::Block;

/// A part the form does not edit, kept byte-for-byte: `borealis_authoring::form::PreservedPart` (its `kept` bytes
/// never travel).
pub type HomebrewPreservedPart = borealis_authoring::form::PreservedPart;

/// The editable state of one draft: `borealis_authoring::form::FormState`. Sent as `to_ui` of the stored form (no
/// memo, no kept bytes); what the UI sends back is merged into the stored form by key (`merge_ui`), so a memo or
/// kept bytes in a request are ignored.
pub type HomebrewForm = borealis_authoring::form::FormState;

/// One validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewProblem {
    /// `error`, `warning` or `info`.
    pub severity: String,
    pub message: String,
    pub path: Option<String>,
    pub element_id: Option<String>,
    pub code: String,
}

/// DATA sub-tab content.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewGeneratedView {
    pub json: String,
    pub tree: Vec<Value>,
}

/// An open draft.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewDraftView {
    pub draft_id: String,
    pub pack_id: String,
    pub element_id: Option<String>,
    pub form: HomebrewForm,
    pub preview: Option<ElementDetail>,
    pub problems: Vec<HomebrewProblem>,
    pub generated: HomebrewGeneratedView,
    pub updated_at: String,
    pub base_changed: bool,
}

/// A draft on this device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewDraftSummary {
    pub draft_id: String,
    pub pack_id: String,
    pub element_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub updated_at: String,
}

/// Save result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewSaveResult {
    pub pack: ContentPack,
    pub element_ids: Vec<String>,
}

/// A local character that uses an element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewCharacterRef {
    pub character_id: String,
    pub name: String,
}

/// Delete scan or delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewDeleteResult {
    pub deleted: bool,
    pub used_by: Vec<HomebrewCharacterRef>,
    pub pack: Option<ContentPack>,
}

/// Multi-select edits. `edition: null` clears an element's own edition (the pack's applies).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomebrewBulkPatch {
    #[serde(deserialize_with = "present")]
    pub edition: Option<Option<Ruleset>>,
    pub add_tags: Option<Vec<String>>,
    pub remove_tags: Option<Vec<String>>,
    pub move_to_pack_id: Option<String>,
}

/// Picker filters.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomebrewPickerFilters {
    pub sources: Option<Vec<String>>,
    pub edition: Option<Ruleset>,
    pub spell_school: Option<String>,
    pub spell_levels: Option<Vec<u32>>,
    pub ritual: Option<bool>,
    pub concentration: Option<bool>,
    /// Bundled (SRD) elements only: "Start from a template" (spec 11, CS1).
    pub srd_only: Option<bool>,
}

/// Paging of a picker query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewPageRequest {
    pub offset: usize,
    pub limit: usize,
}

/// One picker row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewPickerRow {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub summary: String,
    pub prerequisites: Option<String>,
    pub source_name: String,
    pub edition: Option<Ruleset>,
    pub draft: bool,
    /// Spells only (the spell picker's Magic grid, spec 13.7): level 0-9, school and casting facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub school: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spell: Option<crate::gear_facts::SpellFacts>,
}

/// Where-used row. `relation`: `grants`, `selects`, `requires` or `lists`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewElementRef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub pack_id: Option<String>,
    pub relation: String,
}

/// One spell of a SPELL LISTS grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewSpellListRow {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub school: String,
    pub verbal: bool,
    pub somatic: bool,
    pub material: bool,
    pub source_name: String,
    pub on_list: bool,
    pub removable: bool,
}

/// A spell list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewSpellListView {
    pub list_tag: String,
    pub own: bool,
    pub count: usize,
    pub rows: Vec<HomebrewSpellListRow>,
}

/// Try it setup.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomebrewTrialSetup {
    pub edition: Option<Ruleset>,
    pub level: Option<u32>,
    pub class_id: Option<String>,
    pub subclass_id: Option<String>,
    pub species_id: Option<String>,
    pub background_id: Option<String>,
    pub ability_scores: Option<Vec<i32>>,
    pub as_second_class: Option<bool>,
}

/// One Try it change. `elementId: null` clears the slot.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomebrewTrialChange {
    pub level: Option<u32>,
    pub setup: Option<HomebrewTrialSetup>,
    pub slot_id: Option<String>,
    #[serde(deserialize_with = "present")]
    pub element_id: Option<Option<String>>,
}

/// A field that may be absent (`None`), `null` (`Some(None)`) or set: absence keeps, `null` clears.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// One rung of the level ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewLadderRow {
    pub level: u32,
    pub features: Vec<String>,
    pub choices: Vec<String>,
    pub slots: Vec<u32>,
    pub problems: Vec<HomebrewProblem>,
}

/// The trial character.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewTrialView {
    pub trial_id: String,
    pub snapshot: Box<CharacterSnapshot>,
    pub ladder: Vec<HomebrewLadderRow>,
    pub problems: Vec<HomebrewProblem>,
    pub changed: Vec<String>,
}
