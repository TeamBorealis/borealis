//! Build planner.
//!
//! The model proposes a per-level plan. The engine replays it step by step on a cloned state and
//! validates each step against what is legal at that point. The first illegal step goes back to
//! the model with the engine's reasons and the options that are legal there (at most
//! [`MAX_REPAIR_ROUNDS`] repair rounds). The result is the validated plan, an engine digest per
//! level, and the remaining steps marked as unresolved. A [`Plan`] is plain data, so the UI can
//! "apply up to level N" with [`apply_plan`](crate::plan::apply_plan).

use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::{run_task, AiRunner, AiTask, RunOptions};
use borealis_ai::{AiEvent, Usage};
use borealis_engine::{CharacterState, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::error::AssistError;
use crate::numbers::NumberGuard;
use crate::options::{legal_options, LevelUpOption, OptionAction};
use crate::plan::{replay_step, OpenChoice, Plan, PlanStep};
use crate::summary::{level_summary, LevelSummary};

/// The planner may send invalid steps back to the model this many times.
pub const MAX_REPAIR_ROUNDS: u32 = 2;
/// Schema/sequence repair inside one model call.
const MAX_SCHEMA_REPAIRS: u32 = 1;
const MAX_RATIONALE_CHARS: usize = 300;

/// Knobs for [`build_planner`].
#[derive(Debug, Clone, Copy)]
pub struct PlannerOptions<'a> {
    /// Engine-feedback rounds after the first proposal (default and cap: 2).
    pub max_repair_rounds: u32,
    /// Streamed events of the model calls.
    pub sink: Option<&'a UnboundedSender<AiEvent>>,
}

impl Default for PlannerOptions<'_> {
    fn default() -> Self {
        Self {
            max_repair_rounds: MAX_REPAIR_ROUNDS,
            sink: None,
        }
    }
}

/// Model answer of the planner task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanOutput {
    /// Steps for consecutive levels.
    pub steps: Vec<PlanStep>,
    /// Optional remark on the plan's overall idea.
    #[serde(default)]
    pub notes: String,
}

/// Engine feedback on the previous proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    /// The failing part of the previous proposal (JSON).
    pub previous: String,
    /// Why the engine refused it.
    pub problems: Vec<String>,
}

/// The AI task behind [`build_planner`].
#[derive(Debug, Clone)]
pub struct PlanBuild {
    /// First level the model has to plan.
    pub from_level: u32,
    /// Last level to plan.
    pub target_level: u32,
    /// Character brief (engine digest).
    pub brief: String,
    /// The player's goals (untrusted text).
    pub goals: String,
    /// `id | name | prerequisite` lines of the available feats.
    pub feats: Vec<String>,
    /// Decisions that are legal at `from_level` (engine-computed).
    pub decisions: String,
    /// Already validated levels (digest lines).
    pub progress: Vec<String>,
    /// Feedback from the engine, in repair rounds.
    pub feedback: Option<Feedback>,
}

impl AiTask for PlanBuild {
    type Output = PlanOutput;
    const NAME: &'static str = "build_planner";

    fn system_prompt(&self) -> String {
        let mut s = base_system(
            "plan a character's level-by-level build, validated step by step by the game engine",
        );
        s.push_str(
            "\n\nTask rules:\n\
- Return one step per character level from the first to the last requested level, in order. Each step has the level, the path, the slot choices and (when the level grants one) the improvement.\n\
- path: {\"kind\":\"main\"} levels the main class; {\"kind\":\"multi\",\"starting_level\":N} levels an existing multiclass; {\"kind\":\"new_multiclass\",\"class\":\"<Multiclass element id>\"} starts a new multiclass.\n\
- choices: [{\"slot\":\"<slot title>\",\"element\":\"<element id>\"}]. Slot titles and element ids must be copied from <decisions> when listed there; for later levels use the exact class feature slot titles you know from the rules (for example the subclass slot title).\n\
- improvement: {\"kind\":\"plus_two\",\"ability\":\"strength\"}, {\"kind\":\"plus_one_each\",\"abilities\":[\"strength\",\"dexterity\"]} or {\"kind\":\"feat\",\"feat\":\"<feat id>\"}. Only levels that grant an Ability Score Improvement have one. Check feat prerequisites in <feats> against ability scores you will have at that level.\n\
- rationale: one short sentence per step. Do NOT write numbers or statistics in rationales: the app computes every number itself.\n\
- Never invent ids. If you are unsure of a slot title or id, leave that choice out; the app reports it as unresolved.",
        );
        s
    }

