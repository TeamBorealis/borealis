//! A weapon's Mastery properties (2024). A weapon usually has one, but homebrew may give it
//! several, and each one is listed with its own text. Every form content uses is read:
//!
//! 1. `data.mastery_ids` (an array of element ids), then `data.mastery_id` (the single-id form
//!    the bundled SRD weapons and older custom content use);
//! 2. `supports` entries naming a `Mastery Property` element (Aurora's form, one entry each);
//! 3. property names: every `mastery` setter and `data.mastery`, each a name or a comma list
//!    (`Topple, Push`), or an array of names or `Name|Source` references (5etools-format).
//!
//! A property named twice (by id and by name) is listed once, in the first place it appears.

use crate::graph::Graph;
use crate::snapshot::{MasteryPick, MasteryProperty, MasteryRef};
use crate::universe::Universe;
use borealis_content::store::phb24_candidate;
use borealis_content::Element;
use serde_json::Value;

/// Type of the Mastery property elements (`ID_2024_MASTERY_<NAME>`).
pub const MASTERY_PROPERTY_TYPE: &str = "Mastery Property";

/// One Mastery property of a weapon: its name and the element that defines it, when one does.
#[derive(Debug, Clone, Copy)]
pub struct MasteryLink<'a> {
    /// Property name as the element (or else the content) spells it.
    pub name: &'a str,
    /// The `Mastery Property` element, if the content resolves to one.
    pub element: Option<&'a Element>,
}

/// The names of a comma (or semicolon) list.
fn split_names(text: &str) -> impl Iterator<Item = &str> {
    text.split([',', ';'])
        .map(str::trim)
        .filter(|n| !n.is_empty())
}

/// The Mastery property names written on a weapon: its `mastery` setters and `data.mastery`.
fn written_names(weapon: &Element) -> Vec<&str> {
    let setters = weapon
        .setters
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case("mastery"))
        .flat_map(|s| split_names(&s.value));
    let mut out: Vec<&str> = setters.collect();
    match weapon.data.get("mastery") {
        Some(Value::String(s)) => out.extend(split_names(s)),
        Some(Value::Array(items)) => {
            for v in items {
                let uid = match v {
                    Value::String(s) => Some(s.as_str()),
                    other => other.get("uid").and_then(Value::as_str),
                };
                // `Topple|XPHB`: the name is the part before the source
                if let Some(name) = uid.and_then(|u| u.split('|').next()) {
                    out.extend(split_names(name));
                }
            }
        }
        _ => {}
    }
    out
}

/// The ids a weapon names for its Mastery properties: `data.mastery_ids`, then `data.mastery_id`.
fn named_ids(weapon: &Element) -> Vec<&str> {
    let many = weapon
        .data
        .get("mastery_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    let one = weapon.data.get("mastery_id").and_then(Value::as_str);
    many.chain(one).filter(|id| !id.trim().is_empty()).collect()
}

/// Every Mastery property of `weapon`, in content order, each once (see the module docs).
/// `get` resolves an element id; `named` finds the `Mastery Property` element of a name, or
/// returns `None` to keep a written name as it is.
pub fn mastery_links<'a>(
    weapon: &'a Element,
    get: impl Fn(&str) -> Option<&'a Element>,
    named: impl Fn(&str) -> Option<&'a Element>,
) -> Vec<MasteryLink<'a>> {
    let mut out: Vec<MasteryLink<'a>> = Vec::new();
    // a property element's own `data.mastery` is its key, not a property it has
    if weapon.ty == MASTERY_PROPERTY_TYPE {
        return out;
    }
    let mut add = |link: MasteryLink<'a>| {
        if !out.iter().any(|l| l.name.eq_ignore_ascii_case(link.name)) {
            out.push(link);
        }
    };
    let by_id = named_ids(weapon).into_iter().filter_map(&get);
    // Aurora content names Aurora's 2024 core ids (`ID_WOTC_PHB24_*`), which stand for the SRD 5.2.1
    // properties when no pack defines them (as everywhere else, see `phb24_candidate`)
    let supported = weapon
        .supports
        .iter()
        .filter_map(|id| get(id).or_else(|| phb24_candidate(id).and_then(|c| get(&c))))
        .filter(|m| m.ty == MASTERY_PROPERTY_TYPE);
    for m in by_id.chain(supported) {
        add(MasteryLink {
            name: m.name.as_str(),
            element: Some(m),
        });
    }
    for name in written_names(weapon) {
        let element = named(name);
        add(MasteryLink {
            name: element.map_or(name, |m| m.name.as_str()),
            element,
        });
    }
    out
}

