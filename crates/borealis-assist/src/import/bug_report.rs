//! Opt-in import bug reports (`borealis-import-bug-report/1`, decisions BR1-BR3).
//!
//! A report carries the STRUCTURE of an import (element types, ids, names, rules, selects,
//! grants, setters), the import report, error codes and engine diagnostics. Source-derived prose
//! never survives: description-like keys are dropped at every depth, prose longer than a short
//! label is replaced by a length marker, and long quoted passages in messages are cut. The source
//! text is attached only when the caller passes it (the user ticked the box, BR2), up to
//! [`MAX_SOURCE_BYTES`]. Nothing here sends anything.

use std::collections::BTreeMap;

use borealis_content::file::ContentFile;
use borealis_content::ContentStoreBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::anything::{finalize_draft, Edition, ElementStatus, ImportDraft, Role};
use super::report::{
    ImportCheckReport, ImportReport, ReportEntry, ReportIssue, ReviewReport, ReviewReportEntry,
};
use crate::error::AssistError;

/// Format tag of [`BugReport`].
pub const BUG_REPORT_FORMAT: &str = "borealis-import-bug-report/1";
/// Largest source text that may be attached.
pub const MAX_SOURCE_BYTES: usize = 1_000_000;
/// Longest user note kept.
pub const MAX_NOTE_CHARS: usize = 2000;
/// Prose (several words of text) longer than this is replaced by a length marker.
pub const LABEL_MAX_CHARS: usize = 40;
/// Ids, expressions and other non-prose strings longer than this are replaced too.
pub const TOKEN_MAX_CHARS: usize = 160;
/// Element names longer than this are cut.
pub const NAME_MAX_CHARS: usize = 120;
/// Report and diagnostic messages longer than this are cut.
pub const MESSAGE_MAX_CHARS: usize = 400;
/// Most list entries kept per list (elements, issues, diagnostics).
pub const MAX_ITEMS: usize = 2000;
const MAX_ISSUES: usize = 50;
const MAX_FLAGGED: usize = 200;
const MAX_SOURCE_WARNINGS: usize = 200;
const PATTERN_MAX_CHARS: usize = 240;
const MAX_DEPTH: usize = 10;

/// Element keys that carry structure; every other top-level key is dropped.
const STRUCTURE_KEYS: &[&str] = &[
    "id",
    "type",
    "name",
    "source",
    "ruleset",
    "level",
    "supports",
    "requirements",
    "setters",
    "rules",
    "spellcasting",
    "sheet",
    "multiclass",
    "extract",
    "maps_to",
    "renamed_from",
    "legacy_alias",
    "data",
    "profile_overrides",
];
/// Keys whose values are prose, dropped at any depth.
const PROSE_KEYS: &[&str] = &[
    "description",
    "descriptions",
    "text",
    "prerequisite",
    "summary",
    "flavor",
    "flavour",
    "html",
    "entries",
    "notes",
    "note",
    "details",
    "body",
    "content",
    "excerpt",
    "source_text",
    "brief",
    "quote",
    "lore",
];

/// Where the reporting app runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// App version.
    pub version: String,
    /// `std::env::consts::OS`.
    pub os: String,
    /// `std::env::consts::ARCH`.
    pub arch: String,
}

impl AppInfo {
    /// This build on this machine.
    #[must_use]
    pub fn current(version: &str) -> Self {
        Self {
            version: version.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

/// One engine finding (settle invariant, level sweep, store load).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Character level of the test character (0 = while loading), when known.
    pub level: Option<u32>,
    /// Element blamed, when known.
    pub element: Option<String>,
    /// What went wrong.
    pub message: String,
}

/// The source text, attached only on the user's explicit choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAttachment {
    /// File name or label.
    pub name: String,
    /// `text`, `pdf_text`, `attachments`, ...
    pub kind: String,
    /// Character count of `text`.
    pub chars: usize,
    /// The text.
    pub text: String,
}

/// What the user adds.
#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    /// Free text from the user (cut at [`MAX_NOTE_CHARS`]).
    pub note: String,
    /// Element ids the user marked as wrong.
    pub flagged: Vec<String>,
    /// Attach the source text (`None` unless the user ticked the box).
    pub source: Option<(String, String, String)>,
    /// The user's review decisions on the draft (accepted elements, text-only overrides).
    pub review: super::anything::review::ReviewOverrides,
}

