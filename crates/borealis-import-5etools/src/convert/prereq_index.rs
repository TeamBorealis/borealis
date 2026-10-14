//! Name lookups that turn 5etools prerequisite references into element ids.
//!
//! Built once after races, classes, subclasses, features and backgrounds are converted (so
//! only ids that were really emitted are used) and before feats and optional features.
//! Feats, optional features and spells are converted later; their ids come from the link
//! table, which only holds entities of this import.

use super::{feature, Ent, Shared, State};
use crate::model::{self, Feature, Subclass};
use crate::srd::SrdKind;
use crate::text::RefKind;
use crate::util::key;
use borealis_content::Ruleset;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A spell as the `choose` filters of prerequisites see it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpellRow {
    /// Lower-case name.
    pub name: String,
    /// Spell level (0 = cantrip).
    pub level: u32,
    /// Lower-case class names whose list has the spell.
    pub classes: Vec<String>,
    /// Deals damage (`damageInflict`).
    pub damage: bool,
    /// Spell attack codes (`M`, `R`, `O`), upper-case.
    pub attacks: Vec<String>,
    /// Element id.
    pub id: String,
}

/// A class or subclass feature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureRow {
    /// Lower-case name.
    pub name: String,
    /// Lower-case class name.
    pub class: String,
    /// Subclass feature.
    pub sub: bool,
    /// Element id.
    pub id: String,
}

/// Lookups for prerequisite conversion. Empty by default (then only links and SRD ids resolve).
#[derive(Debug, Default)]
pub struct PrereqIndex {
    /// Race / species ids by lower-case name.
    pub races: BTreeMap<String, BTreeSet<String>>,
    /// Subrace ids by `(race, subrace)` lower-case names.
    pub subraces: BTreeMap<(String, String), BTreeSet<String>>,
    /// Races that can be Small.
    pub small_races: BTreeSet<String>,
    /// Names of the Small races (for SRD equivalents).
    pub small_names: BTreeSet<String>,
    /// Subclass ids by `(class, short name or name)` lower-case.
    pub subclasses: BTreeMap<(String, String), BTreeSet<String>>,
    /// Class and subclass features that were emitted.
    pub features: Vec<FeatureRow>,
    /// Feat ids by category code.
    pub feat_categories: BTreeMap<String, BTreeSet<String>>,
    /// Optional feature ids by lower-case name.
    pub optfeatures: BTreeMap<String, BTreeSet<String>>,
    /// Spells.
    pub spells: Vec<SpellRow>,
}

/// The entity lists the index reads.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// Races.
    pub races: &'a [Ent],
    /// Subraces.
    pub subraces: &'a [Ent],
    /// Subclasses.
    pub subclasses: &'a [Ent],
    /// Feats.
    pub feats: &'a [Ent],
    /// Optional features.
    pub optfeatures: &'a [Ent],
    /// Spells.
    pub spells: &'a [Ent],
}

fn str_of<'v>(v: &'v Value, k: &str) -> &'v str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn add(map: &mut BTreeMap<String, BTreeSet<String>>, k: String, id: String) {
    map.entry(k).or_default().insert(id);
}

impl PrereqIndex {
    /// Build the index from the converted state.
    #[must_use]
    pub fn build(sh: &Shared, st: &mut State, inp: Inputs<'_>) -> Self {
        let mut ix = Self::default();
        ix.add_races(sh, st, inp);
        ix.add_classes(sh, st, inp);
        for ent in inp.feats {
            let Some(id) = sh.link(RefKind::Feat, &ent.name, &ent.source) else {
                continue;
            };
            if let Some(cat) = ent.raw.get("category").and_then(Value::as_str) {
                add(&mut ix.feat_categories, cat.to_string(), id);
            }
        }
        for ent in inp.optfeatures {
            if let Some(id) = sh.link(RefKind::OptionalFeature, &ent.name, &ent.source) {
                add(&mut ix.optfeatures, key(&ent.name), id);
            }
        }
        ix.spells = sh.spell_rows.clone();
        ix
    }

