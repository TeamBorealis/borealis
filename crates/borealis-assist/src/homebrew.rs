//! `homebrew_with_balance`: schema-validated generation with a repair loop, then a deterministic
//! balance report against SRD baselines and an engine load check. The model may write prose about
//! the flags (number-checked); it can never change a number.

use std::sync::Arc;

use borealis_ai::task::homebrew::{HomebrewGenerate, HomebrewInput};
use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::validate::{ContentValidator, JsonSchemaValidator, ValidationIssue};
use borealis_ai::task::{run_task, AiRunner, AiTask, ContentKind, RunOptions};
use borealis_content::file::ContentFile;
use borealis_content::{ContentStoreBuilder, Ruleset};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::balance::{
    balance_report, load_into_engine, BalanceReport, Baselines, EngineCheck, Severity,
};
use crate::error::AssistError;
use crate::numbers::NumberGuard;
use crate::ProseSource;

/// The enabled packs and ruleset a homebrew element is generated and checked against.
#[derive(Debug, Clone)]
pub struct PackContext {
    /// Builder holding every enabled pack (cloned for each check).
    pub base: ContentStoreBuilder,
    /// Ruleset of the element and of the test character.
    pub ruleset: Ruleset,
}

pub(crate) fn ai_ruleset(r: Ruleset) -> borealis_ai::task::Ruleset {
    match r {
        Ruleset::Dnd2024 => borealis_ai::task::Ruleset::Dnd2024,
        _ => borealis_ai::task::Ruleset::Dnd2014,
    }
}

/// Request for [`homebrew_with_balance`].
#[derive(Debug, Clone)]
pub struct HomebrewRequest {
    /// What to create.
    pub kind: ContentKind,
    /// The concept in plain language.
    pub description: String,
    /// Fixed name, if any.
    pub name: Option<String>,
    /// Id prefix after `ID_` (e.g. `HB`).
    pub id_prefix: Option<String>,
    /// `source` stamped on the element.
    pub source_name: String,
}

/// Everything `homebrew_with_balance` returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomebrewOutcome {
    /// The generated content-pack document (draft; not added to any store).
    pub content: Value,
    /// Design notes from the generator.
    pub design_notes: String,
    /// Generator warnings.
    pub warnings: Vec<String>,
    /// Deterministic balance reports, one per element.
    pub reports: Vec<BalanceReport>,
    /// Engine load check.
    pub engine_check: EngineCheck,
    /// Prose about the flags.
    pub commentary: String,
    /// Whether the prose is model-written (number-checked) or a template.
    pub commentary_source: ProseSource,
    /// Generation attempts (1 = no repair).
    pub attempts: u32,
}

/// Validator that adds store-level checks (dangling references) to the JSON-Schema check so the
/// repair loop can fix them.
pub(crate) struct StoreValidator {
    schema: JsonSchemaValidator,
    ctx: PackContext,
}

impl StoreValidator {
    pub(crate) fn new(ctx: PackContext) -> Self {
        Self {
            schema: JsonSchemaValidator::content_format(),
            ctx,
        }
    }
}

impl ContentValidator for StoreValidator {
    fn validate(&self, content: &Value) -> Vec<ValidationIssue> {
        let mut issues = self.schema.validate(content);
        if !issues.is_empty() {
            return issues;
        }
        let check = load_into_engine(&self.ctx.base, self.ctx.ruleset, content);
        issues.extend(
            check
                .dangling_refs
                .iter()
                .chain(&check.store_errors)
                .map(|m| ValidationIssue::new("", m.clone())),
        );
        issues
    }
}

/// Balance reports for every element of a content document.
pub fn balance_reports(baselines: &Baselines, content: &Value) -> Vec<BalanceReport> {
    let Ok(file) = ContentFile::from_json_str(&content.to_string()) else {
        return Vec::new();
    };
    file.elements
        .iter()
        .map(|e| balance_report(baselines, e))
        .collect()
}

