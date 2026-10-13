//! Facts about catalogue items read straight from content setters.

use borealis_content::Element;

const COPPER_PER_GOLD: f64 = 100.0;

fn number(text: &str) -> Option<f64> {
    text.trim().replace(',', "").parse::<f64>().ok()
}

/// Price in copper pieces (setter `cost` with a `currency` attribute).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn cost_cp(el: &Element) -> u64 {
    let Some(c) = el.setter("cost") else { return 0 };
    let rate = match c.attribute("currency").map(str::to_lowercase).as_deref() {
        Some("cp") => 0.01,
        Some("sp") => 0.1,
        Some("ep") => 0.5,
        Some("pp") => 10.0,
        _ => 1.0,
    };
    (number(&c.value).unwrap_or(0.0) * rate * COPPER_PER_GOLD)
        .round()
        .max(0.0) as u64
}

/// Weight of one piece in pounds (setter `weight`, attribute `lb`).
#[must_use]
pub fn weight_lb(el: &Element) -> f64 {
    el.setter("weight")
        .and_then(|w| {
            w.attribute("lb")
                .and_then(number)
                .or_else(|| number(&w.value))
        })
        .unwrap_or(0.0)
}

/// Catalogue category (setter `category`, else the element type).
#[must_use]
pub fn category(el: &Element) -> String {
    el.setter_value("category")
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map_or_else(|| el.ty.clone(), str::to_string)
}

/// Does the item need attunement?
#[must_use]
pub fn requires_attunement(el: &Element) -> bool {
    el.setter_value("attunement")
        .map(|v| v.trim().to_lowercase())
        .is_some_and(|v| !v.is_empty() && v != "false" && v != "0")
}

/// Is the element an item a character can carry?
#[must_use]
pub fn is_item(el: &Element) -> bool {
    matches!(el.ty.as_str(), "Item" | "Weapon" | "Armor" | "Magic Item")
}

/// Weapon properties as display words, from the `ID_INTERNAL_WEAPON_PROPERTY_*` support tags.
#[must_use]
pub fn weapon_properties(el: &Element) -> Vec<String> {
    const PREFIX: &str = "ID_INTERNAL_WEAPON_PROPERTY_";
    let mut out: Vec<String> = el
        .supports
        .iter()
        .filter_map(|s| s.strip_prefix(PREFIX))
        .map(|p| crate::ids::title_case(&p.replace('_', " ").to_lowercase()))
        .collect();
    if let Some(v) = el
        .setter_value("versatile")
        .filter(|v| !v.trim().is_empty())
    {
        out.retain(|p| p != "Versatile");
        out.push(format!("Versatile ({})", v.trim()));
    }
    out
}

/// Is the weapon ranged (a `range` setter or a ranged category tag)?
#[must_use]
pub fn is_ranged(el: &Element) -> bool {
    el.supports
        .iter()
        .any(|s| s.to_uppercase().contains("RANGED"))
}

/// Has the finesse property.
#[must_use]
pub fn is_finesse(el: &Element) -> bool {
    el.supports
        .iter()
        .any(|s| s.to_uppercase().ends_with("_FINESSE"))
}
