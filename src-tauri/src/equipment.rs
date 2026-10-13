//! Inventory, equipment, attunement and currency services.

use crate::builder::apply;
use crate::dto::{CharacterSnapshot, ElementSummary, Page};
use crate::dto_ext::{EquipRequest, ItemFilter, ItemPatch, ItemSummary};
use crate::elements::summary;
use crate::error::{ApiError, ApiResult};
use crate::gear_info::{category, cost_cp, is_item, requires_attunement, weight_lb};
use crate::session::Draft;
use crate::state::AppState;
use borealis_engine::commands;
use borealis_engine::money::Coin;
use borealis_engine::state::{CharacterState, Coins, EquipSlot};

const DEFAULT_PAGE: usize = 100;
const MAX_PAGE: usize = 1000;
const MAX_QUANTITY: u32 = 9999;
const MAX_COIN_DELTA: i64 = 1_000_000_000;
const MAX_TEXT_CHARS: usize = 2000;

/// Parse a contract coin key.
///
/// # Errors
/// `invalid_argument` for anything but `cp sp ep gp pp`.
pub fn coin_of(key: &str) -> ApiResult<Coin> {
    match key {
        "cp" => Ok(Coin::Cp),
        "sp" => Ok(Coin::Sp),
        "ep" => Ok(Coin::Ep),
        "gp" => Ok(Coin::Gp),
        "pp" => Ok(Coin::Pp),
        _ => Err(ApiError::invalid("Unknown coin.")),
    }
}

/// Re-express all money with `to` as the highest denomination used (electrum only when asked for).
#[must_use]
pub fn convert_coins(coins: &Coins, to: Coin) -> Coins {
    let mut rest = coins.total_cp();
    let mut out = Coins::default();
    for coin in Coin::ALL.iter().rev().filter(|c| c.rate() <= to.rate()) {
        if *coin == Coin::Ep && to != Coin::Ep {
            continue;
        }
        let n = rest / coin.rate();
        rest -= n * coin.rate();
        out = out.deposit(*coin, n);
    }
    out
}

fn line_index(state: &CharacterState, uid: &str) -> ApiResult<usize> {
    state
        .inventory
        .items
        .iter()
        .position(|i| i.uid == uid)
        .ok_or_else(|| ApiError::not_found("That item is not in the inventory."))
}

fn check_text(text: &str, what: &str) -> ApiResult<()> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::invalid(format!(
            "{what} is longer than {MAX_TEXT_CHARS} characters."
        )));
    }
    Ok(())
}

fn requested_slot(
    el: Option<&borealis_content::Element>,
    request: EquipRequest,
) -> Option<EquipSlot> {
    match request {
        EquipRequest::Auto => None,
        EquipRequest::Primary | EquipRequest::TwoHanded | EquipRequest::Versatile => {
            Some(EquipSlot::PrimaryHand)
        }
        EquipRequest::Secondary => Some(EquipSlot::SecondaryHand),
        EquipRequest::Worn => Some(match el {
            Some(e)
                if e.ty == "Armor"
                    && e.setter_value("armor").map(str::to_lowercase) == Some("shield".into()) =>
            {
                EquipSlot::SecondaryHand
            }
            Some(e) if e.ty == "Armor" => EquipSlot::Armor,
            _ => EquipSlot::Worn,
        }),
    }
}

