//! Breath Weapon attack rows (st3-golden F4): a save DC and damage, both editions.
//!
//! Aurora shows the breath weapon only as sheet text (PHB race-dragonborn.xml: "Your breath
//! weapon does {{breath-weapon:dice count}}d{{breath-weapon:dice size}}
//! {{draconic-ancestry:damage type}} damage in a {{draconic-ancestry:breath}} DC
//! {{breath-weapon:dc}}") and creates no attack row (its rows are weapons only,
//! AttackSectionItem.cs). The row reads the same statistics, so it shows what that text shows:
//! - 2014 (Aurora PHB, SRD 5.1 pack): `breath-weapon:dc`, `breath-weapon:dice count` and
//!   `breath-weapon:dice size`; the damage type and save ability come from the ancestry's inline
//!   statistics `draconic-ancestry:damage type` and `draconic-ancestry:breath` ("(Dex. save)").
//! - 2024 (SRD 5.2.1): a `damage_scaling` rule (dice by character level, damage type from the
//!   inline statistic its `type_from` names) on an element whose `data` gives the save ability
//!   (`save`) and DC formula (`dc`).

use crate::graph::Graph;
use crate::snapshot::{AttackSave, AttackSnapshot};
use crate::stats::StatSet;
use crate::universe::URule;
use serde_json::Value;

const NAME: &str = "Breath Weapon";

/// Text of the first live inline statistic `name` (`draconic-ancestry:damage type` = `Fire`).
pub(crate) fn inline_text(graph: &Graph<'_>, name: &str) -> Option<String> {
    graph.preorder().into_iter().find_map(|id| {
        let n = &graph.nodes[id];
        let level = graph.mgr_level(n.mgr);
        n.e.rules.iter().find_map(|r| match r {
            URule::Stat(st)
                if st.inline && st.name.eq_ignore_ascii_case(name) && st.level <= level =>
            {
                Some(st.value_text.trim().to_string())
            }
            _ => None,
        })
    })
}

/// Save ability named in text such as `5 by 30 ft. line (Dex. save)`.
fn save_ability(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at = lower.find("save")?;
    let word = lower[..at]
        .trim_end()
        .trim_end_matches('.')
        .rsplit(|c: char| !c.is_ascii_alphabetic())
        .next()?;
    crate::ids::ability_index(word).map(|i| crate::ids::ABILITIES[i].to_string())
}

/// `8+constitution+proficiency` -> `8+constitution:modifier+proficiency` (a bare ability in
/// a save DC formula is its modifier).
fn modifier_terms(formula: &str) -> String {
    formula
        .split('+')
        .map(|t| {
            let t = t.trim();
            if crate::ids::ABILITIES.contains(&t.to_lowercase().as_str()) {
                format!("{t}:modifier")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn row(name: &str, dc: i32, ability: String, dice: &str, ty: &str) -> AttackSnapshot {
    AttackSnapshot {
        uid: format!("breath:{}", name.to_lowercase()),
        name: name.to_string(),
        range: String::new(),
        attack: 0,
        damage: format!("{dice} {}", ty.to_lowercase()).trim().to_string(),
        ability: "constitution".into(),
        proficient: true,
        mastery: None,
        crit_range: 20,
        die_floor: None,
        offhand_damage: None,
        ammunition: None,
        save: Some(AttackSave { dc, ability }),
    }
}

/// The 2014 row from the `breath-weapon:*` statistics.
fn legacy_row(graph: &Graph<'_>, stats: &StatSet) -> Option<AttackSnapshot> {
    if !stats.exists("breath-weapon:dc") || !stats.exists("breath-weapon:dice count") {
        return None;
    }
    let count = stats.sum("breath-weapon:dice count");
    let size = stats.sum("breath-weapon:dice size");
    if count <= 0 || size <= 0 {
        return None;
    }
    let ty = inline_text(graph, "draconic-ancestry:damage type").unwrap_or_default();
    let ability = inline_text(graph, "draconic-ancestry:breath")
        .and_then(|t| save_ability(&t))
        .unwrap_or_else(|| "dexterity".into());
    Some(row(
        NAME,
        stats.sum("breath-weapon:dc"),
        ability,
        &format!("{count}d{size}"),
        &ty,
    ))
}

/// The 2024 rows from `damage_scaling` rules with a save in their element's data.
fn scaling_rows(graph: &Graph<'_>, stats: &StatSet, mods: &[i32; 6]) -> Vec<AttackSnapshot> {
    let level = graph.total_level();
    crate::rule_view::live_rules(graph, &["damage_scaling"])
        .iter()
        .filter_map(|l| {
            let save = l.el.data.get("save").and_then(Value::as_str)?;
            let dc_formula = l.el.data.get("dc").and_then(Value::as_str)?;
            let dc =
                crate::play_rules::eval_formula(graph, stats, mods, &modifier_terms(dc_formula))?;
            let ability =
                crate::ids::ability_index(save).map(|i| crate::ids::ABILITIES[i].to_string())?;
            let dice =
                l.c.fields
                    .get("levels")
                    .and_then(Value::as_object)
                    .and_then(|m| crate::cantrips::dice_for_level(m, level))
                    .or_else(|| l.str("dice").map(str::to_string))?;
            let ty = l
                .str("type_from")
                .and_then(|t| inline_text(graph, t))
                .unwrap_or_default();
            let name = l.str("name").unwrap_or(l.el.name.as_str());
            Some(row(name, dc, ability, &dice, &ty))
        })
        .collect()
}

/// Breath Weapon rows of the character (see the module docs).
#[must_use]
pub fn breath_attacks(graph: &Graph<'_>, stats: &StatSet, mods: &[i32; 6]) -> Vec<AttackSnapshot> {
    let mut out = scaling_rows(graph, stats, mods);
    if out.is_empty() {
        out.extend(legacy_row(graph, stats));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::save_ability;

    #[test]
    fn bare_abilities_in_a_dc_formula_are_modifiers() {
        assert_eq!(
            super::modifier_terms("8+constitution+proficiency"),
            "8+constitution:modifier+proficiency"
        );
    }

    #[test]
    fn save_ability_from_the_breath_text() {
        assert_eq!(
            save_ability("5 by 30 ft. line (Dex. save)").as_deref(),
            Some("dexterity")
        );
        assert_eq!(
            save_ability("15 ft. cone (Con. save)").as_deref(),
            Some("constitution")
        );
        assert_eq!(save_ability("15 ft. cone"), None);
    }
}
