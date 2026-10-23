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
    /// species' parts). `superseded_by` lists the backgrounds that bring their own increase:
    /// the added one applies only while the character has none of them (IM9a). `None` = used
    /// as [`Self::adapt`] left it.
    fn origin_increase(
        &self,
        _el: &Element,
        _from: Ruleset,
        _to: &dyn RulesetProfile,
        _superseded_by: &[String],
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
            (Ruleset::Dnd2014, Ruleset::Dnd2024, "Archetype" | "Archetype Feature") => to
                .legacy_subclass_start()
                .and_then(|start| raise_subclass_levels(el, start)),
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
        superseded_by: &[String],
    ) -> Option<(Element, Vec<String>)> {
        match (from, to.ruleset(), el.ty.as_str()) {
            (Ruleset::Dnd2024, Ruleset::Dnd2014, "Species" | "Race") => {
                Some(flexible_origin_increase(el, superseded_by))
            }
            _ => None,
        }
    }
}

/// Name of the choice IM9 adds to a 2024 species in a 2014 character.
pub const FLEXIBLE_INCREASE_NAME: &str = "Ability Score Increase (Other Edition)";

/// Compatibility note for a species whose IM9 increase is switched off because the character's
/// background gives its own (IM9a).
pub const INCREASE_NOT_APPLIED: &str =
    "Ability score increase not applied: the 2024 background gives the increase (IM9a)";

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
/// abilities, as Tasha's custom origin and the newer lineages give. IM9a: the choice requires
/// that none of `superseded_by` (backgrounds with their own increase) is on the character, so
/// only one origin increase applies, the background's.
fn flexible_origin_increase(el: &Element, superseded_by: &[String]) -> (Element, Vec<String>) {
    let mut fields = json!({
        "name": FLEXIBLE_INCREASE_NAME,
        "pool": ABILITIES,
        "options": [[2, 1], [1, 1, 1]],
        "cap": 20,
    });
    if let Some(req) = none_of(superseded_by) {
        fields["requirements"] = json!(req);
    }
    let mut out = el.clone();
    out.rules.push(Rule::Custom(CustomRule {
        kind: "ability_distribution".into(),
        fields: fields.as_object().cloned().unwrap_or_default(),
    }));
    let notes = vec![
        "Ability score increase added: +2 to one ability and +1 to another, or +1 to three \
         (a 2024 species has none; IM9, as Tasha's custom origin), unless the background gives \
         one (IM9a)"
            .to_string(),
    ];
    (out, notes)
}

/// The level a rule is listed at, if it names one (`level` of a grant, select or stat; a numeric
/// `level` field of an extension rule).
fn rule_level(rule: &Rule) -> Option<u32> {
    match rule {
        Rule::Grant(g) => g.level,
        Rule::Select(s) => s.level,
        Rule::Stat(s) => s.level,
        Rule::Custom(c) => c
            .fields
            .get("level")
            .and_then(serde_json::Value::as_u64)
            .and_then(|l| u32::try_from(l).ok()),
        Rule::Unknown(_) => None,
    }
}

fn set_rule_level(rule: &mut Rule, level: u32) {
    match rule {
        Rule::Grant(g) => g.level = Some(level),
        Rule::Select(s) => s.level = Some(level),
        Rule::Stat(s) => s.level = Some(level),
        Rule::Custom(c) => {
            c.fields.insert("level".into(), json!(level));
        }
        Rule::Unknown(_) => {}
    }
}

/// B-44: an older subclass in a 2024 class. Every rule of the subclass or of one of its features
/// listed below `start` (the 2024 subclass level, 3) takes effect at `start`; rules listed at
/// `start` or later keep their level. `None` when no rule is listed below `start`.
fn raise_subclass_levels(el: &Element, start: u32) -> Option<(Element, Vec<String>)> {
    let below = |r: &Rule| rule_level(r).is_some_and(|l| l < start);
    if !el.rules.iter().any(below) {
        return None;
    }
    let mut out = el.clone();
    for r in out.rules.iter_mut().filter(|r| below(r)) {
        set_rule_level(r, start);
    }
    let notes = vec![format!(
        "Subclass from an older book: what it lists below level {start} is gained at level \
         {start}, the 2024 subclass level"
    )];
    Some((out, notes))
}

/// A requirements expression that holds while the character has none of `ids`: `!(A||B||...)`.
/// Ids the requirements grammar cannot name (`ID_` then letters, digits, `_`) are left out, so
/// the expression always parses. `None` when no id is left.
pub(crate) fn none_of(ids: &[String]) -> Option<String> {
    let nameable = |id: &&String| {
        id.len() > "ID_".len() + 1
            && id.starts_with("ID_")
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let ids: Vec<&str> = ids.iter().filter(nameable).map(String::as_str).collect();
    (!ids.is_empty()).then(|| format!("!({})", ids.join("||")))
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
            .origin_increase(&el, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014), &[])
            .expect("adapted");
        assert_eq!(out.rules.len(), 1);
        assert!(raises_ability(&out.rules[0]));
        assert_eq!(notes.len(), 1);
        assert!(el.rules.is_empty(), "the stored element is untouched");
        assert!(a
            .origin_increase(&el, Ruleset::Dnd2014, profile_for(Ruleset::Dnd2024), &[])
            .is_none());
        let mut feat = el.clone();
        feat.ty = "Feat".into();
        assert!(a
            .origin_increase(&feat, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014), &[])
            .is_none());
        assert!(PassThroughAdapter
            .origin_increase(&el, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014), &[])
            .is_none());
    }

    #[test]
    fn the_increase_requires_none_of_the_backgrounds_with_their_own() {
        let el = species(json!([]));
        let backgrounds = ["ID_BG_A".to_string(), "ID_BG_B".into(), "not an id".into()];
        let (out, _) = StandardAdapter
            .origin_increase(
                &el,
                Ruleset::Dnd2024,
                profile_for(Ruleset::Dnd2014),
                &backgrounds,
            )
            .expect("adapted");
        let Rule::Custom(c) = &out.rules[0] else {
            panic!("custom rule")
        };
        assert_eq!(c.fields["requirements"], json!("!(ID_BG_A||ID_BG_B)"));
        assert_eq!(none_of(&["bad".into()]), None);
        let (plain, _) = StandardAdapter
            .origin_increase(&el, Ruleset::Dnd2024, profile_for(Ruleset::Dnd2014), &[])
            .expect("adapted");
        let Rule::Custom(c) = &plain.rules[0] else {
            panic!("custom rule")
        };
        assert!(!c.fields.contains_key("requirements"));
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