    fn add_races(&mut self, sh: &Shared, st: &mut State, inp: Inputs<'_>) {
        for ent in inp.races {
            let Some(id) = sh
                .race_ids
                .get(&format!("{}|{}", key(&ent.name), ent.source.to_uppercase()))
                .filter(|id| st.emitted.contains(*id))
                .cloned()
            else {
                continue;
            };
            let small = ent
                .raw
                .get("size")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|s| s.as_str() == Some("S")));
            if small {
                self.small_races.insert(id.clone());
                self.small_names.insert(key(&ent.name));
            }
            add(&mut self.races, key(&ent.name), id);
        }
        for ent in inp.subraces {
            let sub = str_of(&ent.raw, "name");
            let race = str_of(&ent.raw, "raceName");
            if sub.is_empty() || race.is_empty() {
                continue;
            }
            let id = super::race::subrace_id(st, ent);
            if st.emitted.contains(&id) {
                self.subraces
                    .entry((key(race), key(sub)))
                    .or_default()
                    .insert(id);
            }
        }
    }

    fn add_classes(&mut self, sh: &Shared, st: &mut State, inp: Inputs<'_>) {
        for ent in inp.subclasses {
            let Ok(sc) = model::parse::<Subclass>(&ent.raw) else {
                continue;
            };
            let id = feature::subclass_id(sh, st, &sc, &ent.source);
            if !st.emitted.contains(&id) {
                continue;
            }
            let class = key(&sc.class_name);
            for name in [&sc.short_name, &sc.name] {
                self.subclasses
                    .entry((class.clone(), key(name)))
                    .or_default()
                    .insert(id.clone());
            }
        }
        for (list, sub) in [(&sh.class_features, false), (&sh.subclass_features, true)] {
            for ent in list {
                let Ok(f) = model::parse::<Feature>(&ent.raw) else {
                    continue;
                };
                let id = feature::feature_id(st, &feature::ref_of(&f, sub), sub);
                if st.emitted.contains(&id) {
                    self.features.push(FeatureRow {
                        name: key(&f.name),
                        class: key(&f.class_name),
                        sub,
                        id,
                    });
                }
            }
        }
    }
}

/// Ids of the bundled SRD element with this name in either ruleset.
#[must_use]
pub fn srd_ids(sh: &Shared, kind: SrdKind, name: &str) -> BTreeSet<String> {
    [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .filter_map(|rs| sh.srd.find(kind, rs, name).map(str::to_string))
        .collect()
}

/// The imported spells as `choose` filters see them (built once with the links).
#[must_use]
pub fn spell_rows(sh: &Shared, spells: &[Ent]) -> Vec<SpellRow> {
    let mut rows = Vec::new();
    for ent in spells {
        let Some(id) = sh.link(RefKind::Spell, &ent.name, &ent.source) else {
            continue;
        };
        let attacks = ent
            .raw
            .get("spellAttack")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_uppercase)
                    .collect()
            })
            .unwrap_or_default();
        rows.push(SpellRow {
            name: key(&ent.name),
            level: ent
                .raw
                .get("level")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or_default(),
            classes: sh
                .spell_classes
                .classes(&ent.name, &ent.source)
                .iter()
                .map(|c| key(c))
                .collect(),
            damage: ent
                .raw
                .get("damageInflict")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty()),
            attacks,
            id,
        });
    }
    rows
}

/// Ids of linked entities of `kind` with this name (any source).
#[must_use]
pub fn linked_named(sh: &Shared, kind: RefKind, name: &str) -> BTreeSet<String> {
    let name = key(name);
    sh.links
        .iter()
        .filter(|((k, n, _), _)| *k == kind && *n == name)
        .map(|(_, id)| id.clone())
        .collect()
}
