//! Armor, weapon and spell columns read from content setters, for the Equipment and Magic grids.

use crate::gear_info::weapon_properties;
use borealis_content::Element;
use serde::Serialize;

const FLAG_TRUE: &str = "true";

/// Armor columns of a catalogue row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmorFacts {
    /// `Light`, `Medium`, `Heavy` or `Shield`.
    pub group: String,
    /// AC as printed, e.g. `14 + Dex (max 2)` or `+2`.
    pub ac_text: String,
    /// The flat number in the AC text (the bonus for a shield).
    pub ac_base: i32,
    /// Dexterity modifier is added.
    pub adds_dex: bool,
    /// Largest Dexterity modifier that counts (`None` = no cap).
    pub dex_cap: Option<i32>,
    /// Strength score needed to avoid the speed penalty.
    pub strength: Option<u32>,
    /// Imposes disadvantage on Stealth checks.
    pub stealth_disadvantage: bool,
}

/// Weapon columns of a catalogue row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponFacts {
    /// Damage dice, e.g. `1d8`.
    pub damage: String,
    /// Damage type, e.g. `slashing`.
    pub damage_type: String,
    /// Property words, e.g. `Finesse`, `Versatile (1d10)`.
    pub properties: Vec<String>,
    /// Weapon Mastery property (2024 content), e.g. `Sap`.
    pub mastery: Option<String>,
    /// Normal/long range in feet, e.g. `80/320`.
    pub range: Option<String>,
    /// `Simple` or `Martial`.
    pub proficiency: String,
    /// `Melee` or `Ranged`.
    pub kind: String,
}

/// Casting facts of a spell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellFacts {
    /// e.g. `1 action`.
    pub casting_time: String,
    /// e.g. `60 feet`.
    pub range: String,
    /// e.g. `Concentration, up to 1 minute`.
    pub duration: String,
    /// Verbal component.
    pub verbal: bool,
    /// Somatic component.
    pub somatic: bool,
    /// Material component.
    pub material: bool,
    /// The material text, empty when none.
    pub material_text: String,
    /// Needs concentration.
    pub concentration: bool,
    /// Can be cast as a ritual.
    pub ritual: bool,
}

fn text(el: &Element, name: &str) -> String {
    el.setter_value(name).unwrap_or_default().trim().to_string()
}

fn flag(el: &Element, name: &str) -> bool {
    el.setter_value(name)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case(FLAG_TRUE))
}

fn leading_int(s: &str) -> i32 {
    let digits: String = s
        .trim()
        .trim_start_matches('+')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

fn dex_cap(ac: &str) -> Option<i32> {
    let (_, rest) = ac.split_once("max")?;
    let n = leading_int(rest.trim_start());
    Some(n)
}

/// Armor columns, for `Armor` elements.
#[must_use]
pub fn armor_facts(el: &Element) -> Option<ArmorFacts> {
    if el.ty != "Armor" {
        return None;
    }
    let ac_text = text(el, "armorClass");
    Some(ArmorFacts {
        group: text(el, "armor"),
        ac_base: leading_int(&ac_text),
        adds_dex: ac_text.contains("Dex"),
        dex_cap: dex_cap(&ac_text),
        strength: el
            .setter_value("strength")
            .and_then(|v| v.trim().parse().ok()),
        stealth_disadvantage: text(el, "stealth").eq_ignore_ascii_case("disadvantage"),
        ac_text,
    })
}

/// Weapon columns, for `Weapon` elements.
#[must_use]
pub fn weapon_facts(el: &Element) -> Option<WeaponFacts> {
    if el.ty != "Weapon" {
        return None;
    }
    let damage = el.setter("damage");
    let category = el
        .supports
        .iter()
        .find_map(|s| s.strip_prefix("ID_INTERNAL_WEAPON_CATEGORY_"))
        .unwrap_or_default()
        .to_lowercase();
    let (proficiency, kind) = category.split_once('_').unwrap_or((&category, ""));
    Some(WeaponFacts {
        damage: damage
            .map(|d| d.value.trim().to_string())
            .unwrap_or_default(),
        damage_type: damage
            .and_then(|d| d.attribute("type"))
            .unwrap_or_default()
            .to_string(),
        properties: weapon_properties(el),
        mastery: Some(text(el, "mastery"))
            .filter(|m| !m.is_empty())
            .map(|m| crate::ids::title_case(&m)),
        range: Some(text(el, "range")).filter(|r| !r.is_empty()),
        proficiency: crate::ids::title_case(proficiency),
        kind: crate::ids::title_case(kind),
    })
}

/// Casting facts, for `Spell` elements.
#[must_use]
pub fn spell_facts(el: &Element) -> Option<SpellFacts> {
    if el.ty != "Spell" {
        return None;
    }
    Some(SpellFacts {
        casting_time: text(el, "time"),
        range: text(el, "range"),
        duration: text(el, "duration"),
        verbal: flag(el, "hasVerbalComponent"),
        somatic: flag(el, "hasSomaticComponent"),
        material: flag(el, "hasMaterialComponent"),
        material_text: text(el, "materialComponent"),
        concentration: flag(el, "isConcentration"),
        ritual: flag(el, "isRitual"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ac_text() {
        assert_eq!(leading_int("14 + Dex (max 2)"), 14);
        assert_eq!(leading_int("+2"), 2);
        assert_eq!(dex_cap("14 + Dex (max 2)"), Some(2));
        assert_eq!(dex_cap("11 + Dex"), None);
    }
}
