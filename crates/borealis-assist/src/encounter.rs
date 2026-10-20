//! The encounter balancer, deterministic layer.
//!
//! No model is involved here. The pieces:
//!
//! * [`monster_index`]: CR, XP, type, size, environment tags and the cross-ruleset `maps_to` of every
//!   monster of one ruleset (the 5.1 pack serves 2014, the 5.2.1 pack serves 2024).
//! * [`evaluate`]: difficulty through `borealis-encounter`. The SRD 5.2.1 XP-budget method is the
//!   default for both editions; the classic 2014 method needs user supplied `Tables2014`
//!   ([`Method::classic`] refuses without them, we never ship the DMG tables).
//! * [`suggest_deterministic`]: a greedy picker that fills the budget of a target band.
//! * [`encounter_assistant`]: the AI layer, see [`assistant`].

pub mod assistant;

use std::collections::BTreeMap;

use borealis_content::{ContentStore, MonsterData, Ruleset};
use borealis_encounter::{
    evaluate_encounter, evaluate_encounter_2014, suggest_budget, suggest_budget_2014, Band, Cr,
    EncounterReport, MonsterGroup, Ruleset as MathRuleset, Tables2014,
};
use borealis_engine::snapshot::CharacterSnapshot;
use serde::{Deserialize, Serialize};

use crate::error::AssistError;

pub use assistant::{encounter_assistant, EncounterOptions, EncounterProposal};

/// Default cap on the number of creatures in a suggestion.
pub const DEFAULT_MAX_COUNT: u32 = 8;

/// The party: explicit levels, or character snapshots (the level is taken from each).
#[derive(Debug, Clone)]
pub enum Party {
    /// One level per character.
    Levels(Vec<u8>),
    /// Derived characters.
    Snapshots(Vec<CharacterSnapshot>),
}

impl Party {
    /// The character levels.
    ///
    /// # Errors
    /// [`AssistError::Encounter`] for an empty party or a level outside 1..=20.
    pub fn levels(&self) -> Result<Vec<u8>, AssistError> {
        let levels: Vec<u8> = match self {
            Party::Levels(l) => l.clone(),
            Party::Snapshots(s) => s
                .iter()
                .map(|c| u8::try_from(c.level).unwrap_or(u8::MAX))
                .collect(),
        };
        // Reuse the encounter crate's validation (empty party, level range).
        evaluate_encounter(MathRuleset::Edition2024, &levels, &[])?;
        Ok(levels)
    }
}

/// Which difficulty method is in force.
#[derive(Debug, Clone, Copy)]
pub enum Method<'a> {
    /// SRD 5.2.1 XP budget per character (the default, both editions).
    Srd521,
    /// The classic 2014 threshold and multiplier method over user supplied tables.
    Classic2014(&'a Tables2014),
}

impl<'a> Method<'a> {
    /// The 2014 method, available only when the user supplied (valid) tables.
    ///
    /// # Errors
    /// [`AssistError::Invalid`] without tables, [`AssistError::Encounter`] for malformed tables.
    pub fn classic(tables: Option<&'a Tables2014>) -> Result<Self, AssistError> {
        let tables = tables.ok_or_else(|| {
            AssistError::Invalid(
                "the 2014 encounter method is unavailable: its tables are not in the SRD, supply Tables2014"
                    .to_string(),
            )
        })?;
        tables
            .validate()
            .map_err(borealis_encounter::EncounterError::from)?;
        Ok(Method::Classic2014(tables))
    }

    /// Label for the UI (the active method is always shown).
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Method::Srd521 => "SRD 5.2.1 XP budget",
            Method::Classic2014(_) => "2014 thresholds and multipliers (user tables)",
        }
    }

    fn budget(&self, party: &[u8], band: Band) -> Result<u64, AssistError> {
        Ok(match self {
            Method::Srd521 => suggest_budget(MathRuleset::Edition2024, party, band)?,
            Method::Classic2014(t) => suggest_budget_2014(t, party, band)?,
        })
    }
}

/// One monster of the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterEntry {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Challenge rating.
    pub cr: Cr,
    /// XP of one in an encounter: the stored XP (the printed value for homebrew, decision HB5),
    /// see [`Cr::encounter_xp`].
    pub xp: u64,
    /// Creature type (`beast`, `undead`, ...).
    pub creature_type: String,
    /// Size categories.
    pub size: Vec<String>,
    /// Parenthetical tags (`goblinoid`, ...).
    pub tags: Vec<String>,
    /// Environment tags, when a pack declares any (`Environment:forest` in `supports`). The SRD packs
    /// declare none, so this is usually empty.
    pub environments: Vec<String>,
    /// Counterpart in the other ruleset, from `maps_to`.
    pub maps_to: Option<String>,
    /// Source name.
    pub source: String,
}