/// Generate homebrew, then report its balance against the enabled SRD packs.
///
/// # Errors
/// [`AssistError::Ai`] when generation fails after the repair rounds. A failed commentary call is
/// not an error; the template prose is used.
pub async fn homebrew_with_balance(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    req: &HomebrewRequest,
    opts: &RunOptions<'_>,
) -> Result<HomebrewOutcome, AssistError> {
    let store = ctx.base.clone().build();
    let baselines = Baselines::from_store(&store, ctx.ruleset);
    let input = HomebrewInput {
        kind: req.kind,
        ruleset: ai_ruleset(ctx.ruleset),
        description: req.description.clone(),
        name: req.name.clone(),
        source_name: req.source_name.clone(),
        id_prefix: req.id_prefix.clone(),
        existing_ids: store.iter().map(|s| s.element.id.clone()).collect(),
    };
    let validator = StoreValidator::new(ctx.clone());
    let task = HomebrewGenerate::new(input).with_validator(Arc::new(validator));
    let done = run_task(runner, &task, opts).await?;
    let content = done.output.content;
    let reports = balance_reports(&baselines, &content);
    let engine_check = load_into_engine(&ctx.base, ctx.ruleset, &content);
    let (commentary, commentary_source) = commentary(runner, &reports, opts).await;
    Ok(HomebrewOutcome {
        content,
        design_notes: done.output.notes,
        warnings: done.output.warnings,
        reports,
        engine_check,
        commentary,
        commentary_source,
        attempts: done.attempts,
    })
}

fn describe(r: &BalanceReport) -> Vec<String> {
    let mut lines: Vec<String> = r
        .flags
        .iter()
        .map(|f| {
            format!(
                "{} {}: {} {} (SRD range {} to {}, median {}) [{:?}] {}",
                r.element_type,
                r.name,
                f.metric,
                fmt(f.value),
                fmt(f.baseline_range[0]),
                fmt(f.baseline_range[1]),
                fmt(f.baseline_median),
                f.severity,
                f.note
            )
        })
        .collect();
    lines.extend(r.notes.iter().map(|n| format!("{}: {n}", r.name)));
    lines
}

fn fmt(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// Deterministic prose for the reports.
pub fn template_commentary(reports: &[BalanceReport]) -> String {
    let mut out = Vec::new();
    for r in reports {
        let flagged: Vec<String> = r
            .flagged()
            .map(|f| {
                format!(
                    "{} is {} against an SRD range of {} to {} ({:?})",
                    f.metric,
                    fmt(f.value),
                    fmt(f.baseline_range[0]),
                    fmt(f.baseline_range[1]),
                    f.severity
                )
            })
            .collect();
        if flagged.is_empty() {
            out.push(format!(
                "{} is within the SRD range on every compared metric.",
                r.name
            ));
        } else {
            out.push(format!("{}: {}.", r.name, flagged.join("; ")));
        }
    }
    out.join(" ")
}

fn guard_for(reports: &[BalanceReport]) -> NumberGuard {
    let mut g = NumberGuard::new();
    for line in reports.iter().flat_map(describe) {
        g.allow_text(&line);
    }
    for r in reports {
        g.allow_text(&r.name);
        for f in &r.flags {
            for v in [
                f.value,
                f.baseline_range[0],
                f.baseline_range[1],
                f.baseline_median,
            ] {
                g.allow_text(&fmt(v));
            }
        }
    }
    g
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CommentaryOut {
    commentary: String,
}

struct Commentary {
    lines: Vec<String>,
}

impl AiTask for Commentary {
    type Output = CommentaryOut;
    const NAME: &'static str = "balance_commentary";

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nWrite 2 to 5 short sentences for the designer about the balance flags in <flags>. \
Quote numbers exactly as written there; never compute, round or invent numbers. Suggest direction only (for example \"lower the dice\") without proposing new figures.",
            base_system("comment on a deterministic homebrew balance report")
        )
    }

    fn user_prompt(&self) -> String {
        fence("flags", &self.lines.join("\n"))
    }

    fn output_schema(&self) -> Value {
        json!({"type":"object","properties":{"commentary":{"type":"string","minLength":1,"maxLength":1500}},"required":["commentary"],"additionalProperties":false})
    }

    fn check(&self, _output: &CommentaryOut) -> Vec<String> {
        Vec::new()
    }
}

async fn commentary(
    runner: &dyn AiRunner,
    reports: &[BalanceReport],
    opts: &RunOptions<'_>,
) -> (String, ProseSource) {
    let template = template_commentary(reports);
    if reports.iter().all(|r| r.flagged().next().is_none()) {
        return (template, ProseSource::Template);
    }
    let task = Commentary {
        lines: reports.iter().flat_map(describe).collect(),
    };
    let single = RunOptions {
        max_repairs: 0,
        sink: opts.sink,
    };
    let Ok(done) = run_task(runner, &task, &single).await else {
        return (template, ProseSource::Template);
    };
    let text = done.output.commentary;
    if guard_for(reports).unknown(&text).is_empty() {
        (text, ProseSource::Model)
    } else {
        (template, ProseSource::Template)
    }
}

/// Highest severity over a set of reports.
pub fn worst_severity(reports: &[BalanceReport]) -> Severity {
    reports
        .iter()
        .map(BalanceReport::max_severity)
        .max()
        .unwrap_or(Severity::Ok)
}
