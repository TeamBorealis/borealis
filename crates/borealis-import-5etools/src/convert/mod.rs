//! Converters from 5etools entities to borealis-content elements.

pub mod background;
pub mod build;
pub mod caster;
pub mod class;
pub mod equipment;
pub mod feat;
pub mod feat_saves;
pub mod feature;
pub mod feature_rules;
pub mod item;
pub mod mech;
pub mod misc;
pub mod optfeat;
pub mod prereq;
pub mod prereq_index;
pub mod race;
pub mod spell;
pub mod spell_blocks;
pub mod spell_choices;
pub mod spells_extra;

use crate::ids::IdAlloc;
use crate::options::ImportOptions;
use crate::report::ReportBuilder;
use crate::spell_lookup::SpellClassLookup;
use crate::srd::{SrdIndex, SrdKind};
use crate::text::{RefKind, Renderer};
use crate::util::{key, name_source_key};
use borealis_content::{Element, MapKind, MapsTo, Ruleset};
use serde_json::Value;
use std::collections::BTreeMap;

/// A raw entry after `_copy` resolution and filtering.
#[derive(Debug, Clone)]
pub struct Ent {
    /// The resolved JSON.
    pub raw: Value,
    /// Data file (relative path).
    pub file: String,
    /// Entry name (empty for unnamed subraces).
    pub name: String,
    /// Source abbreviation as written in the data.
    pub source: String,
    /// Ruleset of the entry.
    pub ruleset: Ruleset,
    /// Flagged `srd` / `srd52`.
    pub is_srd: bool,
}

impl Ent {
    /// `Name|SOURCE` for messages.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}|{}", self.name, self.source)
    }
}

/// The result of converting one entity.
#[derive(Debug, Default)]
pub struct Conv {
    /// Produced elements, the primary one first.
    pub elements: Vec<Element>,
    /// What was lost, one sentence per item.
    pub partial: Vec<String>,
}

/// Immutable data shared by all converters.
#[derive(Debug, Default)]
pub struct Shared {
    /// The options of the run.
    pub opts: ImportOptions,
    /// Link targets by `(kind, lower-case name, upper-case source)`.
    pub links: BTreeMap<(RefKind, String, String), String>,
    /// Race ids by `name|source` (lower-case name, upper-case source).
    pub race_ids: BTreeMap<String, String>,
    /// Class ids by `name|source`.
    pub class_ids: BTreeMap<String, String>,
    /// Parent race properties replaced by subraces: race `name|source` -> property -> the
    /// ids of the subraces that replace it.
    pub race_replaced: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Races that have at least one named subrace, by `name|source`.
    pub races_with_subraces: std::collections::BTreeSet<String>,
    /// Class spell lists.
    pub spell_classes: SpellClassLookup,
    /// SRD pack index.
    pub srd: SrdIndex,
    /// Fluff text by `(kind, name|source)`.
    pub fluff: BTreeMap<(String, String), Vec<Value>>,
    /// Ids of languages the core pack does not define, by lower-case name.
    pub languages: BTreeMap<String, String>,
    /// Display names of sources by upper-case abbreviation.
    pub source_names: BTreeMap<String, String>,
    /// Class features (and optional placeholders) in load order.
    pub class_features: Vec<Ent>,
    /// Class feature index by [`feature::class_feature_key`].
    pub class_feature_keys: BTreeMap<String, usize>,
    /// Subclass features in load order.
    pub subclass_features: Vec<Ent>,
    /// Subclass feature index by [`feature::subclass_feature_key`].
    pub subclass_feature_keys: BTreeMap<String, usize>,
    /// Subclass choice titles by class `name|source`.
    pub subclass_titles: BTreeMap<String, String>,
    /// Lower-case names of the imported classes that cast spells.
    pub caster_classes: std::collections::BTreeSet<String>,
    /// The imported spells, for spell choice filters the supports grammar cannot express.
    pub spell_rows: Vec<prereq_index::SpellRow>,
    /// Subclasses printed for several classes of the same name (an XGE subclass for the PHB
    /// and the XPHB class): `source|class|short name` (lower-case, source upper-case) -> the
    /// class source of the first version in load order (it keeps the plain id).
    pub subclass_twins: BTreeMap<String, String>,
    /// Stat names each imported class's table emits (`monk:unarmored movement`), by class
    /// `name|source` (lower-case name, source upper-case).
    pub class_table_stats: BTreeMap<String, std::collections::BTreeSet<String>>,
}

