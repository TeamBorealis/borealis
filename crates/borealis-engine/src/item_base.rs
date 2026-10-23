//! The base item a magic weapon or magic armor stands for when it is used on its own.
//!
//! Aurora models a magic weapon or armor as an adorner applied on top of a base item (a
//! Longsword adorned with "+1 Weapon"). A magic item that names one concrete base item in its
//! `weapon` / `armor` setter (or in the `addition` of its `type` setter), added to the inventory
//! without a base, is used as that base adorned with itself: it takes a hand or the armor
//! slot, gives an attack line or its armor's AC, and its own rules apply as the adorner's do.
//! Items that name a group ("any sword", `ID_INTERNAL_WEAPON_GROUP_SWORDS`) or several bases
//! have no implied base and keep needing one.

use crate::inventory::{item_facts, ItemFacts};
use crate::state::InventoryItem;
use crate::universe::{UElem, Universe};

/// Element types a base item can have, by the magic item's `type` setter value.
const BASES: [(&str, &str, &str); 2] =
    [("weapon", "Weapon", "weapon"), ("armor", "Armor", "armor")];

/// Characters that make a setter value an expression or a list instead of one item.
const LIST_MARKS: [char; 6] = ['|', ',', '&', '(', '[', ';'];

/// Lower-case alphanumeric words, without a trailing "armor" ("Hide Armor" is "Hide").
fn key(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let trimmed = match words.split_last() {
        Some((last, rest)) if last == "armor" && !rest.is_empty() => rest,
        _ => &words[..],
    };
    trimmed.join(" ")
}

/// The single element of type `ty` the reference `r` names (an id, or a name ignoring case and
/// punctuation), preferring the universe's own edition.
fn named<'u>(uni: &'u Universe, ty: &str, r: &str) -> Option<&'u UElem> {
    let r = r.trim();
    if r.is_empty() || r.contains(&LIST_MARKS[..]) {
        return None;
    }
    if let Some(e) = uni.get(r) {
        return (e.el.ty == ty).then_some(e);
    }
    if r.starts_with("ID_") {
        return None;
    }
    let k = key(r);
    let mut foreign = None;
    for e in uni.by_type(ty).filter(|e| key(&e.el.name) == k) {
        if !e.edition.is_foreign() && e.el.ruleset.map_or(true, |rs| rs == uni.ruleset()) {
            return Some(e);
        }
        foreign.get_or_insert(e);
    }
    foreign
}

/// The base weapon or armor `e` stands for when used without one, if it names exactly one.
#[must_use]
pub fn implied_base<'u>(uni: &'u Universe, e: &UElem) -> Option<&'u UElem> {
    if e.el.ty != "Magic Item" {
        return None;
    }
    let ty_setter = e.el.setter("type")?;
    let kind = ty_setter.value.trim().to_lowercase();
    let (_, base_ty, setter) = BASES.iter().find(|(k, _, _)| *k == kind)?;
    let refs =
        e.el.setter_value(setter)
            .into_iter()
            .chain(ty_setter.attribute("addition"));
    refs.filter_map(|r| named(uni, base_ty, r))
        .find(|b| b.el.id != e.el.id)
}

/// The base type (`"Weapon"` / `"Armor"`) and the setter naming the allowed bases of a magic
/// weapon or armor.
fn base_kind(e: &UElem) -> Option<(&'static str, &'static str)> {
    if e.el.ty != "Magic Item" {
        return None;
    }
    let kind = e.el.setter_value("type")?.trim().to_lowercase();
    BASES
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, ty, setter)| (*ty, *setter))
}

/// A generic magic weapon or armor ("Armor, +1", "Weapon, +1") that names no single base: it is
/// added on a base item the user chooses, as in Aurora's second list.
#[must_use]
pub fn needs_base(uni: &Universe, e: &UElem) -> bool {
    base_kind(e).is_some() && implied_base(uni, e).is_none()
}

/// Whether `base` is one of the bases the magic weapon or armor `adorner` can be applied to:
/// a base of its kind matching its `weapon` / `armor` setter (a supports expression of group
/// ids and names, `(ID_..._MEDIUM||ID_..._HEAVY),!Hide`, or a `|` list of names). No setter
/// allows every base of that kind.
#[must_use]
pub fn can_adorn(adorner: &UElem, base: &UElem) -> bool {
    let Some((base_ty, setter)) = base_kind(adorner) else {
        return false;
    };
    if base.el.ty != base_ty {
        return false;
    }
    let Some(v) = adorner
        .el
        .setter_value(setter)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return true;
    };
    let name = key(&base.el.name);
    let term = |t: &str| {
        let t = t.trim();
        if t.starts_with("ID_") {
            base.el.id == t || base.supports.iter().any(|s| s == t)
        } else {
            key(t) == name
        }
    };
    if !v.contains("ID_") {
        return v.split('|').any(term);
    }
    match borealis_content::expr::SupportsExpr::parse(v) {
        Ok(Some(expr)) => crate::eval::eval_bool(expr.root(), &|leaf| match leaf {
            borealis_content::expr::SupportsLeaf::Tag(t) => term(t),
            borealis_content::expr::SupportsLeaf::Dynamic(_) => false,
        }),
        _ => false,
    }
}

/// The base element and adorner an inventory line is used as: its own (base, adorner), or a
/// magic item without an adorner as its implied base adorned with itself.
#[must_use]
pub fn resolve<'u>(
    uni: &'u Universe,
    it: &InventoryItem,
) -> Option<(&'u UElem, Option<&'u UElem>)> {
    let item = uni.get(&it.item)?;
    let Some(adorner) = it.adorner.as_deref() else {
        return Some(match implied_base(uni, item) {
            Some(base) => (base, Some(item)),
            None => (item, None),
        });
    };
    Some((item, uni.get(adorner)))
}

/// [`item_facts`] of what the element is used as: an item with an implied base is equipped
/// like that base, and needs attunement when either of them does.
#[must_use]
pub fn effective_facts(uni: &Universe, e: &UElem) -> ItemFacts {
    let own = item_facts(e);
    match implied_base(uni, e) {
        Some(base) => {
            let f = item_facts(base);
            ItemFacts {
                requires_attunement: own.requires_attunement || f.requires_attunement,
                ..f
            }
        }
        None => own,
    }
}

#[cfg(test)]
mod tests {
    use super::key;

    #[test]
    fn names_compare_without_case_punctuation_or_a_trailing_armor() {
        assert_eq!(key("Hide Armor"), key("hide"));
        assert_eq!(key("Crossbow, Light"), "crossbow light");
        assert_eq!(key("Armor"), "armor", "a bare 'armor' is kept");
    }
}
