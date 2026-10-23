//! Stable semantic AcroForm field ids. THIS IS A CONTRACT: ids are lower-case ASCII segments
//! (`a-z`, `0-9`, `_`) joined by `.`, never positional (`p1_b3`), and never change once shipped.
//! Indexes are 1-based. Continuation pages append `@2`, `@3`... to the id (not generated here).
//!
//! Fixed ids are the `pub const`s below; indexed ids come from the builder functions. Every id a
//! sheet needs is enumerated by [`SheetData::fields`], which is what the layout and the tests use.

use crate::model_types::{CasterData, Edition, EditionFields, SheetData};

/// Widget kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Single-line text.
    Text,
    /// Multi-line text.
    Multiline,
    /// Check box.
    Check,
}

/// One form field of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    /// The semantic id.
    pub id: String,
    /// Widget kind.
    pub kind: FieldKind,
    /// A play-time write-in: left blank when exported.
    pub play: bool,
}

// ---- common core (both editions) -------------------------------------------------------------
/// Character name.
pub const CHAR_NAME: &str = "char.name";
/// Player name.
pub const CHAR_PLAYER: &str = "char.player";
/// Class and level line.
pub const CHAR_CLASS: &str = "char.class";
/// Subclass line.
pub const CHAR_SUBCLASS: &str = "char.subclass";
/// Total level.
pub const CHAR_LEVEL: &str = "char.level";
/// Background.
pub const CHAR_BACKGROUND: &str = "char.background";
/// Alignment.
pub const CHAR_ALIGNMENT: &str = "char.alignment";
/// Experience points.
pub const CHAR_XP: &str = "char.xp";
/// XP for the next level.
pub const CHAR_XP_NEXT: &str = "char.xp_next";
/// Size.
pub const CHAR_SIZE: &str = "char.size";
/// Deity.
pub const CHAR_DEITY: &str = "char.deity";
/// Proficiency bonus.
pub const PROF_BONUS: &str = "prof.bonus";
/// Armour class.
pub const AC: &str = "ac";
/// Conditional AC note.
pub const AC_NOTE: &str = "ac.note";
/// Initiative.
pub const INITIATIVE: &str = "init";
/// Speed line.
pub const SPEED: &str = "speed";
/// Maximum hit points.
pub const HP_MAX: &str = "hp.max";
/// Current hit points (play).
pub const HP_CURRENT: &str = "hp.current";
/// Temporary hit points (play).
pub const HP_TEMP: &str = "hp.temp";
/// Hit dice summary ("5d6").
pub const HD_TOTAL: &str = "hd.total";
/// Armour proficiencies.
pub const PROF_ARMOR: &str = "prof.armor";
/// Weapon proficiencies.
pub const PROF_WEAPONS: &str = "prof.weapons";
/// Tool proficiencies.
pub const PROF_TOOLS: &str = "prof.tools";
/// Languages.
pub const PROF_LANGUAGES: &str = "prof.languages";
/// Senses (Borealis extra).
pub const SENSES: &str = "senses";
/// Resistances, immunities, vulnerabilities (Borealis extra).
pub const DEFENSES: &str = "defenses";
/// Carried weight.
pub const CARRY_WEIGHT: &str = "carry.weight";
/// Carrying capacity.
pub const CARRY_CAPACITY: &str = "carry.capacity";
/// Coin ids in CP SP EP GP PP order.
pub const COINS: [&str; 5] = ["coin.cp", "coin.sp", "coin.ep", "coin.gp", "coin.pp"];
/// Coin ids of the 2014 page-1 equipment summary (Aurora's `details_coinage_*`), CP SP EP GP PP.
pub const SUMMARY_COINS: [&str; 5] = [
    "summary.coin.cp",
    "summary.coin.sp",
    "summary.coin.ep",
    "summary.coin.gp",
    "summary.coin.pp",
];
/// Inventory lines of the 2014 page-1 equipment summary (Aurora's character page lists the
/// first 15 inventory rows).
pub const SUMMARY_ROWS: usize = 15;
/// Story fields (text), in sheet order.
pub const STORY: [&str; 20] = [
    "story.gender",
    "story.age",
    "story.height",
    "story.weight",
    "story.eyes",
    "story.skin",
    "story.hair",
    "story.traits",
    "story.ideals",
    "story.bonds",
    "story.flaws",
    "story.trinket",
    "story.backstory",
    "story.organization",
    "story.allies",
    "story.treasure",
    "story.quest",
    "story.equipment_notes",
    "story.additional_features",
    "story.notes",
];
/// Fixed ids shared by both editions that are always present.
pub const FIXED_COMMON: &[&str] = &[
    CHAR_NAME,
    CHAR_PLAYER,
    CHAR_CLASS,
    CHAR_SUBCLASS,
    CHAR_LEVEL,
    CHAR_BACKGROUND,
    CHAR_ALIGNMENT,
    CHAR_XP,
    CHAR_XP_NEXT,
    CHAR_SIZE,
    CHAR_DEITY,
    PROF_BONUS,
    AC,
    AC_NOTE,
    INITIATIVE,
    SPEED,
    HP_MAX,
    HP_CURRENT,
    HP_TEMP,
    HD_TOTAL,
    PROF_ARMOR,
    PROF_WEAPONS,
    PROF_TOOLS,
    PROF_LANGUAGES,
    SENSES,
    DEFENSES,
    CARRY_WEIGHT,
    CARRY_CAPACITY,
];

