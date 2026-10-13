//! Cross-edition adaptation .
//!
//! An [`EditionAdapter`] is applied once per foreign element when the universe is prepared. It
//! never rewrites stored data: the converted copy lives only in the engine's universe and every
//! conversion is reported in the snapshot's compatibility report.

use crate::ids::ABILITIES;
use crate::profile::RulesetProfile;
use borealis_content::{Element, Rule, Ruleset};

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
/// racial ability score increases (`legacy_species_asi = ignore`, design 7). New conversions
/// follow the same shape: a `fn(&Element) -> Option<(Element, Vec<String>)>` chosen by type.
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
