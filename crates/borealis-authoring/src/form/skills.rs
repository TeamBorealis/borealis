//! The skills loaded content adds (HC16): `Skill` elements the engine reads as character skills
//! ([`borealis_engine::content_skills`]). The creator's skill blocks ("Bonus to skills", "Advantage on a roll") offer
//! them after the eighteen ([`crate::blocks::SKILL_NAME_PICKER`]) and take their names.

use super::Lookup;
use borealis_engine::content_skills::{self, ContentSkill, SKILL_TYPE};

fn loaded(lookup: &dyn Lookup) -> Vec<ContentSkill> {
    content_skills::collect(lookup.loaded_of_type(SKILL_TYPE).into_iter())
}

/// The statistic keys (`alchemy`) of the skills loaded content adds, the eighteen left out.
#[must_use]
pub fn skill_keys(lookup: &dyn Lookup) -> Vec<String> {
    loaded(lookup).into_iter().map(|s| s.key).collect()
}

/// The names of the skills loaded content adds, single spaced as the skill blocks write them (the engine reads a
/// written name by its key), the eighteen left out, sorted.
#[must_use]
pub fn skill_names(lookup: &dyn Lookup) -> Vec<String> {
    let mut names: Vec<String> = loaded(lookup)
        .into_iter()
        .map(|s| s.name.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}
