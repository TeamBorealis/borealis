//! Spell retraining: which filled spell slots may be replaced, and their history.
//!
//! The edition rule (which picks may be swapped on level-up) lives in the ruleset profile
//! ([`crate::profile::RulesetProfile::spell_swap_on_level_up`]); this module only gathers the
//! facts it needs and reads the per-slot history stored in [`CharacterState::slot_history`].

use crate::derive::Evaluation;
use crate::gear::element_ref;
use crate::graph::SlotNode;
use crate::profile::SpellSwapQuery;
use crate::snapshot::{ElementRef, ReplacedRef, SpellcastingSnapshot};
use crate::state::CharacterState;
use borealis_content::{Rule, SelectRule};

/// Derived retraining facts of one slot.
pub(crate) struct RetrainInfo {
    pub replaceable: bool,
    pub gained_level: u32,
    pub replaced: Vec<ReplacedRef>,
}

impl Evaluation<'_> {
    /// Retraining facts of one slot (pure: computed from state and graph).
    pub(crate) fn retrain_info(
        &self,
        state: &CharacterState,
        casters: &[SpellcastingSnapshot],
        slot: &SlotNode,
        rule: &SelectRule,
        class_level: u32,
    ) -> RetrainInfo {
        let history = state.replaced_in(&slot.key);
        let replaced = history
            .iter()
            .map(|p| ReplacedRef {
                pick: self.uni.get(&p.id).map_or_else(
                    || ElementRef {
                        id: p.id.clone(),
                        name: p.id.clone(),
                        ty: "Spell".into(),
                        source: String::new(),
                    },
                    element_ref,
                ),
                level: p.level,
            })
            .collect();
        let stored = self.graph.selections.get(&slot.key).and_then(|s| s.1);
        let gained_level = stored
            .or_else(|| history.last().map(|p| p.level))
            .unwrap_or_else(|| rule.level.unwrap_or(1));
        let replaceable = match (&rule.spellcasting, slot.chosen) {
            (Some(class), Some(chosen)) if slot.rejected.is_none() => {
                let pick = self.graph.nodes[chosen].e;
                let cantrip = pick.spell_level() == Some(0);
                pick.el.ty == "Spell"
                    && !self.swap_used(state, class, class_level, cantrip)
                    && self.hooks.spell_swap_on_level_up(SpellSwapQuery {
                        cantrip,
                        rule_allows: rule.allow_replace,
                        class_allows: casters.iter().any(|c| &c.name == class && c.allow_replace),
                    })
            }
            _ => false,
        };
        RetrainInfo {
            replaceable,
            gained_level,
            replaced,
        }
    }
}

impl Evaluation<'_> {
    /// Whether `class` already swapped a spell (a cantrip when `cantrip`) at class level `level`:
    /// the rules allow one swap per level gained, so every other pick of that kind stays fixed.
    pub(crate) fn swap_used(
        &self,
        state: &CharacterState,
        class: &str,
        level: u32,
        cantrip: bool,
    ) -> bool {
        self.graph.nodes.iter().filter(|n| n.alive).any(|n| {
            n.slots.iter().any(|slot| {
                let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
                    return false;
                };
                rule.spellcasting.as_deref() == Some(class)
                    && state.replaced_in(&slot.key).iter().any(|p| {
                        p.level == level
                            && self
                                .uni
                                .get(&p.id)
                                .is_some_and(|e| (e.spell_level() == Some(0)) == cantrip)
                    })
            })
        })
    }
}