/// `Topple` for `topple`: a written name with each word capitalised.
#[must_use]
pub fn display_name(name: &str) -> String {
    name.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase()
                    .chain(c.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Mastery properties of a weapon element with their plain text (empty when no element
/// defines a written name).
#[must_use]
pub fn properties_of(uni: &Universe, weapon: &Element) -> Vec<MasteryProperty> {
    let get = |id: &str| uni.get(id).map(|u| &u.el);
    let named = |name: &str| {
        uni.by_type(MASTERY_PROPERTY_TYPE)
            .map(|u| &u.el)
            .find(|m| m.name.eq_ignore_ascii_case(name))
    };
    mastery_links(weapon, get, named)
        .into_iter()
        .map(|l| MasteryProperty {
            name: l
                .element
                .map_or_else(|| display_name(l.name), |m| m.name.clone()),
            text: l
                .element
                .map(|m| crate::play_rules::plain_text(&m.description))
                .unwrap_or_default(),
        })
        .collect()
}

/// True if the weapon has at least one Mastery property (a Weapon Mastery slot may take it).
#[must_use]
pub fn has_mastery(uni: &Universe, weapon: &Element) -> bool {
    !mastery_links(weapon, |id| uni.get(id).map(|u| &u.el), |_| None).is_empty()
}

/// Prefix of Aurora's weapon category tags (`ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE`).
const CATEGORY_TAG: &str = "ID_INTERNAL_WEAPON_CATEGORY_";

/// `data` keys of a weapon in Borealis's own shape; such a weapon states its kind in `data`.
const BOREALIS_WEAPON_DATA: [&str; 4] = [
    "weapon_category",
    "mastery_id",
    "mastery_ids",
    "base_weapon",
];

/// The kind of weapon a Weapon Mastery filter tests: its category (`Simple Melee`) and whether it
/// is a melee weapon. A weapon in Borealis's shape gives them in `data.weapon_category` and
/// `data.melee` (none for a weapon made from a base item, which is not a kind of its own); an
/// Aurora-format weapon gives them by its category tag in `supports`.
#[must_use]
pub fn weapon_kind(weapon: &Element) -> (String, bool) {
    if BOREALIS_WEAPON_DATA
        .iter()
        .any(|k| weapon.data.contains_key(*k))
    {
        let category = weapon.data.get("weapon_category").and_then(Value::as_str);
        let melee = weapon.data.get("melee").and_then(Value::as_bool);
        return (category.unwrap_or("").to_string(), melee.unwrap_or(false));
    }
    let tagged = weapon.supports.iter().find_map(|s| {
        let (kind, reach) = s.strip_prefix(CATEGORY_TAG)?.split_once('_')?;
        matches!(kind, "SIMPLE" | "MARTIAL").then(|| (kind, reach))
    });
    match tagged {
        Some((kind, reach)) => (display_name(&format!("{kind} {reach}")), reach == "MELEE"),
        None => (String::new(), false),
    }
}

/// The weapons chosen for Weapon Mastery slots, each with all of its properties.
#[must_use]
pub fn mastery_picks(graph: &Graph<'_>) -> Vec<MasteryPick> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let n = &graph.nodes[id];
        // `Origin::Selected` carries the parent's slot index, not its rule index.
        let parent_is_mastery = n.parent.is_some_and(|p| {
            let parent = &graph.nodes[p];
            matches!(n.origin, crate::graph::Origin::Selected(si)
                if parent.slots.get(si).is_some_and(|slot|
                    matches!(parent.e.el.rules.get(slot.rule), Some(borealis_content::Rule::Select(s))
                        if crate::custom_rules::kind_of(s) == Some("mastery"))))
        });
        if !parent_is_mastery || out.iter().any(|p: &MasteryPick| p.weapon == n.e.el.id) {
            continue;
        }
        let properties = properties_of(graph.uni, &n.e.el);
        if !properties.is_empty() {
            out.push(MasteryPick {
                weapon: n.e.el.id.clone(),
                name: n.e.el.name.clone(),
                properties,
            });
        }
    }
    out
}

/// The attack line's Mastery properties of a weapon, each marked `mastered` when the character
/// chose this kind of weapon (informational otherwise).
#[must_use]
pub fn attack_masteries(
    uni: &Universe,
    weapon: &Element,
    picks: &[MasteryPick],
) -> Vec<MasteryRef> {
    let properties = properties_of(uni, weapon);
    if properties.is_empty() {
        return Vec::new();
    }
    // a magic weapon of a mastered kind (`+1 Longsword` with `data.base_weapon` = a Longsword)
    // is mastered too, whichever pack the picked Longsword comes from
    let base = weapon.data.get("base_weapon").and_then(Value::as_str);
    let base_name = base.and_then(|b| uni.get(b)).map(|b| b.el.name.as_str());
    let mastered = picks.iter().any(|p| {
        p.weapon == weapon.id
            || base == Some(p.weapon.as_str())
            || base_name.is_some_and(|n| n.eq_ignore_ascii_case(&p.name))
    });
    properties
        .into_iter()
        .map(|p| MasteryRef {
            name: p.name,
            text: p.text,
            mastered,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn element(v: Value) -> Element {
        serde_json::from_value(v).expect("element")
    }

    fn property(id: &str, name: &str) -> Element {
        element(json!({"id": id, "name": name, "type": MASTERY_PROPERTY_TYPE, "source": "Test"}))
    }

    fn names(links: &[MasteryLink<'_>]) -> Vec<String> {
        links
            .iter()
            .map(|l| format!("{}{}", l.name, if l.element.is_some() { "" } else { "?" }))
            .collect()
    }

    #[test]
    fn every_form_is_read_in_order_and_each_property_once() {
        let props = [
            property("ID_T_MASTERY_TOPPLE", "Topple"),
            property("ID_T_MASTERY_PUSH", "Push"),
            property("ID_T_MASTERY_SAP", "Sap"),
            property("ID_T_MASTERY_VEX", "Vex"),
        ];
        let get = |id: &str| props.iter().find(|p| p.id == id);
        let named = |n: &str| props.iter().find(|p| p.name.eq_ignore_ascii_case(n));
        let weapon = element(json!({
            "id": "ID_T_WEAPON_THORNWHIP", "name": "Thornwhip", "type": "Weapon", "source": "Test",
            "supports": ["ID_T_MASTERY_SAP", "ID_INTERNAL_WEAPON_PROPERTY_REACH"],
            "setters": [{"name": "mastery", "value": "topple, Vex"}, {"name": "mastery", "value": "Gust"}],
            "data": {"mastery_id": "ID_T_MASTERY_TOPPLE", "mastery_ids": ["ID_T_MASTERY_TOPPLE", "ID_T_MASTERY_PUSH"]}
        }));
        let links = mastery_links(&weapon, get, named);
        assert_eq!(names(&links), ["Topple", "Push", "Sap", "Vex", "Gust?"]);
    }

    #[test]
    fn the_single_id_form_alone_is_one_property() {
        let props = [property("ID_T_MASTERY_SLOW", "Slow")];
        let weapon = element(json!({
            "id": "ID_T_WEAPON_CLUB", "name": "Club", "type": "Weapon", "source": "Test",
            "supports": ["ID_T_MASTERY_SLOW"],
            "setters": [{"name": "mastery", "value": "slow"}],
            "data": {"mastery": "slow", "mastery_id": "ID_T_MASTERY_SLOW"}
        }));
        let links = mastery_links(&weapon, |id| props.iter().find(|p| p.id == id), |_| None);
        assert_eq!(names(&links), ["Slow"]);
    }

    #[test]
    fn five_etools_references_and_comma_lists_are_names() {
        let weapon = element(json!({
            "id": "ID_T_WEAPON_HOOKBLADE", "name": "Hookblade", "type": "Weapon", "source": "Test",
            "data": {"mastery": ["Snare|HB", {"uid": "Ricochet|HB"}, "Snare"]}
        }));
        let links = mastery_links(&weapon, |_| None, |_| None);
        assert_eq!(names(&links), ["Snare?", "Ricochet?"]);
        let text = element(json!({
            "id": "ID_T_WEAPON_HOOKBLADE", "name": "Hookblade", "type": "Weapon", "source": "Test",
            "data": {"mastery": "snare; ricochet"}
        }));
        assert_eq!(
            names(&mastery_links(&text, |_| None, |_| None)),
            ["snare?", "ricochet?"]
        );
        assert_eq!(display_name("tumble back"), "Tumble Back");
    }

    #[test]
    fn the_kind_of_weapon_comes_from_data_or_from_the_aurora_category_tag() {
        let tagged = element(json!({
            "id": "ID_T_WEAPON_BRIARSTAFF", "name": "Briarstaff", "type": "Weapon", "source": "Test",
            "supports": ["ID_INTERNAL_WEAPON_CATEGORY_FIREARM", "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"]
        }));
        assert_eq!(weapon_kind(&tagged), ("Martial Melee".to_string(), true));
        let data = element(json!({
            "id": "ID_T_WEAPON_SLING", "name": "Sling", "type": "Weapon", "source": "Test",
            "supports": ["ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"],
            "data": {"weapon_category": "Simple Ranged", "melee": false}
        }));
        assert_eq!(weapon_kind(&data), ("Simple Ranged".to_string(), false));
        let none =
            element(json!({"id": "ID_T_ROCK", "name": "Rock", "type": "Weapon", "source": "Test"}));
        assert_eq!(weapon_kind(&none), (String::new(), false));
        // a weapon made from a base item names its mastery but is no kind of its own
        let magic = element(json!({
            "id": "ID_T_WEAPON_GLIMMER_SLING", "name": "Glimmer Sling", "type": "Weapon", "source": "Test",
            "supports": ["ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED"],
            "data": {"mastery_id": "ID_T_MASTERY_SLOW"}
        }));
        assert_eq!(weapon_kind(&magic), (String::new(), false));
    }

    #[test]
    fn a_mastery_property_element_has_no_mastery_of_its_own() {
        let topple = element(json!({"id": "ID_T_MASTERY_TOPPLE", "name": "Topple",
            "type": MASTERY_PROPERTY_TYPE, "source": "Test", "data": {"mastery": "topple"}}));
        assert!(mastery_links(&topple, |_| None, |_| None).is_empty());
    }

    #[test]
    fn a_weapon_without_mastery_has_none() {
        let weapon = element(json!({
            "id": "ID_T_WEAPON_STICK", "name": "Stick", "type": "Weapon", "source": "Test",
            "supports": ["ID_INTERNAL_WEAPON_PROPERTY_LIGHT"],
            "data": {"mastery_ids": []}
        }));
        assert!(mastery_links(&weapon, |_| None, |_| None).is_empty());
    }
}
