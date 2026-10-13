//! Level-up advisor.
//!
//! The engine lists every legal option for the next level ([`legal_options`]). The model ranks
//! the best few for the player's goals and explains why, referring to option ids only. Every
//! pick is then applied to a cloned state with the engine's own commands, and the effects
//! (armor class, hit points, save DC, attack bonuses, ...) come from the resulting snapshots.
//! Unknown ids are dropped (one repair round first), and rationales containing numbers the
//! engine did not produce are replaced by a template built from the computed effects.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::{run_task, AiRunner, AiTask, RunOptions};
use borealis_ai::{AiError, AiEvent, Usage};
use borealis_engine::{CharacterState, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::error::AssistError;
use crate::numbers::NumberGuard;
use crate::options::{apply_option, legal_options, LevelUpOption};
use crate::summary::{allow_effects, effects, level_summary, Effects};
use crate::ProseSource;

/// Default number of picks.
pub const DEFAULT_TOP_N: usize = 5;
/// The advisor's repair loop is at most one round.
const MAX_REPAIRS: u32 = 1;
/// Maximum length of one rationale / the summary (characters).
const MAX_RATIONALE_CHARS: usize = 500;

/// Knobs for [`level_up_advisor`].
#[derive(Debug, Clone, Copy)]
pub struct AdvisorOptions<'a> {
    /// How many picks to return.
    pub top_n: usize,
    /// Streamed events of the model call.
    pub sink: Option<&'a UnboundedSender<AiEvent>>,
}

impl Default for AdvisorOptions<'_> {
    fn default() -> Self {
        Self {
            top_n: DEFAULT_TOP_N,
            sink: None,
        }
    }
}

/// One ranked pick as proposed by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawPick {
    /// An option id from the legal set.
    pub option_id: String,
    /// Why the option fits the goals.
    pub rationale: String,
}

/// Model answer of the advisor task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdviceOutput {
    /// Ranked picks, best first.
    pub picks: Vec<RawPick>,
    /// One or two sentences on the overall direction.
    #[serde(default)]
    pub summary: String,
}

/// The AI task behind [`level_up_advisor`].
#[derive(Debug)]
pub struct AdviseLevelUp {
    brief: String,
    goals: String,
    options: Vec<LevelUpOption>,
    top_n: usize,
    last: Mutex<Option<AdviceOutput>>,
}

impl AdviseLevelUp {
    /// A task over a character brief, the player's goals and the legal option set.
    #[must_use]
    pub fn new(brief: String, goals: String, options: Vec<LevelUpOption>, top_n: usize) -> Self {
        Self {
            brief,
            goals,
            options,
            top_n,
            last: Mutex::new(None),
        }
    }

    fn ids(&self) -> BTreeSet<&str> {
        self.options.iter().map(|o| o.id.as_str()).collect()
    }

    fn recorded(&self) -> Option<AdviceOutput> {
        self.last.lock().ok().and_then(|g| g.clone())
    }
}

fn option_line(o: &LevelUpOption) -> String {
    if o.summary.is_empty() {
        format!("{} | {} | {}", o.id, o.group, o.label)
    } else {
        format!("{} | {} | {} | {}", o.id, o.group, o.label, o.summary)
    }
}

impl AiTask for AdviseLevelUp {
    type Output = AdviceOutput;
    const NAME: &'static str = "level_up_advisor";

    fn system_prompt(&self) -> String {
        let mut s = base_system(
            "rank the best options for a character's next level, given the player's goals",
        );
        s.push_str(&format!(
            "\n\nTask rules:\n\
- Pick at most the requested number of options from <options> and rank them best first. Refer to options ONLY by the exact id in the first column.\n\
- The ids are the engine's complete option list (exactly {count} valid ids). They are NOT content element ids: never call a tool to look them up and never invent, shorten or rewrite one (for example a multiclass is `new:<engine id>:...`, copy the whole string). An id that is not in <options> is discarded.\n\
- Prefer covering different decisions (the group column) before offering several picks for one decision.\n\
- Each rationale is one or two sentences about how the option serves the goals. Do NOT write any numbers, bonuses or statistics in rationales or the summary: the app computes the real effect of every pick itself and shows it next to your text.\n\
- If nothing in <options> fits the goals, still return the least bad choices and say so in the summary.",
            count = self.options.len()
        ));
        s
    }

