//! 5etools item filters over weapons (`type=martial weapon|property=light;finesse`).
//!
//! A filter is a list of `|`-separated clauses that must all hold; the values of one clause
//! are `;`-separated alternatives, `!` negates one. Categories and properties are tags of the
//! core `Proficiency` elements (`Martial`, `Melee`, `ID_INTERNAL_WEAPON_PROPERTY_LIGHT`), so a
//! filter becomes either a supports expression (a choice) or the list of core weapon
//! proficiencies it matches (a grant of all of them, XPHB Monk and Rogue).

use crate::core_ids::WEAPON_TAGS;

/// Prefix of the core weapon property tags.
const PROPERTY_TAG: &str = "ID_INTERNAL_WEAPON_PROPERTY_";

/// One alternative of a clause: negated?, tag.
type Alt = (bool, String);

/// The clauses of a filter as tag alternatives; `None` for a filter with parts that are not
/// weapon categories or properties. Mundane / rarity parts are always true of core weapons
/// and dropped.
fn clauses(filter: &str) -> Option<Vec<Vec<Alt>>> {
    let mut out = Vec::new();
    for clause in filter.split('|') {
        let (key, values) = clause.split_once('=')?;
        let mut alts: Vec<Alt> = Vec::new();
        for raw in values.split(';') {
            let (neg, value) = raw.strip_prefix('!').map_or((false, raw), |v| (true, v));
            let tag = match (key.trim(), value.trim()) {
                ("type", "martial weapon") => "Martial".to_string(),
                ("type", "simple weapon") => "Simple".to_string(),
                ("type", "melee weapon") => "Melee".to_string(),
                ("type", "ranged weapon") => "Ranged".to_string(),
                ("type", "mundane weapon") | ("miscellaneous", "mundane") | ("rarity", "none") => {
                    continue
                }
                ("property", p) => {
                    format!("{PROPERTY_TAG}{}", p.replace(['-', ' '], "").to_uppercase())
                }
                _ => return None,
            };
            alts.push((neg, tag));
        }
        if !alts.is_empty() {
            out.push(alts);
        }
    }
    Some(out)
}

/// The supports expression of the core weapon proficiencies a filter selects; `None` for a
/// filter with other parts.
#[must_use]
pub fn weapon_filter_supports(filter: &str) -> Option<String> {
    let render = |(neg, tag): &Alt| if *neg { format!("!{tag}") } else { tag.clone() };
    let mut parts: Vec<String> = vec!["Weapon".into()];
    for alts in clauses(filter)? {
        let rendered: Vec<String> = alts.iter().map(render).collect();
        parts.push(match rendered.as_slice() {
            [one] => one.clone(),
            many => format!("({})", many.join("||")),
        });
    }
    Some(parts.join("&&"))
}

/// The core weapon proficiency ids a filter matches, in table order; `None` for a filter with
/// other parts.
#[must_use]
pub fn weapon_filter_matches(filter: &str) -> Option<Vec<String>> {
    let clauses = clauses(filter)?;
    let ids = WEAPON_TAGS
        .iter()
        .filter(|(_, tags)| {
            let has = |tag: &str| {
                tag.strip_prefix(PROPERTY_TAG)
                    .map_or_else(|| tags.contains(&tag), |p| tags.contains(&p))
            };
            clauses
                .iter()
                .all(|alts| alts.iter().any(|(neg, tag)| has(tag) != *neg))
        })
        .map(|(weapon, _)| format!("ID_PROFICIENCY_WEAPON_PROFICIENCY_{weapon}"))
        .collect();
    Some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    fn prof(s: &str) -> String {
        format!("ID_PROFICIENCY_WEAPON_PROFICIENCY_{s}")
    }

    #[test]
    fn xphb_monk_and_rogue_filters_match_the_srd_weapons() {
        assert_eq!(
            weapon_filter_matches("type=martial weapon|property=light"),
            Some(vec![
                prof("SCIMITAR"),
                prof("SHORTSWORD"),
                prof("CROSSBOW_HAND")
            ])
        );
        assert_eq!(
            weapon_filter_matches("type=martial weapon|property=light;finesse"),
            Some(vec![
                prof("RAPIER"),
                prof("SCIMITAR"),
                prof("SHORTSWORD"),
                prof("WHIP"),
                prof("CROSSBOW_HAND")
            ])
        );
        let not_two_handed =
            weapon_filter_matches("type=simple weapon|property=!two-handed").expect("filter");
        assert!(not_two_handed.contains(&prof("DAGGER")));
        assert!(!not_two_handed.contains(&prof("GREATCLUB")));
        assert!(weapon_filter_matches("source=phb").is_none());
    }

    #[test]
    fn supports_rendering_is_unchanged() {
        assert_eq!(
            weapon_filter_supports(
                "type=martial weapon;mundane weapon|property=light;finesse|miscellaneous=mundane"
            )
            .as_deref(),
            Some("Weapon&&Martial&&(ID_INTERNAL_WEAPON_PROPERTY_LIGHT||ID_INTERNAL_WEAPON_PROPERTY_FINESSE)")
        );
        assert_eq!(
            weapon_filter_supports("type=melee weapon|property=!two-handed|rarity=none").as_deref(),
            Some("Weapon&&Melee&&!ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED")
        );
    }

    /// The static table is the tags of the core pack's weapon proficiency elements.
    #[test]
    fn weapon_table_matches_the_core_pack() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/core/proficiencies.json"
        );
        let raw: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("core")).expect("json");
        let els = raw
            .get("elements")
            .and_then(Value::as_array)
            .or_else(|| raw.as_array())
            .expect("elements");
        let mut core_rows = BTreeSet::new();
        for e in els {
            let id = e["id"].as_str().unwrap_or_default();
            let Some(weapon) = id.strip_prefix("ID_PROFICIENCY_WEAPON_PROFICIENCY_") else {
                continue;
            };
            let Some(supports) = e.get("supports").and_then(Value::as_array) else {
                continue;
            };
            let mut tags: Vec<String> = supports
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|t| match t {
                    "Simple" | "Martial" | "Melee" | "Ranged" => Some(t.to_string()),
                    _ => t.strip_prefix(PROPERTY_TAG).map(str::to_string),
                })
                .collect();
            tags.sort();
            core_rows.insert((weapon.to_string(), tags));
        }
        let table: BTreeSet<(String, Vec<String>)> = WEAPON_TAGS
            .iter()
            .map(|(w, tags)| {
                let mut t: Vec<String> = tags.iter().map(|s| (*s).to_string()).collect();
                t.sort();
                ((*w).to_string(), t)
            })
            .collect();
        assert_eq!(table, core_rows);
    }
}
