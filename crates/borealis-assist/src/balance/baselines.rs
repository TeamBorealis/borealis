//! SRD baselines computed from the enabled packs (never hard-coded power curves).

use std::collections::BTreeMap;

use borealis_content::{ContentStore, Element, Ruleset};

use super::measure::{self, FeatM, MonsterM, SpeciesM, SpellM};
use super::Summary;

/// Design floors used when the enabled packs hold too few samples to define a ceiling.
/// A feat usually adds one ability point and a handful of proficiencies.
pub const FEAT_ASI_FLOOR: f64 = 1.0;
/// See [`FEAT_ASI_FLOOR`].
pub const FEAT_PROF_FLOOR: f64 = 3.0;
/// Magic item bonus ceiling per rarity when the packs list no `+N` item of that rarity.
pub const RARITY_BONUS_FLOOR: [(&str, f64); 5] = [
    ("Common", 0.0),
    ("Uncommon", 1.0),
    ("Rare", 2.0),
    ("Very Rare", 3.0),
    ("Legendary", 3.0),
];
/// Fewer samples than this widen the comparison pool.
pub const MIN_SAMPLES: usize = 3;

/// Measured SRD samples grouped by kind.
#[derive(Debug, Clone, Default)]
pub struct Baselines {
    pub(super) spells: Vec<SpellM>,
    pub(super) feats: Vec<FeatM>,
    pub(super) items: BTreeMap<String, Vec<f64>>,
    pub(super) species: Vec<SpeciesM>,
    pub(super) monsters: Vec<MonsterM>,
}

fn in_scope(el: &Element, ruleset: Ruleset) -> bool {
    el.ruleset
        .map_or(true, |r| r == ruleset || r == Ruleset::Any)
}

impl Baselines {
    /// Measure every element of the enabled packs that belongs to `ruleset`.
    pub fn from_store(store: &ContentStore, ruleset: Ruleset) -> Self {
        let mut b = Self::default();
        for stored in store.iter() {
            let el = &stored.element;
            if !in_scope(el, ruleset) {
                continue;
            }
            match el.ty.as_str() {
                "Spell" => b.spells.push(measure::spell(el)),
                "Feat" => b.feats.push(measure::feat(el)),
                "Race" | "Species" => b.species.push(measure::species(el)),
                "Magic Item" => {
                    let bonus = measure::item_bonus(el);
                    if let (Some(r), true) = (measure::rarity(el), bonus > 0.0) {
                        b.items.entry(r).or_default().push(bonus);
                    }
                }
                "Monster" => b.monsters.extend(measure::monster(el)),
                _ => {}
            }
        }
        b
    }

    /// Number of measured samples per kind: `(spells, feats, items, species, monsters)`.
    pub fn sample_counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.spells.len(),
            self.feats.len(),
            self.items.values().map(Vec::len).sum(),
            self.species.len(),
            self.monsters.len(),
        )
    }

    /// Damage of same-level spells of the same area class; widens to the whole level when thin.
    pub(super) fn spell_damage(&self, level: u32, area: bool) -> (Summary, &'static str) {
        let at_level = |f: &dyn Fn(&&SpellM) -> bool| -> Vec<f64> {
            self.spells
                .iter()
                .filter(|s| s.level == level && s.damage > 0.0)
                .filter(f)
                .map(|s| s.damage)
                .collect()
        };
        let same = at_level(&|s| s.area == area);
        if same.len() >= MIN_SAMPLES {
            return (Summary::of(&same), "same level and target type");
        }
        (
            Summary::of(&at_level(&|_| true)),
            "same level, any target type",
        )
    }

    pub(super) fn item_bonus(&self, rarity: &str) -> Summary {
        let floor = RARITY_BONUS_FLOOR
            .iter()
            .find(|(r, _)| r.eq_ignore_ascii_case(rarity))
            .map_or(0.0, |(_, v)| *v);
        let mut samples = self
            .items
            .iter()
            .find(|(r, _)| r.eq_ignore_ascii_case(rarity))
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        samples.push(floor);
        Summary::of(&samples)
    }

    /// Monsters of the same CR, widened by neighbouring CRs until enough samples exist.
    pub(super) fn monsters_near(&self, rank: usize) -> (Vec<&MonsterM>, usize) {
        let max_rank = self.monsters.iter().map(|m| m.cr_rank).max().unwrap_or(0);
        for width in 0..=max_rank.max(rank) {
            let pool: Vec<&MonsterM> = self
                .monsters
                .iter()
                .filter(|m| m.cr_rank.abs_diff(rank) <= width)
                .collect();
            if pool.len() >= MIN_SAMPLES {
                return (pool, width);
            }
        }
        (self.monsters.iter().collect(), usize::MAX)
    }
}
