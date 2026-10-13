//! Engine-grounded AI orchestration for Borealis.
//!
//! The rule of this crate: the model only proposes element ids, choices and prose. The engine
//! validates every proposal and recomputes every number; nothing numeric the model claims is
//! shown unless the engine computed it.
//!
//! * [`explain`]: `explain_number`, prose over a stat provenance trace, number-checked, with
//!   a deterministic template fallback and an offline variant.
//! * [`advisor`]: `level_up_advisor`, ranks the engine's legal next-level options and
//!   attaches engine-computed deltas.
//! * [`planner`]: `build_planner`, proposes a per-level plan, replays and repairs it with
//!   engine feedback. Plans are data ([`plan::Plan`]).
//!
//! * [`encounter`]: the encounter balancer: a deterministic monster index, evaluator and
//!   budget-filling suggester, plus `encounter_assistant`, where the model picks from a pre-filtered
//!   candidate list and the engine recomputes the difficulty (one repair round, then a
//!   deterministic fallback).
//!
//! * [`homebrew`]: `homebrew_with_balance`: generation with a repair loop, then a deterministic
//!   balance report against SRD baselines computed from the enabled packs and an engine load check.
//! * [`import`]: `import_text_to_pack`: text or PDF text layer to a `user-import` pack, chunked,
//!   validated, repaired, with a per-chunk converted/needs-review/failed report.
//!
//! * [`import::anything`]: `import_anything`: detect, plan, generate, validate (engine sweep, level
//!   1 to 20), review, revise and commit for classes, subclasses and every other element kind.
//!
//! All model calls go through [`borealis_ai::task::AiRunner`], so tests use a fake runner and no
//! real CLI is ever started.

pub mod advisor;
pub mod balance;
pub mod encounter;
pub mod error;
pub mod explain;
pub mod homebrew;
pub mod import;
pub mod numbers;
pub mod options;
pub mod plan;
pub mod planner;
pub mod summary;

use serde::{Deserialize, Serialize};

pub use advisor::{level_up_advisor, AdvisorOptions, LevelUpAdvice};
pub use balance::{
    balance_report, load_into_engine, BalanceFlag, BalanceReport, Baselines, EngineCheck, Severity,
};
pub use encounter::{
    encounter_assistant, evaluate, monster_index, suggest_deterministic, Constraints,
    EncounterOptions, EncounterProposal, Method, MonsterEntry, Party, Suggestion,
};
pub use error::AssistError;
pub use explain::{explain_number, explain_number_offline, Explanation};
pub use homebrew::{homebrew_with_balance, HomebrewOutcome, HomebrewRequest, PackContext};
pub use import::anything::{
    choose_parent, continue_draft, detect_source, finalize_draft, import_anything, plan_import,
    revise_draft, AnythingRequest, Detection, Edition, ImportDraft, ImportKind, ImportPlan,
    ImportStep, VisionRunner,
};
pub use import::{
    import_text_to_pack, ChunkReport, ChunkStatus, ImportOutcome, ImportRequest, ImportSource,
};
pub use options::{apply_option, legal_options, LevelPath, LevelUpOption};
pub use plan::{apply_plan, replay_step, Plan, PlanStep};
pub use planner::{build_planner, BuildPlan, PlannerOptions};

/// Where a piece of prose came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProseSource {
    /// Written by the model and verified against engine data.
    Model,
    /// Built deterministically from engine data.
    Template,
}
