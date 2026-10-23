//! The name an inventory line is shown with.
//!
//! Aurora names an adorned line (a magic item layered on a base armor or weapon) by the magic
//! item's `name-format` setter, with `{{parent}}` / `$(parent)` the base item's name and
//! `{{enhancement}}` / `$(enhancement)` the magic item's `enhancement` setter: "Armor, +1"
//! (`{{parent}} +{{enhancement}}`) on Chain Mail is "Chain Mail +1". In the inventory an
//! adorner without a `name-format` keeps both names, "Chain Mail (Adamantine Armor)"; its
//! attack row keeps the base weapon's name.

use crate::state::InventoryItem;
use crate::universe::{UElem, Universe};

/// `base` named by the `name-format` of `adorner`; `None` when it has none.
#[must_use]
pub fn formatted_name(base: &UElem, adorner: &UElem) -> Option<String> {
    let format = adorner
        .el
        .setter_value("name-format")
        .map(str::trim)
        .filter(|f| !f.is_empty())?;
    let enhancement = adorner.el.setter_value("enhancement").unwrap_or("").trim();
    Some(
        format
            .replace("{{parent}}", &base.el.name)
            .replace("$(parent)", &base.el.name)
            .replace("{{enhancement}}", enhancement)
            .replace("$(enhancement)", enhancement),
    )
}

/// The inventory name of `base` under `adorner`.
#[must_use]
pub fn adorned_name(base: &UElem, adorner: &UElem) -> String {
    formatted_name(base, adorner)
        .unwrap_or_else(|| format!("{} ({})", base.el.name, adorner.el.name))
}

/// The attack row name of a line: its alternative name, else its adorner's `name-format`,
/// else the item's own name. `None` when the item is not in the universe.
#[must_use]
pub fn attack_name(uni: &Universe, it: &InventoryItem) -> Option<String> {
    if let Some(alt) = it.alt_name.as_ref() {
        return Some(alt.clone());
    }
    let item = uni.get(&it.item)?;
    let formatted = it
        .adorner
        .as_deref()
        .and_then(|a| uni.get(a))
        .and_then(|a| formatted_name(item, a));
    Some(formatted.unwrap_or_else(|| item.el.name.clone()))
}
