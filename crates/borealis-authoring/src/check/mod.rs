//! Checks of authored content (spec 8): the live pipeline of the homebrew creator ([`live_problems`]: form checks,
//! schema, the content validator over the loaded content plus the draft, rule problems, creator lints), the engine
//! checks run when the user pauses ([`deep_problems`]: a test character takes the element, the 1-20 level sweep of
//! classes and subclasses, choices that offer nothing, balance hints), and a plain sentence for every code
//! ([`messages`]). The load check, the sweep and the balance report are shared with the import pipeline.

pub mod balance;
mod clash;
mod deep;
mod engine;
mod fields;
mod lints;
mod live;
pub mod messages;
mod monster_messages;
mod problem;
pub mod sweep;

pub use balance::{balance_report, BalanceFlag, BalanceReport, Baselines, Severity};
pub use clash::{id_clash_problems, ID_CLASH, ID_CLASH_MESSAGE};
pub use deep::{deep_problems, sweep_targets, DeepInput};
pub use engine::{load_into_engine, take_element, CharacterCheck, EngineCheck};
pub use fields::is_dice;
pub use live::{issue_problems, live_problems, LiveInput, PackElement};
pub use problem::{dedupe, Problem, ProblemSeverity};
pub use sweep::{
    engine_sweep, sweep_engine, sweep_findings, SweepFinding, SweepIssue, SweepProblem, SweepTarget,
};
