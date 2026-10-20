//! The import report: what was converted, converted partially, or skipped, and why.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One entity that was converted with losses or skipped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReportNote {
    /// `Name|SOURCE` of the 5etools entry (or the element id when one was produced).
    pub element: String,
    /// Kind of entity (`race`, `class`, `spell`, ...).
    pub kind: String,
    /// Data file the entry came from (relative to the data folder).
    pub file: String,
    /// What was lost or why the entry was skipped.
    pub reason: String,
    /// Why a partial conversion could not be complete ([`reason_code`]); empty for skips.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
}

/// Reason codes of partial-conversion notes: what is missing, and where (`ENGINE_` = a rule
/// kind or behaviour the engine lacks, `CORE_` = an element the core pack lacks, `DATA_` =
/// the 5etools data itself cannot be mapped exactly).
pub mod code {
    /// A rolled charge count (`{@dice 1d4 - 1}`) or none at all: resources need a fixed max.
    pub const ROLLED_CHARGES: &str = "ENGINE_RESOURCE_ROLLED_MAX";
    /// A proficiency (firearms, improvised weapons, air / space vehicles) the core pack lacks.
    pub const NO_CORE_PROFICIENCY: &str = "CORE_NO_PROFICIENCY_ELEMENT";
    /// A weapon category choice: a character already proficient gets an empty select.
    pub const EMPTY_CHOICE: &str = "ENGINE_EMPTY_CHOICE";
    /// A choice of named elements and categories at once: a select is ids or tags.
    pub const MIXED_CHOICE: &str = "ENGINE_MIXED_ID_TAG_CHOICE";
    /// A spell choice whose filter matches no spell of the data.
    pub const NO_MATCHING_SPELL: &str = "DATA_NO_MATCHING_SPELL";
    /// A pack content entry that is plain text, not an item.
    pub const NOT_AN_ITEM: &str = "DATA_PACK_CONTENT_NOT_AN_ITEM";
    /// A speed multiplier (Boots of Speed).
    pub const SPEED_MULTIPLIER: &str = "ENGINE_SPEED_MULTIPLIER";
    /// A bonus to every speed the character has.
    pub const ALL_SPEEDS: &str = "ENGINE_BONUS_TO_EVERY_SPEED";
    /// Item proficiencies that are random, in a table, or "proficient with this item", and the
    /// doubled bonus of a proficiency an item lets its owner choose.
    pub const ITEM_PROFICIENCY: &str = "ENGINE_ITEM_PROFICIENCY_TEXT";
    /// A prerequisite the requirement grammar cannot test (house renown).
    pub const UNCHECKED_PREREQUISITE: &str = "ENGINE_UNCHECKED_PREREQUISITE";
    /// A progression key that is neither a level nor `*`.
    pub const UNKNOWN_PROGRESSION: &str = "DATA_UNKNOWN_PROGRESSION_KEY";
    /// An emitted rule the engine would ignore (an unknown custom rule kind).
    pub const IGNORED_RULE: &str = "ENGINE_UNKNOWN_RULE_KIND";
    /// No code matches the note (a new kind of loss: classify it).
    pub const UNCLASSIFIED: &str = "UNCLASSIFIED";
    /// A character creation option kept as text: no builder choice offers it.
    pub const CHARACTER_OPTION: &str = "ENGINE_NO_CHARACTER_OPTION_SLOT";
    /// A sidekick class (TCE): it levels a companion creature, not a character.
    pub const SIDEKICK_CLASS: &str = "OUT_OF_SCOPE_SIDEKICK_CLASS";
    /// A part printed with a creature that is not its stat block (variant sidebars, lair
    /// actions and regional effects, mythic actions, section introductions).
    pub const CREATURE_SECTION: &str = "OUT_OF_SCOPE_CREATURE_SECTION";
    /// A creature field kept in part: a printed remark or alternative the stat block model has
    /// no field for (alternate speeds, a skill "plus PB", an armor class "varies").
    pub const CREATURE_FIELD: &str = "MODEL_CREATURE_FIELD_TEXT";
    /// Hit points, an ability score, the AC or the CR printed as text, not a number ("half the
    /// summoner's", "Unknown"). A creature without a CR keeps AC / HP text as a display note
    /// instead (decision IM12a), so for AC / HP this is a creature with a CR, or one with no
    /// text at all.
    pub const CREATURE_NOT_A_NUMBER: &str = "DATA_CREATURE_STAT_NOT_A_NUMBER";
    /// A creature type that is a choice ("celestial, fey or fiend") or not one of the 14 types.
    pub const CREATURE_TYPE: &str = "MODEL_CREATURE_TYPE";
    /// A creature whose stat block fails the model's consistency checks.
    pub const CREATURE_INVALID: &str = "DATA_CREATURE_STAT_BLOCK_INVALID";
    /// A creature `_copy` whose `_mod` (or template) uses a mode the importer does not apply
    /// (`scalarAddHit`, `maxSize`): applying the rest would leave wrong numbers.
    pub const COPY_MODE: &str = "DATA_COPY_MOD_MODE_NOT_APPLIED";
    /// A `_copy` whose base, template or `_mod` target is not in the data.
    pub const COPY_TARGET: &str = "DATA_COPY_TARGET_MISSING";
}

