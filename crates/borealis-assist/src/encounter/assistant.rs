//! The themed-encounter assistant.
//!
//! The model proposes monster ids, counts and tactics prose, choosing ONLY from a candidate list this
//! module pre-filters and passes in. The engine recomputes the difficulty. An off-band proposal goes back
//! to the model once with the engine's numbers (the task's single repair round); if it is still off-band
//! (or the model failed) the deterministic suggestion is returned instead. Ids that are not candidates
//! are stripped, and sentences of the prose that state a number the data does not confirm are removed.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::{run_task, AiRunner, AiTask, RunOptions};
use borealis_ai::{AiError, AiEvent, Usage};
use borealis_content::{ContentStore, MonsterData, Ruleset};
use borealis_encounter::{Band, EncounterReport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use super::{
    evaluate, monster_index, monster_lines, suggest_deterministic, Constraints, EncounterMonster,
    Method, MonsterEntry, Party,
};
use crate::error::AssistError;
use crate::numbers::NumberGuard;
use crate::ProseSource;

/// Most candidates shown to the model.
const MAX_CANDIDATES: usize = 150;
/// The repair loop is at most one round.
const MAX_REPAIRS: u32 = 1;
/// Longest tactics text (characters).
const MAX_TACTICS_CHARS: usize = 1500;

/// Knobs for [`encounter_assistant`].
#[derive(Debug, Clone, Copy)]
pub struct EncounterOptions<'a> {
    /// Whose monster pack to use (5.1 for 2014, 5.2.1 for 2024).
    pub ruleset: Ruleset,
    /// Difficulty to aim for.
    pub band: Band,
    /// Difficulty method (SRD 5.2.1 budget unless the user supplied 2014 tables).
    pub method: Method<'a>,
    /// Type / count / theme limits.
    pub constraints: &'a Constraints,
    /// Streamed events of the model call.
    pub sink: Option<&'a UnboundedSender<AiEvent>>,
}

static NO_CONSTRAINTS: Constraints = Constraints {
    types: Vec::new(),
    max_count: None,
    theme_tags: Vec::new(),
};

impl Default for EncounterOptions<'_> {
    fn default() -> Self {
        Self {
            ruleset: Ruleset::Dnd2024,
            band: Band::Moderate,
            method: Method::Srd521,
            constraints: &NO_CONSTRAINTS,
            sink: None,
        }
    }
}

/// One monster line as written by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMonster {
    /// A candidate id.
    pub id: String,
    /// How many.
    pub count: u32,
}

/// The model's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEncounter {
    /// The creatures.
    pub monsters: Vec<RawMonster>,
    /// Tactics prose, with no statistics.
    #[serde(default)]
    pub tactics: String,
}

/// Where the monsters came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterSource {
    /// The model's selection, verified on-band by the engine.
    Model,
    /// The deterministic suggestion (model off-band, unusable or unavailable).
    Deterministic,
}

/// A model suggestion that was removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovedMonster {
    /// The id as written.
    pub id: String,
    /// Why.
    pub reason: String,
}

/// The assistant's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterProposal {
    /// Origin of the monster list.
    pub source: EncounterSource,
    /// Why the deterministic fallback was used.
    pub fallback_reason: Option<String>,
    /// Monster pack ruleset.
    pub ruleset: Ruleset,
    /// Label of the difficulty method in force.
    pub method: String,
    /// Target band.
    pub target: Band,
    /// Budget of the target band.
    pub budget: u64,
    /// Character levels.
    pub party: Vec<u8>,
    /// Creatures with engine-computed totals.
    pub monsters: Vec<EncounterMonster>,
    /// Engine report of exactly these creatures.
    pub report: EncounterReport,
    /// Tactics prose.
    pub tactics: String,
    /// Origin of `tactics`.
    pub tactics_source: ProseSource,
    /// Model monsters that were stripped.
    pub removed: Vec<RemovedMonster>,
    /// Numbers in the model's prose that the data does not confirm (their sentences were dropped).
    pub removed_claims: Vec<String>,
    /// Model attempts (2 = the repair round was used).
    pub attempts: u32,
    /// Model usage.
    pub usage: Option<Usage>,
}