    fn user_prompt(&self) -> String {
        let mut out = format!(
            "Plan levels {} to {} for this character.\n\n{}\n\n{}\n\n{}\n\n{}",
            self.from_level,
            self.target_level,
            fence("character", &self.brief),
            fence("user_input", &self.goals),
            fence("feats", &self.feats.join("\n")),
            fence("decisions", &self.decisions),
        );
        if !self.progress.is_empty() {
            out.push_str("\n\nLevels already validated (do not repeat them):\n");
            out.push_str(&fence("progress", &self.progress.join("\n")));
        }
        if let Some(f) = &self.feedback {
            out.push_str("\n\nThe game engine refused part of your previous plan. Plan these levels again, fixing every problem:\n");
            out.push_str(&fence("previous_plan", &f.previous));
            out.push('\n');
            out.push_str(&fence("problems", &f.problems.join("\n")));
        }
        out
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "maxItems": 20,
                    "items": {
                        "type": "object",
                        "properties": {
                            "level": {"type": "integer", "minimum": 1, "maximum": 20},
                            "path": {
                                "type": "object",
                                "properties": {
                                    "kind": {"enum": ["main", "multi", "new_multiclass"]},
                                    "starting_level": {"type": "integer", "minimum": 1},
                                    "class": {"type": "string"}
                                },
                                "required": ["kind"]
                            },
                            "choices": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "slot": {"type": "string", "minLength": 1},
                                        "ordinal": {"type": "integer", "minimum": 1},
                                        "element": {"type": "string", "minLength": 1}
                                    },
                                    "required": ["slot", "element"],
                                    "additionalProperties": false
                                }
                            },
                            "improvement": {
                                "type": "object",
                                "properties": {
                                    "kind": {"enum": ["plus_two", "plus_one_each", "feat"]},
                                    "ability": {"type": "string"},
                                    "abilities": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2},
                                    "feat": {"type": "string"}
                                },
                                "required": ["kind"]
                            },
                            "rationale": {"type": "string", "maxLength": MAX_RATIONALE_CHARS}
                        },
                        "required": ["level", "path"]
                    }
                },
                "notes": {"type": "string", "maxLength": MAX_RATIONALE_CHARS * 2}
            },
            "required": ["steps"],
            "additionalProperties": false
        })
    }

    fn check(&self, output: &PlanOutput) -> Vec<String> {
        let mut errors = Vec::new();
        for (i, s) in output.steps.iter().enumerate() {
            let want = self.from_level + u32::try_from(i).unwrap_or(0);
            if s.level != want {
                errors.push(format!(
                    "step {} has level {} but steps must be consecutive starting at level {}",
                    i + 1,
                    s.level,
                    self.from_level
                ));
                break;
            }
        }
        if output.steps.len() > (self.target_level + 1 - self.from_level) as usize {
            errors.push(format!("plan goes beyond level {}", self.target_level));
        }
        errors
    }
}

/// A validated level of the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedLevel {
    /// The validated step.
    pub step: PlanStep,
    /// Engine digest after this level.
    pub snapshot: LevelSummary,
    /// Required choices this level opened that the plan did not fill.
    pub open_choices: Vec<OpenChoice>,
    /// True when the model's rationale was removed because it contained unverified numbers.
    pub rationale_redacted: bool,
}

/// A level the plan could not be completed for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedLevel {
    /// The character level.
    pub level: u32,
    /// The last proposal for it (absent when the model never proposed one).
    pub proposed: Option<PlanStep>,
    /// Why it is unresolved: engine reasons, or `blocked by level N`.
    pub reasons: Vec<String>,
}