    fn user_prompt(&self) -> String {
        let mut lines = vec!["option id | decision | option | description".to_string()];
        lines.extend(self.options.iter().map(option_line));
        format!(
            "Return the top {} options for this character's next level. <options> holds exactly {} valid ids; answer only with ids copied from it.\n\n{}\n\n{}\n\n{}",
            self.top_n,
            self.options.len(),
            fence("character", &self.brief),
            fence("user_input", &self.goals),
            fence("options", &lines.join("\n"))
        )
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "picks": {
                    "type": "array",
                    "maxItems": self.top_n.max(1) * 2,
                    "items": {
                        "type": "object",
                        "properties": {
                            "option_id": {"type": "string", "minLength": 1},
                            "rationale": {"type": "string", "maxLength": MAX_RATIONALE_CHARS}
                        },
                        "required": ["option_id", "rationale"],
                        "additionalProperties": false
                    }
                },
                "summary": {"type": "string", "maxLength": MAX_RATIONALE_CHARS}
            },
            "required": ["picks"],
            "additionalProperties": false
        })
    }

    fn check(&self, output: &AdviceOutput) -> Vec<String> {
        if let Ok(mut g) = self.last.lock() {
            *g = Some(output.clone());
        }
        let ids = self.ids();
        output
            .picks
            .iter()
            .filter(|p| !ids.contains(p.option_id.as_str()))
            .map(|p| {
                format!(
                    "option_id \"{}\" is not in <options>; use only ids copied exactly from the first column",
                    p.option_id
                )
            })
            .collect()
    }
}

/// A ranked pick with its engine-computed effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisedPick {
    /// 1 = best.
    pub rank: usize,
    /// The legal option (apply it with [`apply_option`]).
    pub option: LevelUpOption,
    /// Why it fits (model prose, or a template when the model's numbers could not be verified).
    pub rationale: String,
    /// Origin of `rationale`.
    pub rationale_source: ProseSource,
    /// What taking the level plus this option changes, from the engine.
    pub effects: Effects,
    /// The commands that were run on the cloned state.
    pub applied_steps: Vec<String>,
}

/// A model suggestion that was not shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedPick {
    /// The id as written by the model.
    pub option_id: String,
    /// Why it was dropped.
    pub reason: String,
}

/// The advisor's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelUpAdvice {
    /// Character level before.
    pub current_level: u32,
    /// Character level after taking the next level.
    pub next_level: u32,
    /// Verified picks, best first.
    pub picks: Vec<AdvisedPick>,
    /// Overall direction, when the model's text passed the number check.
    pub summary: Option<String>,
    /// Suggestions that were removed.
    pub dropped: Vec<DroppedPick>,
    /// How many legal options the model could choose from.
    pub options_considered: usize,
    /// Model attempts (2 means the repair round was used).
    pub attempts: u32,
    /// Model usage.
    pub usage: Option<Usage>,
}

/// Plain-text rendering of computed effects (template rationale).
#[must_use]
pub fn effects_text(e: &Effects) -> String {
    if e.stats.is_empty() && e.gained.is_empty() {
        return "no change to the headline statistics".to_string();
    }
    let mut parts: Vec<String> = e
        .stats
        .iter()
        .map(|d| format!("{} {} -> {}", d.stat, d.before, d.after))
        .collect();
    if !e.gained.is_empty() {
        parts.push(format!("gains {}", e.gained.join(", ")));
    }
    parts.join("; ")
}

