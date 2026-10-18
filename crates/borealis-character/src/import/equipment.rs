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

/// Where Aurora's loader puts an item saved as equipped, from `equipped/@location`
/// (`RefactoredEquipmentSectionViewModel` collection handler: an exact `switch` on the text).
/// `None` = a location Aurora does not restore: the item stays unequipped there too.
/// The second value is "held versatile in both hands".
fn slot_of(ctx: &Ctx<'_>, item_id: &str, location: Option<&str>) -> Option<(EquipSlot, bool)> {
    let location = location.unwrap_or("").trim();
    match location {
        "One-Hand" | "Primary" | "Primary Hand" | "mainhand" => {
            Some((EquipSlot::PrimaryHand, false))
        }
        "Secondary" | "Secondary Hand" | "offhand" => Some((EquipSlot::SecondaryHand, false)),
        "Two-Handed (Versatile)" => Some((EquipSlot::PrimaryHand, true)),
        "" | "body" | "Armor" | "Two-Handed" => Some((slot_by_item(ctx, item_id), false)),
        _ => None,
    }
}

/// Aurora's plain "equip" of an item (`EquipEquipmentItem`), decided by the item's `slot`
/// setter (of the base item an item implies): armor/body -> armor, onehand + secondary ->
/// off hand, twohand or onehand -> main hand (a two-handed weapon then fills both hands),
/// primary / secondary -> that hand, anything else is worn.
fn slot_by_item(ctx: &Ctx<'_>, item_id: &str) -> EquipSlot {
    let Some(e) = ctx.uni.get(item_id) else {
        return EquipSlot::Worn;
    };
    let base = borealis_engine::item_base::implied_base(ctx.uni, e).unwrap_or(e);
    let slots: Vec<String> = base
        .el
        .setter_value("slot")
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();
    let has = |s: &str| slots.iter().any(|x| x == s);
    if has("armor") || has("body") {
        EquipSlot::Armor
    } else if has("onehand") && has("secondary") {
        EquipSlot::SecondaryHand
    } else if has("twohand") || has("onehand") || has("primary") {
        EquipSlot::PrimaryHand
    } else if has("secondary") {
        EquipSlot::SecondaryHand
    } else {
        EquipSlot::Worn
    }
}

/// Aurora's attacks section is a list the user fills; it keeps (and recomputes) the line of a
/// listed weapon that is not equipped. Keep the inventory uids of its item-linked lines.
fn listed_attacks(character: &Character, ctx: &Ctx<'_>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for uid in character
        .details
        .attacks
        .iter()
        .filter_map(|a| a.item_uid.as_deref())
    {
        if ctx.state.inventory.items.iter().any(|i| i.uid == uid) && !out.iter().any(|u| u == uid) {
            out.push(uid.to_string());
        }
    }
    out
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
    for (n, node) in eq.elements().filter(|e| e.name == "item").enumerate() {
        let raw_id = node.attr("id").unwrap_or("");
        let id = resolve_item_id(ctx, raw_id);
        if ctx.uni.get(&id).is_none() && !ctx.report.missing_content.iter().any(|m| m.id == id) {
            ctx.report.missing_content.push(MissingContent {
                id: id.clone(),
                ty: "Item".into(),
                name: node.attr("name").unwrap_or("").to_string(),
                // an Aurora proxy ("Additional Feat, X") whose target is not installed
                context: if borealis_engine::proxy::is_proxy_id(&id) {
                    "proxy item".into()
                } else {
                    "inventory".into()
                },
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
            match slot_of(ctx, &id, e.attr("location")) {
                Some((slot, versatile)) => {
                    it.equipped = Some(slot);
                    it.versatile = versatile;
                }
                None => ctx.report.warnings.push(format!(
                    "{} is saved as equipped at \"{}\", a location Aurora does not restore; \
                     imported unequipped, as Aurora loads it",
                    node.attr("name").unwrap_or(&id),
                    e.attr("location").unwrap_or("")
                )),
            }
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
        if let Some(a) = it.adorner.clone() {
            map.by_id.entry(a).or_default().push(uid.clone());
        }
        map.by_id.entry(id).or_default().push(uid);
        ctx.state.inventory.items.push(it);
    }
    ctx.state.inventory.listed_attacks = listed_attacks(character, ctx);
    map
}