/// The planner's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildPlan {
    /// Level of the character when planning started.
    pub start_level: u32,
    /// Requested last level.
    pub target_level: u32,
    /// Validated levels, in order.
    pub levels: Vec<PlannedLevel>,
    /// Levels that could not be validated (the first carries the engine's reasons).
    pub unresolved: Vec<UnresolvedLevel>,
    /// True when every level up to the target validated.
    pub complete: bool,
    /// Engine-feedback rounds used (0 = the first proposal was already valid).
    pub repair_rounds: u32,
    /// Model remark (not verified; may be empty).
    pub notes: String,
    /// Usage summed over all model calls.
    pub usage: Option<Usage>,
}

impl BuildPlan {
    /// The validated steps as plain data.
    #[must_use]
    pub fn plan(&self) -> Plan {
        Plan {
            steps: self.levels.iter().map(|l| l.step.clone()).collect(),
        }
    }

    /// Highest character level that is fully validated.
    #[must_use]
    pub fn validated_through(&self) -> u32 {
        self.levels
            .last()
            .map_or(self.start_level, |l| l.snapshot.level)
    }
}

fn decisions_text(options: &[LevelUpOption]) -> String {
    use std::collections::BTreeMap;
    let mut by_group: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for o in options {
        let entry = by_group.entry((o.path.key(), o.group.clone())).or_default();
        match &o.action {
            OptionAction::TakeLevel => entry.push(format!("{} (path {})", o.label, o.id)),
            OptionAction::Select { element, .. } => entry.push(format!("{element} ({})", o.label)),
            OptionAction::Improve {
                what: crate::options::ImprovementSpec::Feat { feat },
                ..
            } => entry.push(format!("feat {feat}")),
            OptionAction::Improve { .. } => {}
        }
    }
    let mut lines = Vec::new();
    for ((path, group), items) in by_group {
        if items.is_empty() {
            lines.push(format!("[{path}] {group}: ability score +2/+1+1 or a feat"));
        } else {
            lines.push(format!("[{path}] {group}: {}", items.join(", ")));
        }
    }
    if lines.is_empty() {
        "(none)".to_string()
    } else {
        lines.join("\n")
    }
}

fn feat_lines(engine: &Engine, state: &CharacterState) -> Vec<String> {
    engine
        .universe(state.ruleset)
        .by_type("Feat")
        .map(|f| {
            let pre = if f.el.prerequisite.is_empty() {
                &f.el.requirements
            } else {
                &f.el.prerequisite
            };
            format!(
                "{} | {} | {}",
                f.el.id,
                f.el.name,
                if pre.is_empty() { "none" } else { pre }
            )
        })
        .collect()
}

fn sanitize_rationale(
    engine: &Engine,
    state: &CharacterState,
    step: &PlanStep,
    snap: &LevelSummary,
    target: u32,
) -> (String, bool) {
    let text = step.rationale.trim();
    if text.is_empty() {
        return (String::new(), false);
    }
    let mut g = NumberGuard::new();
    g.allow_int(i64::from(step.level));
    g.allow_int(i64::from(target));
    g.allow_int(i64::from(snap.hit_points));
    g.allow_int(i64::from(snap.armor_class));
    g.allow_int(i64::from(snap.initiative));
    g.allow_int(i64::from(snap.proficiency_bonus));
    for (_, v) in snap.abilities.iter().chain(snap.spell_save_dc.iter()) {
        g.allow_int(i64::from(*v));
    }
    for c in &step.choices {
        if let Some(e) = engine.universe(state.ruleset).get(&c.element) {
            g.allow_text(&e.el.name);
        }
    }
    if g.unknown(text).is_empty() {
        (text.to_string(), false)
    } else {
        (String::new(), true)
    }
}

async fn propose(
    runner: &dyn AiRunner,
    task: &PlanBuild,
    sink: Option<&UnboundedSender<AiEvent>>,
    usage: &mut Option<Usage>,
) -> Result<PlanOutput, AssistError> {
    let opts = RunOptions {
        max_repairs: MAX_SCHEMA_REPAIRS,
        sink,
    };
    let r = run_task(runner, task, &opts).await?;
    if let Some(u) = r.usage {
        *usage = Some(usage.take().map_or_else(|| u.clone(), |t| t.add(&u)));
    }
    Ok(r.output)
}

