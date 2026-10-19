//! Cross-edition adaptation .
//!
//! An [`EditionAdapter`] is applied once per foreign element when the universe is prepared. It
//! never rewrites stored data: the converted copy lives only in the engine's universe and every
//! conversion is reported in the snapshot's compatibility report.

use crate::ids::ABILITIES;
use crate::profile::RulesetProfile;
use borealis_content::rules::CustomRule;
use borealis_content::{Element, Rule, Ruleset};
use serde_json::json;

/// Note recorded for foreign content that is used without any change.
pub const USED_AS_WRITTEN: &str = "used as written";

/// The result of adapting one foreign element.
#[derive(Debug, Clone, PartialEq)]
pub enum Adaptation {
    /// Usable as it is.
    Unchanged,
    /// Usable after the given changes.
    Converted {
        /// The converted copy (the stored element is untouched).
        element: Box<Element>,
        /// What changed, for the compatibility report.
        notes: Vec<String>,
    },
    /// Not usable in the target profile.
    Rejected {
        /// Why.
        reason: String,
    },
}

/// Strategy that adapts an element authored for another edition.
///
/// Called only for elements whose ruleset differs from the profile's and that the profile already
/// accepts (`RulesetProfile::accepts_foreign`).
pub trait EditionAdapter: Send + Sync + std::fmt::Debug {
    /// Adapt `el` (authored for `from`) for characters on `to`.
    fn adapt(&self, el: &Element, from: Ruleset, to: &dyn RulesetProfile) -> Adaptation;

    /// IM9: the ability score increase a foreign species gets when neither it nor any of its
    /// traits or lineage options carries one (the universe checks that, since it needs the
    /// species' parts). `None` = used as [`Self::adapt`] left it.
    fn origin_increase(
        &self,
        _el: &Element,
        _from: Ruleset,
        _to: &dyn RulesetProfile,
    ) -> Option<(Element, Vec<String>)> {
        None
    }
}

/// Uses every accepted foreign element as written.
#[derive(Debug, Clone, Copy, Default)]
pub struct PassThroughAdapter;

impl EditionAdapter for PassThroughAdapter {
    fn adapt(&self, _el: &Element, _from: Ruleset, _to: &dyn RulesetProfile) -> Adaptation {
        Adaptation::Unchanged
    }
}

/// The shipped adapter. Conversions so far: 2014 Race / Sub Race in a 2024 character drops the
/// racial ability score increases (`legacy_species_asi = ignore`, design 7); a 2024 species
/// without any ability increase in a 2014 character gets a flexible +2/+1 or +1/+1/+1 choice
/// (IM9, [`EditionAdapter::origin_increase`]). New conversions follow the same shape: a
/// `fn(&Element) -> Option<(Element, Vec<String>)>` chosen by type.
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardAdapter;

impl EditionAdapter for StandardAdapter {
    fn adapt(&self, el: &Element, from: Ruleset, to: &dyn RulesetProfile) -> Adaptation {
        let converted = match (from, to.ruleset(), el.ty.as_str()) {
            (Ruleset::Dnd2014, Ruleset::Dnd2024, "Race" | "Sub Race") => ignore_racial_asi(el),
            _ => None,
        };
        match converted {
            Some((element, notes)) => Adaptation::Converted {
                element: Box::new(element),
                notes,
            },
            None => Adaptation::Unchanged,
        }
    }

    fn origin_increase(
        &self,
        el: &Element,
        from: Ruleset,
        to: &dyn RulesetProfile,
    ) -> Option<(Element, Vec<String>)> {
        match (from, to.ruleset(), el.ty.as_str()) {
            (Ruleset::Dnd2024, Ruleset::Dnd2014, "Species" | "Race") => {
                Some(flexible_origin_increase(el))
            }
            _ => None,
        }
    }
}

/// Name of the choice IM9 adds to a 2024 species in a 2014 character.
pub const FLEXIBLE_INCREASE_NAME: &str = "Ability Score Increase (Other Edition)";