/// One error class and normalised message pattern, for grouping reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCode {
    /// Category, for example `rule_error`, `unlocated`, `engine`, `invented_rule_kind`.
    pub code: String,
    /// The message with ids, numbers and quotes replaced by placeholders.
    pub pattern: String,
    /// How often it occurs in this report.
    pub count: u32,
}

/// One element's structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementStructure {
    /// Element id.
    pub id: String,
    /// Element type.
    #[serde(rename = "type")]
    pub ty: String,
    /// Display name (kept for triage).
    pub name: String,
    /// Draft status (`ok`, `repaired`, `needs_review`, `failed`), drafts only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Validation issues, drafts only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
    /// Length of the removed description.
    pub description_chars: usize,
    /// Structural keys with prose removed.
    pub structure: Value,
}

/// The payload sent to the report endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BugReport {
    /// Always [`BUG_REPORT_FORMAT`].
    pub format: String,
    /// App and OS.
    pub app: AppInfo,
    /// `dnd2014` / `dnd2024`, when known.
    pub edition: Option<String>,
    /// `draft` (before adding to the library) or `pack` (an installed import).
    pub origin: String,
    /// Counts.
    pub summary: BTreeMap<String, usize>,
    /// Grouped error codes.
    pub codes: Vec<ErrorCode>,
    /// The import report, messages cut.
    pub report: ImportReport,
    /// Engine findings.
    pub diagnostics: Vec<Diagnostic>,
    /// Element structure.
    pub elements: Vec<ElementStructure>,
    /// Element ids the user marked as wrong.
    pub flagged: Vec<String>,
    /// The user's note.
    pub note: String,
    /// The source, only when the user chose to include it.
    pub source: Option<SourceAttachment>,
}

/// A report for an installed import pack: its `report.json`, `content.json` and the engine
/// findings of a check run by the caller.
///
/// # Errors
/// [`AssistError::Invalid`] when the attached source exceeds [`MAX_SOURCE_BYTES`].
pub fn bug_report_from_pack(
    app: AppInfo,
    edition: Option<String>,
    report: &ImportReport,
    content: &ContentFile,
    diagnostics: Vec<Diagnostic>,
    options: ReportOptions,
) -> Result<BugReport, AssistError> {
    let elements = content
        .elements
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|e| serde_json::to_value(e).ok())
        .map(|v| structure_of(&v, None, &[]))
        .collect();
    assemble(app, edition, "pack", report, elements, diagnostics, options)
}

/// A report for a draft under review: the exact import report when the draft can be finalized,
/// else one derived from the draft; the level-sweep issues as diagnostics.
///
/// # Errors
/// [`AssistError::Invalid`] when the attached source exceeds [`MAX_SOURCE_BYTES`].
pub fn bug_report_from_draft(
    app: AppInfo,
    base: &ContentStoreBuilder,
    draft: &ImportDraft,
    options: ReportOptions,
) -> Result<BugReport, AssistError> {
    let report = finalize_draft(base, draft)
        .ok()
        .and_then(|f| f.pack.report)
        .unwrap_or_else(|| draft_report(draft));
    let report = ImportReport {
        review: Some(super::anything::review::review_report(
            draft,
            &options.review,
            &[],
        )),
        ..report
    };
    let elements = draft
        .elements
        .iter()
        .take(MAX_ITEMS)
        .map(|e| {
            let mut s = structure_of(&e.json, Some(status_name(e.status)), &e.issues);
            if s.id.is_empty() {
                s.id = cut(&e.id, TOKEN_MAX_CHARS);
            }
            if s.ty.is_empty() {
                s.ty = cut(&e.ty, 64);
            }
            if s.name.is_empty() {
                s.name = cut(&e.name, NAME_MAX_CHARS);
            }
            s
        })
        .collect();
    let diagnostics = draft
        .sweep_issues
        .iter()
        .map(|i| Diagnostic {
            level: Some(i.level),
            element: i.element_id.clone(),
            message: i.message.clone(),
        })
        .collect();
    let edition = Some(
        match draft.edition {
            Edition::Y2014 => "dnd2014",
            Edition::Y2024 => "dnd2024",
        }
        .to_string(),
    );
    assemble(
        app,
        edition,
        "draft",
        &report,
        elements,
        diagnostics,
        options,
    )
}