// ---- 2014 only -------------------------------------------------------------------------------
/// Race.
pub const RACE: &str = "char.race";
/// Sub-race.
pub const SUBRACE: &str = "char.subrace";
/// Inspiration.
pub const INSPIRATION: &str = "insp";
/// Fixed ids present only on 2014 sheets.
pub const FIXED_2014: &[&str] = &[RACE, SUBRACE, INSPIRATION];

// ---- 2024 only -------------------------------------------------------------------------------
/// Species.
pub const SPECIES: &str = "char.species";
/// Lineage.
pub const LINEAGE: &str = "char.lineage";
/// Origin feat.
pub const ORIGIN_FEAT: &str = "char.origin_feat";
/// Heroic Inspiration.
pub const HEROIC_INSPIRATION: &str = "insp.heroic";
/// Shield equipped tick.
pub const AC_SHIELD: &str = "ac.shield";
/// Armour training ticks.
pub const ARMOR_TRAINING: [&str; 4] = [
    "armor.light",
    "armor.medium",
    "armor.heavy",
    "armor.shields",
];
/// Fixed ids present only on 2024 sheets.
pub const FIXED_2024: &[&str] = &[
    SPECIES,
    LINEAGE,
    ORIGIN_FEAT,
    HEROIC_INSPIRATION,
    AC_SHIELD,
    "armor.light",
    "armor.medium",
    "armor.heavy",
    "armor.shields",
];

// ---- Borealis extras -------------------------------------------------------------------------
/// Companion name.
pub const COMPANION: &str = "xtra.companion";

