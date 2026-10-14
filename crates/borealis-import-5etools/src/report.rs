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
    }
}