/// Rank and verify the legal options for the next level.
///
/// # Errors
/// [`AssistError::Ai`] when the model call fails (or answers with something unusable), or
/// [`AssistError::Engine`] when the options cannot be computed.
pub async fn level_up_advisor(
    runner: &dyn AiRunner,
    engine: &Engine,
    state: &CharacterState,
    goals: &str,
    options: &AdvisorOptions<'_>,
) -> Result<LevelUpAdvice, AssistError> {
    let legal = legal_options(engine, state)?;
    let before = engine.derive(state);
    let brief = level_summary(None, &before).line();
    let top_n = options.top_n.max(1);
    let task = AdviseLevelUp::new(brief, goals.to_string(), legal.clone(), top_n);
    let run_opts = RunOptions {
        max_repairs: MAX_REPAIRS,
        sink: options.sink,
    };
    let (raw, attempts, usage) = if legal.is_empty() {
        (
            AdviceOutput {
                picks: Vec::new(),
                summary: String::new(),
            },
            0,
            None,
        )
    } else {
        match run_task(runner, &task, &run_opts).await {
            Ok(r) => (r.output, r.attempts, r.usage),
            Err(AiError::Validation { attempts, errors }) => match task.recorded() {
                // Invalid ids survived the repair round: keep the valid picks, drop the rest.
                Some(out) if !errors.is_empty() => (out, attempts, None),
                _ => return Err(AiError::Validation { attempts, errors }.into()),
            },
            Err(e) => return Err(e.into()),
        }
    };
    Ok(verify(
        engine, state, &before, &legal, raw, top_n, attempts, usage,
    ))
}

#[allow(clippy::too_many_arguments)]
fn verify(
    engine: &Engine,
    state: &CharacterState,
    before: &borealis_engine::snapshot::CharacterSnapshot,
    legal: &[LevelUpOption],
    raw: AdviceOutput,
    top_n: usize,
    attempts: u32,
    usage: Option<Usage>,
) -> LevelUpAdvice {
    let by_id: BTreeMap<&str, &LevelUpOption> = legal.iter().map(|o| (o.id.as_str(), o)).collect();
    let mut dropped = Vec::new();
    let mut picks: Vec<AdvisedPick> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut summary_guard = NumberGuard::new();
    summary_guard.allow_int(i64::from(before.level));
    summary_guard.allow_int(i64::from(before.level) + 1);
    for p in raw.picks {
        if picks.len() >= top_n {
            break;
        }
        let Some(option) = by_id.get(p.option_id.as_str()) else {
            dropped.push(DroppedPick {
                option_id: p.option_id,
                reason: "not a legal option for the next level".to_string(),
            });
            continue;
        };
        if !seen.insert(option.id.clone()) {
            continue;
        }
        let applied = match apply_option(engine, state, option) {
            Ok(a) => a,
            Err(e) => {
                dropped.push(DroppedPick {
                    option_id: option.id.clone(),
                    reason: format!("the engine refused it: {e}"),
                });
                continue;
            }
        };
        let after = engine.derive(&applied.state);
        let fx = effects(before, &after);
        let mut guard = NumberGuard::new();
        allow_effects(&mut guard, &fx);
        allow_effects(&mut summary_guard, &fx);
        for text in [&option.label, &option.group, &option.summary] {
            guard.allow_text(text);
            summary_guard.allow_text(text);
        }
        guard.allow_int(i64::from(before.level));
        guard.allow_int(i64::from(after.level));
        let rationale = p.rationale.trim().to_string();
        let (rationale, rationale_source) =
            if rationale.is_empty() || !guard.unknown(&rationale).is_empty() {
                (
                    format!("{}: {}.", option.label, effects_text(&fx)),
                    ProseSource::Template,
                )
            } else {
                (rationale, ProseSource::Model)
            };
        picks.push(AdvisedPick {
            rank: picks.len() + 1,
            option: (*option).clone(),
            rationale,
            rationale_source,
            effects: fx,
            applied_steps: applied.steps,
        });
    }
    let summary = Some(raw.summary.trim().to_string())
        .filter(|s| !s.is_empty() && summary_guard.unknown(s).is_empty());
    LevelUpAdvice {
        current_level: before.level,
        next_level: before.level + 1,
        picks,
        summary,
        dropped,
        options_considered: legal.len(),
        attempts,
        usage,
    }
}