/// Does a rule raise an ability score: a plain ability stat (not an inline display value), an
/// `ability_increase` / `ability_distribution` choice (raw, or rewritten into its list select),
/// or an Aurora `Ability Score Improvement` select?
#[must_use]
pub fn raises_ability(rule: &Rule) -> bool {
    match rule {
        Rule::Stat(s) => {
            ABILITIES.contains(&s.name.trim().to_lowercase().as_str()) && !s.inline.unwrap_or(false)
        }
        Rule::Custom(c) => matches!(c.kind.as_str(), "ability_increase" | "ability_distribution"),
        Rule::Select(s) => {
            s.ty == "Ability Score Improvement"
                || crate::custom_rules::kind_of(s) == Some("ability")
        }
        _ => false,
    }
}

/// Appends (so earlier rule indices and slot keys stay put) a choice of +2 to one ability and +1
/// to another, or +1 to three, capped at 20: the 2024 background distribution over all six
/// abilities, as Tasha's custom origin and the newer lineages give.
fn flexible_origin_increase(el: &Element) -> (Element, Vec<String>) {
    let fields = json!({
        "name": FLEXIBLE_INCREASE_NAME,
        "pool": ABILITIES,
        "options": [[2, 1], [1, 1, 1]],
        "cap": 20,
    });
    let mut out = el.clone();
    out.rules.push(Rule::Custom(CustomRule {
        kind: "ability_distribution".into(),
        fields: fields.as_object().cloned().unwrap_or_default(),
    }));
    let notes = vec![
        "Ability score increase added: +2 to one ability and +1 to another, or +1 to three \
         (a 2024 species has none; IM9, as Tasha's custom origin)"
            .to_string(),
    ];
    (out, notes)
}

/// Removes plain ability-score stat rules from a race. `None` when there are none.
fn ignore_racial_asi(el: &Element) -> Option<(Element, Vec<String>)> {
    let is_asi = |r: &Rule| matches!(r, Rule::Stat(s) if ABILITIES.contains(&s.name.as_str()) && !s.inline.unwrap_or(false));
    let dropped: Vec<String> = el
        .rules
        .iter()
        .filter(|r| is_asi(r))
        .filter_map(|r| match r {
            Rule::Stat(s) => Some(format!("{} {}", s.name, s.value)),
            _ => None,
        })
        .collect();
    if dropped.is_empty() {
        return None;
    }
    let mut out = el.clone();
    out.rules.retain(|r| !is_asi(r));
    let notes = vec![
        format!(
            "Racial ability score increases ignored (legacy_species_asi = ignore): {}",
            dropped.join(", ")
        ),
        "2024 characters take ability score increases from their background".to_string(),
    ];
    Some((out, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::profile_for;

    fn species(rules: serde_json::Value) -> Element {
        serde_json::from_value(
            json!({"id": "ID_T", "name": "T", "type": "Species", "source": "Test",
                                      "ruleset": "dnd2024", "rules": rules}),
        )
        .expect("element")
    }

    #[test]
    fn origin_increase_only_for_a_2024_species_in_a_2014_character() {
        let el = species(json!([]));
        let a = StandardAdapter;
        let (out, notes) = a
            .origin_increase(&el, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014))
            .expect("adapted");
        assert_eq!(out.rules.len(), 1);
        assert!(raises_ability(&out.rules[0]));
        assert_eq!(notes.len(), 1);
        assert!(el.rules.is_empty(), "the stored element is untouched");
        assert!(a
            .origin_increase(&el, Ruleset::Dnd2014, profile_for(Ruleset::Dnd2024))
            .is_none());
        let mut feat = el.clone();
        feat.ty = "Feat".into();
        assert!(a
            .origin_increase(&feat, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014))
            .is_none());
        assert!(PassThroughAdapter
            .origin_increase(&el, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014))
            .is_none());
    }

    #[test]
    fn raises_ability_sees_stats_choices_but_not_inline_values() {
        let el = species(json!([
            {"kind": "stat", "name": "Strength", "value": "1"},
            {"kind": "stat", "name": "strength", "value": "1", "inline": true},
            {"kind": "stat", "name": "innate speed", "value": "30"},
            {"kind": "ability_increase", "options": [{"abilities": 1, "amount": 1}]}
        ]));
        let seen: Vec<bool> = el.rules.iter().map(raises_ability).collect();
        assert_eq!(seen, vec![true, false, false, true]);
    }
}