// ---- indexed builders ------------------------------------------------------------------------
/// `abil.str.score`.
#[must_use]
pub fn abil_score(key: &str) -> String {
    format!("abil.{key}.score")
}
/// `abil.str.mod`.
#[must_use]
pub fn abil_mod(key: &str) -> String {
    format!("abil.{key}.mod")
}
/// `save.str`.
#[must_use]
pub fn save(key: &str) -> String {
    format!("save.{key}")
}
/// `save.str.prof` (check box).
#[must_use]
pub fn save_prof(key: &str) -> String {
    format!("save.{key}.prof")
}
/// `skill.arcana`.
#[must_use]
pub fn skill(key: &str) -> String {
    format!("skill.{key}")
}
/// `skill.arcana.prof` (check box).
#[must_use]
pub fn skill_prof(key: &str) -> String {
    format!("skill.{key}.prof")
}
/// `skill.arcana.exp` (check box).
#[must_use]
pub fn skill_exp(key: &str) -> String {
    format!("skill.{key}.exp")
}
/// `passive.perception`.
#[must_use]
pub fn passive(key: &str) -> String {
    format!("passive.{key}")
}
/// `hd.d6.total`.
#[must_use]
pub fn hd_total(die: u32) -> String {
    format!("hd.d{die}.total")
}
/// `hd.d6.spent.1` (check box).
#[must_use]
pub fn hd_spent(die: u32, n: u32) -> String {
    format!("hd.d{die}.spent.{n}")
}
/// `death.success.1` (check box).
#[must_use]
pub fn death_success(n: u32) -> String {
    format!("death.success.{n}")
}
/// `death.failure.1` (check box).
#[must_use]
pub fn death_failure(n: u32) -> String {
    format!("death.failure.{n}")
}
/// `atk.1.name`; `part` is `name`, `bonus`, `damage`, `range`, `notes` or `mastery` (2024).
#[must_use]
pub fn atk(n: usize, part: &str) -> String {
    format!("atk.{n}.{part}")
}
/// `feat.12.name` / `feat.12.text`.
#[must_use]
pub fn feat(n: usize, part: &str) -> String {
    format!("feat.{n}.{part}")
}
/// `use.1.name`.
#[must_use]
pub fn use_name(n: usize) -> String {
    format!("use.{n}.name")
}
/// `use.1.box.2` (check box).
#[must_use]
pub fn use_box(n: usize, m: i32) -> String {
    format!("use.{n}.box.{m}")
}
/// `equip.1.name`; `part` is `name`, `qty` or `weight`.
#[must_use]
pub fn equip(n: usize, part: &str) -> String {
    format!("equip.{n}.{part}")
}
/// `summary.equip.1.name`: line 1 of the 2014 page-1 equipment summary; `part` is `qty` or
/// `name`.
#[must_use]
pub fn summary_equip(n: usize, part: &str) -> String {
    format!("summary.equip.{n}.{part}")
}
/// `attune.1` (check box) and `attune.1.name`.
#[must_use]
pub fn attune(n: usize) -> String {
    format!("attune.{n}")
}
/// `attune.1.name`.
#[must_use]
pub fn attune_name(n: usize) -> String {
    format!("attune.{n}.name")
}
/// `cond.poisoned` (check box); spaces become `_`.
#[must_use]
pub fn cond(name: &str) -> String {
    format!("cond.{}", name.to_lowercase().replace(' ', "_"))
}
/// `exh.3` (check box).
#[must_use]
pub fn exh(n: u32) -> String {
    format!("exh.{n}")
}
/// `mastery.1` (2024).
#[must_use]
pub fn mastery(n: usize) -> String {
    format!("mastery.{n}")
}
/// `spell.1.class`; `part` is `class`, `ability`, `mod`, `dc`, `attack`, `prepared`.
#[must_use]
pub fn spell_head(caster: usize, part: &str) -> String {
    format!("spell.{caster}.{part}")
}
/// `spell.1.slot.3.total`.
#[must_use]
pub fn spell_slot_total(caster: usize, level: usize) -> String {
    format!("spell.{caster}.slot.{level}.total")
}
/// `spell.1.slot.3.used.2` (check box).
#[must_use]
pub fn spell_slot_used(caster: usize, level: usize, n: i32) -> String {
    format!("spell.{caster}.slot.{level}.used.{n}")
}
/// `spell.1.pact.total`, `spell.1.pact.level`.
#[must_use]
pub fn spell_pact(caster: usize, part: &str) -> String {
    format!("spell.{caster}.pact.{part}")
}
/// `spell.1.pact.used.1` (check box).
#[must_use]
pub fn spell_pact_used(caster: usize, n: i32) -> String {
    format!("spell.{caster}.pact.used.{n}")
}
/// `spell.1.list.17.name`; `part` is `name`, `level`, `prep` (check box), `time`, `range`,
/// `comp`, `effect`.
#[must_use]
pub fn spell_row(caster: usize, n: usize, part: &str) -> String {
    format!("spell.{caster}.list.{n}.{part}")
}
/// `spell.pool.slot.3.total`.
#[must_use]
pub fn pool_slot_total(level: usize) -> String {
    format!("spell.pool.slot.{level}.total")
}
/// `spell.pool.slot.3.used.2` (check box).
#[must_use]
pub fn pool_slot_used(level: usize, n: i32) -> String {
    format!("spell.pool.slot.{level}.used.{n}")
}
/// `spell.pool.level`.
pub const POOL_LEVEL: &str = "spell.pool.level";

// ---- enumeration -----------------------------------------------------------------------------
struct Out(Vec<FieldSpec>);

impl Out {
    fn push(&mut self, id: impl Into<String>, kind: FieldKind, play: bool) {
        self.0.push(FieldSpec {
            id: id.into(),
            kind,
            play,
        });
    }
    fn text(&mut self, id: impl Into<String>) {
        self.push(id, FieldKind::Text, false);
    }
    fn check(&mut self, id: impl Into<String>) {
        self.push(id, FieldKind::Check, false);
    }
    fn play_check(&mut self, id: impl Into<String>) {
        self.push(id, FieldKind::Check, true);
    }
}

