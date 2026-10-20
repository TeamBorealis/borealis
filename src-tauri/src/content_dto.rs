//! Contract DTOs of the Content, Manage and compatibility commands.

use crate::dto::{ElementRef, Ruleset};
use borealis_content::IssueCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a pack came from (`PackKind` in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PackKind {
    /// Shipped with the app.
    #[serde(rename = "bundled")]
    Bundled,
    /// Converted from a user file.
    #[serde(rename = "imported")]
    Imported,
    /// Installed from an update index.
    #[serde(rename = "downloaded")]
    Downloaded,
    /// The user's homebrew pack.
    #[serde(rename = "homebrew")]
    Homebrew,
    /// Converted from the user's local 5etools data (not redistributable).
    #[serde(rename = "local-import")]
    LocalImport,
}

/// An installed pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPack {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PackKind,
    pub rulesets: Vec<Ruleset>,
    pub element_count: usize,
    pub enabled: bool,
    pub license: String,
    pub attribution: String,
    pub source_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redistributable: Option<bool>,
}

/// A content source (book) inside a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSource {
    pub id: String,
    pub name: String,
    pub group: String,
    pub pack_id: String,
    pub element_count: usize,
    pub description: String,
    pub playtest: bool,
}

/// `import_pack` argument.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPackRequest {
    pub file_name: String,
    pub content_base64: String,
    #[serde(default)]
    pub format: Option<String>,
}

/// One conversion finding, located precisely: the source file (path inside the imported set),
/// the element id and type it concerns, the source line, and a closed code
/// ([`IssueCode`], snake_case on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationIssue {
    pub severity: &'static str,
    pub code: IssueCode,
    pub message: String,
    pub element: Option<String>,
    /// The element's `type` in the source (`Spell`, `Class Feature`, ...).
    pub element_type: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// Outcome of converting one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub file_name: String,
    pub format: String,
    pub converted: usize,
    pub skipped: usize,
    pub issues: Vec<MigrationIssue>,
}

/// `import_pack` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportPackResult {
    pub pack: Option<ContentPack>,
    pub report: MigrationReport,
}

/// A user-added update index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentIndex {
    pub url: String,
    pub name: String,
    pub last_checked: Option<String>,
    pub file_count: usize,
    pub updates_available: usize,
    pub requires_newer_app: bool,
    /// Signed by the pinned key; `false` shows an "unverified" badge.
    pub verified: bool,
    /// Why the last check failed or was incomplete (e.g. "the server answered 404 Not Found for 3 files").
    pub problem: Option<String>,
}

/// `update_indexes` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIndexesResult {
    pub updated: usize,
    pub failed: Vec<String>,
    pub reloaded: bool,
    /// Aurora index files whose content changed since their first download (applied, reported).
    pub changed: Vec<String>,
    /// Hosts an index downloaded from for the first time.
    pub new_hosts: Vec<String>,
    /// One conversion report per pack this update converted (as a file import returns).
    pub reports: Vec<MigrationReport>,
}

/// Progress of a long content operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentProgress {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    pub message: String,
}

/// One book found by `scan_5etools_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiveToolsSource {
    pub abbr: String,
    pub name: String,
    pub ruleset: String,
    /// `book`, `adventure`, `homebrew` or `other`.
    pub category: String,
    pub counts: BTreeMap<String, usize>,
}

/// `scan_5etools_folder` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiveToolsScan {
    pub sources: Vec<FiveToolsSource>,
}

/// `import_5etools` argument.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiveToolsImportRequest {
    /// `pick_path` token (purpose `5etools-folder`), never a raw path.
    pub folder: String,
    pub sources: Vec<String>,
    #[serde(default)]
    pub include_fluff: bool,
}

/// A report note (partial or skipped element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportNote {
    pub element: String,
    /// Entity kind (`feat`, `language`, ...).
    pub kind: String,
    pub reason: String,
}

/// `import_5etools` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiveToolsImportReport {
    pub converted: usize,
    /// Packs that already existed and were replaced in place.
    pub replaced: usize,
    pub partial: Vec<ImportNote>,
    pub skipped: Vec<ImportNote>,
    pub packs: Vec<ContentPack>,
}

/// A pick that a source restriction would drop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedPick {
    pub slot_id: String,
    pub slot_title: String,
    pub element: String,
}

/// `preview_source_restrictions` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestrictionPreview {
    pub affected: Vec<AffectedPick>,
}

/// One foreign-edition element in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatEntry {
    pub element: ElementRef,
    pub from: Ruleset,
    pub converted: bool,
    pub notes: Vec<String>,
}

/// `get_compatibility_report` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityReport {
    /// Strict mode: new picks show same-edition content only.
    pub strict: bool,
    pub entries: Vec<CompatEntry>,
}

/// Manual attack as sent by the UI (`id` absent when new).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AttackInput {
    pub id: Option<String>,
    pub name: String,
    pub range: String,
    pub attack_bonus: String,
    pub damage: String,
    pub description: String,
    pub ability: Option<String>,
    pub show_in_combat: bool,
    /// Print this attack as a card; absent keeps the default (follows the weapon line's "show card" flag, off for typed rows).
    pub as_card: Option<bool>,
}

/// Direction for `move_attack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveDirection {
    Up,
    Down,
}

/// The companion tab state (`CompanionState` in the UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionState {
    pub creature: Option<ElementRef>,
    pub name: String,
    pub portrait: Option<String>,
    pub armor_class: i32,
    pub hp: i32,
    /// The armor class as text when the stat block prints it as a formula (decision IM12a);
    /// the tab shows it instead of `armor_class`.
    pub armor_class_note: Option<String>,
    /// The hit points as text when the stat block prints them as a formula (decision IM12a);
    /// the tab shows it instead of `hp`.
    pub hp_note: Option<String>,
    /// The challenge rating as printed (`1/4`), the note that stands in for it, or an em dash
    /// (decision IM12); the tab shows it as is.
    pub challenge: String,
    pub speed: i32,
    pub abilities: crate::dto::AbilityRecord<i32>,
}
