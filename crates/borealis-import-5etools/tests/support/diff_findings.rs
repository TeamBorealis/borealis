//! Classified differences of the corpus differentials (acceptance step 3d), shared by
//! `borealis-import-5etools/tests/diff_corpus.rs` (F-full vs bundled SRD) and the F-full side of
//! `src-tauri/tests/diff_corpus.rs` (A-core vs F-full). SRD-overlap content only (PHB / XPHB
//! classes, species and backgrounds the SRD also has); findings that name other content live in
//! `specs/acceptance-private/diff/`.
#![allow(dead_code)]

use super::diff::{Finding, Owner};

const IMPORTER: Owner = Owner::Importer;

/// Every difference outside LOSSY / CORPUS, classified (2026-10-18 corpus run; the importer is
/// `borealis-import-5etools`, class features in `src/convert/feature_rules.rs` /
/// `feature.rs`). The test fails while any Borealis-bug entry is still observed, and when an
/// entry goes stale.
pub const FINDINGS: &[Finding] = &[
    ("F1", "Barbarian", &["ac"], &[], IMPORTER,
     "Unarmored Defense (5etools classFeature, both editions) converts to a feature without an AC formula: L1 AC 11 instead of 10+Dex+Con"),
    ("F2", "Sorcerer/Draconic", &["ac", "hp"], &[], IMPORTER,
     "Draconic Resilience (subclass feature) gives neither the 13+Dex AC nor +1 HP per sorcerer level"),
    ("F3", "Barbarian", &["speed"], &[], IMPORTER, "Fast Movement (+10 ft at level 5) not converted"),
    ("F3", "Monk", &["speed"], &[], IMPORTER, "Unarmored Movement (+10 ft from level 2) not converted"),
    ("F3", "Dnd2024 Ranger", &["speed"], &[], IMPORTER, "XPHB Roving (level 6: +10 ft, climb and swim speed) not converted"),
    ("F3", "Dnd2024 Rogue/Thief", &["speed"], &[], IMPORTER, "XPHB Thief Second-Story Work climb speed (level 3) not converted"),
    ("F4", "Barbarian", &["ability.", "hp", "save.", "skill."], &[20], IMPORTER,
     "Primal Champion (+4 Str and Con at level 20) not converted"),
    ("F4", "Dnd2024 Monk", &["ability.", "ac", "initiative", "skill.", "save.dexterity"], &[20], IMPORTER,
     "XPHB Body and Mind (+4 Dex and Wis at level 20) not converted"),
    ("F5", "Rogue/", &["select.Class Feature", "skill."], &[], IMPORTER,
     "Rogue Expertise (level 1 and 6) not converted into an expertise select"),
    ("F5", "Bard/", &["select.Class Feature", "skill."], &[], IMPORTER,
     "Bard Expertise (level 3/2 and 10/9) not converted into an expertise select"),
    ("F5", "Dnd2024 Ranger", &["select.Class Feature", "select.Language", "skill."], &[], IMPORTER,
     "XPHB Deft Explorer (expertise + two languages, level 2) not converted"),
    ("F6", "Bard/College of Lore", &["select.Proficiency"], &[], IMPORTER,
     "College of Lore Bonus Proficiencies (three skills of choice) not converted"),
    ("F6", "Dnd2024 Barbarian", &["select.Proficiency", "skill.intimidation"], &[], IMPORTER,
     "XPHB Primal Knowledge (one more barbarian skill, level 3) not converted"),
    ("F6", "Dnd2024 Rogue", &["select.Language"], &[], IMPORTER,
     "XPHB Thieves' Cant extra language not converted"),
    ("F7", "Monk", &["select.Proficiency"], &[], IMPORTER,
     "Monk tool choice 'one artisan's tool or musical instrument' becomes two selects (two tools granted)"),
    ("F8", "Half-Elf", &["ability.", "ac", "hp", "initiative", "save.", "skill.", "select.Ability Score Improvement", "select.Racial Trait"], &[], IMPORTER,
     "PHB Half-Elf ability choose count 2 is an Ability Score Improvement select whose two slots both accept the same ability (CON +2); the source asks for two different scores (src/convert/mech/ability.rs ability_choose)"),
    ("F9", "Dnd2014 Paladin", &["casting"], &[1], IMPORTER,
     "PHB Paladin spellcasting block exists at level 1 (prepare limit 1, no slots); Spellcasting starts at level 2"),
    ("F10", "Dnd2024 Cleric", &["select.Option"], &[], IMPORTER,
     "XPHB Divine Order (Protector / Thaumaturge, level 1) and Blessed Strikes (level 7) choices not offered"),
    ("F10", "Dnd2024 Druid", &["select.Option", "spell_choices", "skill."], &[], IMPORTER,
     "XPHB Primal Order (Magician / Warden) not offered (Magician's cantrip and Arcana/Nature bonus missing)"),
    ("F10", "Dnd2024 Druid", &["select.List"], &[], IMPORTER, "XPHB Circle of the Land land-type choice not offered"),
    ("F10", "Dnd2024 Ranger", &["select.Option"], &[], IMPORTER, "XPHB Hunter's Prey option choice not offered"),
    ("F10", "Dnd2024 Warlock", &["select.List"], &[11, 20], IMPORTER,
     "XPHB Fiendish Resilience damage-type choice (level 10) not offered"),
    ("F11", "Dnd2014 Monk", &["save."], &[20], IMPORTER, "Diamond Soul (all saves, level 14) not converted"),
    ("F11", "Dnd2024 Monk", &["save."], &[20], IMPORTER, "XPHB Disciplined Survivor (all saves, level 14) not converted"),
    ("F11", "Rogue/", &["save."], &[20], IMPORTER, "Slippery Mind (Wis save; XPHB also Cha, level 15) not converted"),
    ("F11", "Dnd2014 Paladin", &["save."], &[], IMPORTER, "Aura of Protection (+Cha to saves, level 6) not converted"),
    ("F15", "Dnd2014 Bard", &["initiative"], &[], IMPORTER,
     "Jack of All Trades (half proficiency on unproficient checks, initiative included, level 2) not converted"),
    ("F12", "Dwarf Criminal", &["initiative"], &[], IMPORTER,
     "XPHB Alert (origin feat of Criminal) initiative bonus (+proficiency) not applied"),
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
];