/// Key of [`Shared::subclass_twins`].
#[must_use]
pub fn subclass_twin_key(source: &str, class_name: &str, short_name: &str) -> String {
    format!(
        "{}|{}|{}",
        source.to_uppercase(),
        class_name.to_lowercase(),
        short_name.to_lowercase()
    )
}

/// Spellcasting classes of the core rules (used when the class itself is not imported).
const CORE_CASTERS: [&str; 9] = [
    "artificer",
    "bard",
    "cleric",
    "druid",
    "paladin",
    "ranger",
    "sorcerer",
    "warlock",
    "wizard",
];

impl Shared {
    /// Does the class (by name) have its own spellcasting?
    #[must_use]
    pub fn is_caster_class(&self, name: &str) -> bool {
        let lc = name.to_lowercase();
        self.caster_classes.contains(&lc) || CORE_CASTERS.contains(&lc.as_str())
    }

    /// Element id for a link target.
    #[must_use]
    pub fn link(&self, kind: RefKind, name: &str, source: &str) -> Option<String> {
        self.links
            .get(&(kind, key(name), source.to_uppercase()))
            .cloned()
    }

    /// A renderer that links into the imported entities.
    #[must_use]
    pub fn renderer<'a>(
        &'a self,
        resolver: &'a dyn Fn(RefKind, &str, &str) -> Option<String>,
    ) -> Renderer<'a> {
        Renderer::new(resolver)
    }

    /// Display name of a source abbreviation.
    #[must_use]
    pub fn source_display(&self, abbr: &str) -> String {
        self.source_names
            .get(&abbr.to_uppercase())
            .cloned()
            .unwrap_or_else(|| crate::source::default_source_name(abbr))
    }

    /// The id of a language: core id, imported id, or a deterministic id.
    #[must_use]
    pub fn language_id(&self, name: &str) -> String {
        let lower = key(name);
        if let Some(core) = crate::core_ids::core_language_id(&lower) {
            return core;
        }
        self.languages
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| format!("ID_5ET_LANGUAGE_{}", crate::util::id_fragment(&lower)))
    }

    /// Resolve a `reprintedAs` uid (`Name|SOURCE`) to `(id, ruleset)`.
    #[must_use]
    pub fn reprint(&self, kind: RefKind, uid: &str) -> Option<(String, Ruleset)> {
        let (name, source) = uid.split_once('|')?;
        let source = source.split('|').next().unwrap_or(source);
        let id = self.link(kind, name, source)?;
        Some((id, crate::source::ruleset_for(source, None)))
    }

    /// Fluff entries for an entity.
    #[must_use]
    pub fn fluff_for(&self, kind: &str, name: &str, source: &str) -> Option<&Vec<Value>> {
        self.fluff
            .get(&(kind.to_string(), name_source_key(name, source)))
    }

    /// `maps_to` for an entity of `ruleset` with `kind`/`name`, from the SRD index and the
    /// `reprintedAs` links.
    #[must_use]
    pub fn maps_to(
        &self,
        kind: SrdKind,
        ruleset: Ruleset,
        name: &str,
        reprinted_as: &[Value],
        reprint_id: &dyn Fn(&str) -> Option<(String, Ruleset)>,
    ) -> Option<MapsTo> {
        let mut maps = MapsTo::default();
        let mut any = false;
        let srd_hit = self.srd.find(kind, ruleset, name).map(str::to_string);
        if let Some(id) = srd_hit {
            set_side(&mut maps, ruleset, id);
            any = true;
        }
        for r in reprinted_as {
            let uid = match r {
                Value::String(s) => s.as_str(),
                other => other.get("uid").and_then(Value::as_str).unwrap_or(""),
            };
            if let Some((id, other_rs)) = reprint_id(uid) {
                if other_rs != ruleset && side(&maps, other_rs).is_none() {
                    let id = self
                        .srd
                        .find(kind, other_rs, name)
                        .map_or(id, str::to_string);
                    set_side(&mut maps, other_rs, id);
                    any = true;
                }
            }
        }
        if !any {
            return None;
        }
        maps.kind = Some(MapKind::Equivalent);
        Some(maps)
    }
}

fn side(maps: &MapsTo, rs: Ruleset) -> Option<&String> {
    match rs {
        Ruleset::Dnd2014 => maps.dnd2014.as_ref(),
        Ruleset::Dnd2024 => maps.dnd2024.as_ref(),
        Ruleset::Any => None,
    }
}

