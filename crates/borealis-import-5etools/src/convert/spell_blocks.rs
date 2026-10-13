//! Spellcasting blocks for additional spells: the block that names a non-class spell source
//! (`Tiefling`, `Feat: Fey Touched`) with its ability, and the extension blocks of expanded
//! spell lists. An element carries at most one block, so extra blocks live on small helper
//! elements (`Grants`, granted by the owner) derived from the owner's id.

use super::build::{grant_at, select};
use super::new_element;
use super::spells_extra::{Casting, SpellAbility};
use borealis_content::{Element, Rule, Ruleset, Sheet, Spellcasting, SpellcastingList};
use std::collections::BTreeMap;

/// The element that receives the additional spell rules.
#[derive(Debug, Clone, Copy)]
pub struct Carrier<'a> {
    /// Element id.
    pub id: &'a str,
    /// Display name.
    pub name: &'a str,
    /// Source display name.
    pub source: &'a str,
    /// Ruleset.
    pub ruleset: Ruleset,
    /// The carrier has no spellcasting block yet and its rules are not gated.
    pub free: bool,
}

/// What [`place`] produced.
#[derive(Debug, Default)]
pub struct Placed {
    /// Rules for the carrier (helper grants, the ability select).
    pub rules: Vec<Rule>,
    /// A block for the carrier itself.
    pub spellcasting: Option<Spellcasting>,
    /// Helper elements.
    pub elements: Vec<Element>,
}

fn helper(c: &Carrier<'_>, suffix: &str, name: &str, block: Spellcasting) -> Element {
    let mut e = new_element(
        &format!("{}_{suffix}", c.id),
        name,
        "Grants",
        c.source,
        c.ruleset,
    );
    e.description = format!("<p>Spellcasting of {}.</p>", crate::text::tags::esc(c.name));
    e.sheet = Some(Sheet {
        display: Some(false),
        ..Sheet::default()
    });
    e.spellcasting = Some(block);
    e
}

/// Lowest level of the rules (1 when any rule applies from the start).
fn first_level(rules: &[Rule]) -> u32 {
    rules
        .iter()
        .map(|r| match r {
            Rule::Custom(c) => c
                .fields
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .and_then(|l| u32::try_from(l).ok())
                .unwrap_or(1),
            other => other.level(),
        })
        .min()
        .unwrap_or(1)
        .max(1)
}

/// Place the blocks of one group. `own_block`: the rules use a label of their own (not a
/// class), so a block with that name and the group's ability must exist; it appears from the
/// level of the first rule.
#[must_use]
pub fn place(c: &Carrier<'_>, casting: &Casting, rules: &[Rule], own_block: bool) -> Placed {
    let mut out = Placed::default();
    let mut free = c.free;
    if own_block && !rules.is_empty() {
        let level = first_level(rules);
        let block = |ability: Option<&str>| Spellcasting {
            name: Some(casting.label.clone()),
            ability: ability.map(str::to_string),
            ..Spellcasting::default()
        };
        match &casting.ability {
            SpellAbility::Choose(abilities) => {
                // a support tag, not an element id (ids are checked for dangling references)
                let tag = format!("SPELLCASTING_OF_{}", c.id.trim_start_matches("ID_"));
                for a in abilities {
                    let suffix = format!("SPELLCASTING_{}", a[..3].to_uppercase());
                    let mut e = helper(c, &suffix, a, block(Some(a)));
                    e.supports = vec![tag.clone()];
                    out.elements.push(e);
                }
                let mut s = select(
                    "Grants",
                    &format!("Spellcasting Ability ({})", casting.label),
                    Some(&tag),
                    1,
                );
                s.level = (level > 1).then_some(level);
                out.rules.push(Rule::Select(s));
            }
            SpellAbility::Fixed(a) if free && level == 1 => {
                out.spellcasting = Some(block(Some(a)));
                free = false;
            }
            SpellAbility::Unspecified if free && level == 1 => {
                out.spellcasting = Some(block(None));
                free = false;
            }
            other => {
                let ability = match other {
                    SpellAbility::Fixed(a) => Some(a.as_str()),
                    _ => None,
                };
                let name = format!("{} Spellcasting", c.name);
                out.elements
                    .push(helper(c, "SPELLCASTING", &name, block(ability)));
                out.rules
                    .push(grant_at(&format!("{}_SPELLCASTING", c.id), "Grants", level));
            }
        }
    }
    let mut by_level: BTreeMap<u32, Vec<SpellcastingList>> = BTreeMap::new();
    for (level, entry) in &casting.expanded {
        let list = by_level.entry(*level).or_default();
        if !list.contains(entry) {
            list.push(entry.clone());
        }
    }
    for (level, extends) in by_level {
        let block = Spellcasting {
            name: (!own_block).then(|| casting.label.clone()),
            extend: Some(true),
            all: own_block.then_some(true),
            extends,
            ..Spellcasting::default()
        };
        if free && level == 1 {
            out.spellcasting = Some(block);
            free = false;
            continue;
        }
        let suffix = if level == 1 {
            "EXPANDED_SPELLS".to_string()
        } else {
            format!("EXPANDED_SPELLS_{level}")
        };
        let name = format!("{} Expanded Spells", c.name);
        out.elements.push(helper(c, &suffix, &name, block));
        out.rules
            .push(grant_at(&format!("{}_{suffix}", c.id), "Grants", level));
    }
    out
}

