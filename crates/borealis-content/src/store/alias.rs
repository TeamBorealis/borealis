//! Id aliases resolved while a store is assembled.
//!
//! Newer community Aurora content references Aurora's 2024 Player's Handbook ids
//! (`ID_WOTC_PHB24_*`). That book is not public, but most of the referenced spells, items,
//! feats and class features exist in the bundled SRD 5.2.1 pack under `ID_2024_*`. When such an
//! id is not defined by any loaded pack, every reference to it is rewritten to the SRD 5.2.1
//! equivalent, so grants, selects and requirements resolve. A user who installs real PHB 2024
//! content defines the id, and then nothing is rewritten.

use crate::element::Element;
use crate::rules::Rule;
use std::collections::HashSet;

/// Prefix of Aurora's 2024 Player's Handbook ids.
const PHB24_PREFIX: &str = "ID_WOTC_PHB24_";
/// Prefix of the bundled SRD 5.2.1 ids.
const SRD521_PREFIX: &str = "ID_2024_";

/// PHB 2024 id suffixes whose SRD 5.2.1 element has a different name (renamed SRD spells,
/// weapon masteries, ammunition bundles, invocation options). Everything else maps by suffix.
const RENAMED: &[(&str, &str)] = &[
    ("SPELL_BIGBYS_HAND", "SPELL_ARCANE_HAND"),
    ("SPELL_EVARDS_BLACK_TENTACLES", "SPELL_BLACK_TENTACLES"),
    ("SPELL_LEOMUNDS_SECRET_CHEST", "SPELL_SECRET_CHEST"),
    ("SPELL_MORDENKAINENS_FAITHFUL_HOUND", "SPELL_FAITHFUL_HOUND"),
    ("SPELL_NYSTULS_MAGIC_AURA", "SPELL_ARCANISTS_MAGIC_AURA"),
    ("SPELL_OTILUKES_RESILIENT_SPHERE", "SPELL_RESILIENT_SPHERE"),
    ("SPELL_RARYS_TELEPATHIC_BOND", "SPELL_TELEPATHIC_BOND"),
    ("SPELL_TASHAS_HIDEOUS_LAUGHTER", "SPELL_HIDEOUS_LAUGHTER"),
    ("WEAPON_PROPERTY_CLEAVE", "MASTERY_CLEAVE"),
    ("WEAPON_PROPERTY_GRAZE", "MASTERY_GRAZE"),
    ("WEAPON_PROPERTY_NICK", "MASTERY_NICK"),
    ("WEAPON_PROPERTY_PUSH", "MASTERY_PUSH"),
    ("WEAPON_PROPERTY_SAP", "MASTERY_SAP"),
    ("WEAPON_PROPERTY_SLOW", "MASTERY_SLOW"),
    ("WEAPON_PROPERTY_TOPPLE", "MASTERY_TOPPLE"),
    ("WEAPON_PROPERTY_VEX", "MASTERY_VEX"),
    ("ITEM_ARROW", "ITEM_AMMUNITION_ARROWS"),
    ("ITEM_BOLT", "ITEM_AMMUNITION_BOLTS"),
    ("ITEM_BULLET_FIREARM", "ITEM_AMMUNITION_BULLETS_FIREARM"),
    ("ITEM_BULLET_SLING", "ITEM_AMMUNITION_BULLETS_SLING"),
    ("ITEM_NEEDLE", "ITEM_AMMUNITION_NEEDLES"),
    (
        "CLASS_FEATURE_ELDRITCH_INVOCATION_PACT_OF_THE_BLADE",
        "OPTION_ELDRITCH_INVOCATION_PACT_OF_THE_BLADE",
    ),
    (
        "CLASS_FEATURE_ELDRITCH_INVOCATION_PACT_OF_THE_CHAIN",
        "OPTION_ELDRITCH_INVOCATION_PACT_OF_THE_CHAIN",
    ),
    (
        "CLASS_FEATURE_ELDRITCH_INVOCATION_PACT_OF_THE_TOME",
        "OPTION_ELDRITCH_INVOCATION_PACT_OF_THE_TOME",
    ),
];

