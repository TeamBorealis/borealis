//! Rules engine: element graph, grants/selects/stats, requirement evaluators, levelling,
//! multiclass and spellcasting, with ruleset profile hooks.

pub mod adapter;
pub mod attack_rules;
pub mod cantrips;
pub mod combat;
pub mod commands;
pub mod compat;
pub mod custom_rules;
pub mod diag;
pub mod engine;
pub mod eval;
pub mod gear;
pub mod graph;
pub mod ids;
pub mod invariants;
pub mod inventory;
pub mod levels;
pub mod money;
pub mod notes_d;
pub mod options;
pub mod packages;
pub mod play_rules;
pub mod profile;
pub mod provenance;
pub mod rule_view;
pub mod runtime;
pub mod settings;
pub mod snapshot;
pub mod spell_rules;
pub mod spells;
pub mod state;
pub mod stats;
pub mod tables;
pub mod universe;
pub mod wild_shape;

mod derive;
mod features;
mod retrain;
pub mod text;

pub use adapter::{Adaptation, EditionAdapter, PassThroughAdapter, StandardAdapter};
pub use compat::{CompatEntry, CompatibilityReport, EditionBadge};
pub use diag::{Diagnostic, Severity};
pub use engine::Engine;
pub use profile::{
    profile_for, AbilityGeneration, AbilityScoreSource, CasterRounding, Dnd2014, Dnd2024,
    ExhaustionModel, InspirationModel, ProficiencySource, RulesetProfile, SpellPreparation,
    SpellSwapQuery,
};
pub use provenance::{EvaluateOptions, StatName, StatTrace, StepKind, TraceStep};
pub use settings::{EngineSettings, HouseRules};
pub use snapshot::CharacterSnapshot;
pub use state::{CharacterState, Choice, ClassSlot, SlotKey};
pub use tables::ProfileTables;
pub use universe::EditionStatus;
