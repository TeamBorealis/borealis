//! Deterministic content authoring for Borealis: the parts the homebrew creator (1.5.0,
//! `specs/homebrew-creator-spec.md` section 15.1) shares with the import pipeline of `borealis-assist`.
//!
//! * [`ids`]: deterministic element ids (`ID_<PREFIX>_<TYPE>_<NAME>`).
//! * [`caster`]: spellcasting tables and rules in the shape the bundled SRD classes use.
//! * [`check`]: the engine load check, the 1-20 level sweep and the balance report against SRD baselines.
//! * [`expr_build`]: the condition, tag filter, "while wearing" and formula builders and their exact text.
//! * [`blocks`]: the rule block catalog (every rule kind the engine reads) and the blocks' plain sentences.
//! * [`stats`]: the statistic catalog of the statistic picker.
//!
//! No AI and no network anywhere in this crate (G7): it depends on `borealis-content` and `borealis-engine` only.

pub mod blocks;
pub mod caster;
pub mod check;
mod edition;
pub mod expr_build;
pub mod ids;
pub mod stats;

pub use edition::Edition;