impl AppState {
    /// Search the item catalogue (name ranking: prefix before substring), paged.
    ///
    /// # Errors
    /// `not_found` when `character_id` is given but not open.
    pub fn search_items(&self, filter: &ItemFilter) -> ApiResult<Page<ItemSummary>> {
        let (ruleset, restricted) = match &filter.character_id {
            Some(id) => self.read(id, |_, s| {
                Ok((
                    Some(s.character.ruleset()),
                    s.character.build.restricted_sources.clone(),
                ))
            })?,
            None => (None, Vec::new()),
        };
        let hub = self.hub()?;
        let needle = filter
            .query
            .as_deref()
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let wanted_category = filter.category.as_deref().map(str::to_lowercase);
        let mut hits: Vec<(u8, &borealis_content::Element)> = hub
            .store
            .iter()
            .map(|s| &s.element)
            .filter(|e| is_item(e))
            .filter(|e| ruleset.map_or(true, |r| e.ruleset.map_or(true, |x| x.compatible_with(r))))
            .filter(|e| !restricted.iter().any(|r| r == &e.source))
            .filter(|e| filter.source.as_ref().map_or(true, |s| s == &e.source))
            .filter(|e| {
                wanted_category
                    .as_ref()
                    .map_or(true, |c| category(e).to_lowercase() == *c)
            })
            .filter_map(|e| match &needle {
                None => Some((0, e)),
                Some(n) => name_rank(&e.name, n).map(|r| (r, e)),
            })
            .collect();
        hits.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.name.cmp(&b.1.name))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        let total = hits.len();
        let take = filter.limit.unwrap_or(DEFAULT_PAGE).min(MAX_PAGE);
        let items = hits
            .into_iter()
            .skip(filter.offset.unwrap_or(0))
            .take(take)
            .map(|(_, e)| item_row(&hub.store, e))
            .collect();
        Ok(Page { total, items })
    }

    /// Add an item, optionally paying for it (make-change applies). `base_item_id` layers a magic
    /// item on a base item.
    ///
    /// # Errors
    /// `invalid_argument` for a bad quantity, `requirement_failed` for unknown items or too little money.
    pub fn add_item(
        &self,
        id: &str,
        item_id: &str,
        quantity: u32,
        buy: bool,
        base_item_id: Option<&str>,
    ) -> ApiResult<CharacterSnapshot> {
        if !(1..=MAX_QUANTITY).contains(&quantity) {
            return Err(ApiError::invalid("Quantity must be between 1 and 9999."));
        }
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let store = &ctx.hub.store;
            let el = store
                .element(item_id)
                .filter(|e| is_item(e))
                .ok_or_else(|| ApiError::not_found("That item does not exist."))?;
            let (base, adorner) = match base_item_id {
                Some(b) => (b, Some(item_id)),
                None => (item_id, None),
            };
            if buy {
                let total = cost_cp(el).saturating_mul(u64::from(quantity));
                let state = commands::withdraw(&d.character.build, Coin::Cp, total)
                    .map_err(|_| ApiError::requirement("Not enough money to buy that."))?;
                d.character.build = state.state;
            }
            if adorner.is_none() && stack_onto_existing(&mut d.character.build, base, quantity) {
                return Ok(());
            }
            let out = commands::add_item(engine, &d.character.build, base, quantity)?;
            apply(d, out);
            if let Some(a) = adorner {
                let uid = d
                    .character
                    .build
                    .inventory
                    .items
                    .last()
                    .map(|i| i.uid.clone())
                    .ok_or_else(|| ApiError::internal("item was not added"))?;
                let out = commands::set_adorner(engine, &d.character.build, &uid, Some(a))?;
                apply(d, out);
            }
            Ok(())
        })
    }

    /// Remove one piece of a stack, or the whole line.
    ///
    /// # Errors
    /// `not_found` for an unknown line.
    pub fn remove_item(&self, id: &str, uid: &str, all: bool) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let i = line_index(&d.character.build, uid)?;
            if !all && d.character.build.inventory.items[i].amount > 1 {
                d.character.build.inventory.items[i].amount -= 1;
                return Ok(());
            }
            let out = commands::remove_item(&ctx.hub.engine, &d.character.build, uid)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Equip an item; attunement is taken when the limit allows, else a warning is returned.
    ///
    /// # Errors
    /// `not_found`, or `requirement_failed` for items that cannot be equipped.
    pub fn equip_item(
        &self,
        id: &str,
        uid: &str,
        request: EquipRequest,
    ) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let i = line_index(&d.character.build, uid)?;
            let el = ctx
                .hub
                .store
                .element(&d.character.build.inventory.items[i].item);
            if request == EquipRequest::Versatile
                && !el.is_some_and(|e| {
                    e.setter_value("versatile")
                        .is_some_and(|v| !v.trim().is_empty())
                })
            {
                return Err(ApiError::requirement("That weapon is not versatile."));
            }
            let slot = requested_slot(el, request);
            let out = commands::equip(&ctx.hub.engine, &d.character.build, uid, slot)?;
            apply(d, out);
            if request == EquipRequest::Versatile {
                hold_with_both_hands(d, uid);
            }
            Ok(())
        })
    }

    /// Unequip an item (ends attunement).
    ///
    /// # Errors
    /// `not_found` for an unknown line.
    pub fn unequip_item(&self, id: &str, uid: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let out = commands::unequip(&ctx.hub.engine, &d.character.build, uid)?;
            apply(d, out);
            Ok(())
        })
    }

    /// Attune to or end attunement with an item (limit enforced).
    ///
    /// # Errors
    /// `requirement_failed` when the item is not equipped, needs no attunement or the limit is reached.
    pub fn set_attuned(&self, id: &str, uid: &str, attuned: bool) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let out = if attuned {
                commands::attune(engine, &d.character.build, uid)?
            } else {
                commands::unattune(engine, &d.character.build, uid)?
            };
            apply(d, out);
            Ok(())
        })
    }

    /// Move an item to storage 1 or 2, or back to being carried (0).
    ///
    /// # Errors
    /// `invalid_argument` for another container, `not_found` for an unknown line.
    pub fn store_item(&self, id: &str, uid: &str, storage: u8) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |_, d| {
            let name = match storage {
                0 => None,
                1 | 2 => d
                    .character
                    .build
                    .inventory
                    .storages
                    .get(usize::from(storage) - 1)
                    .cloned(),
                _ => return Err(ApiError::invalid("Choose carried, storage 1 or storage 2.")),
            };
            if storage != 0 && name.is_none() {
                return Err(ApiError::invalid("That storage does not exist."));
            }
            let out = commands::move_to_storage(&d.character.build, uid, name.as_deref())?;
            apply(d, out);
            Ok(())
        })
    }

    /// Move a line one place up or down.
    ///
    /// # Errors
    /// `not_found` for an unknown line, `invalid_argument` for a bad direction.
    pub fn move_item(&self, id: &str, uid: &str, direction: &str) -> ApiResult<CharacterSnapshot> {
        let step: isize = match direction {
            "up" => -1,
            "down" => 1,
            _ => return Err(ApiError::invalid("Direction must be up or down.")),
        };
        self.mutate(id, |_, d| {
            let items = &mut d.character.build.inventory.items;
            let i = items
                .iter()
                .position(|x| x.uid == uid)
                .ok_or_else(|| ApiError::not_found("That item is not in the inventory."))?;
            if let Some(j) = i.checked_add_signed(step).filter(|j| *j < items.len()) {
                items.swap(i, j);
            }
            Ok(())
        })
    }

    /// Edit the display name, notes and visibility of a line.
    ///
    /// # Errors
    /// `not_found` for an unknown line, `invalid_argument` for overlong text.
    pub fn update_item_details(
        &self,
        id: &str,
        uid: &str,
        patch: &ItemPatch,
    ) -> ApiResult<CharacterSnapshot> {
        if let Some(n) = &patch.display_name {
            check_text(n, "The name")?;
        }
        if let Some(n) = &patch.notes {
            check_text(n, "The notes")?;
        }
        self.mutate(id, |_, d| {
            let i = line_index(&d.character.build, uid)?;
            let item = &mut d.character.build.inventory.items[i];
            if let Some(n) = &patch.display_name {
                let n = n.trim();
                item.alt_name = (!n.is_empty()).then(|| n.to_string());
            }
            if let Some(n) = &patch.notes {
                item.notes = (!n.trim().is_empty()).then(|| n.clone());
            }
            if let Some(show) = patch.list_in_inventory {
                item.hidden = !show;
            }
            if let Some(card) = patch.show_card {
                item.card = card;
            }
            Ok(())
        })
    }

    /// Open an equipment pack into its contents.
    ///
    /// # Errors
    /// `invalid_argument` when the line is not a pack.
    pub fn extract_pack(&self, id: &str, uid: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let engine = &ctx.hub.engine;
            let i = line_index(&d.character.build, uid)?;
            let line = d.character.build.inventory.items[i].clone();
            let contents = ctx
                .hub
                .store
                .element(&line.item)
                .and_then(|e| e.extract.as_ref())
                .filter(|x| !x.items.is_empty())
                .ok_or_else(|| ApiError::invalid("That item is not an equipment pack."))?;
            let out = commands::remove_item(engine, &d.character.build, uid)?;
            apply(d, out);
            for _ in 0..line.amount {
                for part in &contents.items {
                    let amount = part.amount.unwrap_or(1);
                    if !stack_onto_existing(&mut d.character.build, &part.id, amount) {
                        let out = commands::add_item(engine, &d.character.build, &part.id, amount)?;
                        apply(d, out);
                    }
                }
            }
            Ok(())
        })
    }

    /// Add (`delta` > 0) or remove (`delta` < 0, with make-change) coins.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown coin or absurd amount, `requirement_failed` when the
    /// total value is insufficient.
    pub fn adjust_coins(&self, id: &str, coin: &str, delta: i64) -> ApiResult<CharacterSnapshot> {
        let coin = coin_of(coin)?;
        if delta.abs() > MAX_COIN_DELTA {
            return Err(ApiError::invalid("That amount of coins is too large."));
        }
        let n = delta.unsigned_abs();
        self.mutate(id, |_, d| {
            if delta >= 0 {
                d.character.build = commands::deposit(&d.character.build, coin, n);
                return Ok(());
            }
            let out = commands::withdraw(&d.character.build, coin, n)
                .map_err(|_| ApiError::requirement("Not enough coins."))?;
            apply(d, out);
            Ok(())
        })
    }

    /// Re-express all coins with `to` as the highest denomination.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown coin.
    pub fn convert_coins(&self, id: &str, to: &str) -> ApiResult<CharacterSnapshot> {
        let to = coin_of(to)?;
        self.mutate(id, |_, d| {
            let inv = &mut d.character.build.inventory;
            inv.coins = convert_coins(&inv.coins, to);
            Ok(())
        })
    }

    /// Rename the two storage containers; stored items follow their container.
    ///
    /// # Errors
    /// `invalid_argument` for empty, overlong or identical names.
    pub fn set_storage_names(&self, id: &str, names: [String; 2]) -> ApiResult<CharacterSnapshot> {
        let names = names.map(|n| n.trim().to_string());
        if names.iter().any(|n| n.is_empty() || n.chars().count() > 60) {
            return Err(ApiError::invalid(
                "Storage names must be 1 to 60 characters.",
            ));
        }
        if names[0].eq_ignore_ascii_case(&names[1]) {
            return Err(ApiError::invalid("The two storages need different names."));
        }
        self.mutate(id, |_, d| {
            let inv = &mut d.character.build.inventory;
            let old = inv.storages.clone();
            for item in &mut inv.items {
                let slot = item
                    .storage
                    .as_ref()
                    .and_then(|s| old.iter().position(|o| o == s));
                item.storage = slot.and_then(|p| names.get(p).cloned());
            }
            inv.storages = names.to_vec();
            Ok(())
        })
    }
}

