//! The migration report: counts, warnings and unsupported constructs.

use borealis_content::{count_severity, ContentFile, Issue, Severity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Counts gathered from converted files. All maps are ordered so output is deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    /// Converted elements.
    pub elements: usize,
    /// `<append>` records.
    pub appends: usize,
    /// Setters on elements, multiclass nodes and appends (rule setters are not counted).
    pub setters: usize,
    /// Rules on elements, multiclass nodes and appends.
    pub rules: usize,
    /// Rules per kind (`grant`, `select`, `stat`, `unknown`, `custom`).
    pub rules_by_kind: BTreeMap<String, usize>,
    /// Elements per type string.
    pub elements_by_type: BTreeMap<String, usize>,
    /// Elements dropped because of errors.
    pub dropped_elements: usize,
    /// Rules dropped because a required attribute was missing (their element is kept).
    #[serde(default)]
    pub dropped_rules: usize,
    /// Legacy spellings normalised (`grant@name`, `statistic`, `cap`, ...).
    pub normalized: BTreeMap<String, usize>,
    /// Unknown child nodes preserved in `extra.children`, keyed `owner/tag`.
    pub unknown_children: BTreeMap<String, usize>,
    /// Unknown attributes preserved in `extra.attributes`, keyed `owner@attribute`.
    pub unknown_attributes: BTreeMap<String, usize>,
}

fn add_all(into: &mut BTreeMap<String, usize>, from: &BTreeMap<String, usize>) {
    for (k, v) in from {
        *into.entry(k.clone()).or_default() += v;
    }
}

impl Stats {
    /// Add the structural counts of a converted file.
    pub fn count_file(&mut self, file: &ContentFile) {
        self.elements += file.elements.len();
        self.appends += file.appends.len();
        for e in &file.elements {
            *self.elements_by_type.entry(e.ty.clone()).or_default() += 1;
            self.setters += e.setters.len();
            for r in e.all_rules() {
                self.rules += 1;
                *self
                    .rules_by_kind
                    .entry(r.kind().as_str().to_string())
                    .or_default() += 1;
            }
            if let Some(m) = &e.multiclass {
                self.setters += m.setters.len();
            }
        }
        for a in &file.appends {
            self.setters += a.setters.len();
            for r in &a.rules {
                self.rules += 1;
                *self
                    .rules_by_kind
                    .entry(r.kind().as_str().to_string())
                    .or_default() += 1;
            }
        }
    }

    /// Add another set of counts.
    pub fn merge(&mut self, other: &Stats) {
        self.elements += other.elements;
        self.appends += other.appends;
        self.setters += other.setters;
        self.rules += other.rules;
        self.dropped_elements += other.dropped_elements;
        self.dropped_rules += other.dropped_rules;
        add_all(&mut self.rules_by_kind, &other.rules_by_kind);
        add_all(&mut self.elements_by_type, &other.elements_by_type);
        add_all(&mut self.normalized, &other.normalized);
        add_all(&mut self.unknown_children, &other.unknown_children);
        add_all(&mut self.unknown_attributes, &other.unknown_attributes);
    }
}

/// What happened to one input file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReport {
    /// Input path relative to the input root.
    pub input: String,
    /// Output path relative to the output directory (absent when nothing was written).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// `content`, `index` or `skipped`.
    pub kind: String,
    /// Counts for this file.
    pub stats: Stats,
}

/// Totals for the whole run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Files read.
    pub files_read: usize,
    /// Content files converted.
    pub content_files: usize,
    /// Index files converted.
    pub index_files: usize,
    /// Files skipped.
    pub skipped_files: usize,
    /// Error count.
    pub errors: usize,
    /// Warning count.
    pub warnings: usize,
    /// Info count.
    pub infos: usize,
}

/// The full report written by `--report`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    /// Always `borealis-migrate`.
    pub tool: String,
    /// Tool version.
    pub version: String,
    /// Run totals.
    pub summary: Summary,
    /// Aggregate counts.
    pub totals: Stats,
    /// Per-file results in processing order.
    pub files: Vec<FileReport>,
    /// Every issue in processing order.
    pub issues: Vec<Issue>,
    /// Source element type by id, for every `<element>` read (converted or dropped; a renamed
    /// element under both ids). Not part of the `--report` JSON.
    #[serde(skip)]
    pub element_types: BTreeMap<String, String>,
    /// Ids that had to be renamed (source id -> new id); the source id stays an alias.
    #[serde(skip)]
    pub renamed: BTreeMap<String, String>,
}

impl MigrationReport {
    /// Assemble a report from per-file results and issues.
    #[must_use]
    pub fn new(files: Vec<FileReport>, issues: Vec<Issue>) -> Self {
        let mut totals = Stats::default();
        for f in &files {
            totals.merge(&f.stats);
        }
        let count_kind = |k: &str| files.iter().filter(|f| f.kind == k).count();
        let summary = Summary {
            files_read: files.len(),
            content_files: count_kind("content"),
            index_files: count_kind("index"),
            skipped_files: count_kind("skipped"),
            errors: count_severity(&issues, Severity::Error),
            warnings: count_severity(&issues, Severity::Warning),
            infos: count_severity(&issues, Severity::Info),
        };
        Self {
            tool: "borealis-migrate".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            summary,
            totals,
            files,
            issues,
            element_types: BTreeMap::new(),
            renamed: BTreeMap::new(),
        }
    }

    /// Attach the element types and renames gathered during the run.
    #[must_use]
    pub fn with_elements(
        mut self,
        types: BTreeMap<String, String>,
        renamed: BTreeMap<String, String>,
    ) -> Self {
        self.element_types = types;
        self.renamed = renamed;
        self
    }

    /// The source type of element `id` (`None` when no `<element>` with that id was read).
    #[must_use]
    pub fn element_type(&self, id: &str) -> Option<&str> {
        self.element_types.get(id).map(String::as_str)
    }

    /// True when the run is clean: no errors, and (if `strict`) no warnings either.
    #[must_use]
    pub fn passes(&self, strict: bool) -> bool {
        self.summary.errors == 0 && (!strict || self.summary.warnings == 0)
    }

    /// Serialize as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// Serialization failure (not expected).
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{Element, IssueCode, Rule, StatRule};

    #[test]
    fn counts_files_and_merges() {
        let mut e = Element::new("ID_A", "A", "Class", "Core");
        e.rules.push(Rule::Stat(StatRule::default()));
        let mut stats = Stats::default();
        stats.count_file(&ContentFile::new(vec![e]));
        assert_eq!(
            (stats.elements, stats.rules, stats.rules_by_kind["stat"]),
            (1, 1, 1)
        );
        let mut total = Stats::default();
        total.merge(&stats);
        total.merge(&stats);
        assert_eq!(total.elements_by_type["Class"], 2);
    }

    #[test]
    fn summary_and_strictness() {
        let files = vec![FileReport {
            input: "a.xml".into(),
            output: Some("a.json".into()),
            kind: "content".into(),
            stats: Stats::default(),
        }];
        let report = MigrationReport::new(files, vec![Issue::warning(IssueCode::Normalized, "w")]);
        assert_eq!(report.summary.warnings, 1);
        assert!(report.passes(false));
        assert!(!report.passes(true));
        assert!(report.to_json_pretty().unwrap().ends_with("}\n"));
    }
}