/// Give an option element (no block, no gate) its spell rules and blocks; returns the helper
/// elements.
pub fn fill(
    el: &mut Element,
    rules: Vec<Rule>,
    casting: &Casting,
    own_block: bool,
) -> Vec<Element> {
    let source = el.source.clone();
    let carrier = Carrier {
        id: &el.id,
        name: &el.name,
        source: &source,
        ruleset: el.ruleset.unwrap_or(Ruleset::Dnd2014),
        free: el.spellcasting.is_none(),
    };
    let placed = place(&carrier, casting, &rules, own_block);
    el.rules.extend(rules);
    el.rules.extend(placed.rules);
    if placed.spellcasting.is_some() {
        el.spellcasting = placed.spellcasting;
    }
    placed.elements
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::build::grant;

    fn carrier(free: bool) -> Carrier<'static> {
        Carrier {
            id: "ID_X",
            name: "Xeno",
            source: "Test",
            ruleset: Ruleset::Dnd2014,
            free,
        }
    }

    fn casting(ability: SpellAbility, expanded: Vec<(u32, SpellcastingList)>) -> Casting {
        Casting {
            label: "Xeno".into(),
            ability,
            expanded,
        }
    }

    #[test]
    fn fixed_ability_goes_on_a_free_carrier() {
        let p = place(
            &carrier(true),
            &casting(SpellAbility::Fixed("Charisma".into()), vec![]),
            &[grant("ID_S", "Spell")],
            true,
        );
        let b = p.spellcasting.unwrap();
        assert_eq!(
            (b.name.as_deref(), b.ability.as_deref()),
            (Some("Xeno"), Some("Charisma"))
        );
        assert!(p.elements.is_empty() && p.rules.is_empty());
    }

    #[test]
    fn late_or_gated_blocks_use_a_granted_helper() {
        let p = place(
            &carrier(true),
            &casting(SpellAbility::Unspecified, vec![]),
            &[grant_at("ID_S", "Spell", 18)],
            true,
        );
        assert!(p.spellcasting.is_none());
        assert_eq!(p.elements[0].id, "ID_X_SPELLCASTING");
        assert_eq!(p.elements[0].ty, "Grants");
        assert_eq!(p.rules[0].level(), 18);
        let p = place(
            &carrier(false),
            &casting(SpellAbility::Fixed("Wisdom".into()), vec![]),
            &[grant("ID_S", "Spell")],
            true,
        );
        assert_eq!(
            p.elements[0]
                .spellcasting
                .as_ref()
                .unwrap()
                .ability
                .as_deref(),
            Some("Wisdom")
        );
    }

    #[test]
    fn ability_choice_offers_one_helper_per_ability() {
        let p = place(
            &carrier(true),
            &casting(
                SpellAbility::Choose(vec!["Intelligence".into(), "Wisdom".into()]),
                vec![],
            ),
            &[grant("ID_S", "Spell")],
            true,
        );
        assert_eq!(p.elements.len(), 2);
        assert_eq!(p.elements[1].id, "ID_X_SPELLCASTING_WIS");
        assert_eq!(p.elements[1].supports, vec!["SPELLCASTING_OF_X"]);
        let Rule::Select(s) = &p.rules[0] else {
            panic!()
        };
        assert_eq!(s.supports.as_deref(), Some("SPELLCASTING_OF_X"));
        assert_eq!(s.ty, "Grants");
    }

    #[test]
    fn expansions_extend_all_classes_or_the_named_class_by_level() {
        let mut prepared = SpellcastingList::new("ID_B");
        prepared.prepared = Some(true);
        let p = place(
            &carrier(true),
            &casting(
                SpellAbility::Unspecified,
                vec![
                    (1, SpellcastingList::new("ID_A")),
                    (1, prepared),
                    (10, SpellcastingList::new("1&&Cleric")),
                ],
            ),
            &[],
            true,
        );
        let b = p.spellcasting.unwrap();
        assert_eq!((b.extend, b.all, b.name), (Some(true), Some(true), None));
        assert_eq!(b.extends.len(), 2);
        assert_eq!(p.elements[0].id, "ID_X_EXPANDED_SPELLS_10");
        assert_eq!(p.rules[0].level(), 10);
        // class spells: the extension names the class
        let p = place(
            &carrier(false),
            &casting(
                SpellAbility::Unspecified,
                vec![(1, SpellcastingList::new("ID_A"))],
            ),
            &[grant("ID_S", "Spell")],
            false,
        );
        let b = p.elements[0].spellcasting.as_ref().unwrap();
        assert_eq!((b.name.as_deref(), b.all), (Some("Xeno"), None));
        assert_eq!(p.elements[0].id, "ID_X_EXPANDED_SPELLS");
    }
}