/// Rank a name against a lowercase needle: 0 exact, 1 prefix, 2 word prefix, 3 substring.
fn name_rank(name: &str, needle: &str) -> Option<u8> {
    let n = name.to_lowercase();
    if n == needle {
        Some(0)
    } else if n.starts_with(needle) {
        Some(1)
    } else if n.split_whitespace().any(|w| w.starts_with(needle)) {
        Some(2)
    } else if n.contains(needle) {
        Some(3)
    } else {
        None
    }
}

fn item_row(store: &borealis_content::ContentStore, e: &borealis_content::Element) -> ItemSummary {
    let row: ElementSummary = summary(store, e);
    ItemSummary {
        summary: row,
        category: category(e),
        cost_cp: cost_cp(e),
        weight: weight_lb(e),
        requires_attunement: requires_attunement(e),
    }
}

/// Add to an identical plain stack instead of creating a line. Returns whether it stacked.
fn stack_onto_existing(state: &mut CharacterState, item: &str, quantity: u32) -> bool {
    let line = state.inventory.items.iter_mut().find(|i| {
        i.item == item
            && i.adorner.is_none()
            && i.equipped.is_none()
            && i.storage.is_none()
            && i.alt_name.is_none()
    });
    match line {
        Some(l) => {
            l.amount = l.amount.saturating_add(quantity).min(MAX_QUANTITY);
            true
        }
        None => false,
    }
}

/// Hold a versatile weapon in both hands: the off hand is freed.
fn hold_with_both_hands(d: &mut Draft, uid: &str) {
    for it in &mut d.character.build.inventory.items {
        if it.uid == uid {
            it.versatile = true;
        } else if it.equipped == Some(EquipSlot::SecondaryHand) {
            it.equipped = None;
            it.attuned = false;
        }
    }
}
