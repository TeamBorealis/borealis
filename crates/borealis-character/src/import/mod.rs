//! Importer for Aurora `.dnd5e` character files (`character-file-format.md`).
//!
//! The file stores choices as a tree of slot nodes. Each slot is mapped to our slot key
//! (owner element id, rule index, ordinal) by looking the rule up on the owner by type and name,
//! using the legacy CRC only to break ties. Everything that cannot be resolved is
//! reported and, where possible, kept in the character so it can be repaired later.

mod equipment;
mod recheck;
mod sections;
mod tree;

use crate::model::{Character, ImportInfo};
use borealis_content::Ruleset;
use borealis_engine::state::CharacterState;
use borealis_engine::{Diagnostic, Engine};
use borealis_migrate::dom::{parse_document, XmlElement};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use recheck::{name_hint, recheck, source_hint, RecheckOutcome};

/// Why a file could not be imported at all.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    /// The XML is malformed.
    #[error("not valid XML: {0}")]
    Xml(String),
    /// The root element is not `character`.
    #[error("not a character file")]
    NotCharacter,
    /// `character/@version` is missing or malformed.
    #[error("missing or malformed version attribute: {0:?}")]
    BadVersion(String),
    /// A required section is absent.
    #[error("the file has no <{0}> section")]
    MissingSection(&'static str),
}

/// A content reference that the installed content does not define.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingContent {
    /// Element id.
    pub id: String,
    /// Element type recorded in the file.
    pub ty: String,
    /// Display name recorded in the file.
    pub name: String,
    /// Where it was referenced (`option`, `selection`, `inventory`, `dependency`, ...).
    pub context: String,
}

/// A slot that could not be mapped onto a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmappedSlot {
    /// Owner element id.
    pub owner: String,
    /// Rule type.
    pub ty: String,
    /// Rule name.
    pub name: String,
    /// What the slot held.
    pub chosen: String,
    /// Why mapping failed.
    pub reason: String,
}

/// Everything the user should know about an import.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Version of the application that wrote the file.
    pub source_version: String,
    /// Level shown in the file's list card.
    pub file_level: Option<u32>,
    /// Character level after import.
    pub level: u32,
    /// Number of selections mapped onto slots.
    pub selections_imported: u32,
    /// Unresolved content references.
    pub missing_content: Vec<MissingContent>,
    /// Slots that could not be mapped.
    pub unmapped_slots: Vec<UnmappedSlot>,
    /// Stored choices that the current rules reject (kept in the character, flagged by the engine).
    pub invalid_selections: Vec<String>,
    /// Stored choices whose slot is not open under the current rules.
    pub inactive_selections: u32,
    /// Places where numbers differ from the legacy file on purpose (`legacy.*` diagnostics).
    pub legacy_divergences: Vec<Diagnostic>,
    /// Other remarks.
    pub warnings: Vec<String>,
}

/// The imported character and its report.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The character.
    pub character: Character,
    /// The report.
    pub report: ImportReport,
}

pub(crate) struct Ctx<'e> {
    pub uni: &'e borealis_engine::universe::Universe,
    pub state: CharacterState,
    pub report: ImportReport,
    pub unresolved: Vec<crate::model::UnresolvedSelection>,
    /// Every id the saved character registered (`sum` manifest and element tree); `None` when
    /// unknown (re-check).
    pub saved_ids: Option<std::collections::HashSet<String>>,
}

fn valid_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Attributes of a character file that hold element ids.
const ID_ATTRS: [&str; 3] = ["id", "registered", "class"];

/// Map ids that Borealis' id rules reject (and the migrator therefore normalised) onto the
/// migrated element, so characters built on such homebrew keep their selections.
fn canonicalise_ids(node: &mut XmlElement, uni: &borealis_engine::universe::Universe) {
    use borealis_content::idnorm::{hashed_safe_id, is_unsafe_id, safe_id};
    for (key, value) in &mut node.attrs {
        if !ID_ATTRS.contains(&key.as_str()) || !is_unsafe_id(value) || uni.get(value).is_some() {
            continue;
        }
        if let Some(found) = [hashed_safe_id(value), safe_id(value)]
            .into_iter()
            .find(|c| uni.get(c).is_some())
        {
            *value = found;
        }
    }
    for child in &mut node.children {
        if let borealis_migrate::dom::XmlNode::Element(e) = child {
            canonicalise_ids(e, uni);
        }
    }
}