/// The reason code of a partial-conversion note (see [`code`]).
#[must_use]
pub fn reason_code(reason: &str) -> &'static str {
    let has = |needle: &str| reason.contains(needle);
    if (reason.starts_with("charges ") && has("were not converted"))
        || has("but the item has no charge count")
    {
        code::ROLLED_CHARGES
    } else if has("has no core element") {
        code::NO_CORE_PROFICIENCY
    } else if reason.starts_with("weapon proficiency chosen by filter") {
        code::EMPTY_CHOICE
    } else if has("was widened to") || has("some tools in a tool choice") {
        code::MIXED_CHOICE
    } else if has("matches no imported spell") {
        code::NO_MATCHING_SPELL
    } else if reason.starts_with("pack content") && has("is not an item") {
        code::NOT_AN_ITEM
    } else if reason.starts_with("speed change multiply") {
        code::SPEED_MULTIPLIER
    } else if has("applied to the walking speed only") {
        code::ALL_SPEEDS
    } else if has("the proficiencies the item grants were not found")
        || has("doubled proficiency bonus of the chosen proficiency")
    {
        code::ITEM_PROFICIENCY
    } else if reason.starts_with("prerequisite not enforced") {
        code::UNCHECKED_PREREQUISITE
    } else if reason.starts_with("progression key") {
        code::UNKNOWN_PROGRESSION
    } else if reason.starts_with("character option") {
        code::CHARACTER_OPTION
    } else if reason.starts_with("sidekick class") {
        code::SIDEKICK_CLASS
    } else if reason.starts_with("creature section: ") {
        code::CREATURE_SECTION
    } else if reason.starts_with("creature field: ") {
        code::CREATURE_FIELD
    } else {
        code::UNCLASSIFIED
    }
}

/// Reason code of a skip note, when the skip is classified (empty otherwise).
#[must_use]
pub fn skip_code(reason: &str) -> &'static str {
    let creature = |rest: &str| {
        reason
            .strip_prefix("creature stat block: ")
            .is_some_and(|r| r.starts_with(rest))
    };
    let not_a_number = reason.starts_with("creature stat block: ")
        && (reason.contains("is not a number") || reason.contains("are not a number"));
    if reason.starts_with("sidekick class") {
        code::SIDEKICK_CLASS
    } else if not_a_number {
        code::CREATURE_NOT_A_NUMBER
    } else if creature("the creature type")
        || creature("creature type")
        || creature("no creature type")
    {
        code::CREATURE_TYPE
    } else if reason.starts_with("creature stat block: ") {
        code::CREATURE_INVALID
    } else if reason.starts_with("cannot resolve _copy: ")
        && reason.contains("unsupported _mod mode")
    {
        code::COPY_MODE
    } else if reason.starts_with("cannot resolve _copy: ")
        && (reason.contains("not found") || reason.contains("circular"))
    {
        code::COPY_TARGET
    } else {
        ""
    }
}

/// Counts for one kind of entity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindCounts {
    /// Entries seen.
    pub total: usize,
    /// Entries that produced elements (including partial ones).
    pub converted: usize,
    /// Entries that produced elements but lost information.
    pub partial: usize,
    /// Entries that produced nothing.
    pub skipped: usize,
}

impl KindCounts {
    fn add(&mut self, other: Self) {
        self.total += other.total;
        self.converted += other.converted;
        self.partial += other.partial;
        self.skipped += other.skipped;
    }
}

/// Per-file counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReport {
    /// Path relative to the data folder.
    pub path: String,
    /// Counts by entity kind.
    pub kinds: BTreeMap<String, KindCounts>,
}