impl MonsterEntry {
    /// `count` of this monster as an encounter group, at [`MonsterEntry::xp`] each.
    #[must_use]
    pub fn group(&self, count: u32) -> MonsterGroup {
        MonsterGroup {
            cr: self.cr,
            xp: self.xp,
            count,
        }
    }

    /// True when `tag` (case-insensitive) names the type, a tag, an environment, or a word of the name.
    #[must_use]
    pub fn matches_theme(&self, tag: &str) -> bool {
        let t = tag.trim().to_lowercase();
        !t.is_empty()
            && (self.creature_type == t
                || self
                    .tags
                    .iter()
                    .chain(&self.environments)
                    .any(|x| x.to_lowercase() == t)
                || self.name.to_lowercase().split_whitespace().any(|w| w == t))
    }
}

/// Index of the monsters of `ruleset` (empty for [`Ruleset::Any`]), sorted by CR then name.
#[must_use]
pub fn monster_index(store: &ContentStore, ruleset: Ruleset) -> Vec<MonsterEntry> {
    let mut out: Vec<MonsterEntry> = store
        .by_type("Monster")
        .filter_map(|s| {
            let e = &s.element;
            let data = MonsterData::from_element(e).ok()?;
            if e.ruleset.unwrap_or(data.ruleset) != ruleset {
                return None;
            }
            // a creature without a challenge rating has no XP (IM12), one whose armor class or
            // hit points are a formula (IM12a) no fixed numbers: not an encounter monster
            data.armor_class()?;
            data.hit_points()?;
            let cr: Cr = data.cr?.as_text().parse().ok()?;
            let maps_to = e.maps_to.as_ref().and_then(|m| match ruleset {
                Ruleset::Dnd2014 => m.dnd2024.clone(),
                _ => m.dnd2014.clone(),
            });
            Some(MonsterEntry {
                id: e.id.clone(),
                name: e.name.clone(),
                cr,
                xp: cr.encounter_xp(data.xp.map(u64::from)),
                creature_type: data.creature_type.as_str().to_string(),
                size: data.size.iter().map(|s| s.as_str().to_string()).collect(),
                tags: data.tags.iter().map(|t| t.to_lowercase()).collect(),
                environments: environments(&e.supports),
                maps_to,
                source: e.source.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (a.cr, &a.name, &a.id).cmp(&(b.cr, &b.name, &b.id)));
    out
}

fn environments(supports: &[String]) -> Vec<String> {
    supports
        .iter()
        .filter_map(|s| {
            let (head, rest) = s.split_once(':')?;
            matches!(head.trim().to_lowercase().as_str(), "env" | "environment")
                .then(|| rest.trim().to_lowercase())
        })
        .collect()
}

/// Difficulty of `monsters` against `party` under `method`.
///
/// # Errors
/// [`AssistError::Encounter`] for an invalid party.
pub fn evaluate(
    party: &[u8],
    monsters: &[MonsterGroup],
    method: &Method<'_>,
) -> Result<EncounterReport, AssistError> {
    Ok(match method {
        Method::Srd521 => evaluate_encounter(MathRuleset::Edition2024, party, monsters)?,
        Method::Classic2014(t) => evaluate_encounter_2014(t, party, monsters)?,
    })
}

/// Limits on what a suggestion may contain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    /// Allowed creature types (empty = any).
    #[serde(default)]
    pub types: Vec<String>,
    /// Most creatures in total (default [`DEFAULT_MAX_COUNT`]).
    #[serde(default)]
    pub max_count: Option<u32>,
    /// Theme tags (type, tag, environment or name word); a monster must match at least one.
    #[serde(default)]
    pub theme_tags: Vec<String>,
}

impl Constraints {
    /// The effective creature cap.
    #[must_use]
    pub fn cap(&self) -> u32 {
        self.max_count.unwrap_or(DEFAULT_MAX_COUNT).max(1)
    }

    /// Whether the constraints admit `m`.
    #[must_use]
    pub fn allows(&self, m: &MonsterEntry) -> bool {
        let type_ok = self.types.is_empty()
            || self
                .types
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(&m.creature_type));
        let theme_ok =
            self.theme_tags.is_empty() || self.theme_tags.iter().any(|t| m.matches_theme(t));
        type_ok && theme_ok
    }
}

/// A monster line of an encounter with engine-computed totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterMonster {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Challenge rating.
    pub cr: Cr,
    /// How many.
    pub count: u32,
    /// XP of one.
    pub xp_each: u64,
    /// XP of the group.
    pub total_xp: u64,
    /// Creature type.
    pub creature_type: String,
    /// Counterpart in the other ruleset.
    pub maps_to: Option<String>,
}

