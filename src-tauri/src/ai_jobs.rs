//! The AI jobs: owned inputs prepared on the IPC thread (`prepare`), run later without any lock (`run_job`).

use crate::ai_dto::{camelize, AiMessage, AiMode, AiTaskRequest};
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::{AppState, ContentHub};
use borealis_ai::retrieval::{Filters, LazyIndex};
use borealis_ai::task::cited_qa::{answer_question, DEFAULT_TOP_K};
use borealis_ai::task::narrative::{Length, Narrative, NarrativeInput, NarrativeKind};
use borealis_ai::task::{run_task, AiRunner, Ruleset as AiRuleset, RunOptions};
use borealis_ai::{AiEvent, AiRequest};
use borealis_assist::{explain_number, explain_number_offline, level_up_advisor, AdvisorOptions};
use borealis_content::{Ruleset, SourceRestriction};
use borealis_engine::CharacterState;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

const CHAT_SYSTEM: &str =
    "You are the assistant of Borealis, a Dungeons & Dragons character builder. \
Answer briefly and concretely. Say so when you are unsure; never invent rules text.";

/// A prepared task: everything it needs, owned.
pub enum Job {
    /// Free chat.
    Chat {
        prompt: String,
        history: Vec<AiMessage>,
    },
    /// Cited rules answer.
    RulesQa {
        index: Arc<LazyIndex>,
        question: String,
        ruleset: Ruleset,
        summary: Option<String>,
        filters: Filters,
    },
    /// Explain a number.
    Explain {
        hub: Arc<ContentHub>,
        state: CharacterState,
        stat: String,
    },
    /// Level-up advisor.
    LevelUp {
        hub: Arc<ContentHub>,
        state: CharacterState,
        goals: String,
    },
    /// Backstory / NPC.
    Narrative { input: NarrativeInput },
}

/// What a finished job produced.
pub struct JobOutput {
    /// Human readable text (sent as one `delta` for non-chat tasks).
    pub text: String,
    /// Result kind (`rules_qa`, `explain`, ...); `None` for chat.
    pub kind: Option<&'static str>,
    /// Typed result (camelCase keys).
    pub data: Value,
}

impl Job {
    /// Explain has an offline fallback; every other job needs a provider.
    #[must_use]
    pub fn needs_provider(&self) -> bool {
        !matches!(self, Job::Explain { .. })
    }
}

fn ai_ruleset(r: Ruleset) -> AiRuleset {
    match r {
        Ruleset::Dnd2024 => AiRuleset::Dnd2024,
        _ => AiRuleset::Dnd2014,
    }
}

fn param<'a>(req: &'a AiTaskRequest, key: &str) -> Option<&'a Value> {
    req.params.as_ref().and_then(|p| p.get(key))
}

fn param_str(req: &AiTaskRequest, key: &str) -> Option<String> {
    param(req, key).and_then(Value::as_str).map(str::to_string)
}

fn character_summary(hub: &ContentHub, name: &str, state: &CharacterState) -> Value {
    let snap = hub.engine.derive(state);
    let classes: Vec<Value> = snap
        .classes
        .iter()
        .filter_map(|c| {
            c.class
                .as_ref()
                .map(|e| json!({"name": e.name, "level": c.level}))
        })
        .collect();
    let abilities: Value = snap
        .abilities
        .iter()
        .map(|a| (a.name.clone(), json!(a.score)))
        .collect::<serde_json::Map<_, _>>()
        .into();
    json!({"name": name, "level": snap.level, "classes": classes, "abilities": abilities,
           "armorClass": snap.armor_class, "hitPoints": snap.hit_points})
}

fn require_character(req: &AiTaskRequest) -> ApiResult<&str> {
    req.character_id
        .as_deref()
        .ok_or_else(|| ApiError::invalid("This task needs a characterId."))
}

fn goals(req: &AiTaskRequest) -> String {
    req.prompt.trim().to_string()
}