impl FileReport {
    /// Sum over all kinds.
    #[must_use]
    pub fn totals(&self) -> KindCounts {
        let mut sum = KindCounts::default();
        for c in self.kinds.values() {
            sum.add(*c);
        }
        sum
    }
}

/// Summary of one produced pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackReport {
    /// Source abbreviation.
    pub source: String,
    /// Pack id.
    pub pack_id: String,
    /// `dnd2014`, `dnd2024` or `mixed`.
    pub ruleset: String,
    /// Element counts by element type.
    pub elements: BTreeMap<String, usize>,
}

/// The full report of an import run. Serialises deterministically.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Counts per data file, sorted by path.
    pub files: Vec<FileReport>,
    /// Entities that produced elements (including partial ones).
    pub converted: usize,
    /// Entities converted with losses, with reasons.
    pub partial: Vec<ReportNote>,
    /// Entities that produced nothing, with reasons.
    pub skipped: Vec<ReportNote>,
    /// Run-level observations (unsupported `_mod` modes, unknown entry types, ...).
    pub warnings: Vec<String>,
    /// Element ids referenced by rules that no imported pack defines (core ids excluded).
    pub external_references: Vec<String>,
    /// One entry per produced pack.
    pub packs: Vec<PackReport>,
    /// Emitted rules the engine would ignore (`content.unknown_rule_kind` /
    /// `content.unknown_rule_field`): a converter bug, never expected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ReportNote>,
}

impl ImportReport {
    /// Total counts over all files.
    #[must_use]
    pub fn totals(&self) -> KindCounts {
        let mut sum = KindCounts::default();
        for f in &self.files {
            sum.add(f.totals());
        }
        sum
    }

    /// Pretty JSON, newline terminated.
    ///
    /// # Errors
    /// Serialisation errors (not expected for this type).
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

/// Accumulates report data while converting.
#[derive(Debug, Default)]
pub struct ReportBuilder {
    files: BTreeMap<String, BTreeMap<String, KindCounts>>,
    partial: Vec<ReportNote>,
    skipped: Vec<ReportNote>,
    warnings: Vec<String>,
}

impl ReportBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn counts(&mut self, file: &str, kind: &str) -> &mut KindCounts {
        self.files
            .entry(file.to_string())
            .or_default()
            .entry(kind.to_string())
            .or_default()
    }

    /// An entity converted without losses.
    pub fn converted(&mut self, file: &str, kind: &str) {
        let c = self.counts(file, kind);
        c.total += 1;
        c.converted += 1;
    }

    /// An entity converted with losses.
    pub fn partial(&mut self, file: &str, kind: &str, element: &str, reasons: &[String]) {
        let c = self.counts(file, kind);
        c.total += 1;
        c.converted += 1;
        c.partial += 1;
        for reason in reasons {
            self.partial.push(ReportNote {
                element: element.to_string(),
                kind: kind.to_string(),
                file: file.to_string(),
                reason: reason.clone(),
                code: reason_code(reason).to_string(),
            });
        }
    }

    /// An entity that produced nothing.
    pub fn skipped(&mut self, file: &str, kind: &str, element: &str, reason: &str) {
        let c = self.counts(file, kind);
        c.total += 1;
        c.skipped += 1;
        self.skipped.push(ReportNote {
            element: element.to_string(),
            kind: kind.to_string(),
            file: file.to_string(),
            reason: reason.to_string(),
            code: skip_code(reason).to_string(),
        });
    }