fn status_name(s: ElementStatus) -> &'static str {
    match s {
        ElementStatus::Pending => "pending",
        ElementStatus::Ok => "ok",
        ElementStatus::Repaired => "repaired",
        ElementStatus::NeedsReview => "needs_review",
        ElementStatus::Failed => "failed",
    }
}

/// The report a draft that cannot be finalized yet would get: rule errors, fixes, unsupported
/// content, sections not located in the source and reading notes.
fn draft_report(draft: &ImportDraft) -> ImportReport {
    let mut r = ImportReport::default();
    for e in &draft.elements {
        let issue = |m: &String| ReportIssue {
            element: e.id.clone(),
            message: m.clone(),
        };
        r.errors.extend(e.rule_errors.iter().map(issue));
        r.normalised.extend(e.normalised.iter().map(issue));
    }
    let planned = |name: &str| {
        let n = name.trim().to_lowercase();
        draft
            .plan
            .groups
            .iter()
            .any(|g| g.name.trim().to_lowercase() == n)
    };
    r.entries.extend(
        draft
            .detection
            .other
            .iter()
            .filter(|o| !planned(&o.name))
            .map(|o| ReportEntry {
                source: o.name.clone(),
                outcome: "unsupported".into(),
                elements: Vec::new(),
                note: format!("no element kind for {}", o.what),
            }),
    );
    r.warnings.extend(
        draft
            .plan
            .elements
            .iter()
            .filter(|p| p.excerpt.is_none() && p.role != Role::SubclassGate)
            .map(|p| ReportIssue {
                element: p.id.clone(),
                message: "the section was not located in the source".into(),
            }),
    );
    r.source_warnings
        .extend(draft.detection.notes.iter().cloned());
    r
}

fn assemble(
    app: AppInfo,
    edition: Option<String>,
    origin: &str,
    report: &ImportReport,
    elements: Vec<ElementStructure>,
    diagnostics: Vec<Diagnostic>,
    options: ReportOptions,
) -> Result<BugReport, AssistError> {
    let report = clean_report(report);
    let diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .take(MAX_ITEMS)
        .map(|d| Diagnostic {
            level: d.level,
            element: d.element.map(|e| cut(&e, TOKEN_MAX_CHARS)),
            message: clean_message(&d.message),
        })
        .collect();
    let flagged: Vec<String> = options
        .flagged
        .iter()
        .take(MAX_FLAGGED)
        .map(|f| cut(f, TOKEN_MAX_CHARS))
        .collect();
    let source = match options.source {
        Some((_, _, text)) if text.len() > MAX_SOURCE_BYTES => {
            return Err(AssistError::Invalid(format!(
                "the source is too large to attach ({} bytes, limit {MAX_SOURCE_BYTES})",
                text.len()
            )))
        }
        Some((name, kind, text)) => Some(SourceAttachment {
            name: cut(&name, 200),
            kind: cut(&kind, 32),
            chars: text.chars().count(),
            text,
        }),
        None => None,
    };
    let codes = error_codes(&report, &diagnostics, &elements, &flagged);
    let mut summary = BTreeMap::new();
    summary.insert("elements".into(), elements.len());
    summary.insert("errors".into(), report.errors.len());
    summary.insert("warnings".into(), report.warnings.len());
    summary.insert("source_warnings".into(), report.source_warnings.len());
    summary.insert("unlocated".into(), report.warnings.len());
    let unsupported = report.entries.iter().filter(|e| e.outcome == "unsupported");
    summary.insert("unsupported".into(), unsupported.count());
    summary.insert("diagnostics".into(), diagnostics.len());
    summary.insert("flagged".into(), flagged.len());
    Ok(BugReport {
        format: BUG_REPORT_FORMAT.to_string(),
        app: AppInfo {
            version: cut(&app.version, 32),
            os: cut(&app.os, 32),
            arch: cut(&app.arch, 32),
        },
        edition: edition.map(|e| cut(&e, 16)),
        origin: origin.to_string(),
        summary,
        codes,
        report,
        diagnostics,
        elements,
        flagged,
        note: options.note.trim().chars().take(MAX_NOTE_CHARS).collect(),
        source,
    })
}