/// Import a `.dnd5e` file's XML text.
///
/// # Errors
/// [`ImportError`] when the file is not a usable character file; anything softer is reported.
pub fn import_dnd5e(engine: &Engine, xml: &str) -> Result<ImportOutcome, ImportError> {
    let mut root = parse_document(xml).map_err(|e| ImportError::Xml(e.to_string()))?;
    canonicalise_ids(&mut root, engine.universe(Ruleset::Dnd2014));
    if root.name != "character" {
        return Err(ImportError::NotCharacter);
    }
    let version = root.attr("version").unwrap_or("").to_string();
    if !valid_version(&version) {
        return Err(ImportError::BadVersion(version));
    }
    let build = root
        .child("build")
        .ok_or(ImportError::MissingSection("build"))?;
    let ruleset = Ruleset::Dnd2014;
    let mut ctx = Ctx {
        uni: engine.universe(ruleset),
        state: CharacterState {
            ruleset,
            ..CharacterState::default()
        },
        report: ImportReport {
            source_version: version.clone(),
            ..ImportReport::default()
        },
        unresolved: Vec::new(),
        saved_ids: Some(saved_ids(build)),
    };
    let mut character = Character::with_ruleset("", ruleset);
    sections::read_header(&root, &mut character, &mut ctx);
    sections::read_input(build, &mut character, &mut ctx);
    sections::read_abilities(build, &mut ctx);
    let items = equipment::read_equipment(build, &mut character, &mut ctx);
    sections::read_sources(&root, &mut ctx);
    let elements = build
        .child("elements")
        .ok_or(ImportError::MissingSection("elements"))?;
    tree::read_elements(elements, &items, &mut ctx);
    sections::read_magic(build, &mut ctx);
    sections::read_dependencies(build, &mut ctx);
    let types = sum_types(build);
    tidy_missing(&mut ctx.report.missing_content, &types);
    finish(engine, root_level(&root), version, character, ctx)
}

/// Every element id the saved character registered: the `sum` manifest plus the ids and
/// choices of the element tree.
fn saved_ids(build: &XmlElement) -> std::collections::HashSet<String> {
    fn walk(e: &XmlElement, out: &mut std::collections::HashSet<String>) {
        for c in e.elements().filter(|c| c.name == "element") {
            for a in ["id", "registered"] {
                if let Some(v) = c.attr(a).filter(|v| !v.is_empty()) {
                    out.insert(v.to_string());
                }
            }
            walk(c, out);
        }
    }
    let mut out = std::collections::HashSet::new();
    for section in ["sum", "elements"] {
        if let Some(s) = build.child(section) {
            walk(s, &mut out);
        }
    }
    out
}

/// Element id -> type, from the `sum` manifest (the complete list of registered ids).
fn sum_types(build: &XmlElement) -> std::collections::HashMap<String, String> {
    build
        .child("sum")
        .into_iter()
        .flat_map(|s| s.elements().filter(|e| e.name == "element"))
        .filter_map(|e| Some((e.attr("id")?.to_string(), e.attr("type")?.to_string())))
        .collect()
}

/// Fill in element types the tree nodes did not carry. Aurora's internal ids (`ID_INTERNAL_*`,
/// its proxy items `ID_*_INTERNAL_ITEM_*_PROXY_*`) are listed like any other id: the engine
/// knows the ones it supports (the core pack, `borealis_engine::proxy`), so one it lacks is
/// something the character loses and the report must say so.
pub(crate) fn tidy_missing(
    list: &mut [MissingContent],
    types: &std::collections::HashMap<String, String>,
) {
    for m in list.iter_mut().filter(|m| m.ty.is_empty()) {
        if let Some(t) = types.get(&m.id) {
            m.ty.clone_from(t);
        }
    }
}

fn root_level(root: &XmlElement) -> Option<u32> {
    root.child("display-properties")?
        .child("level")?
        .text()
        .trim()
        .parse()
        .ok()
}

fn finish(
    engine: &Engine,
    file_level: Option<u32>,
    version: String,
    mut character: Character,
    mut ctx: Ctx<'_>,
) -> Result<ImportOutcome, ImportError> {
    ctx.report.file_level = file_level;
    ctx.report.level = ctx.state.level();
    if let Some(l) = file_level {
        if l != ctx.state.level() {
            ctx.report.warnings.push(format!(
                "the file says level {l} but {} level nodes were found",
                ctx.state.level()
            ));
        }
    }
    let snap = engine.derive_with(&ctx.state, false);
    let active: std::collections::HashSet<_> = snap.slots.iter().map(|s| s.key.clone()).collect();
    ctx.report.inactive_selections = u32::try_from(
        ctx.state
            .selections
            .iter()
            .filter(|s| !active.contains(&s.slot))
            .count(),
    )
    .unwrap_or(0);
    for inv in &snap.invalid_selections {
        ctx.report.invalid_selections.push(format!(
            "{} / rule {} -> {:?}",
            inv.slot.owner, inv.slot.rule, inv.choice
        ));
    }
    ctx.report.legacy_divergences = snap
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("legacy."))
        .cloned()
        .collect();
    character.build = ctx.state;
    character.unresolved = ctx.unresolved;
    character.import = Some(ImportInfo {
        source: "aurora-dnd5e".into(),
        source_version: version,
        missing_content: u32::try_from(ctx.report.missing_content.len()).unwrap_or(u32::MAX),
        legacy_divergences: u32::try_from(ctx.report.legacy_divergences.len()).unwrap_or(u32::MAX),
        missing: ctx.report.missing_content.clone(),
    });
    Ok(ImportOutcome {
        character,
        report: ctx.report,
    })
}
