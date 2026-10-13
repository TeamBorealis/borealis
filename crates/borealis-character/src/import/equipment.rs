//! The `build/equipment` inventory.

use super::sections::{norm, text_of};
use super::{Ctx, MissingContent};
use crate::model::Character;
use borealis_engine::state::{EquipSlot, InventoryItem};
use borealis_migrate::dom::XmlElement;
use std::cell::RefCell;
use std::collections::HashMap;

/// Hands out inventory uids in file order for item nodes that carry rules.
#[derive(Default)]
pub(super) struct ItemMap {
    by_id: HashMap<String, Vec<String>>,
    used: RefCell<HashMap<String, usize>>,
}

impl ItemMap {
    pub(super) fn next_uid(&self, item_id: &str) -> Option<String> {
        let uids = self.by_id.get(item_id)?;
        let mut used = self.used.borrow_mut();
        let n = used.entry(item_id.to_string()).or_insert(0);
        let uid = uids.get(*n).cloned();
        *n += 1;
        uid
    }
}

fn truthy(el: Option<&XmlElement>) -> bool {
    el.is_some_and(|e| e.text().trim().eq_ignore_ascii_case("true"))
}

fn resolve_item_id(ctx: &Ctx<'_>, id: &str) -> String {
    if ctx.uni.get(id).is_some() {
        return id.to_string();
    }
    for (from, to) in [
        ("ID_WOTC_ITEM", "ID_WOTC_PHB_ITEM"),
        ("ID_WOTC_WEAPON", "ID_WOTC_PHB_WEAPON"),
    ] {
        if id.contains(from) {
            let alt = id.replacen(from, to, 1);
            if ctx.uni.get(&alt).is_some() {
                return alt;
            }
        }
    }
    id.to_string()
}

fn slot_of(equipped: &XmlElement) -> EquipSlot {
    let loc = equipped.attr("location").unwrap_or("").to_lowercase();
    if loc.contains("armor") {
        EquipSlot::Armor
    } else if loc.contains("secondary") {
        EquipSlot::SecondaryHand
    } else if loc.contains("primary") {
        EquipSlot::PrimaryHand
    } else {
        EquipSlot::Worn
    }
}

pub(super) fn read_equipment(
    build: &XmlElement,
    character: &mut Character,
    ctx: &mut Ctx<'_>,
) -> ItemMap {
    let mut map = ItemMap::default();
    let Some(eq) = build.child("equipment") else {
        return map;
    };
    ctx.state.inventory.storages = eq
        .elements()
        .filter(|e| e.name == "storage")
        .filter_map(|s| s.attr("name"))
        .map(String::from)
        .collect();
    let _ = character;
    for (n, node) in eq.elements().filter(|e| e.name == "item").enumerate() {
        let raw_id = node.attr("id").unwrap_or("");
        let id = resolve_item_id(ctx, raw_id);
        if ctx.uni.get(&id).is_none() && !ctx.report.missing_content.iter().any(|m| m.id == id) {
            ctx.report.missing_content.push(MissingContent {
                id: id.clone(),
                ty: "Item".into(),
                name: node.attr("name").unwrap_or("").to_string(),
                context: "inventory".into(),
            });
        }
        let uid = node
            .attr("identifier")
            .filter(|u| !u.is_empty())
            .map_or_else(|| format!("item-{}", n + 1), str::to_lowercase);
        let mut it = InventoryItem::new(uid.clone(), id.clone());
        it.amount = node
            .attr("amount")
            .and_then(|a| a.trim().parse().ok())
            .unwrap_or(1);
        it.hidden = node
            .attr("hidden")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        it.sidebar = node
            .attr("sidebar")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        it.acquired_from = node
            .attr("aquired")
            .filter(|v| !v.is_empty())
            .map(String::from);
        if let Some(adorner) = node
            .child("items")
            .and_then(|i| i.child("adorner"))
            .and_then(|a| a.attr("id"))
            .filter(|a| !a.is_empty())
        {
            let a = resolve_item_id(ctx, adorner);
            if ctx.uni.get(&a).is_some() {
                it.adorner = Some(a);
            } else {
                ctx.report.missing_content.push(MissingContent {
                    id: a,
                    ty: "Magic Item".into(),
                    name: String::new(),
                    context: "adorner".into(),
                });
            }
        }
        if let Some(e) = node
            .child("equipped")
            .filter(|e| e.text().trim().eq_ignore_ascii_case("true"))
        {
            it.equipped = Some(slot_of(e));
            it.versatile = e
                .attr("versatile")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"))
                || e.attr("location")
                    .is_some_and(|l| l.to_lowercase().contains("versatile"));
        }
        it.attuned = truthy(node.child("attunement"));
        if let Some(d) = node.child("details") {
            it.card = d
                .attr("card")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"));
            let alt = text_of(d, "name");
            it.alt_name = (!alt.is_empty()).then_some(alt);
            let notes = norm(&text_of(d, "notes"));
            it.notes = (!notes.is_empty()).then_some(notes);
        }
        it.storage = node
            .child("storage")
            .map(|s| text_of(s, "location"))
            .filter(|l| !l.is_empty());
        map.by_id.entry(id).or_default().push(uid);
        ctx.state.inventory.items.push(it);
    }
    map
}