/// True when the report shows a failed self-check (BR1): errors, unlocated sections,
/// unsupported content, engine findings or invented rule kinds.
#[must_use]
pub fn self_checks_failed(report: &BugReport) -> bool {
    report
        .codes
        .iter()
        .any(|c| c.code != "normalised" && c.code != "user_flagged")
}

fn clean_report(r: &ImportReport) -> ImportReport {
    let issue = |i: &ReportIssue| ReportIssue {
        element: cut(&i.element, TOKEN_MAX_CHARS),
        message: clean_message(&i.message),
    };
    let issues = |v: &[ReportIssue]| v.iter().take(MAX_ITEMS).map(issue).collect();
    ImportReport {
        format: r.format.clone(),
        entries: r
            .entries
            .iter()
            .take(MAX_ITEMS)
            .map(|e| ReportEntry {
                source: cut(&e.source, NAME_MAX_CHARS),
                outcome: cut(&e.outcome, 32),
                elements: e
                    .elements
                    .iter()
                    .take(50)
                    .map(|x| cut(x, TOKEN_MAX_CHARS))
                    .collect(),
                note: clean_message(&e.note),
            })
            .collect(),
        errors: issues(&r.errors),
        normalised: issues(&r.normalised),
        warnings: issues(&r.warnings),
        source_warnings: r
            .source_warnings
            .iter()
            .take(MAX_SOURCE_WARNINGS)
            .map(|s| clean_message(s))
            .collect(),
        review: r.review.as_ref().map(|v| ReviewReport {
            elements: v
                .elements
                .iter()
                .take(MAX_ITEMS)
                .map(|e| ReviewReportEntry {
                    element: cut(&e.element, TOKEN_MAX_CHARS),
                    state: cut(&e.state, 32),
                    code: cut(&e.code, 64),
                    ..e.clone()
                })
                .collect(),
            import_checks: v
                .import_checks
                .iter()
                .take(MAX_ITEMS)
                .map(|c| ImportCheckReport {
                    code: cut(&c.code, 64),
                    count: c.count,
                })
                .collect(),
            ..v.clone()
        }),
    }
}

fn structure_of(json: &Value, status: Option<&str>, issues: &[String]) -> ElementStructure {
    let field = |k: &str| json.get(k).and_then(Value::as_str).unwrap_or_default();
    let mut kept = Map::new();
    if let Some(obj) = json.as_object() {
        for key in STRUCTURE_KEYS {
            if matches!(*key, "id" | "type" | "name") {
                continue;
            }
            if let Some(v) = obj.get(*key) {
                kept.insert((*key).to_string(), scrub(v, 0));
            }
        }
    }
    ElementStructure {
        id: cut(field("id"), TOKEN_MAX_CHARS),
        ty: cut(field("type"), 64),
        name: cut(field("name"), NAME_MAX_CHARS),
        status: status.map(str::to_string),
        issues: issues
            .iter()
            .take(MAX_ISSUES)
            .map(|i| clean_message(i))
            .collect(),
        description_chars: field("description").chars().count(),
        structure: Value::Object(kept),
    }
}