/// The AI task behind [`encounter_assistant`].
#[derive(Debug)]
struct ProposeEncounter<'a> {
    request: String,
    party: Vec<u8>,
    band: Band,
    budget: u64,
    method: Method<'a>,
    cap: u32,
    candidates: Vec<MonsterEntry>,
    last: Mutex<Option<RawEncounter>>,
}

impl ProposeEncounter<'_> {
    fn by_id(&self) -> BTreeMap<&str, &MonsterEntry> {
        self.candidates.iter().map(|m| (m.id.as_str(), m)).collect()
    }

    fn recorded(&self) -> Option<RawEncounter> {
        self.last.lock().ok().and_then(|g| g.clone())
    }
}

fn counts_of(raw: &RawEncounter, known: &BTreeMap<&str, &MonsterEntry>) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for m in raw
        .monsters
        .iter()
        .filter(|m| known.contains_key(m.id.as_str()))
    {
        *counts.entry(m.id.clone()).or_insert(0u32) += m.count.max(1);
    }
    counts
}

impl AiTask for ProposeEncounter<'_> {
    type Output = RawEncounter;
    const NAME: &'static str = "encounter_assistant";

    fn system_prompt(&self) -> String {
        let mut s = base_system(
            "design a themed D&D encounter for a party from a list of candidate monsters",
        );
        s.push_str(&format!(
            "\n\nTask rules:\n\
- Choose creatures ONLY from <candidates>, copying the id from the first column exactly. Never invent a creature.\n\
- Use at most {} creatures in total. Aim for the target difficulty; the app recomputes the real difficulty from the data and will send corrections.\n\
- The tactics are one short paragraph: how the creatures fight together and use the terrain. Do NOT write stat block numbers (hit points, armor class, damage, challenge rating, XP): the app shows the real data itself. Counts of creatures are fine.",
            self.cap
        ));
        s
    }

    fn user_prompt(&self) -> String {
        let mut lines = vec!["id | name | CR | XP each | type | size".to_string()];
        lines.extend(self.candidates.iter().map(|m| {
            format!(
                "{} | {} | {} | {} | {} | {}",
                m.id,
                m.name,
                m.cr,
                m.xp,
                m.creature_type,
                m.size.join("/")
            )
        }));
        let party = format!(
            "{} characters, levels {}.\nTarget difficulty: {:?}. Method: {}. Budget for that difficulty: {} XP (never exceed it).",
            self.party.len(),
            self.party.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
            self.band,
            self.method_label(),
            self.budget
        );
        format!(
            "Propose one encounter.\n\n{}\n\n{}\n\n{}",
            fence("party", &party),
            fence("user_input", &self.request),
            fence("candidates", &lines.join("\n"))
        )
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "monsters": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 12,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string", "minLength": 1},
                            "count": {"type": "integer", "minimum": 1, "maximum": 30}
                        },
                        "required": ["id", "count"],
                        "additionalProperties": false
                    }
                },
                "tactics": {"type": "string", "maxLength": MAX_TACTICS_CHARS}
            },
            "required": ["monsters", "tactics"],
            "additionalProperties": false
        })
    }

    fn check(&self, output: &RawEncounter) -> Vec<String> {
        if let Ok(mut g) = self.last.lock() {
            *g = Some(output.clone());
        }
        let known = self.by_id();
        let mut issues: Vec<String> = output
            .monsters
            .iter()
            .filter(|m| !known.contains_key(m.id.as_str()))
            .map(|m| format!("id \"{}\" is not in <candidates>; use only ids copied exactly from the first column", m.id))
            .collect();
        let counts = counts_of(output, &known);
        let total: u32 = counts.values().sum();
        if total > self.cap {
            issues.push(format!(
                "{total} creatures is too many; use at most {}",
                self.cap
            ));
        }
        let (_, groups) = monster_lines(&known, &counts);
        match evaluate(&self.party, &groups, &self.method) {
            Ok(r) if r.band != self.band => issues.push(format!(
                "the engine computes {} total XP ({} adjusted) for these creatures, which is {:?}; the target is {:?} with a budget of {} XP. Change the creatures or counts",
                r.total_xp, r.adjusted_xp, r.band, self.band, self.budget
            )),
            Ok(_) => {}
            Err(e) => issues.push(e.to_string()),
        }
        issues
    }
}