    /// A run-level observation (deduplicated at build time).
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    /// Finish into a deterministic report (packs and references are filled by the caller).
    #[must_use]
    pub fn build(self) -> ImportReport {
        let mut partial = self.partial;
        partial.sort();
        partial.dedup();
        let mut skipped = self.skipped;
        skipped.sort();
        skipped.dedup();
        let mut warnings = self.warnings;
        warnings.sort();
        warnings.dedup();
        let files: Vec<FileReport> = self
            .files
            .into_iter()
            .map(|(path, kinds)| FileReport { path, kinds })
            .collect();
        let converted = files.iter().map(|f| f.totals().converted).sum();
        ImportReport {
            files,
            converted,
            partial,
            skipped,
            warnings,
            external_references: Vec::new(),
            packs: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_sidekick_class_skip_carries_its_code() {
        assert_eq!(
            skip_code("sidekick class: it levels a companion creature, not a character"),
            code::SIDEKICK_CLASS
        );
        assert_eq!(skip_code("class has no hit die"), "");
    }

    #[test]
    fn every_creature_skip_and_loss_carries_a_code() {
        for (reason, want) in [
            (
                "creature stat block: hit points are not a number (half)",
                code::CREATURE_NOT_A_NUMBER,
            ),
            (
                "creature stat block: the str score is not a number",
                code::CREATURE_NOT_A_NUMBER,
            ),
            (
                "creature stat block: challenge rating \"Unknown\" is not a number",
                code::CREATURE_NOT_A_NUMBER,
            ),
            (
                "creature stat block: the creature type is a choice (x)",
                code::CREATURE_TYPE,
            ),
            (
                "creature stat block: creature type \"drow\" is not known",
                code::CREATURE_TYPE,
            ),
            (
                "creature stat block: stat block not valid: speed: no speed given",
                code::CREATURE_INVALID,
            ),
            (
                "cannot resolve _copy: unsupported _mod mode \"maxSize\"",
                code::COPY_MODE,
            ),
            (
                "cannot resolve _copy: replaceArr: target not found in action",
                code::COPY_TARGET,
            ),
        ] {
            assert_eq!(skip_code(reason), want, "{reason}");
        }
        assert_eq!(
            reason_code("creature section: variant sidebars not imported"),
            code::CREATURE_SECTION
        );
        assert_eq!(
            reason_code("creature field: alternate speeds not imported"),
            code::CREATURE_FIELD
        );
    }

    use super::*;

    #[test]
    fn counts_and_sorting_are_deterministic() {
        let mut b = ReportBuilder::new();
        b.converted("races.json", "race");
        b.partial(
            "races.json",
            "race",
            "Elf|PHB",
            &["b reason".into(), "a reason".into()],
        );
        b.skipped("spells/spells-x.json", "spell", "Zap|X", "bad shape");
        b.warn("w2");
        b.warn("w1");
        b.warn("w1");
        let r = b.build();
        assert_eq!(r.converted, 2);
        assert_eq!(r.files.len(), 2);
        assert_eq!(r.files[0].path, "races.json");
        assert_eq!(r.files[0].kinds["race"].partial, 1);
        assert_eq!(r.partial[0].reason, "a reason");
        assert_eq!(r.warnings, vec!["w1", "w2"]);
        assert_eq!(r.totals().total, 3);
        assert_eq!(r.totals().skipped, 1);
        let json = r.to_json_pretty().unwrap();
        assert!(json.ends_with("}\n"));
        let back: ImportReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
        assert_eq!(r.partial[0].code, code::UNCLASSIFIED);
        assert!(r.skipped[0].code.is_empty());
        assert!(
            !json.contains("\"code\": \"\""),
            "empty codes are not written"
        );
    }

    #[test]
    fn reason_codes_of_the_known_losses() {
        for (reason, expected) in [
            (
                "charges \"{@dice 1d3}\" were not converted",
                code::ROLLED_CHARGES,
            ),
            (
                "wish costs 1 charges but the item has no charge count",
                code::ROLLED_CHARGES,
            ),
            (
                "weapon proficiency \"firearms\" has no core element",
                code::NO_CORE_PROFICIENCY,
            ),
            (
                "tool proficiency \"vehicles (air)\" has no core element",
                code::NO_CORE_PROFICIENCY,
            ),
            (
                "weapon proficiency chosen by filter \"type=martial weapon\" not converted",
                code::EMPTY_CHOICE,
            ),
            (
                "a tool choice mixing categories and single tools was widened to the categories",
                code::MIXED_CHOICE,
            ),
            (
                "spell choice filter \"level=0\" matches no imported spell (nothing to offer)",
                code::NO_MATCHING_SPELL,
            ),
            (
                "pack content \"censer\" is not an item and was not converted",
                code::NOT_AN_ITEM,
            ),
            (
                "speed change multiply walk was not converted",
                code::SPEED_MULTIPLIER,
            ),
            (
                "speed bonus applied to the walking speed only",
                code::ALL_SPEEDS,
            ),
            (
                "the proficiencies the item grants were not found in its text",
                code::ITEM_PROFICIENCY,
            ),
            (
                "the doubled proficiency bonus of the chosen proficiency is not applied",
                code::ITEM_PROFICIENCY,
            ),
            (
                "prerequisite not enforced: Renown 3+",
                code::UNCHECKED_PREREQUISITE,
            ),
            (
                "progression key \"x\" was applied at level 1",
                code::UNKNOWN_PROGRESSION,
            ),
            ("something new", code::UNCLASSIFIED),
        ] {
            assert_eq!(reason_code(reason), expected, "{reason}");
        }
    }
}