/// Drop prose keys and replace prose strings longer than a label, recursively.
fn scrub(v: &Value, depth: usize) -> Value {
    if depth >= MAX_DEPTH {
        return Value::String("[nested]".into());
    }
    match v {
        Value::String(s) => Value::String(scrub_str(s)),
        Value::Array(a) => Value::Array(
            a.iter()
                .take(MAX_ITEMS)
                .map(|x| scrub(x, depth + 1))
                .collect(),
        ),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| !PROSE_KEYS.contains(&k.to_ascii_lowercase().as_str()))
                .map(|(k, x)| (cut(k, 64), scrub(x, depth + 1)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn scrub_str(s: &str) -> String {
    let n = s.chars().count();
    if (is_prose(s) && n > LABEL_MAX_CHARS) || n > TOKEN_MAX_CHARS {
        format!("[{n} chars removed]")
    } else {
        s.to_string()
    }
}

/// Several words, most of them plain words: a sentence rather than an id or expression.
fn is_prose(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    let plain = words
        .iter()
        .filter(|w| {
            let w = w.trim_end_matches(['.', ',', ';', ':', '!', '?', ')']);
            let w = w.trim_start_matches('(');
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_alphabetic() || c == '\'' || c == '-')
        })
        .count();
    plain * 10 >= words.len() * 6
}

/// A message cut to [`MESSAGE_MAX_CHARS`] with long quoted passages replaced.
fn clean_message(m: &str) -> String {
    let mut out = String::new();
    let mut rest = m;
    while let Some(start) = rest.find(['"', '\u{201c}']) {
        let quote_len = rest[start..].chars().next().map_or(1, char::len_utf8);
        let after = &rest[start + quote_len..];
        let Some(end) = after.find(['"', '\u{201d}']) else {
            break;
        };
        let inner = &after[..end];
        out.push_str(&rest[..start]);
        if inner.chars().count() > LABEL_MAX_CHARS {
            out.push_str(&format!("\"[{} chars removed]\"", inner.chars().count()));
        } else {
            out.push('"');
            out.push_str(inner);
            out.push('"');
        }
        let close_len = after[end..].chars().next().map_or(1, char::len_utf8);
        rest = &after[end + close_len..];
    }
    out.push_str(rest);
    let out = if is_prose(&out) && out.chars().count() > MESSAGE_MAX_CHARS {
        format!("[{} chars removed]", out.chars().count())
    } else {
        out
    };
    cut(&out, MESSAGE_MAX_CHARS)
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('\u{2026}');
    t
}

/// The message with quotes, ids and numbers replaced by placeholders.
fn pattern(message: &str) -> String {
    let mut out = String::new();
    let mut in_quote = false;
    for word in message.split_whitespace() {
        if in_quote {
            if word.ends_with('"') {
                in_quote = false;
            }
            continue;
        }
        let w = if word.starts_with('"') {
            in_quote = !(word.len() > 1 && word.ends_with('"'));
            "<q>".to_string()
        } else if word.contains('_') && word.chars().any(|c| c.is_ascii_uppercase()) {
            "<id>".to_string()
        } else if word.chars().any(|c| c.is_ascii_digit()) {
            "<n>".to_string()
        } else {
            word.to_string()
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&w);
    }
    cut(&out, PATTERN_MAX_CHARS)
}

fn error_codes(
    report: &ImportReport,
    diagnostics: &[Diagnostic],
    elements: &[ElementStructure],
    flagged: &[String],
) -> Vec<ErrorCode> {
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut add = |code: &str, p: String| *counts.entry((code.to_string(), p)).or_default() += 1;
    report
        .errors
        .iter()
        .for_each(|i| add("rule_error", pattern(&i.message)));
    report
        .normalised
        .iter()
        .for_each(|i| add("normalised", pattern(&i.message)));
    report
        .warnings
        .iter()
        .for_each(|i| add("unlocated", pattern(&i.message)));
    report
        .source_warnings
        .iter()
        .for_each(|s| add("source_warning", pattern(s)));
    for e in &report.entries {
        if matches!(
            e.outcome.as_str(),
            "unsupported" | "skipped" | "dropped" | "unchecked"
        ) {
            add(&e.outcome, pattern(&e.note));
        }
    }
    diagnostics
        .iter()
        .for_each(|d| add("engine", pattern(&d.message)));
    for e in elements {
        match e.status.as_deref() {
            Some("failed") => add("element_failed", e.ty.clone()),
            Some("needs_review") => add("needs_review", e.ty.clone()),
            _ => {}
        }
        e.issues
            .iter()
            .for_each(|i| add("element_issue", pattern(i)));
        for kind in invented_kinds(&e.structure) {
            add("invented_rule_kind", kind);
        }
        if flagged.contains(&e.id) {
            add("user_flagged", e.ty.clone());
        }
    }
    counts
        .into_iter()
        .take(500)
        .map(|((code, pattern), count)| ErrorCode {
            code,
            pattern,
            count,
        })
        .collect()
}

/// Rule kinds outside grant/select/stat.
fn invented_kinds(structure: &Value) -> Vec<String> {
    structure
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("kind").and_then(Value::as_str))
        .filter(|k| !matches!(*k, "grant" | "select" | "stat"))
        .map(|k| cut(k, 64))
        .collect()
}

#[cfg(test)]
#[path = "bug_report_tests.rs"]
mod tests;