/// The SRD 5.2.1 id an Aurora PHB 2024 id may stand for (not checked against the store).
#[must_use]
pub(crate) fn phb24_candidate(id: &str) -> Option<String> {
    let rest = id.strip_prefix(PHB24_PREFIX)?;
    if let Some((_, to)) = RENAMED.iter().find(|(from, _)| *from == rest) {
        return Some(format!("{SRD521_PREFIX}{to}"));
    }
    // tools are items in the PHB ids and their own kind in SRD 5.2.1
    let rest = rest
        .strip_prefix("ITEM_TOOL_")
        .map_or_else(|| rest.to_string(), |t| format!("TOOL_{t}"));
    Some(format!("{SRD521_PREFIX}{rest}"))
}

/// Rewrite every `ID_*` token of `text` that `map` resolves; `None` when nothing changed.
fn rewrite_text(text: &str, map: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    if !text.contains(PHB24_PREFIX) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String, changed: &mut bool| {
        match map(token) {
            Some(new) => {
                out.push_str(&new);
                *changed = true;
            }
            None => out.push_str(token),
        }
        token.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            token.push(c);
        } else {
            flush(&mut token, &mut out, &mut changed);
            out.push(c);
        }
    }
    flush(&mut token, &mut out, &mut changed);
    changed.then_some(out)
}

fn rewrite_opt(text: Option<&String>, map: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    text.and_then(|t| rewrite_text(t, map))
}

fn rewrite_rule(rule: &Rule, map: &dyn Fn(&str) -> Option<String>) -> Option<Rule> {
    match rule {
        Rule::Grant(g) => {
            let id = map(&g.id);
            let req = rewrite_opt(g.requirements.as_ref(), map);
            (id.is_some() || req.is_some()).then(|| {
                let mut n = g.clone();
                n.id = id.unwrap_or_else(|| g.id.clone());
                n.requirements = req.or_else(|| g.requirements.clone());
                Rule::Grant(n)
            })
        }
        Rule::Select(s) => {
            let sup = rewrite_opt(s.supports.as_ref(), map);
            let req = rewrite_opt(s.requirements.as_ref(), map);
            let def = s.default_id.as_deref().and_then(map);
            (sup.is_some() || req.is_some() || def.is_some()).then(|| {
                let mut n = s.clone();
                n.supports = sup.or_else(|| s.supports.clone());
                n.requirements = req.or_else(|| s.requirements.clone());
                n.default_id = def.or_else(|| s.default_id.clone());
                Rule::Select(n)
            })
        }
        Rule::Stat(st) => rewrite_opt(st.requirements.as_ref(), map).map(|req| {
            let mut n = st.clone();
            n.requirements = Some(req);
            Rule::Stat(n)
        }),
        Rule::Unknown(_) | Rule::Custom(_) => None,
    }
}

fn rewrite_rules(rules: &[Rule], map: &dyn Fn(&str) -> Option<String>) -> Option<Vec<Rule>> {
    let new: Vec<Option<Rule>> = rules.iter().map(|r| rewrite_rule(r, map)).collect();
    new.iter().any(Option::is_some).then(|| {
        new.into_iter()
            .zip(rules)
            .map(|(n, old)| n.unwrap_or_else(|| old.clone()))
            .collect()
    })
}

