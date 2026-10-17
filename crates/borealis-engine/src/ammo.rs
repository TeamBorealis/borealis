//! Ammunition a weapon fires: carried `+N` ammunition (`Arrow +1`, an `Ammunition, +1` adorner on
//! arrows) adds its enhancement to the attack and damage of a weapon with the Ammunition property
//! that fires its kind.
//!
//! An ammunition item has the setter `category` (or `type`) `Ammunition`, or an `ammunition` kind
//! setter, and an `enhancement`.
//! Its kind is the setter `ammunition` (`arrow`, `bolt`, `sling bullet`, `needle`, ...), else read
//! from its name; a weapon's kind the same way (a crossbow fires bolts, a bow arrows). An unknown
//! kind on either side matches any ("any ammunition").

use crate::item_base;
use crate::state::CharacterState;
use crate::universe::{UElem, Universe};
use borealis_content::Element;

/// Supports tag of the Ammunition weapon property.
pub const AMMUNITION_PROPERTY: &str = "ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION";
/// Setter naming the ammunition kind of an ammunition item or of a weapon.
const KIND_SETTER: &str = "ammunition";
/// Category / type value of ammunition items.
const AMMUNITION: &str = "ammunition";

/// Ammunition a weapon fires, with the enhancement it adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// Display name of the inventory line.
    pub name: String,
    /// Attack and damage bonus.
    pub enhancement: i32,
}

/// The `+N` of an element's `enhancement` setter.
#[must_use]
pub fn enhancement_of(el: &Element) -> Option<i32> {
    el.setter_value("enhancement")
        .and_then(|v| v.trim().trim_start_matches('+').parse::<i32>().ok())
}

/// Category or type `Ammunition`, or (not a weapon) an `ammunition` kind setter: a bundle such
/// as `+1 Arrows (20)` filed under Equipment Packs.
fn is_ammunition(el: &Element) -> bool {
    ["category", "type"].iter().any(|s| {
        el.setter_value(s)
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(AMMUNITION))
    }) || (el.ty != "Weapon" && el.setter(KIND_SETTER).is_some())
}

/// Kind from an explicit setter, else from the name of an ammunition item.
fn ammo_kind(el: &Element) -> Option<String> {
    if let Some(k) = explicit_kind(el) {
        return Some(k);
    }
    let n = el.name.to_lowercase();
    let kind = if n.contains("bolt") {
        "bolt"
    } else if n.contains("arrow") {
        "arrow"
    } else if n.contains("needle") {
        "needle"
    } else if n.contains("sling") {
        "sling bullet"
    } else if n.contains("bullet") {
        "bullet"
    } else {
        return None;
    };
    Some(kind.to_string())
}

/// Kind from an explicit setter, else from the name of a weapon.
fn weapon_kind(el: &Element) -> Option<String> {
    if let Some(k) = explicit_kind(el) {
        return Some(k);
    }
    let n = el.name.to_lowercase();
    let kind = if n.contains("crossbow") {
        "bolt"
    } else if n.contains("blowgun") {
        "needle"
    } else if n.contains("sling") {
        "sling bullet"
    } else if n.contains("bow") {
        "arrow"
    } else if ["pistol", "musket", "rifle", "revolver"]
        .iter()
        .any(|f| n.contains(f))
    {
        "bullet"
    } else {
        return None;
    };
    Some(kind.to_string())
}

fn explicit_kind(el: &Element) -> Option<String> {
    let k = el.setter_value(KIND_SETTER)?.trim().to_lowercase();
    let k = k.trim_end_matches('s').to_string();
    (!k.is_empty() && k != "any" && k != "true").then_some(k)
}

fn kinds_match(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// The ammunition `weapon` fires from the character's carried lines (storage left out): an
/// equipped matching line first, then the highest enhancement. `None` when the weapon has no
/// Ammunition property or no carried line adds a bonus.
#[must_use]
pub fn loaded(uni: &Universe, state: &CharacterState, weapon: &UElem) -> Option<Loaded> {
    if !weapon.supports.iter().any(|s| s == AMMUNITION_PROPERTY) {
        return None;
    }
    let wants = weapon_kind(&weapon.el);
    let mut best: Option<(bool, i32, String)> = None;
    for it in state
        .inventory
        .items
        .iter()
        .filter(|it| it.storage.is_none() && it.amount > 0)
    {
        let Some((base, adorner)) = item_base::resolve(uni, it) else {
            continue;
        };
        let parts: Vec<&Element> = std::iter::once(&base.el)
            .chain(adorner.map(|a| &a.el))
            .collect();
        if !parts.iter().any(|p| is_ammunition(p)) {
            continue;
        }
        let enhancement = parts
            .iter()
            .rev()
            .find_map(|p| enhancement_of(p))
            .unwrap_or(0);
        let kind = parts.iter().rev().find_map(|p| ammo_kind(p));
        if enhancement <= 0 || !kinds_match(wants.as_deref(), kind.as_deref()) {
            continue;
        }
        let key = (it.equipped.is_some(), enhancement);
        if best.as_ref().map_or(true, |b| key > (b.0, b.1)) {
            let name = it.alt_name.clone().unwrap_or_else(|| match adorner {
                Some(a) => format!("{} ({})", base.el.name, a.el.name),
                None => base.el.name.clone(),
            });
            best = Some((key.0, key.1, name));
        }
    }
    best.map(|(_, enhancement, name)| Loaded { name, enhancement })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Setter;

    fn el(name: &str, kind: Option<&str>) -> Element {
        let mut e = Element {
            name: name.into(),
            ..Element::default()
        };
        if let Some(k) = kind {
            e.setters.push(Setter::new(KIND_SETTER, k));
        }
        e
    }

    #[test]
    fn kinds_come_from_the_setter_or_the_name() {
        assert_eq!(
            weapon_kind(&el("Light Crossbow", None)).as_deref(),
            Some("bolt")
        );
        assert_eq!(weapon_kind(&el("Longbow", None)).as_deref(), Some("arrow"));
        assert_eq!(
            weapon_kind(&el("Sling", None)).as_deref(),
            Some("sling bullet")
        );
        assert_eq!(weapon_kind(&el("Hand Cannon", None)), None);
        assert_eq!(
            weapon_kind(&el("Odd", Some("Arrows"))).as_deref(),
            Some("arrow")
        );
        assert_eq!(
            ammo_kind(&el("+1 Crossbow Bolt", None)).as_deref(),
            Some("bolt")
        );
        assert_eq!(
            ammo_kind(&el("Arrows (20)", None)).as_deref(),
            Some("arrow")
        );
        assert_eq!(ammo_kind(&el("Ammunition, +1", None)), None);
        assert!(kinds_match(Some("arrow"), None) && !kinds_match(Some("arrow"), Some("bolt")));
    }
}
