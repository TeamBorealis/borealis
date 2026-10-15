//! One review state per draft element: everything the pipeline knows about an element (status,
//! issues, rule errors, source checks, sweep issues, fidelity, unmodelled mechanics, balance,
//! model notes) collapsed into `ok`, `check` or `not_imported`, with the most important reason
//! in one line. Only `check` elements need the user's eyes; the rest of what we know stays in
//! `details`.
//!
//! The user's own decisions ([`ReviewOverrides`]) are kept beside the draft: an element marked
//! as fine stays `ok` until its JSON changes, and a text-only override says whether the sheet is
//! expected to compute an element's mechanics.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::{DraftElement, ElementStatus, ImportDraft, Role};
use crate::import::report::{ReviewReport, ReviewReportEntry};

/// The three states the review screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLevel {
    /// Nothing the user needs to look at.
    Ok,
    /// Something is probably wrong or missing; `reason` says what.
    Check,
    /// No usable element: it will not be in the pack.
    NotImported,
}

/// Stable codes of the leading reason (no source text; the bug report carries these).
pub mod code {
    /// Nothing to check.
    pub const OK: &str = "ok";
    /// The user marked it as fine.
    pub const ACCEPTED: &str = "accepted";
    /// Not generated yet.
    pub const PENDING: &str = "pending";
    /// Generation or every repair failed; no output.
    pub const FAILED: &str = "failed";
    /// The rules engine rejects it (an unresolved sweep issue or validation error).
    pub const ENGINE: &str = "engine";
    /// A source-derived acceptance check failed (`check:<check code>`).
    pub const CHECK_PREFIX: &str = "check:";
    /// Rules were removed because the engine would ignore them.
    pub const RULE_REMOVED: &str = "rule_removed";
    /// The source states a mechanic the element lost entirely.
    pub const MECHANIC_LOST: &str = "mechanic_lost";
    /// Description sentences the source does not have.
    pub const UNTRACED: &str = "untraced";
    /// A species with no speed: its passage was most likely cut short.
    pub const INCOMPLETE: &str = "incomplete";
    /// Its source passage was never found.
    pub const UNLOCATED: &str = "unlocated";
}

/// What the review screen shows for one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewState {
    /// The state.
    pub state: ReviewLevel,
    /// Machine code of the leading reason (see [`code`]).
    pub code: String,
    /// One line in plain words (empty when `ok` with nothing to say).
    pub reason: String,
    /// Everything else we know, most important first (model notes, normalisations, balance).
    pub details: Vec<String>,
    /// The sheet will not compute some of this element's mechanics.
    pub text_only: bool,
    /// Why it is text only (empty when not).
    pub text_only_reason: String,
    /// The user marked it as fine (and it has not changed since).
    pub accepted: bool,
    /// The user's text-only override, when set.
    pub text_only_override: Option<bool>,
}

/// The user's review decisions for one draft (kept beside it; written into the pack's report).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewOverrides {
    /// Element id -> fingerprint of the JSON the user accepted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accepted: BTreeMap<String, String>,
    /// Element id -> "the sheet does not compute this" (true) or "it does" (false).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub text_only: BTreeMap<String, bool>,
}

impl ReviewOverrides {
    /// Mark `e` as fine as it is now (`false` clears the mark).
    pub fn set_accepted(&mut self, e: &DraftElement, accepted: bool) {
        if accepted {
            self.accepted.insert(e.id.clone(), fingerprint(&e.json));
        } else {
            self.accepted.remove(&e.id);
        }
    }

    /// Set or clear (`None`) the text-only override of `id`.
    pub fn set_text_only(&mut self, id: &str, value: Option<bool>) {
        match value {
            Some(v) => self.text_only.insert(id.to_string(), v),
            None => self.text_only.remove(id),
        };
    }

    /// Whether the acceptance of `e` still holds (its JSON is unchanged).
    #[must_use]
    pub fn is_accepted(&self, e: &DraftElement) -> bool {
        self.accepted
            .get(&e.id)
            .is_some_and(|f| *f == fingerprint(&e.json))
    }

    /// Drop overrides of elements no longer in `draft` and acceptances whose element changed.
    pub fn prune(&mut self, draft: &ImportDraft) {
        self.accepted.retain(|id, f| {
            draft
                .element(id)
                .is_some_and(|e| *f == fingerprint(&e.json))
        });
        self.text_only.retain(|id, _| draft.element(id).is_some());
    }
}

/// FNV-1a over the compact JSON: stable across runs and platforms.
fn fingerprint(v: &Value) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in v.to_string().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Per-draft findings computed once (fidelity needs an index of the whole source).
struct DraftFindings {
    untraced: HashMap<String, Vec<String>>,
}

impl DraftFindings {
    fn of(draft: &ImportDraft) -> Self {
        Self {
            untraced: super::fidelity::findings(draft).into_iter().collect(),
        }
    }
}

/// The review state of `el` in `draft`, without user overrides.
#[must_use]
pub fn review_state(el: &DraftElement, draft: &ImportDraft) -> ReviewState {
    state_of(
        el,
        draft,
        &DraftFindings::of(draft),
        &ReviewOverrides::default(),
    )
}