/// `e` with its references rewritten by `map`, or `None` when it references nothing to rewrite.
fn rewrite_element(e: &Element, map: &dyn Fn(&str) -> Option<String>) -> Option<Element> {
    let req = rewrite_text(&e.requirements, map);
    let rules = rewrite_rules(&e.rules, map);
    let multiclass = e.multiclass.as_ref().and_then(|m| {
        let req = rewrite_text(&m.requirements, map);
        let rules = rewrite_rules(&m.rules, map);
        (req.is_some() || rules.is_some()).then(|| {
            let mut n = m.clone();
            n.requirements = req.unwrap_or_else(|| m.requirements.clone());
            n.rules = rules.unwrap_or_else(|| m.rules.clone());
            n
        })
    });
    let spellcasting = e.spellcasting.as_ref().and_then(|sc| {
        let ext: Vec<Option<String>> = sc
            .extends
            .iter()
            .map(|x| rewrite_text(&x.supports, map))
            .collect();
        ext.iter().any(Option::is_some).then(|| {
            let mut n = sc.clone();
            for (x, new) in n.extends.iter_mut().zip(ext) {
                if let Some(s) = new {
                    x.supports = s;
                }
            }
            n
        })
    });
    let extract = e.extract.as_ref().and_then(|x| {
        let ids: Vec<Option<String>> = x.items.iter().map(|i| map(&i.id)).collect();
        ids.iter().any(Option::is_some).then(|| {
            let mut n = x.clone();
            for (item, new) in n.items.iter_mut().zip(ids) {
                if let Some(id) = new {
                    item.id = id;
                }
            }
            n
        })
    });
    if req.is_none()
        && rules.is_none()
        && multiclass.is_none()
        && spellcasting.is_none()
        && extract.is_none()
    {
        return None;
    }
    let mut n = e.clone();
    if let Some(r) = req {
        n.requirements = r;
    }
    if let Some(r) = rules {
        n.rules = r;
    }
    if multiclass.is_some() {
        n.multiclass = multiclass;
    }
    if spellcasting.is_some() {
        n.spellcasting = spellcasting;
    }
    if extract.is_some() {
        n.extract = extract;
    }
    Some(n)
}

/// Rewrite references to undefined PHB 2024 ids that have a loaded SRD 5.2.1 equivalent.
/// Returns the elements in the same order; untouched elements are moved, not cloned.
pub(crate) fn resolve_phb24_aliases<T>(
    items: Vec<T>,
    element: impl Fn(&T) -> &Element,
    with: impl Fn(T, Element) -> T,
) -> Vec<T> {
    let defined: HashSet<String> = items.iter().map(|i| element(i).id.clone()).collect();
    let map = |id: &str| -> Option<String> {
        if defined.contains(id) {
            return None;
        }
        phb24_candidate(id).filter(|c| defined.contains(c))
    };
    items
        .into_iter()
        .map(|i| match rewrite_element(element(&i), &map) {
            Some(new) => with(i, new),
            None => i,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_map_by_suffix_and_renames() {
        assert_eq!(
            phb24_candidate("ID_WOTC_PHB24_SPELL_ALARM").as_deref(),
            Some("ID_2024_SPELL_ALARM")
        );
        assert_eq!(
            phb24_candidate("ID_WOTC_PHB24_SPELL_BIGBYS_HAND").as_deref(),
            Some("ID_2024_SPELL_ARCANE_HAND")
        );
        assert_eq!(
            phb24_candidate("ID_WOTC_PHB24_WEAPON_PROPERTY_VEX").as_deref(),
            Some("ID_2024_MASTERY_VEX")
        );
        assert_eq!(
            phb24_candidate("ID_WOTC_PHB24_ITEM_TOOL_THIEVES_TOOLS").as_deref(),
            Some("ID_2024_TOOL_THIEVES_TOOLS")
        );
        assert_eq!(phb24_candidate("ID_WOTC_PHB_SPELL_ALARM"), None);
    }

    #[test]
    fn text_tokens_are_rewritten_only_when_mapped() {
        let map = |id: &str| (id == "ID_WOTC_PHB24_A").then(|| "ID_2024_A".to_string());
        assert_eq!(
            rewrite_text("ID_WOTC_PHB24_A||!ID_WOTC_PHB24_AB,[level:3]", &map).as_deref(),
            Some("ID_2024_A||!ID_WOTC_PHB24_AB,[level:3]")
        );
        assert_eq!(rewrite_text("ID_WOTC_PHB24_AB|ID_X", &map), None);
        assert_eq!(rewrite_text("ID_X", &map), None);
    }
}
