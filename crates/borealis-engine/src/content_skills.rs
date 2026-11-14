//! Skills that content adds (HC1, item HtCY8): a `Skill` element with an `ability` setter, as the homebrew creator's
//! Skill form writes it, is a character skill like the eighteen of [`crate::ids::SKILLS`]. Its statistic key is its
//! name in lower case (`linguistics`), with the same groups as theirs (`:proficiency`, `:misc`, `:passive`), and a
//! character who has the element (granted, or picked by a skill choice: `Proficiency` selects also offer `Skill`
//! elements, [`crate::universe::type_family`]) is proficient in it, as having an SRD skill's `Proficiency` element
//! makes one proficient.
//!
//! Aurora's own content and every import express skills as `Proficiency` elements tagged Skill and never write a
//! `Skill` element (`specs/content-format.md`), so this adds nothing to an imported character.

use crate::ids::{ability_index, SKILLS};
use crate::universe::{Cond, UStat, Val};
use borealis_content::Element;
use std::collections::HashSet;

/// The element type of a skill.
pub const SKILL_TYPE: &str = "Skill";
/// The setter naming the skill's ability.
const ABILITY_SETTER: &str = "ability";
/// The proficiency rule's bonus label: a second source of proficiency in the same skill doesn't stack, as with the
/// SRD's skill proficiencies.
const PROFICIENCY_BONUS_LABEL: &str = "proficiency";

/// A skill a content element adds besides the eighteen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSkill {
    /// Statistic key (the name in lower case).
    pub key: String,
    /// The element's name, as the sheet shows it.
    pub name: String,
    /// Index of its ability in [`crate::ids::ABILITIES`].
    pub ability: usize,
    /// The element id.
    pub id: String,
}

/// Names a skill may not take: statistic groups that are not skills (`strength`, `ac`, `level`...).
fn reserved() -> &'static HashSet<String> {
    static RESERVED: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
    RESERVED.get_or_init(|| {
        crate::stats::builtin_groups()
            .into_iter()
            .filter(|g| !SKILLS.iter().any(|(s, _)| s == g))
            .collect()
    })
}

/// The statistic key and ability index of a `Skill` element, when it is a usable skill: an `ability` setter naming an
/// ability, and a name that is neither empty, nor holds `:`, nor is a statistic that is not a skill. A skill named like
/// one of the eighteen is that skill.
#[must_use]
pub fn skill_of(el: &Element) -> Option<(String, usize)> {
    if el.ty != SKILL_TYPE {
        return None;
    }
    let ability = ability_index(el.setter_value(ABILITY_SETTER)?)?;
    Some((skill_key(&el.name)?, ability))
}

/// The statistic key of a skill named `name` (lower case, single spaces), unless the name can't be a skill's: empty,
/// holding `:`, or a statistic that is not a skill (`Strength`, `AC`, `Level`, `Ability Checks`...).
#[must_use]
pub fn skill_key(name: &str) -> Option<String> {
    let key = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!key.is_empty() && !key.contains(':') && !reserved().contains(&key)).then_some(key)
}

/// The proficiency a character who has the `Skill` element gets: `<key>:proficiency` = the proficiency bonus.
#[must_use]
pub(crate) fn proficiency_stat(el: &Element) -> Option<UStat> {
    let (key, _) = skill_of(el)?;
    Some(UStat {
        name: format!("{key}:proficiency"),
        value: Val::parse("proficiency"),
        maximum: None,
        bonus: Some(PROFICIENCY_BONUS_LABEL.to_string()),
        level: 1,
        label: el.name.clone(),
        inline: false,
        req: Cond::None,
        equipped: Cond::None,
        owner_id: el.id.clone(),
        owner_name: el.name.clone(),
        value_text: "proficiency".to_string(),
        req_text: None,
        equipped_text: None,
    })
}

/// The skills content adds, in load order, each key once (the first element of a name wins); the eighteen are left
/// out (a `Skill` named Arcana is Arcana).
#[must_use]
pub fn collect<'a>(elements: impl Iterator<Item = &'a Element>) -> Vec<ContentSkill> {
    let mut out: Vec<ContentSkill> = Vec::new();
    for el in elements {
        let Some((key, ability)) = skill_of(el) else {
            continue;
        };
        if SKILLS.iter().any(|(s, _)| *s == key) || out.iter().any(|s| s.key == key) {
            continue;
        }
        out.push(ContentSkill {
            key,
            name: el.name.trim().to_string(),
            ability,
            id: el.id.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn el(name: &str, ability: Option<&str>) -> Element {
        let mut v = json!({"id": "ID_X", "name": name, "type": "Skill", "source": "T"});
        if let Some(a) = ability {
            v["setters"] = json!([{"name": "ability", "value": a}]);
        }
        serde_json::from_value(v).expect("element")
    }

    #[test]
    fn a_skill_needs_an_ability_and_a_name_that_is_no_other_statistic() {
        assert_eq!(
            skill_of(&el("  Sea  Lore ", Some("wis"))),
            Some(("sea lore".to_string(), 4))
        );
        assert_eq!(
            skill_of(&el("Arcana", Some("Intelligence"))).map(|s| s.1),
            Some(3)
        );
        for bad in ["Strength", "AC", "Level", "Ability Checks", "Lore: Sea", ""] {
            assert_eq!(skill_of(&el(bad, Some("Dexterity"))), None, "{bad}");
        }
        assert_eq!(skill_of(&el("Linguistics", None)), None);
        assert_eq!(skill_of(&el("Linguistics", Some("Luck"))), None);
    }

    #[test]
    fn collect_keeps_the_first_of_a_name_and_leaves_out_the_eighteen() {
        let a = el("Linguistics", Some("Intelligence"));
        let mut b = el("linguistics", Some("Wisdom"));
        b.id = "ID_Y".into();
        let c = el("Arcana", Some("Intelligence"));
        let found = collect([&a, &b, &c].into_iter());
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].ability, found[0].id.as_str()), (3, "ID_X"));
    }
}