/// The review state of every element of `draft`, in draft order, with the user's overrides.
#[must_use]
pub fn review_all(draft: &ImportDraft, overrides: &ReviewOverrides) -> Vec<ReviewState> {
    let found = DraftFindings::of(draft);
    draft
        .elements
        .iter()
        .map(|e| state_of(e, draft, &found, overrides))
        .collect()
}

/// The element's source passage: `(start line, end line, text)`, 1-based and inclusive.
#[must_use]
pub fn source_passage(el: &DraftElement, draft: &ImportDraft) -> Option<(usize, usize, String)> {
    let ex = el.excerpt.as_ref()?;
    let lines: Vec<&str> = draft.source_text.lines().collect();
    let start = ex.start_line.max(1);
    let end = ex.end_line.min(lines.len());
    (start <= end).then(|| (start, end, lines[start - 1..end].join("\n")))
}

/// One leading reason.
struct Reason {
    code: String,
    line: String,
}

fn reason(code: &str, line: impl Into<String>) -> Reason {
    Reason {
        code: code.to_string(),
        line: line.into(),
    }
}

/// Longest quote of source or element text inside a reason line.
const REASON_QUOTE_CHARS: usize = 140;

fn short(s: &str) -> String {
    let mut out: String = s.chars().take(REASON_QUOTE_CHARS).collect();
    if s.chars().count() > REASON_QUOTE_CHARS {
        out.push_str("...");
    }
    out
}

fn sentence_case(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Issues the pipeline itself writes (not model notes), by prefix.
const PIPELINE_PREFIXES: [&str; 5] = [
    "generation failed",
    "missing element",
    "left as text after a repair",
    "description not traced",
    "level ",
];

fn is_pipeline_issue(i: &str) -> bool {
    PIPELINE_PREFIXES.iter().any(|p| i.starts_with(p))
}

/// The reasons that make `el` worth a look, most important first.
fn check_reasons(el: &DraftElement, draft: &ImportDraft, found: &DraftFindings) -> Vec<Reason> {
    let mut out = Vec::new();
    if el.status == ElementStatus::Failed {
        let last = el.issues.last().map_or("", String::as_str);
        out.push(reason(
            code::ENGINE,
            format!("The rules engine rejects it: {}", short(last)),
        ));
    }
    for c in el.checks.iter().filter(|c| c.failed) {
        out.push(reason(
            &format!("{}{}", code::CHECK_PREFIX, c.code),
            sentence_case(&c.message),
        ));
    }
    for i in draft
        .sweep_issues
        .iter()
        .filter(|i| i.element_id.as_deref() == Some(el.id.as_str()))
        .filter(|_| el.status != ElementStatus::Failed)
    {
        out.push(reason(
            code::ENGINE,
            format!("The rules engine found a problem: {}", short(&i.message)),
        ));
    }
    if let Some(first) = el.rule_errors.first() {
        out.push(reason(
            code::RULE_REMOVED,
            format!(
                "{} rule(s) were removed because the app can't use them: {}",
                el.rule_errors.len(),
                short(first)
            ),
        ));
    }
    if super::mechanics::species_without_speed(draft, el) {
        out.push(reason(
            code::INCOMPLETE,
            "It has no speed: its source passage was probably cut short. Point to the full passage or add the missing traits.",
        ));
    }
    let passage = source_passage(el, draft).map(|(_, _, t)| t);
    for gap in super::mechanics::element_mechanics(draft, el, passage.as_deref()).gaps {
        out.push(reason(code::MECHANIC_LOST, sentence_case(&short(&gap))));
    }
    if let Some(found) = found.untraced.get(&el.id) {
        out.push(reason(
            code::UNTRACED,
            format!(
                "{} description sentence(s) are not in the source, e.g. \"{}\"",
                found.len(),
                short(found.first().map_or("", String::as_str))
            ),
        ));
    }
    if let Some(msg) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == el.id)
        .and_then(|p| super::batch::unlocated_warning(&draft.source_text, p))
    {
        out.push(reason(code::UNLOCATED, sentence_case(&short(&msg))));
    }
    out
}

/// Informational lines: model notes, deterministic fixes, balance.
fn info_details(el: &DraftElement) -> Vec<String> {
    let mut out: Vec<String> = el
        .issues
        .iter()
        .filter(|i| !(el.status == ElementStatus::Failed && is_pipeline_issue(i)))
        .cloned()
        .collect();
    out.extend(el.rule_errors.iter().skip(1).cloned());
    out.extend(
        el.normalised
            .iter()
            .map(|n| format!("Fixed automatically: {n}")),
    );
    if let Some(b) = &el.balance {
        out.extend(
            b.flags
                .iter()
                .filter(|f| f.severity != crate::balance::Severity::Ok)
                .map(|f| format!("Balance: {} ({})", f.note, f.metric)),
        );
    }
    out
}

