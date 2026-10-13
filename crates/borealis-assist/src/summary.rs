//! Engine-computed summaries: stat tables, deltas between two snapshots, per-level digests.
//!
//! Everything numeric the assistant shows comes from here (or from a stat trace), never from the
//! model.

use std::collections::BTreeMap;

use borealis_engine::snapshot::CharacterSnapshot;
use serde::{Deserialize, Serialize};

/// One statistic that differs between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDelta {
    /// Display name, e.g. `armor class` or `Wizard spell save DC`.
    pub stat: String,
    /// Value before.
    pub before: i64,
    /// Value after.
    pub after: i64,
    /// `after - before`.
    pub change: i64,
}

/// What an operation changed, as computed by the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effects {
    /// Changed statistics, in a stable order.
    pub stats: Vec<StatDelta>,
    /// Names of features, traits and feats that appear in the new snapshot only.
    pub gained: Vec<String>,
}

/// The headline statistics of a snapshot.
#[must_use]
pub fn stat_table(snap: &CharacterSnapshot) -> BTreeMap<String, i64> {
    let mut t = BTreeMap::new();
    t.insert("character level".to_string(), i64::from(snap.level));
    t.insert("hit points".to_string(), i64::from(snap.hit_points));
    t.insert("armor class".to_string(), i64::from(snap.armor_class));
    t.insert("initiative".to_string(), i64::from(snap.initiative));
    t.insert(
        "proficiency bonus".to_string(),
        i64::from(snap.proficiency_bonus),
    );
    t.insert("walking speed".to_string(), i64::from(snap.speed.walk));
    for a in &snap.abilities {
        t.insert(format!("{} score", a.name), i64::from(a.score));
        t.insert(format!("{} modifier", a.name), i64::from(a.modifier));
        t.insert(format!("{} saving throw", a.name), i64::from(a.save));
    }
    for c in &snap.spellcasting {
        t.insert(format!("{} spell save DC", c.name), i64::from(c.dc));
        t.insert(
            format!("{} spell attack bonus", c.name),
            i64::from(c.attack),
        );
        if let Some(p) = c.prepare_limit {
            t.insert(format!("{} prepared spells", c.name), i64::from(p));
        }
        for (i, n) in c.slots.iter().enumerate() {
            if *n > 0 {
                t.insert(
                    format!("{} level {} spell slots", c.name, i + 1),
                    i64::from(*n),
                );
            }
        }
    }
    if let Some(m) = &snap.multiclass_slots {
        for (i, n) in m.slots.iter().enumerate() {
            if *n > 0 {
                t.insert(
                    format!("multiclass level {} spell slots", i + 1),
                    i64::from(*n),
                );
            }
        }
    }
    for a in &snap.attacks {
        t.insert(format!("attack bonus: {}", a.name), i64::from(a.attack));
    }
    t
}

/// The differences between `before` and `after` (only changed statistics are listed).
#[must_use]
pub fn effects(before: &CharacterSnapshot, after: &CharacterSnapshot) -> Effects {
    let b = stat_table(before);
    let a = stat_table(after);
    let mut stats = Vec::new();
    for (stat, av) in &a {
        let bv = b.get(stat).copied().unwrap_or(0);
        if bv != *av {
            stats.push(StatDelta {
                stat: stat.clone(),
                before: bv,
                after: *av,
                change: av - bv,
            });
        }
    }
    for (stat, bv) in &b {
        if !a.contains_key(stat) {
            stats.push(StatDelta {
                stat: stat.clone(),
                before: *bv,
                after: 0,
                change: -bv,
            });
        }
    }
    let known: Vec<&str> = before.features.iter().map(|f| f.id.as_str()).collect();
    let gained = after
        .features
        .iter()
        .filter(|f| !known.contains(&f.id.as_str()))
        .map(|f| f.name.clone())
        .collect();
    Effects { stats, gained }
}

/// A per-level digest of a character, all computed by the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelSummary {
    /// Character level.
    pub level: u32,
    /// `Fighter 3 (Champion)` style entries, one per class.
    pub classes: Vec<String>,
    /// Hit point maximum.
    pub hit_points: i32,
    /// Armor class.
    pub armor_class: i32,
    /// Initiative.
    pub initiative: i32,
    /// Proficiency bonus.
    pub proficiency_bonus: i32,
    /// Ability scores in STR..CHA order.
    pub abilities: Vec<(String, i32)>,
    /// Spell save DC per spellcasting class.
    pub spell_save_dc: Vec<(String, i32)>,
    /// Features, traits and feats new at this level.
    pub gained: Vec<String>,
}

/// Digest `after`, listing the features that are new compared to `before`.
#[must_use]
pub fn level_summary(
    before: Option<&CharacterSnapshot>,
    after: &CharacterSnapshot,
) -> LevelSummary {
    let classes = after
        .classes
        .iter()
        .map(|c| {
            let name = c.class.as_ref().map_or("(no class)", |e| e.name.as_str());
            let subs: Vec<&str> = c.archetypes.iter().map(|a| a.name.as_str()).collect();
            if subs.is_empty() {
                format!("{name} {}", c.level)
            } else {
                format!("{name} {} ({})", c.level, subs.join(", "))
            }
        })
        .collect();
    let gained = before.map_or_else(Vec::new, |b| effects(b, after).gained);
    LevelSummary {
        level: after.level,
        classes,
        hit_points: after.hit_points,
        armor_class: after.armor_class,
        initiative: after.initiative,
        proficiency_bonus: after.proficiency_bonus,
        abilities: after
            .abilities
            .iter()
            .map(|a| (a.name.clone(), a.score))
            .collect(),
        spell_save_dc: after
            .spellcasting
            .iter()
            .map(|c| (c.name.clone(), c.dc))
            .collect(),
        gained,
    }
}

impl LevelSummary {
    /// One compact line for prompts and logs.
    #[must_use]
    pub fn line(&self) -> String {
        let abil: Vec<String> = self
            .abilities
            .iter()
            .map(|(n, s)| {
                format!(
                    "{} {s}",
                    n.chars().take(3).collect::<String>().to_uppercase()
                )
            })
            .collect();
        format!(
            "level {}: {}; HP {}, AC {}, proficiency +{}, {}",
            self.level,
            self.classes.join(" / "),
            self.hit_points,
            self.armor_class,
            self.proficiency_bonus,
            abil.join(" ")
        )
    }
}

/// Every number in a [`LevelSummary`]/[`Effects`]-style table, for the number guard.
pub fn allow_effects(g: &mut crate::numbers::NumberGuard, e: &Effects) {
    for d in &e.stats {
        g.allow_int(d.before);
        g.allow_int(d.after);
        g.allow_int(d.change);
        g.allow_text(&d.stat);
    }
    for n in &e.gained {
        g.allow_text(n);
    }
}