impl ProposeEncounter<'_> {
    fn method_label(&self) -> &'static str {
        self.method.label()
    }
}

/// Pre-filter the candidates handed to the model: allowed by the constraints, no CR 0, nothing above the
/// party's highest level or the band budget, request-themed monsters first, then an even spread of CRs.
fn candidate_list(
    pool: &[MonsterEntry],
    constraints: &Constraints,
    request: &str,
    party: &[u8],
    budget: u64,
) -> Vec<MonsterEntry> {
    let top = party.iter().copied().max().unwrap_or(1);
    let words: Vec<String> = request
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect();
    let eligible: Vec<&MonsterEntry> = pool
        .iter()
        .filter(|m| m.xp > 0 && m.xp <= budget && !m.cr.exceeds_level(top) && constraints.allows(m))
        .collect();
    let (themed, rest): (Vec<&MonsterEntry>, Vec<&MonsterEntry>) = eligible
        .into_iter()
        .partition(|m| words.iter().any(|w| m.matches_theme(w)));
    let mut out: Vec<MonsterEntry> = themed.into_iter().take(MAX_CANDIDATES).cloned().collect();
    let room = MAX_CANDIDATES.saturating_sub(out.len());
    if room > 0 && !rest.is_empty() {
        let step = rest.len().div_ceil(room).max(1);
        out.extend(rest.into_iter().step_by(step).take(room).cloned());
    }
    out.sort_by(|a, b| (a.cr, &a.name, &a.id).cmp(&(b.cr, &b.name, &b.id)));
    out
}

/// Propose a themed encounter and verify it.
///
/// A failed model call, an unusable answer or an answer that stays off-band after the repair round gives the
/// deterministic suggestion (`source` = [`EncounterSource::Deterministic`], with `fallback_reason`).
///
/// # Errors
/// [`AssistError::Encounter`] for an invalid party or a band without a budget, [`AssistError::Invalid`] when
/// the constraints admit no monster at all.
pub async fn encounter_assistant(
    runner: &dyn AiRunner,
    store: &ContentStore,
    party: &Party,
    request_text: &str,
    options: &EncounterOptions<'_>,
) -> Result<EncounterProposal, AssistError> {
    let levels = party.levels()?;
    let pool = monster_index(store, options.ruleset);
    let fallback = suggest_deterministic(
        &levels,
        options.band,
        options.constraints,
        &pool,
        &options.method,
    )?;
    let candidates = candidate_list(
        &pool,
        options.constraints,
        request_text,
        &levels,
        fallback.budget,
    );
    let task = ProposeEncounter {
        request: request_text.to_string(),
        party: levels.clone(),
        band: options.band,
        budget: fallback.budget,
        method: options.method,
        cap: options.constraints.cap(),
        candidates,
        last: Mutex::new(None),
    };
    let run_opts = RunOptions {
        max_repairs: MAX_REPAIRS,
        sink: options.sink,
    };
    let (raw, attempts, usage, failure) = match run_task(runner, &task, &run_opts).await {
        Ok(r) => (Some(r.output), r.attempts, r.usage, None),
        Err(AiError::Validation { attempts, errors }) => {
            (task.recorded(), attempts, None, Some(errors.join("; ")))
        }
        Err(e) => (None, 0, None, Some(e.to_string())),
    };
    let mut proposal = EncounterProposal {
        source: EncounterSource::Deterministic,
        fallback_reason: None,
        ruleset: options.ruleset,
        method: options.method.label().to_string(),
        target: options.band,
        budget: fallback.budget,
        party: levels.clone(),
        monsters: fallback.monsters.clone(),
        report: fallback.report.clone(),
        tactics: String::new(),
        tactics_source: ProseSource::Template,
        removed: Vec::new(),
        removed_claims: Vec::new(),
        attempts,
        usage,
    };
    let Some(raw) = raw else {
        proposal.fallback_reason = Some(failure.unwrap_or_else(|| "no model output".to_string()));
        proposal.tactics = template_tactics(&proposal.monsters);
        return Ok(proposal);
    };
    let known = task.by_id();
    proposal.removed = raw
        .monsters
        .iter()
        .filter(|m| !known.contains_key(m.id.as_str()))
        .map(|m| RemovedMonster {
            id: m.id.clone(),
            reason: "not a candidate monster in the content".to_string(),
        })
        .collect();
    let counts = counts_of(&raw, &known);
    let total: u32 = counts.values().sum();
    let (lines, groups) = monster_lines(&known, &counts);
    let report = evaluate(&levels, &groups, &options.method)?;
    if lines.is_empty() || total > task.cap || report.band != options.band {
        proposal.fallback_reason = Some(match failure {
            Some(f) => format!("the model's encounter stayed invalid after the repair round: {f}"),
            None => "the model's encounter was not usable".to_string(),
        });
        proposal.tactics = template_tactics(&proposal.monsters);
        return Ok(proposal);
    }
    let (tactics, removed_claims) =
        verified_tactics(store, &raw.tactics, &lines, &report, &levels, task.budget);
    proposal.source = EncounterSource::Model;
    proposal.monsters = lines;
    proposal.report = report;
    proposal.removed_claims = removed_claims;
    proposal.tactics_source = if tactics.is_empty() {
        ProseSource::Template
    } else {
        ProseSource::Model
    };
    proposal.tactics = if tactics.is_empty() {
        template_tactics(&proposal.monsters)
    } else {
        tactics
    };
    Ok(proposal)
}