fn set_side(maps: &mut MapsTo, rs: Ruleset, id: String) {
    match rs {
        Ruleset::Dnd2014 => maps.dnd2014 = Some(id),
        Ruleset::Dnd2024 => maps.dnd2024 = Some(id),
        Ruleset::Any => {}
    }
}

/// Mutable state of a run.
#[derive(Debug, Default)]
pub struct State {
    /// Id allocator.
    pub ids: IdAlloc,
    /// Report under construction.
    pub report: ReportBuilder,
    /// Produced elements by upper-case source abbreviation.
    pub out: BTreeMap<String, Vec<Element>>,
    /// Class/subclass features already converted (by their key).
    pub done_features: std::collections::BTreeSet<String>,
    /// Ids of every element emitted so far (duplicates are dropped).
    pub emitted: std::collections::BTreeSet<String>,
    /// Lookups for prerequisites (built before feats and optional features).
    pub prereq: prereq_index::PrereqIndex,
}

impl State {
    /// Record a finished conversion and store its elements under `source`.
    pub fn emit(&mut self, ent: &Ent, kind: &str, conv: Conv) {
        let Conv {
            elements,
            mut partial,
        } = conv;
        let before = elements.len();
        let elements: Vec<Element> = elements
            .into_iter()
            .filter(|e| self.emitted.insert(e.id.clone()))
            .collect();
        if elements.len() < before {
            partial.push(format!(
                "{} element(s) with an id that was already imported were dropped",
                before - elements.len()
            ));
        }
        if elements.is_empty() {
            let reason = if before == 0 {
                "no element produced".to_string()
            } else {
                "duplicate of an entry that was already imported".to_string()
            };
            self.report.skipped(&ent.file, kind, &ent.label(), &reason);
            return;
        }
        let bucket = self.out.entry(ent.source.to_uppercase()).or_default();
        let primary = elements[0].id.clone();
        bucket.extend(elements);
        if partial.is_empty() {
            self.report.converted(&ent.file, kind);
        } else {
            self.report.partial(&ent.file, kind, &primary, &partial);
        }
    }
}

/// A sheet block with one text entry.
#[must_use]
pub fn sheet_text(text: &str) -> borealis_content::Sheet {
    borealis_content::Sheet {
        display: Some(true),
        descriptions: vec![borealis_content::SheetEntry {
            text: text.to_string(),
            ..borealis_content::SheetEntry::default()
        }],
        ..borealis_content::Sheet::default()
    }
}

/// Collect renderer observations into the report.
pub fn drain_renderer(st: &mut State, r: &Renderer<'_>, file: &str, what: &str) {
    for t in &r.unknown_types {
        st.report
            .warn(format!("{file}: no renderer for entry type {t:?} ({what})"));
    }
    if r.dropped_media > 0 {
        st.report.warn(format!(
            "{file}: {} image/stat block entries were not imported ({what})",
            r.dropped_media
        ));
    }
}

/// Append fluff (lore) text to a description.
pub fn append_fluff(
    sh: &Shared,
    kind: &str,
    ent: &Ent,
    description: &mut String,
    renderer: &mut Renderer<'_>,
) {
    if !sh.opts.include_fluff {
        return;
    }
    if let Some(fluff) = sh.fluff_for(kind, &ent.name, &ent.source) {
        let html = renderer.entries(fluff);
        if !html.is_empty() {
            description.push_str("<h4>Lore</h4>");
            description.push_str(&html);
        }
    }
}

/// Build an element header.
#[must_use]
pub fn new_element(
    id: &str,
    name: &str,
    ty: &str,
    source_display: &str,
    ruleset: Ruleset,
) -> Element {
    let mut e = Element::new(id, name, ty, source_display);
    e.ruleset = Some(ruleset);
    e
}

/// Record an unconverted detail on the element so nothing is silently lost.
pub fn note_unmapped(e: &mut Element, key: &str, value: &str) {
    let slot = e.extra.attributes.entry(format!("5et:{key}")).or_default();
    if !slot.is_empty() {
        slot.push_str("; ");
    }
    slot.push_str(value);
}

/// First sentence-ish chunk of plain text, at most `max` characters.
#[must_use]
pub fn short_text(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    match cut.rfind(". ") {
        Some(i) if i > max / 3 => cut[..=i].to_string(),
        _ => format!("{}...", cut.trim_end()),
    }
}
