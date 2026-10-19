//! The equipment screen's category list, built the way Aurora builds it
//! (`RefactoredEquipmentSectionViewModel.InitializeCategoryCollection` and
//! `UpdateCategoryCollection`).
//!
//! Order: Aurora's 22 fixed names, then every other `category` setter value found on an item, in
//! content load order, those not starting with "Additional " before those that do. Items sit in
//! their category, except that every Weapon element sits in Weapons and every Armor element in
//! Armor, and an item with no category falls back to Adventuring Gear (Wondrous Items for a magic
//! item). A category is shown only when one of its items survives the character's ruleset and
//! source restrictions (Aurora hides a category whose items are all from restricted sources, an
//! empty one included), and an "Additional <X> Spell" category only when the character has
//! spellcasting <X>.

use crate::dto_ext::ItemCategory;
use crate::error::ApiResult;
use crate::gear_info::is_item;
use crate::state::AppState;
use borealis_content::{Element, Ruleset};
use std::collections::{HashMap, HashSet};

/// Aurora's fixed category names, in its order.
pub const FIXED_CATEGORIES: [&str; 22] = [
    "Adventuring Gear",
    "Treasure",
    "Trade Goods",
    "Equipment Packs",
    "Tools",
    "Musical Instruments",
    "Armor",
    "Magic Armor",
    "Weapons",
    "Magic Weapons",
    "Ammunition",
    "Spellcasting Focus",
    "Wondrous Items",
    "Supernatural Gifts",
    "Staffs",
    "Rods",
    "Wands",
    "Rings",
    "Potions",
    "Poison",
    "Scrolls",
    "Spell Scrolls",
];

const ADDITIONAL: &str = "Additional ";
const SPELL_WORD: &str = " Spell";
const PLAIN_ADDITIONAL_SPELL: &str = "Additional Spell";
const WEAPONS: &str = "Weapons";
const ARMOR: &str = "Armor";
const GEAR: &str = "Adventuring Gear";
const WONDROUS: &str = "Wondrous Items";

/// The items a character may see: a compatible ruleset and an unrestricted source.
#[derive(Debug, Clone, Default)]
pub struct ItemScope {
    ruleset: Option<Ruleset>,
    restricted: Vec<String>,
}

impl ItemScope {
    /// Is `el` visible in this scope?
    #[must_use]
    pub fn admits(&self, el: &Element) -> bool {
        let ruleset_ok = self
            .ruleset
            .map_or(true, |r| el.ruleset.map_or(true, |x| x.compatible_with(r)));
        ruleset_ok && !self.restricted.iter().any(|r| r == &el.source)
    }
}

/// The `category` setter, trimmed; empty when there is none (Aurora's `Item.Category`).
fn raw_category(el: &Element) -> &str {
    el.setter_value("category").map_or("", str::trim)
}

/// The category an item is listed under on the equipment screen.
#[must_use]
pub fn list_category(el: &Element) -> &str {
    match el.ty.as_str() {
        "Weapon" => WEAPONS,
        "Armor" => ARMOR,
        ty => match raw_category(el) {
            "" if ty == "Magic Item" => WONDROUS,
            "" => GEAR,
            c => c,
        },
    }
}

/// Every category name in Aurora's order (before any is hidden).
#[must_use]
pub fn ordered_names<'a>(items: impl IntoIterator<Item = &'a Element>) -> Vec<String> {
    let mut seen: HashSet<&str> = FIXED_CATEGORIES.into_iter().collect();
    let mut plain: Vec<&str> = Vec::new();
    let mut additional: Vec<&str> = Vec::new();
    for el in items {
        let c = raw_category(el);
        if c.is_empty() || !seen.insert(c) {
            continue;
        }
        if c.starts_with(ADDITIONAL) {
            additional.push(c);
        } else {
            plain.push(c);
        }
    }
    FIXED_CATEGORIES
        .into_iter()
        .chain(plain)
        .chain(additional)
        .map(str::to_string)
        .collect()
}

/// Aurora shows an "Additional <X> Spell" category only to a character with spellcasting `<X>`;
/// every other category passes.
#[must_use]
pub fn spell_category_allowed(name: &str, casting: &[String]) -> bool {
    let gated =
        name.contains(ADDITIONAL) && name.contains(SPELL_WORD) && name != PLAIN_ADDITIONAL_SPELL;
    !gated
        || casting
            .iter()
            .any(|c| name == format!("{ADDITIONAL}{c}{SPELL_WORD}"))
}

/// The shown categories with their visible item counts. `items` are all catalogue items in
/// content order; `scope` decides which ones the character sees; `casting` are the character's
/// spellcasting names.
#[must_use]
pub fn category_list(
    items: &[&Element],
    scope: &ItemScope,
    casting: &[String],
) -> Vec<ItemCategory> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for el in items.iter().filter(|e| scope.admits(e)) {
        *counts.entry(list_category(el)).or_default() += 1;
    }
    ordered_names(items.iter().copied())
        .into_iter()
        .filter(|name| spell_category_allowed(name, casting))
        .filter_map(|name| {
            let count = counts.get(name.as_str()).copied().unwrap_or(0);
            (count > 0).then_some(ItemCategory { name, count })
        })
        .collect()
}

impl AppState {
    /// The item scope of a character (everything when `character_id` is `None`).
    ///
    /// # Errors
    /// `not_found` when the character is not open.
    pub fn item_scope(&self, character_id: Option<&str>) -> ApiResult<ItemScope> {
        let Some(id) = character_id else {
            return Ok(ItemScope::default());
        };
        self.read(id, |_, s| {
            Ok(ItemScope {
                ruleset: Some(s.character.ruleset()),
                restricted: s.character.build.restricted_sources.clone(),
            })
        })
    }

    /// The equipment screen's categories for a character, in Aurora's order, with item counts.
    ///
    /// # Errors
    /// `not_found` when the character is not open.
    pub fn list_item_categories(&self, character_id: &str) -> ApiResult<Vec<ItemCategory>> {
        let scope = self.item_scope(Some(character_id))?;
        let casting: Vec<String> = self.read(character_id, |ctx, s| {
            let snap = ctx.hub.engine.derive_with(&s.character.build, false);
            Ok(snap.spellcasting.into_iter().map(|c| c.name).collect())
        })?;
        let hub = self.hub()?;
        let items: Vec<&Element> = hub
            .store
            .iter()
            .map(|s| &s.element)
            .filter(|e| is_item(e))
            .collect();
        Ok(category_list(&items, &scope, &casting))
    }
}

#[cfg(test)]
#[path = "item_categories_tests.rs"]
mod tests;
