//! Checks of authored content against the engine: the load check (a test character takes the element), the
//! 1-20 level sweep of classes and subclasses, and the balance report against SRD baselines.

pub mod balance;
mod engine;
pub mod sweep;

pub use balance::{balance_report, BalanceFlag, BalanceReport, Baselines, Severity};
pub use engine::{load_into_engine, CharacterCheck, EngineCheck};
pub use sweep::{engine_sweep, SweepIssue, SweepTarget};
