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
/// Hunter's Prey, Fiendish Resilience), F11, F12 and F15 fixed by det-5e-5). The test fails while any Borealis-bug entry is still observed, and when an
/// entry goes stale.
pub const FINDINGS: &[Finding] = &[
    ("F9", "Dnd2014 Paladin", &["casting"], &[1], IMPORTER,
     "PHB Paladin spellcasting block exists at level 1 (prepare limit 1, no slots); Spellcasting starts at level 2"),
    ("F8", "Half-Elf", &["select.Racial Trait", "select.List"], &[], Owner::Source,
     "encoding: the PHB Half-Elf's two +1 increases (`ability choose count 2`) import as one engine choice of two different abilities (a List of the 10 pairs, as the SRD 5.2.1 feats' increases); the SRD 5.1 pack offers two picks of five Racial Trait elements. The same allocations are reachable (source_truth ability (choice): 0 wrong) and the fill takes the same abilities; only the select shape differs"),
    ("F10", "Dnd2024 Druid", &["select.List"], &[], IMPORTER, "XPHB Circle of the Land land-type choice not offered"),
    ("F13", "Warlock", &["select.Class Feature", "select.Condition", "select.Option", "select.Feat", "skill.", "speed", "initiative"], &[], IMPORTER,
     "selects offer the same-named spell / option from several sources and editions, and accept it twice: the PHB Warlock cantrip select holds the bundled SRD and the 5etools Chill Touch (both picked, so no Eldritch Blast and no Agonizing Blast); the XPHB Warlock's invocation select holds PHB, TCE and XPHB copies (Agonizing Blast picked twice). Bundled 2024 types these selects Option, imports Class Feature"),
    ("F13", "Dnd2024 Sorcerer", &["select.Class Feature", "select.Option"], &[], IMPORTER,
     "XPHB Metamagic select offers PHB and XPHB copies of each option (Careful Spell picked twice); bundled types it Option, imports Class Feature"),
    ("F14", "Dnd2024 Fighter/Champion Dragonborn", &["select.Condition", "select.Species Trait Option"], &[], IMPORTER,
     "XPHB species with _versions (Dragonborn, Gnome, Goliath, Tiefling): the base species is offered without its ancestry / lineage choice (the versions are separate species) and with a free resistance choice not bound to it"),
    ("F14", "Gnome", &["select.Species Trait Option", "select.List"], &[], IMPORTER, "see F14 Dragonborn (Gnome lineage)"),
    ("F14", "Goliath", &["select.Species Trait Option"], &[], IMPORTER, "see F14 Dragonborn (Giant Ancestry)"),
    ("F14", "Dnd2024 Fighter/Champion Tiefling", &["select.Condition"], &[], IMPORTER, "see F14 Dragonborn (Fiendish Legacy resistance)"),
    ("S1", "Dnd2014 Fighter/Champion Dragonborn", &["select.Condition", "select.Racial Trait"], &[], Owner::Source,
     "5etools PHB Dragonborn encodes Draconic Ancestry as `resist` choose-from-5 damage types (the 10-colour table is entry text only); the SRD pack offers the 10 ancestries"),
    ("F16", "Dnd2014 Ranger/Hunter", &["select.Class Feature"], &[], IMPORTER,
     "PHB Favored Enemy / Natural Explorer state their choices in prose (no 5etools field); the SRD 5.1 pack's Ranger now offers Favored Enemy and Favored Terrain selects (content/srd-5.1/classes/ranger.json, A2 fix, uncommitted at the time of writing): the import offers none. Not carried over: the SRD selects are typed Class Feature over SRD option tags, the shape Fighting Style shares (its select comes from optionalfeatureProgression), see specs/handoffs/det-5e-5.md"),
];