fn caster_fields(out: &mut Out, n: usize, c: &CasterData) {
    for part in ["class", "ability", "mod", "dc", "attack", "prepared"] {
        out.text(spell_head(n, part));
    }
    // Pooled casters print one shared slot table (`pool.*` ids), not per-class trackers.
    let class_slots: &[i32] = if c.slots_pooled { &[] } else { &c.slots };
    for (i, total) in class_slots.iter().enumerate() {
        let level = i + 1;
        out.text(spell_slot_total(n, level));
        for k in 1..=*total {
            out.play_check(spell_slot_used(n, level, k));
        }
    }
    if let Some(p) = c.pact {
        out.text(spell_pact(n, "total"));
        out.text(spell_pact(n, "level"));
        for k in 1..=p.count {
            out.play_check(spell_pact_used(n, k));
        }
    }
    let rows = c.cantrips.iter().chain(&c.spells);
    for (i, _) in rows.enumerate() {
        let r = i + 1;
        for part in ["name", "level", "time", "range", "comp", "effect"] {
            out.text(spell_row(n, r, part));
        }
        out.check(spell_row(n, r, "prep"));
    }
}

impl SheetData {
    /// Every form field this sheet needs, in a stable order. Ids are unique.
    #[must_use]
    pub fn fields(&self) -> Vec<FieldSpec> {
        let mut o = Out(Vec::new());
        for id in FIXED_COMMON {
            o.push(
                *id,
                FieldKind::Text,
                matches!(*id, "hp.current" | "hp.temp"),
            );
        }
        for a in &self.abilities {
            o.text(abil_score(a.key));
            o.text(abil_mod(a.key));
            o.text(save(a.key));
            o.check(save_prof(a.key));
        }
        for s in &self.skills {
            o.text(skill(&s.key));
            o.check(skill_prof(&s.key));
            o.check(skill_exp(&s.key));
        }
        for p in &self.passives {
            o.text(passive(&p.key));
        }
        for h in &self.combat.hit_dice {
            o.text(hd_total(h.die));
            for n in 1..=h.count {
                o.play_check(hd_spent(h.die, n));
            }
        }
        for n in 1..=3 {
            o.play_check(death_success(n));
            o.play_check(death_failure(n));
        }
        for (i, _) in self.combat.attacks.iter().enumerate() {
            for part in ["name", "bonus", "damage", "range", "notes"] {
                o.text(atk(i + 1, part));
            }
        }
        for (i, _) in self.features.iter().enumerate() {
            o.text(feat(i + 1, "name"));
            o.push(feat(i + 1, "text"), FieldKind::Multiline, false);
        }
        for (i, u) in self.limited_uses.iter().enumerate() {
            o.text(use_name(i + 1));
            for m in 1..=u.max {
                o.play_check(use_box(i + 1, m));
            }
        }
        for (i, _) in self.gear.items.iter().enumerate() {
            for part in ["name", "qty", "weight"] {
                o.text(equip(i + 1, part));
            }
        }
        for c in COINS {
            o.text(c);
        }
        if self.edition == Edition::Dnd2014 {
            for c in SUMMARY_COINS {
                o.text(c);
            }
            // Every summary line is a field (Aurora's page 1 has 15 rows); lines past the last
            // item are blank write-ins.
            for n in 1..=SUMMARY_ROWS {
                o.text(summary_equip(n, "qty"));
                o.text(summary_equip(n, "name"));
            }
        }
        for n in 1..=self.gear.attunement_limit as usize {
            o.play_check(attune(n));
            o.text(attune_name(n));
        }
        for c in self.play.conditions {
            o.play_check(cond(c));
        }
        for n in 1..=self.play.exhaustion.max_levels {
            o.play_check(exh(n));
        }
        for (i, c) in self.spellcasting.casters.iter().enumerate() {
            caster_fields(&mut o, i + 1, c);
        }
        if self.spellcasting.pooled.is_some() {
            o.text(POOL_LEVEL);
            for (i, total) in self
                .spellcasting
                .pooled
                .iter()
                .flat_map(|p| p.slots)
                .enumerate()
            {
                o.text(pool_slot_total(i + 1));
                for k in 1..=total {
                    o.play_check(pool_slot_used(i + 1, k));
                }
            }
        }
        for id in STORY {
            o.push(id, FieldKind::Multiline, false);
        }
        o.text(COMPANION);
        let mut emit = |id: &str, k: FieldKind, p: bool| o.push(id, k, p);
        match &self.edition_fields {
            EditionFields::Y2014(_) => crate::model_2014::fields(&mut emit),
            EditionFields::Y2024(f) => {
                crate::model_2024::fields(f, self.combat.attacks.len(), &mut emit);
            }
        }
        o.0
    }
}