/// Plan a build up to `target_level` and validate it with the engine.
///
/// # Errors
/// [`AssistError::Invalid`] for a target that is not above the current level (or above 20);
/// [`AssistError::Ai`] when the model call itself fails. Illegal steps are not errors: they come
/// back as unresolved levels.
pub async fn build_planner(
    runner: &dyn AiRunner,
    engine: &Engine,
    state: &CharacterState,
    target_level: u32,
    goals: &str,
    options: &PlannerOptions<'_>,
) -> Result<BuildPlan, AssistError> {
    let start_level = state.level();
    if target_level > 20 || target_level <= start_level {
        return Err(AssistError::Invalid(format!(
            "the target level must be between {} and 20",
            start_level + 1
        )));
    }
    let feats = feat_lines(engine, state);
    let rounds = options.max_repair_rounds.min(MAX_REPAIR_ROUNDS);
    let mut usage = None;
    let mut cur = state.clone();
    let mut prev_snap = engine.derive(&cur);
    let mut levels: Vec<PlannedLevel> = Vec::new();
    let mut feedback: Option<Feedback> = None;
    let mut notes = String::new();
    let mut repair_rounds = 0;
    let mut last_failure: Option<UnresolvedLevel> = None;

    loop {
        let from_level = cur.level() + 1;
        let task = PlanBuild {
            from_level,
            target_level,
            brief: level_summary(None, &prev_snap).line(),
            goals: goals.to_string(),
            feats: feats.clone(),
            decisions: decisions_text(&legal_options(engine, &cur)?),
            progress: levels.iter().map(|l| l.snapshot.line()).collect(),
            feedback: feedback.take(),
        };
        let proposal = propose(runner, &task, options.sink, &mut usage).await?;
        if notes.is_empty() {
            notes = proposal.notes.trim().to_string();
        }
        let mut failure: Option<(PlanStep, Vec<String>)> = None;
        for step in &proposal.steps {
            match replay_step(engine, &cur, step) {
                Ok(o) => {
                    let (rationale, redacted) =
                        sanitize_rationale(engine, &cur, step, &o.summary, target_level);
                    let mut step = step.clone();
                    step.rationale = rationale;
                    levels.push(PlannedLevel {
                        step,
                        snapshot: o.summary,
                        open_choices: o.open_choices,
                        rationale_redacted: redacted,
                    });
                    cur = o.state;
                    prev_snap = engine.derive(&cur);
                }
                Err(issues) => {
                    failure = Some((step.clone(), issues));
                    break;
                }
            }
        }
        if cur.level() >= target_level {
            break;
        }
        let (previous, problems) = match &failure {
            Some((step, issues)) => (
                serde_json::to_string(&proposal_tail(&proposal, step.level)).unwrap_or_default(),
                issues.clone(),
            ),
            None => (
                serde_json::to_string(&proposal).unwrap_or_default(),
                vec![format!(
                    "the plan stops at level {}; it must reach level {target_level}",
                    cur.level()
                )],
            ),
        };
        last_failure = Some(UnresolvedLevel {
            level: cur.level() + 1,
            proposed: failure.map(|(s, _)| s),
            reasons: problems.clone(),
        });
        if repair_rounds >= rounds {
            break;
        }
        repair_rounds += 1;
        feedback = Some(Feedback { previous, problems });
    }

    let reached = cur.level();
    let mut unresolved = Vec::new();
    if reached < target_level {
        let first = last_failure.unwrap_or(UnresolvedLevel {
            level: reached + 1,
            proposed: None,
            reasons: vec!["the model did not propose this level".to_string()],
        });
        let first_level = first.level;
        unresolved.push(first);
        for level in first_level + 1..=target_level {
            unresolved.push(UnresolvedLevel {
                level,
                proposed: None,
                reasons: vec![format!("blocked by level {first_level}")],
            });
        }
    }
    Ok(BuildPlan {
        start_level,
        target_level,
        levels,
        unresolved,
        complete: reached >= target_level,
        repair_rounds,
        notes,
        usage,
    })
}

fn proposal_tail(p: &PlanOutput, from_level: u32) -> PlanOutput {
    PlanOutput {
        steps: p
            .steps
            .iter()
            .filter(|s| s.level >= from_level)
            .cloned()
            .collect(),
        notes: String::new(),
    }
}
