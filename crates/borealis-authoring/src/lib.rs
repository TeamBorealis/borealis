//! Deterministic content authoring for Borealis: the parts the homebrew creator (1.5.0,
//! `specs/homebrew-creator-spec.md` section 15.1) shares with the import pipeline of `borealis-assist`.
//!
//! * [`ids`]: deterministic element ids (`ID_<PREFIX>_<TYPE>_<NAME>`).
//! * [`caster`]: spellcasting tables and rules in the shape the bundled SRD classes use.
//! * [`check`]: the engine load check, the 1-20 level sweep and the balance report against SRD baselines.
//!
//! No AI and no network anywhere in this crate (G7): it depends on `borealis-content` and `borealis-engine` only.

pub mod caster;
pub mod check;
mod edition;
pub mod ids;

pub use edition::Edition;