fn template_tactics(monsters: &[EncounterMonster]) -> String {
    let list = monsters
        .iter()
        .map(|m| format!("{} x {}", m.count, m.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Encounter: {list}. No verified tactics are available; run each creature from its own stat block.")
}

/// Keep the sentences whose numbers all match the data; report the numbers of the dropped ones.
fn verified_tactics(
    store: &ContentStore,
    text: &str,
    monsters: &[EncounterMonster],
    report: &EncounterReport,
    party: &[u8],
    budget: u64,
) -> (String, Vec<String>) {
    let mut guard = NumberGuard::new();
    let ints = [
        i64::try_from(report.total_xp).unwrap_or(i64::MAX),
        i64::try_from(report.adjusted_xp).unwrap_or(i64::MAX),
        i64::try_from(budget).unwrap_or(i64::MAX),
        i64::from(report.monster_count),
        i64::try_from(party.len()).unwrap_or(0),
    ];
    ints.into_iter().for_each(|v| guard.allow_int(v));
    party.iter().for_each(|l| guard.allow_int(i64::from(*l)));
    let t = &report.thresholds;
    [t.low, t.moderate, t.high]
        .into_iter()
        .chain(t.extreme)
        .for_each(|v| guard.allow_int(i64::try_from(v).unwrap_or(0)));
    for m in monsters {
        guard.allow_int(i64::from(m.count));
        guard.allow_int(i64::try_from(m.xp_each).unwrap_or(0));
        guard.allow_int(i64::try_from(m.total_xp).unwrap_or(0));
        guard.allow_text(&m.name);
        guard.allow_text(&m.cr.to_string());
        if let Some(d) = store
            .element(&m.id)
            .and_then(|e| MonsterData::from_element(e).ok())
        {
            guard.allow_int(i64::from(d.hp.average));
            d.ac.iter()
                .for_each(|a| guard.allow_int(i64::from(a.value)));
        }
    }
    let mut kept = Vec::new();
    let mut removed: BTreeSet<String> = BTreeSet::new();
    for sentence in sentences(text) {
        let bad = guard.unknown(sentence);
        if bad.is_empty() {
            kept.push(sentence);
        } else {
            removed.extend(bad);
        }
    }
    (kept.join(" "), removed.into_iter().collect())
}

/// Split after `.`, `!` or `?` that is followed by whitespace (decimals like 1.5 stay together).
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(pos, c)) in bytes.iter().enumerate() {
        let ends = matches!(c, '.' | '!' | '?')
            && !bytes.get(i + 1).is_some_and(|&(_, n)| !n.is_whitespace());
        if ends {
            let end = pos + c.len_utf8();
            let s = text[start..end].trim();
            if !s.is_empty() {
                out.push(s);
            }
            start = end;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}
