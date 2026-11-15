//! The base items a generic magic weapon or armor can be added on (Aurora's second list on the
//! Magic Armor / Magic Weapons categories, `specs/ui-inventory.md` 6.1).

use crate::dto_ext::ItemSummary;
use crate::equipment::item_row;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_engine::item_base::{can_adorn, needs_base};
use borealis_engine::universe::{UElem, Universe};

/// The element types a base item can have.
const BASE_TYPES: [&str; 2] = ["Armor", "Weapon"];

/// Check the base chosen for `item_id` (a generic magic weapon or armor needs one, and it must
/// be one of its allowed bases).
///
/// # Errors
/// `requirement_failed` when a needed base is missing or not allowed.
pub fn check_base(uni: &Universe, item_id: &str, base_id: Option<&str>) -> ApiResult<()> {
    let Some(item) = uni.get(item_id) else {
        return Ok(());
    };
    match base_id {
        None if needs_base(uni, item) => Err(ApiError::requirement(format!(
            "Choose the base item for {} first.",
            item.el.name
        ))),
        None => Ok(()),
        Some(b) => match uni.get(b) {
            Some(base) if can_adorn(item, base) => Ok(()),
            Some(base) => Err(ApiError::requirement(format!(
                "{} cannot be made into {}.",
                base.el.name, item.el.name
            ))),
            None => Err(ApiError::not_found("That base item does not exist.")),
        },
    }
}

/// The base items of `item` (a generic magic weapon or armor) that `admits`, by name then id.
pub fn bases_by_name<'u>(
    uni: &'u Universe,
    item: &UElem,
    admits: impl Fn(&UElem) -> bool,
) -> Vec<&'u UElem> {
    let mut bases: Vec<&UElem> = BASE_TYPES
        .iter()
        .flat_map(|ty| uni.by_type(ty))
        .filter(|b| admits(b) && can_adorn(item, b))
        .collect();
    bases.sort_by(|a, b| {
        a.el.name
            .cmp(&b.el.name)
            .then_with(|| a.el.id.cmp(&b.el.id))
    });
    bases
}

impl AppState {
    /// The base items `item_id` can be added on, by name; empty when it needs no base (a plain
    /// item, or a magic item that names its one base).
    ///
    /// # Errors
    /// `not_found` when the character is not open.
    pub fn list_base_items(
        &self,
        character_id: &str,
        item_id: &str,
    ) -> ApiResult<Vec<ItemSummary>> {
        let scope = self.item_scope(Some(character_id))?;
        let hub = self.hub()?;
        let uni = hub.engine.universe(scope.universe_ruleset());
        let Some(item) = uni.get(item_id).filter(|e| needs_base(uni, e)) else {
            return Ok(Vec::new());
        };
        Ok(bases_by_name(uni, item, |b| scope.admits(&b.el))
            .into_iter()
            .filter_map(|b| hub.store.element(&b.el.id))
            .map(|el| item_row(&hub.store, el))
            .collect())
    }
}

#[cfg(test)]
#[path = "base_items_tests.rs"]
mod tests;
