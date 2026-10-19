//! Classified differences of the corpus differentials (acceptance step 3d), shared by
//! `borealis-import-5etools/tests/diff_corpus.rs` (F-full vs bundled SRD) and the F-full side of
//! `src-tauri/tests/diff_corpus.rs` (A-core vs F-full). SRD-overlap content only (PHB / XPHB
//! classes, species and backgrounds the SRD also has); findings that name other content live in
//! `specs/acceptance-private/diff/`.
#![allow(dead_code)]

use super::diff::{Finding, Owner};

const IMPORTER: Owner = Owner::Importer;

/// Every difference outside LOSSY / CORPUS, classified (2026-10-18 corpus run; the importer is
/// `borealis-import-5etools`, class features in `src/convert/feature_rules.rs`,
/// `src/srd_features.rs` and `feature.rs`; F1-F7, F10 (Divine / Primal Order, Blessed Strikes,
/// Hunter's Prey, Fiendish Resilience), F11, F12 and F15 fixed by det-5e-5; F9 (half-caster
/// Spellcasting feature), F10 (Circle of the Land land type, Nature's Ward), F14 (species
/// `_versions` as options), F16 (Favored Enemy / Favored Terrain) and the importer half of F13
/// fixed by det-5e-6; the PHB Warlock half of F13 fixed by the engine's T11 equivalence classes, 8bdb080;
/// F17 (Hunter's Prey family) and F18 (Dragon Ancestor) fixed by det-5e-9: the SRD's own
/// stat-only Archetype Feature option selects are taken, `SrdFeatures::portable_feature`). The test fails while any Borealis-bug entry is still observed, and when an
/// entry goes stale.
pub const FINDINGS: &[Finding] = &[
    ("F8", "Half-Elf", &["select.Racial Trait", "select.List"], &[], Owner::Source,
     "encoding: the PHB Half-Elf's two +1 increases (`ability choose count 2`) import as one engine choice of two different abilities (a List of the 10 pairs, as the SRD 5.2.1 feats' increases); the SRD 5.1 pack offers two picks of five Racial Trait elements. The same allocations are reachable (source_truth ability (choice): 0 wrong) and the fill takes the same abilities; only the select shape differs"),
    ("F13", "Dnd2024 Sorcerer", &["select.Class Feature", "select.Option"], &[], Owner::Engine,
     "XPHB Metamagic select offers PHB and XPHB copies of each option (Careful Spell picked twice) (both sides now type it `Option`; det-5e-6)"),
    ("S1", "Dnd2014 Fighter/Champion Dragonborn", &["select.Condition", "select.Racial Trait"], &[], Owner::Source,
     "5etools PHB Dragonborn encodes Draconic Ancestry as `resist` choose-from-5 damage types (the 10-colour table is entry text only); the SRD pack offers the 10 ancestries"),
];
