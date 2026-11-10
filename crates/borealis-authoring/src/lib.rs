//! Deterministic content authoring for Borealis: the parts the homebrew creator (1.5.0,
//! `specs/homebrew-creator-spec.md` section 15.1) shares with the import pipeline of `borealis-assist`.
//!
//! * [`ids`]: deterministic element ids (`ID_<PREFIX>_<TYPE>_<NAME>`).
//! * [`caster`]: spellcasting tables and rules in the shape the bundled SRD classes use.
//! * [`check`]: the engine load check, the 1-20 level sweep and the balance report against SRD baselines.
//! * [`expr_build`]: the condition, tag filter, "while wearing" and formula builders and their exact text.
//! * [`blocks`]: the rule block catalog (every rule kind the engine reads) and the blocks' plain sentences.
//! * [`stats`]: the statistic catalog of the statistic picker.
//! * [`form`]: the creator's form model (fields bound to model locations, rule blocks, the preserved bucket) and the
//!   element/form round trip.
//! * [`rich`]: the description tree and its one HTML writer (`borealis_content::rich`, re-exported here; see
//!   `specs/handoffs/hc-wb04.md` for why it stays in `borealis-content`).
//!
//! No AI and no network anywhere in this crate (G7): it depends on `borealis-content` and `borealis-engine` only.

pub mod blocks;
pub mod caster;
pub mod check;
mod edition;
pub mod expr_build;
pub mod form;
pub mod ids;
pub mod stats;

pub use borealis_content::rich;

pub use edition::Edition;