/// Build the job for `req` (reads the character and the content; no await, no lock kept).
pub fn prepare(state: &AppState, req: &AiTaskRequest, index: &Arc<LazyIndex>) -> ApiResult<Job> {
    let hub = state.hub()?;
    let settings = state.settings()?;
    let with_char = |id: &str| {
        state.read(id, |_, s| {
            Ok((s.character.build.clone(), s.character.meta.name.clone()))
        })
    };
    match req.mode {
        AiMode::Chat => {
            if req.prompt.trim().is_empty() {
                return Err(ApiError::invalid("Write a message first."));
            }
            Ok(Job::Chat {
                prompt: req.prompt.clone(),
                history: req.history.clone(),
            })
        }
        AiMode::Homebrew => Err(ApiError::invalid(
            "Homebrew drafting is wired in the next slice.",
        )),
        AiMode::RulesQa => {
            if req.prompt.trim().is_empty() {
                return Err(ApiError::invalid("Ask a rules question first."));
            }
            let (ruleset, summary, restriction) = match req.character_id.as_deref() {
                Some(id) => {
                    let (build, name) = with_char(id)?;
                    let summary = character_summary(&hub, &name, &build).to_string();
                    (
                        build.ruleset,
                        Some(summary),
                        SourceRestriction::excluding(build.restricted_sources.clone()),
                    )
                }
                None => (settings.default_ruleset, None, SourceRestriction::none()),
            };
            let filters = Filters {
                ruleset: Some(ruleset),
                enabled_packs: None,
                restriction,
            };
            Ok(Job::RulesQa {
                index: Arc::clone(index),
                question: req.prompt.clone(),
                ruleset,
                summary,
                filters,
            })
        }
        AiMode::Explain => {
            let (state_c, _) = with_char(require_character(req)?)?;
            let stat = param_str(req, "stat").unwrap_or_else(|| req.prompt.clone());
            if stat.trim().is_empty() {
                return Err(ApiError::invalid("Name the statistic to explain."));
            }
            Ok(Job::Explain {
                hub,
                state: state_c,
                stat,
            })
        }
        AiMode::BuildAdvisor => {
            let (state_c, _) = with_char(require_character(req)?)?;
            Ok(Job::LevelUp {
                hub,
                state: state_c,
                goals: goals(req),
            })
        }
        AiMode::Backstory | AiMode::Npc => {
            let character = match req.character_id.as_deref() {
                Some(id) => {
                    let (build, name) = with_char(id)?;
                    Some(character_summary(&hub, &name, &build))
                }
                None => None,
            };
            let ruleset = req
                .character_id
                .as_deref()
                .map_or(Ok(settings.default_ruleset), |id| {
                    state.read(id, |_, s| Ok(s.character.ruleset()))
                })?;
            let length = match param_str(req, "length").as_deref() {
                Some("short") => Length::Short,
                Some("long") => Length::Long,
                _ => Length::Medium,
            };
            let kind = match (req.mode, param_str(req, "kind").as_deref()) {
                (AiMode::Npc, _) => NarrativeKind::Npc,
                (_, Some("personality")) => NarrativeKind::Personality,
                _ => NarrativeKind::Backstory,
            };
            let constraints = (!req.prompt.trim().is_empty()).then(|| req.prompt.clone());
            let input = NarrativeInput {
                kind,
                ruleset: ai_ruleset(ruleset),
                character,
                tone: param_str(req, "tone"),
                constraints,
                length,
            };
            Ok(Job::Narrative { input })
        }
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> ApiResult<Value> {
    serde_json::to_value(v)
        .map(camelize)
        .map_err(ApiError::internal)
}

fn ai_err<E: std::fmt::Display + 'static>(e: E) -> ApiError {
    match (&e as &dyn std::any::Any).downcast_ref::<borealis_ai::AiError>() {
        Some(ai) => ApiError::from_ai(ai),
        None => ApiError::new(ErrorCode::AiUnavailable, e.to_string()),
    }
}

fn history_prompt(prompt: &str, history: &[AiMessage]) -> String {
    let mut out = String::new();
    for m in history {
        let who = if m.role == "assistant" {
            "Assistant"
        } else {
            "User"
        };
        out.push_str(&format!("{who}: {}\n", m.text));
    }
    out.push_str(&format!("User: {prompt}\nAssistant:"));
    out
}

/// Run a prepared job. `runner` is `None` only for jobs that do not need a provider.
///
/// # Errors
/// `ai_unavailable` for provider failures, `invalid_argument` for rejected inputs.
pub async fn run_job(
    job: Job,
    runner: Option<Arc<dyn AiRunner>>,
    sink: &UnboundedSender<AiEvent>,
) -> ApiResult<JobOutput> {
    let opts = RunOptions {
        sink: Some(sink),
        ..RunOptions::default()
    };
    let need = || {
        runner
            .as_deref()
            .ok_or_else(|| ai_err("No AI provider is available."))
    };
    match job {
        Job::Chat { prompt, history } => {
            let req = AiRequest {
                prompt: history_prompt(&prompt, &history),
                system: Some(CHAT_SYSTEM.into()),
                schema: None,
            };
            let out = need()?.run(&req, Some(sink)).await.map_err(ai_err)?;
            Ok(JobOutput {
                text: out.text,
                kind: None,
                data: Value::Null,
            })
        }
        Job::RulesQa {
            index,
            question,
            ruleset,
            summary,
            filters,
        } => {
            let r = answer_question(
                need()?,
                &index,
                &question,
                ai_ruleset(ruleset),
                summary,
                &filters,
                DEFAULT_TOP_K,
                &opts,
            )
            .await
            .map_err(ai_err)?;
            let text = r.output.answer_markdown.clone();
            Ok(JobOutput {
                text,
                kind: Some("rules_qa"),
                data: to_value(&r.output)?,
            })
        }
        Job::Explain { hub, state, stat } => {
            let ex = match runner.as_deref() {
                Some(r) => explain_number(r, &hub.engine, &state, &stat, &opts).await,
                None => explain_number_offline(&hub.engine, &state, &stat),
            };
            Ok(JobOutput {
                text: ex.text.clone(),
                kind: Some("explain"),
                data: to_value(&ex)?,
            })
        }
        Job::LevelUp { hub, state, goals } => {
            let o = AdvisorOptions {
                top_n: 5,
                sink: Some(sink),
            };
            let advice = level_up_advisor(need()?, &hub.engine, &state, &goals, &o)
                .await
                .map_err(ai_err)?;
            let data = to_value(&advice)?;
            let text = data
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(JobOutput {
                text,
                kind: Some("level_up"),
                data,
            })
        }
        Job::Narrative { input } => {
            let r = run_task(need()?, &Narrative { input }, &opts)
                .await
                .map_err(ai_err)?;
            let text = r.output.text.clone();
            Ok(JobOutput {
                text,
                kind: Some("narrative"),
                data: to_value(&r.output)?,
            })
        }
    }
}