impl EncounterMonster {
    fn new(m: &MonsterEntry, count: u32) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            cr: m.cr,
            count,
            xp_each: m.xp,
            total_xp: m.xp.saturating_mul(u64::from(count)),
            creature_type: m.creature_type.clone(),
            maps_to: m.maps_to.clone(),
        }
    }
}

/// Engine-checked monster lines (`pool` is looked up by id; unknown ids are skipped).
pub(crate) fn monster_lines(
    pool: &BTreeMap<&str, &MonsterEntry>,
    counts: &BTreeMap<String, u32>,
) -> (Vec<EncounterMonster>, Vec<MonsterGroup>) {
    let mut lines = Vec::new();
    let mut groups = Vec::new();
    for (id, &n) in counts {
        if let Some(m) = pool.get(id.as_str()) {
            lines.push(EncounterMonster::new(m, n));
            groups.push(m.group(n));
        }
    }
    (lines, groups)
}

/// A deterministic encounter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// The creatures.
    pub monsters: Vec<EncounterMonster>,
    /// Engine report of exactly these creatures.
    pub report: EncounterReport,
    /// The band that was aimed for.
    pub target: Band,
    /// Budget of the target band (XP, adjusted XP under 2014).
    pub budget: u64,
    /// Whether the report landed in the target band.
    pub reached: bool,
}

/// Fill the budget of `band` greedily: repeatedly add the creature that raises the (adjusted) XP the
/// most while the engine still reports a band at or below the target. Creatures above the party's
/// highest level, CR 0 and creatures excluded by `constraints` are never used.
///
/// # Errors
/// [`AssistError::Encounter`] when the band has no budget (Trivial, or Extreme under the SRD method) or the
/// party is invalid; [`AssistError::Invalid`] when no monster is admitted.
pub fn suggest_deterministic(
    party: &[u8],
    band: Band,
    constraints: &Constraints,
    pool: &[MonsterEntry],
    method: &Method<'_>,
) -> Result<Suggestion, AssistError> {
    let budget = method.budget(party, band)?;
    if band == Band::Trivial {
        return Err(AssistError::Invalid(
            "Trivial is not a target band".to_string(),
        ));
    }
    let top = party.iter().copied().max().unwrap_or(1);
    let mut cands: Vec<&MonsterEntry> = pool
        .iter()
        .filter(|m| m.xp > 0 && !m.cr.exceeds_level(top) && constraints.allows(m))
        .collect();
    if cands.is_empty() {
        return Err(AssistError::Invalid(
            "no monster satisfies the constraints".to_string(),
        ));
    }
    cands.sort_by(|a, b| b.xp.cmp(&a.xp).then_with(|| a.id.cmp(&b.id)));
    let cap = constraints.cap();
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    let mut total = 0u32;
    while total < cap {
        let current = groups_of(&counts, &cands);
        let mut best: Option<(&MonsterEntry, u64, bool)> = None;
        for m in &cands {
            let mut g = current.clone();
            g.push(m.group(1));
            let r = evaluate(party, &g, method)?;
            if r.band > band {
                continue;
            }
            let (adj, hit) = (
                r.adjusted_xp,
                band == Band::Extreme && r.band == Band::Extreme,
            );
            let better = match best {
                None => true,
                // Extreme: reach it with as little as possible, otherwise climb.
                Some((_, _, h)) if hit != h => hit,
                Some((_, a, true)) => adj < a,
                Some((_, a, false)) => adj > a,
            };
            if better {
                best = Some((m, adj, hit));
            }
        }
        let Some((m, _, _)) = best else { break };
        *counts.entry(m.id.as_str()).or_insert(0) += 1;
        total += 1;
        if band == Band::Extreme
            && evaluate(party, &groups_of(&counts, &cands), method)?.band == Band::Extreme
        {
            break;
        }
    }
    let by_id: BTreeMap<&str, &MonsterEntry> = cands.iter().map(|m| (m.id.as_str(), *m)).collect();
    let owned: BTreeMap<String, u32> = counts.iter().map(|(k, v)| ((*k).to_string(), *v)).collect();
    let (monsters, groups) = monster_lines(&by_id, &owned);
    let report = evaluate(party, &groups, method)?;
    Ok(Suggestion {
        reached: report.band == band,
        monsters,
        report,
        target: band,
        budget,
    })
}

fn groups_of(counts: &BTreeMap<&str, u32>, cands: &[&MonsterEntry]) -> Vec<MonsterGroup> {
    counts
        .iter()
        .filter_map(|(id, &n)| cands.iter().find(|m| m.id == *id).map(|m| m.group(n)))
        .collect()
}