fn text_only_of(el: &DraftElement, draft: &ImportDraft) -> Option<String> {
    if el.json.is_null() {
        return None;
    }
    let found = super::mechanics::element_mechanics(draft, el, None).text_only;
    if let Some(first) = found.first() {
        return Some(sentence_case(&short(first)));
    }
    let new_property =
        el.ty == "Weapon Property" && super::items::known_property(&el.name).is_none();
    new_property.then(|| "A new weapon property: the app applies no rules for it.".to_string())
}

fn state_of(
    el: &DraftElement,
    draft: &ImportDraft,
    found: &DraftFindings,
    overrides: &ReviewOverrides,
) -> ReviewState {
    let auto_text_only = text_only_of(el, draft);
    let text_only_override = overrides.text_only.get(&el.id).copied();
    let text_only = text_only_override.unwrap_or(auto_text_only.is_some());
    let text_only_reason = match (text_only, &auto_text_only, text_only_override) {
        (false, _, _) => String::new(),
        (true, Some(r), _) => r.clone(),
        (true, None, _) => "You marked this as text only: the sheet won't do its math.".into(),
    };
    let mut details = info_details(el);
    let base = ReviewState {
        state: ReviewLevel::Ok,
        code: code::OK.into(),
        reason: String::new(),
        details: Vec::new(),
        text_only,
        text_only_reason,
        accepted: false,
        text_only_override,
    };
    if el.json.is_null() {
        let pending = el.status == ElementStatus::Pending;
        let first = el
            .issues
            .first()
            .map_or("no usable element was generated", String::as_str);
        return ReviewState {
            state: ReviewLevel::NotImported,
            code: if pending { code::PENDING } else { code::FAILED }.into(),
            reason: if pending {
                "Not generated yet.".into()
            } else {
                format!("Not imported: {}", short(first))
            },
            details,
            ..base
        };
    }
    let mut reasons = check_reasons(el, draft, found);
    // A gap the user says the sheet is not meant to compute is no longer a gap.
    if text_only_override.is_some() {
        reasons.retain(|r| r.code != code::MECHANIC_LOST);
    }
    if reasons.is_empty() {
        return ReviewState { details, ..base };
    }
    let lead = reasons.remove(0);
    let rest = reasons.into_iter().map(|r| r.line);
    details = rest.chain(details).collect();
    if overrides.is_accepted(el) && el.role != Role::SubclassGate {
        details.insert(0, lead.line);
        return ReviewState {
            code: code::ACCEPTED.into(),
            reason: "You checked this and marked it as fine.".into(),
            accepted: true,
            details,
            ..base
        };
    }
    ReviewState {
        state: ReviewLevel::Check,
        code: lead.code,
        reason: lead.line,
        details,
        ..base
    }
}

/// Counts of each state, for the review header and the reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCounts {
    /// `ok` elements.
    pub ok: usize,
    /// `check` elements.
    pub check: usize,
    /// `not_imported` elements.
    pub not_imported: usize,
    /// Elements whose mechanics the sheet will not (fully) compute.
    pub text_only: usize,
    /// Elements the user marked as fine.
    pub accepted: usize,
}

impl ReviewCounts {
    /// Counts of `states`.
    #[must_use]
    pub fn of(states: &[ReviewState]) -> Self {
        let mut c = Self::default();
        for s in states {
            match s.state {
                ReviewLevel::Ok => c.ok += 1,
                ReviewLevel::Check => c.check += 1,
                ReviewLevel::NotImported => c.not_imported += 1,
            }
            c.text_only += usize::from(s.text_only);
            c.accepted += usize::from(s.accepted);
        }
        c
    }
}

/// The review summary written into the pack's report and the bug report: counts, and codes of
/// every element that is not plainly `ok`. Ids are mapped through `renamed` (`(old, new)`).
#[must_use]
pub fn review_report(
    draft: &ImportDraft,
    overrides: &ReviewOverrides,
    renamed: &[(String, String)],
) -> ReviewReport {
    let states = review_all(draft, overrides);
    let c = ReviewCounts::of(&states);
    let final_id = |id: &str| {
        renamed
            .iter()
            .find(|(old, _)| old == id)
            .map_or_else(|| id.to_string(), |(_, new)| new.clone())
    };
    let elements = draft
        .elements
        .iter()
        .zip(&states)
        .filter(|(_, s)| {
            s.state != ReviewLevel::Ok
                || s.text_only
                || s.accepted
                || s.text_only_override.is_some()
        })
        .map(|(e, s)| ReviewReportEntry {
            element: final_id(&e.id),
            state: level_name(s.state).into(),
            code: s.code.clone(),
            text_only: s.text_only,
            text_only_override: s.text_only_override,
            accepted: s.accepted,
        })
        .collect();
    ReviewReport {
        ok: c.ok,
        check: c.check,
        not_imported: c.not_imported,
        text_only: c.text_only,
        accepted: c.accepted,
        elements,
    }
}

fn level_name(l: ReviewLevel) -> &'static str {
    match l {
        ReviewLevel::Ok => "ok",
        ReviewLevel::Check => "check",
        ReviewLevel::NotImported => "not_imported",
    }
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
