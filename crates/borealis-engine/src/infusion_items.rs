//! The magic item an infusion pick makes, kept in the inventory with the pick.
//!
//! Opt-in ([`crate::EngineSettings::infusion_items`], off by default): with the setting off none of
//! this runs, and a character with infusion picks loads, replays and saves as it always did.
//!
//! A picked feature (a tinker's infusion, a Class Feature chosen in a select) makes a magic
//! item, found in the content, in this order:
//!
//! 1. a Magic Item of the same name that marks itself as an infusion (its `keywords` or `rarity`
//!    name one): "Striding Boots", or "Glowing Weapon" on a chosen weapon;
//! 2. else, when the pick's current sheet text says it makes an item "into a +N weapon" (or armor,
//!    or shield), the content's generic `+N` item of that kind (`{{parent}} +{{enhancement}}`, no
//!    attunement, needs a base): `Weapon, +2` from the level the text says +2;
//! 3. else nothing (the pick is not one that makes an item).
//!
//! Each pick has a stable link id ([`link_of`]: its slot and element), stored on the one
//! inventory line it owns ([`InventoryItem::infusion_link`]). [`reconcile`] keeps them together
//! after every change and when a character loads (an updated app, an import):
//!
//! - on load ([`Mode::Load`]: an updated app, an import, a sync) a pick without a line first adopts
//!   an unlinked line of its item (added by hand, as Aurora users do); otherwise, and always
//!   after a change ([`Mode::Change`]: a new pick), it gets one new copy of its own, even when the
//!   character already owns one, never merged into another line; an item that needs a
//!   base and has none chosen adds nothing and is listed in
//!   [`InventorySnapshot::infusion_prompts`] ("Choose the item for ..."), picked through the
//!   legal-base list;
//! - a line the user chose for a pick, or added by hand (adopted, [`InventoryItem::infusion_adopted`]),
//!   is only unlinked when the pick goes, kept as it is and never upgraded;
//! - a line the pick added, whose pick is no longer selected, loses one copy (that line); a pick whose content is
//!   missing keeps its line;
//! - a `+N` line follows the pick's level (`+1` becomes `+2`);
//! - a line the user deletes stays deleted: its link is kept in
//!   [`crate::state::Inventory::infusion_declined`] until the pick goes, and no copy is added again.
//!
//! Nothing is read for a character with no `item infusions:max` and no linked line.
//!
//! Loading twice changes nothing more: lines are matched by link id, never by name.
//!
//! Parts: a live feature whose text says an item "counts as separate items" for infusions,
//! followed by a `:` list (`armor, boots, helmet, and the armor's special weapon`), splits that
//! item into parts ([`parts`]). Each part holds one infusion, and only one that fits it
//! ([`legal_parts`]): a part named a weapon takes a feature's derived weapon, a part named armor
//! takes body armor (an armor infusion is layered on the special armor line,
//! [`InventoryItem::infusion`]), another part takes a worn magic item with no base whose slot
//! fits it. What that feature adds to `item infusions:max` ([`MAX_STAT`]) is usable only on
//! parts. A new infusion item takes a free part that fits it.

use crate::commands::{self};
use crate::diag::{code, Diagnostic, Severity};
use crate::engine::Engine;
use crate::item_base::{can_adorn, needs_base};
use crate::snapshot::{CharacterSnapshot, InfusionPrompt, InfusionTarget, InfusionsSnapshot};
use crate::state::{CharacterState, Choice, EquipSlot, InventoryItem, Selection};
use crate::universe::{UElem, Universe};
use std::collections::HashSet;

/// The statistic holding the most infusion items at once.
pub const MAX_STAT: &str = "item infusions:max";

/// The infusion layered on a line, when it is installed.
#[must_use]
pub fn layer_of<'u>(uni: &'u Universe, it: &InventoryItem) -> Option<&'u UElem> {
    it.infusion.as_deref().and_then(|i| uni.get(i))
}

/// The line is (or carries) the item of pick `link`.
fn linked_to(it: &InventoryItem, link: &str) -> bool {
    it.infusion_link.as_deref() == Some(link) || it.infusion_layer_link.as_deref() == Some(link)
}

/// The line is (or carries) an infusion pick's item.
fn linked(it: &InventoryItem) -> bool {
    it.infusion_link.is_some() || it.infusion_layer_link.is_some()
}

/// The parts one item splits into for infusions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    /// Part names, as the feature lists them.
    pub labels: Vec<String>,
    /// The feature's name.
    pub from: String,
    /// What the feature adds to [`MAX_STAT`] (usable only on parts).
    pub bonus: i32,
}

/// The parts of the live feature that splits an item for infusions.
#[must_use]
pub fn parts(snap: &CharacterSnapshot) -> Option<Parts> {
    snap.features.iter().find_map(|f| {
        let text = crate::feature_weapon::plain(
            f.sheet_summary.as_deref().unwrap_or(f.description.as_str()),
        );
        let labels = crate::feature_weapon::parts_in(&text);
        (labels.len() > 1).then(|| Parts {
            labels,
            from: f.name.clone(),
            bonus: snap.stats.get(MAX_STAT).map_or(0, |s| {
                s.sources
                    .iter()
                    .filter(|(l, _)| *l == f.name)
                    .map(|(_, v)| *v)
                    .sum()
            }),
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartKind {
    Weapon,
    Armor,
    Other,
}

fn part_kind(label: &str) -> PartKind {
    let l = label.to_lowercase();
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).collect();
    if words.iter().any(|w| w.starts_with("weapon")) {
        PartKind::Weapon
    } else if words.iter().any(|w| *w == "armor" || *w == "armour") {
        PartKind::Armor
    } else {
        PartKind::Other
    }
}

/// The words of a name or a part's label, lowercase and singular ("Boots" gives `boot`).
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !["the", "and", "for"].contains(w))
        .map(|w| match w.strip_suffix('s') {
            Some(one) if w.len() > 3 => one.to_string(),
            _ => w.to_string(),
        })
        .collect()
}

/// One word names the other, or a longer form of it (`helm`, `helmet`).
fn same_word(a: &str, b: &str) -> bool {
    a == b || (a.len().min(b.len()) >= 4 && (a.starts_with(b) || b.starts_with(a)))
}

/// `name` has a word of `label` (as [`words`]).
fn names_part(name: &str, label: &[String]) -> bool {
    words(name)
        .iter()
        .any(|n| label.iter().any(|l| same_word(n, l)))
}

/// A slot that names no body part.
fn generic_slot(slot: &str) -> bool {
    slot.is_empty() || slot == "misc" || slot == "wondrous"
}

/// A worn item fits a part: by the item's slot, or, when the content gives no body slot (`misc`,
/// none), by its own name ("... Boots ..."); a slot for another body part never fits. The body
/// slots of a part are read from the content: those of the magic items named for it (`feet` for
/// "boots", `head` for "helmet"). A part no item is named for takes any worn item.
fn slot_fits(uni: &Universe, e: &UElem, label: &str) -> bool {
    let label = words(label);
    let slot = lower(e.el.setter_value("slot")).trim().to_string();
    let mut body: Vec<String> = uni
        .by_type("Magic Item")
        .filter(|m| names_part(&m.el.name, &label))
        .map(|m| lower(m.el.setter_value("slot")).trim().to_string())
        .filter(|s| !generic_slot(s))
        .collect();
    body.sort();
    body.dedup();
    if body.is_empty() || body.contains(&slot) {
        return true;
    }
    generic_slot(&slot) && names_part(&e.el.name, &label)
}

/// Does line `it` fit part `label`?
fn fits(uni: &Universe, it: &InventoryItem, label: &str) -> bool {
    let Some((base, adorner)) = crate::item_base::resolve(uni, it) else {
        return false;
    };
    match part_kind(label) {
        PartKind::Weapon => crate::feature_weapon::source_feature(&base.el).is_some(),
        // the special armor: body armor a nonmagical magic item turned into it, or carrying a layer
        PartKind::Armor => {
            crate::inventory::item_facts(base).body_armor
                && (it.infusion.is_some()
                    || adorner.is_some_and(|a| !crate::item_base::is_magic(&a.el)))
        }
        PartKind::Other => {
            adorner.is_none()
                && it.infusion.is_none()
                && base.el.ty == "Magic Item"
                && !base.el.setter_value("type").is_some_and(|t| {
                    ["weapon", "armor"].contains(&t.trim().to_lowercase().as_str())
                })
                && slot_fits(uni, base, label)
        }
    }
}

/// The free parts line `uid` may sit on (its own part included).
#[must_use]
pub fn legal_parts(
    uni: &Universe,
    state: &CharacterState,
    parts: &Parts,
    uid: &str,
) -> Vec<String> {
    let Some(it) = state.inventory.items.iter().find(|i| i.uid == uid) else {
        return Vec::new();
    };
    parts
        .labels
        .iter()
        .filter(|p| {
            !state
                .inventory
                .items
                .iter()
                .any(|o| o.uid != uid && o.part.as_deref() == Some(p.as_str()))
        })
        .filter(|p| fits(uni, it, p))
        .cloned()
        .collect()
}

/// Lines an armor infusion of `items` can be layered on: special armor (body armor adorned by a
/// nonmagical magic item) on a free armor part.
fn targets(
    uni: &Universe,
    state: &CharacterState,
    parts: &Parts,
    items: &[String],
) -> Vec<InfusionTarget> {
    state
        .inventory
        .items
        .iter()
        .filter(|it| it.infusion.is_none() && !linked(it))
        .filter_map(|it| {
            let base = uni.get(&it.item)?;
            let adorner = uni.get(it.adorner.as_deref()?)?;
            if crate::item_base::is_magic(&adorner.el)
                || !items
                    .iter()
                    .any(|c| uni.get(c).is_some_and(|c| can_adorn(c, base)))
            {
                return None;
            }
            let part = legal_parts(uni, state, parts, &it.uid)
                .into_iter()
                .find(|p| part_kind(p) == PartKind::Armor)?;
            Some(InfusionTarget {
                uid: it.uid.clone(),
                name: crate::item_name::adorned_name(base, adorner),
                part,
            })
        })
        .collect()
}

/// When [`reconcile`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A character was loaded (adopt matching lines before adding).
    Load,
    /// After a change (a new pick gets a copy of its own).
    Change,
}

/// The magic items a pick makes and whether they need a base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Link id of the pick.
    pub link: String,
    /// The picked element.
    pub infusion_id: String,
    /// Its name.
    pub infusion: String,
    /// Candidate magic items (one by name; or the `+N` items of the kind, one per base kind).
    pub items: Vec<String>,
    /// The items are put on a base the user chooses.
    pub needs_base: bool,
    /// The item is the pick's own (same name, marked as an infusion), not a generic `+N` item:
    /// only such an item is adopted on load without asking (a generic `+1 Weapon` line may be
    /// the character's loot).
    pub named: bool,
}

/// The stable link id of a selection's element choice.
#[must_use]
pub fn link_of(sel: &Selection) -> Option<String> {
    let Choice::Element(id) = &sel.choice else {
        return None;
    };
    let k = &sel.slot;
    let inst = k
        .instance
        .as_deref()
        .map_or(String::new(), |i| format!("@{i}"));
    Some(format!("{}:{}:{}{inst}:{id}", k.owner, k.rule, k.ordinal))
}

fn lower(s: Option<&str>) -> String {
    s.unwrap_or("").to_lowercase()
}

/// The element names itself an infusion (`keywords` or `rarity`).
fn marks_infusion(e: &UElem) -> bool {
    lower(e.el.setter_value("keywords")).contains("infusion")
        || lower(e.el.setter_value("rarity")).contains("infusion")
}

/// Rule 1: the infusion Magic Item of the pick's name.
fn named_item<'u>(uni: &'u Universe, pick: &UElem) -> Option<&'u UElem> {
    let name = pick.el.name.trim().to_lowercase();
    uni.by_type("Magic Item")
        .filter(|m| m.el.name.trim().to_lowercase() == name && marks_infusion(m))
        .min_by_key(|m| (m.edition.is_foreign(), m.el.id.clone()))
}

/// Rule 2: every "into a +N weapon|armor|shield" in the pick's current sheet text.
fn enhancement_in(text: &str) -> Vec<(&'static str, i32)> {
    const AT: &str = "into a +";
    let t = text.to_lowercase();
    let mut out = Vec::new();
    for (i, _) in t.match_indices(AT) {
        let rest = &t[i + AT.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(n) = digits.parse::<i32>() else {
            continue;
        };
        let word = rest[digits.len()..].split_whitespace().next();
        let kind = match word.map(|w| w.trim_matches(|c: char| !c.is_alphabetic())) {
            Some("weapon") => "Weapon",
            Some("armor" | "shield") => "Armor",
            _ => continue,
        };
        if !out.contains(&(kind, n)) {
            out.push((kind, n));
        }
    }
    out
}

/// A `name-format` that only adds the enhancement to the parent's name (`{{parent}} +{{enhancement}}`,
/// `+{{enhancement}} {{parent}}`): the content's generic `+N` item, not one with a name of its own.
fn is_generic_format(format: &str) -> bool {
    let has = |a: &str, b: &str| format.contains(a) || format.contains(b);
    if !has("{{parent}}", "$(parent)") || !has("{{enhancement}}", "$(enhancement)") {
        return false;
    }
    let rest = [
        "{{parent}}",
        "$(parent)",
        "{{enhancement}}",
        "$(enhancement)",
    ]
    .iter()
    .fold(format.to_string(), |f, p| f.replace(p, ""));
    !rest.chars().any(char::is_alphanumeric)
}

/// The content's generic `+N` magic items of base type `kind`.
fn generic_items(uni: &Universe, kind: &str, n: i32) -> Vec<String> {
    let mut out: Vec<&UElem> = uni
        .by_type("Magic Item")
        .filter(|m| {
            m.el.setter_value("type")
                .is_some_and(|t| t.trim().eq_ignore_ascii_case(kind))
                && crate::ammo::enhancement_of(&m.el) == Some(n)
                && m.el
                    .setter_value("name-format")
                    .is_some_and(is_generic_format)
                && !crate::inventory::item_facts(m).requires_attunement
                && needs_base(uni, m)
        })
        .collect();
    out.sort_by_key(|m| (m.edition.is_foreign(), m.el.id.clone()));
    out.into_iter().map(|m| m.el.id.clone()).collect()
}

/// The plans of the character's live picks.
#[must_use]
pub fn plans(uni: &Universe, state: &CharacterState, snap: &CharacterSnapshot) -> Vec<Plan> {
    state
        .selections
        .iter()
        .filter_map(|sel| {
            let link = link_of(sel)?;
            let Choice::Element(id) = &sel.choice else {
                return None;
            };
            let feature = snap.features.iter().find(|f| f.id == *id)?;
            let pick = uni.get(id).filter(|e| e.el.ty != "Magic Item")?;
            if let Some(item) = named_item(uni, pick) {
                return Some(Plan {
                    link,
                    infusion_id: id.clone(),
                    infusion: pick.el.name.clone(),
                    needs_base: needs_base(uni, item),
                    named: true,
                    items: vec![item.el.id.clone()],
                });
            }
            let items: Vec<String> = enhancement_in(feature.sheet_summary.as_deref().unwrap_or(""))
                .into_iter()
                .flat_map(|(kind, n)| generic_items(uni, kind, n))
                .collect();
            (!items.is_empty()).then(|| Plan {
                link,
                infusion_id: id.clone(),
                infusion: pick.el.name.clone(),
                items,
                needs_base: true,
                named: false,
            })
        })
        .collect()
}

/// The line holds one of the plan's items (as the item, or as its adorner).
fn holds(plan: &Plan, it: &crate::state::InventoryItem) -> bool {
    match &it.adorner {
        Some(a) => plan.items.contains(a),
        None => !plan.needs_base && plan.items.contains(&it.item),
    }
}

/// Unlinked lines that already hold one of the plan's items: offered in its prompt ("use the
/// one you own"), never taken without asking.
fn owned(uni: &Universe, state: &CharacterState, plan: &Plan) -> Vec<InfusionTarget> {
    state
        .inventory
        .items
        .iter()
        .filter(|it| !linked(it) && holds(plan, it))
        .filter_map(|it| {
            let base = uni.get(&it.item)?;
            let name = match it.adorner.as_deref().and_then(|a| uni.get(a)) {
                Some(a) => crate::item_name::adorned_name(base, a),
                None => base.el.name.clone(),
            };
            Some(InfusionTarget {
                uid: it.uid.clone(),
                name: it.alt_name.clone().unwrap_or(name),
                part: String::new(),
            })
        })
        .collect()
}

/// Link the owned line `uid` (an [`InfusionPrompt::owned`] entry) to pick `link`: the user chose
/// to use that copy for the infusion.
///
/// # Errors
/// A message when the pick does not wait or the line is not one it can use.
pub fn adopt(
    engine: &Engine,
    state: &CharacterState,
    link: &str,
    uid: &str,
) -> Result<CharacterState, String> {
    let snap = engine.derive_with(state, false);
    let ok = snap
        .inventory
        .infusion_prompts
        .iter()
        .any(|p| p.link == link && p.owned.iter().any(|o| o.uid == uid));
    if !ok {
        return Err("That item cannot be used for this infusion.".into());
    }
    let mut next = state.clone();
    if let Some(it) = next.inventory.items.iter_mut().find(|i| i.uid == uid) {
        it.infusion_link = Some(link.to_string());
        it.infusion_adopted = true;
    }
    let next = take_part(engine, next, uid);
    Ok(next)
}

/// `(owner, slot rest, element)` of a link id.
fn link_parts(link: &str) -> Option<(&str, &str, &str)> {
    let (owner, rest) = link.split_once(':')?;
    let (slot, id) = rest.rsplit_once(':')?;
    Some((owner, slot, id))
}

/// A selected link that is `old` with one id renamed (same slot position, same owner or same
/// element, and the id that differs no longer in the content): a line keeps its link through a
/// content rename instead of being removed and added. Swapping the pick for another (whose old
/// id still exists) is not a rename: its line goes.
fn renamed_link<'a>(uni: &Universe, old: &str, free: &[&'a String]) -> Option<&'a String> {
    let (o_owner, o_slot, o_id) = link_parts(old)?;
    let gone = |id: &str| uni.get(id).is_none();
    let mut hits = free.iter().filter(|l| {
        link_parts(l).is_some_and(|(owner, slot, id)| {
            slot == o_slot
                && ((owner == o_owner && id != o_id && gone(o_id))
                    || (id == o_id && owner != o_owner && gone(o_owner)))
        })
    });
    let first = hits.next()?;
    hits.next().is_none().then_some(*first)
}

/// Picks whose item needs a base nobody chose yet (no linked or adoptable line).
#[must_use]
pub fn prompts(
    uni: &Universe,
    state: &CharacterState,
    snap: &CharacterSnapshot,
) -> Vec<InfusionPrompt> {
    prompts_with(uni, state, snap, parts(snap).as_ref())
}

/// [`prompts`] with the character's `parts` already read.
fn prompts_with(
    uni: &Universe,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    pts: Option<&Parts>,
) -> Vec<InfusionPrompt> {
    plans(uni, state, snap)
        .into_iter()
        .filter(|p| p.needs_base)
        .filter(|p| {
            !state
                .inventory
                .items
                .iter()
                .any(|it| linked_to(it, &p.link))
        })
        .map(|p| InfusionPrompt {
            owned: owned(uni, state, &p),
            targets: pts.map_or_else(Vec::new, |pt| targets(uni, state, pt, &p.items)),
            link: p.link,
            infusion_id: p.infusion_id,
            infusion: p.infusion,
            items: p.items,
        })
        .collect()
}

/// Equip a new line where it fits without moving anything, then attune it when it asks for it
/// and a slot is free (else it stays marked as needing attunement).
#[must_use]
pub fn place(engine: &Engine, state: &CharacterState, uid: &str) -> CharacterState {
    let uni = engine.universe(state.ruleset);
    let Some(line) = state.inventory.items.iter().find(|i| i.uid == uid) else {
        return state.clone();
    };
    let Some((base, _)) = crate::item_base::resolve(uni, line) else {
        return state.clone();
    };
    let f = crate::inventory::item_facts(base);
    let taken = |slots: &[EquipSlot]| {
        state
            .inventory
            .items
            .iter()
            .any(|i| i.equipped.is_some_and(|s| slots.contains(&s)))
    };
    let free = if f.body_armor {
        !taken(&[EquipSlot::Armor])
    } else if f.weapon || f.shield {
        !taken(&[EquipSlot::PrimaryHand, EquipSlot::SecondaryHand])
    } else {
        true
    };
    let mut next = state.clone();
    if free && crate::item_base::effective_facts(uni, base).equippable {
        if let Ok(out) = commands::equip(engine, &next, uid, None) {
            next = out.state;
        }
    }
    if let Ok(out) = commands::attune(engine, &next, uid) {
        next = out.state;
    }
    take_part(engine, next, uid)
}

/// Put a new infusion line on the first free part that fits it, when the character has parts.
#[must_use]
pub fn take_part(engine: &Engine, state: CharacterState, uid: &str) -> CharacterState {
    if !engine.settings().infusion_items {
        return state;
    }
    let uni = engine.universe(state.ruleset);
    let snap = engine.derive_with(&state, false);
    let Some(parts) = parts(&snap) else {
        return state;
    };
    let Some(part) = legal_parts(uni, &state, &parts, uid).into_iter().next() else {
        return state;
    };
    let mut next = state;
    if let Some(it) = next.inventory.items.iter_mut().find(|i| i.uid == uid) {
        if it.part.is_none() {
            it.part = Some(part);
        }
    }
    next
}

/// Layer the armor infusion `item` of pick `link` on line `uid` (an [`InfusionPrompt::targets`]
/// entry) as its part.
///
/// # Errors
/// A message when the pick does not wait for the item or the line is not one of its targets.
pub fn layer(
    engine: &Engine,
    state: &CharacterState,
    link: &str,
    item: &str,
    uid: &str,
) -> Result<CharacterState, String> {
    let snap = engine.derive_with(state, false);
    let prompt = snap
        .inventory
        .infusion_prompts
        .iter()
        .find(|p| p.link == link && p.items.iter().any(|i| i == item))
        .ok_or("That infusion does not wait for this item.")?;
    let target = prompt
        .targets
        .iter()
        .find(|t| t.uid == uid)
        .ok_or("That item cannot take this infusion.")?;
    let uni = engine.universe(state.ruleset);
    // the requested item when it fits the armor, else the pick's item that does
    let base = state
        .inventory
        .items
        .iter()
        .find(|i| i.uid == uid)
        .and_then(|i| uni.get(&i.item));
    let fits = |m: &str| base.is_some_and(|b| uni.get(m).is_some_and(|m| can_adorn(m, b)));
    let item = std::iter::once(item)
        .chain(prompt.items.iter().map(String::as_str))
        .find(|m| fits(m))
        .ok_or("That item cannot take this infusion.")?;
    let mut next = state.clone();
    if let Some(it) = next.inventory.items.iter_mut().find(|i| i.uid == uid) {
        it.infusion = Some(item.to_string());
        it.infusion_layer_link = Some(link.to_string());
        it.part = Some(target.part.clone());
    }
    Ok(next)
}

/// Set the part an infusion line sits on (`None` takes it off its part).
///
/// # Errors
/// A message when the line is not an infusion line or the part is not free and fitting.
pub fn set_part(
    engine: &Engine,
    state: &CharacterState,
    uid: &str,
    part: Option<&str>,
) -> Result<CharacterState, String> {
    if !engine.settings().infusion_items {
        return Err("Infusion items are off in the settings.".into());
    }
    let mut next = state.clone();
    let line = next
        .inventory
        .items
        .iter()
        .find(|i| i.uid == uid)
        .ok_or("That item is not in the inventory.")?;
    if let Some(p) = part {
        if !linked(line) {
            return Err("Only an infusion's item sits on a part.".into());
        }
        let snap = engine.derive_with(state, false);
        let parts = parts(&snap).ok_or("Nothing splits an item into parts.")?;
        if !legal_parts(engine.universe(state.ruleset), state, &parts, uid)
            .iter()
            .any(|l| l == p)
        {
            return Err(format!("The {p} is taken, or this item does not fit it."));
        }
    }
    if let Some(it) = next.inventory.items.iter_mut().find(|i| i.uid == uid) {
        it.part = part.map(str::to_string);
    }
    Ok(next)
}

/// Keep each pick's item line with it (see the module docs). `None` when nothing changes.
#[must_use]
pub fn reconcile(engine: &Engine, state: &CharacterState, mode: Mode) -> Option<CharacterState> {
    if !engine.settings().infusion_items {
        return None;
    }
    let has_link = state.inventory.items.iter().any(linked);
    let uni = engine.universe(state.ruleset);
    // cheap test first: no linked line and no pick that can make an item
    let any_pick = state.selections.iter().any(|s| match &s.choice {
        Choice::Element(id) => uni.get(id).is_some_and(|e| {
            named_item(uni, e).is_some()
                || e.el
                    .sheet
                    .iter()
                    .flat_map(|s| &s.descriptions)
                    .any(|d| !enhancement_in(&d.text).is_empty())
        }),
        Choice::ListItem(_) => false,
    });
    if !has_link && !any_pick && state.inventory.infusion_declined.is_empty() {
        return None;
    }
    let mut next = state.clone();
    let mut changed = false;
    // a line whose pick is no longer selected loses one copy (missing content keeps it)
    let selected: HashSet<String> = state.selections.iter().filter_map(link_of).collect();
    // a deleted line is only remembered while its pick stays
    let declined = next.inventory.infusion_declined.len();
    next.inventory
        .infusion_declined
        .retain(|l| selected.contains(l));
    changed |= next.inventory.infusion_declined.len() != declined;
    // a link whose pick or slot id was renamed moves to the renamed link: same line, same state
    let used: HashSet<String> = state
        .inventory
        .items
        .iter()
        .flat_map(|i| [i.infusion_link.clone(), i.infusion_layer_link.clone()])
        .flatten()
        .collect();
    let free: Vec<&String> = selected.iter().filter(|l| !used.contains(*l)).collect();
    for it in &mut next.inventory.items {
        for link in [&mut it.infusion_link, &mut it.infusion_layer_link] {
            if let Some(old) = link.as_ref().filter(|l| !selected.contains(*l)) {
                if let Some(new) = renamed_link(uni, old, &free) {
                    *link = Some(new.clone());
                    changed = true;
                }
            }
        }
    }
    let mut kept = Vec::with_capacity(next.inventory.items.len());
    for mut it in std::mem::take(&mut next.inventory.items) {
        // a layer whose pick is gone comes off; the line itself stays
        if it
            .infusion_layer_link
            .as_deref()
            .is_some_and(|l| !selected.contains(l))
        {
            it.infusion = None;
            it.infusion_layer_link = None;
            it.part = None;
            changed = true;
        }
        match it.infusion_link.as_deref() {
            // the user's own line, used for the pick: unlinked and kept exactly as it is
            Some(l) if !selected.contains(l) && it.infusion_adopted => {
                changed = true;
                it.infusion_link = None;
                it.infusion_adopted = false;
                it.part = None;
                kept.push(it);
            }
            Some(l) if !selected.contains(l) => {
                changed = true;
                if it.amount > 1 {
                    it.amount -= 1;
                    it.infusion_link = None;
                    kept.push(it);
                }
            }
            _ => kept.push(it),
        }
    }
    next.inventory.items = kept;
    if !any_pick {
        return changed.then_some(next);
    }
    let snap = engine.derive_with(&next, false);
    // no infusion maximum and no linked line: nothing to keep
    if !snap.stats.contains_key(MAX_STAT) && !next.inventory.items.iter().any(linked) {
        return changed.then_some(next);
    }
    for plan in plans(uni, &next, &snap) {
        if let Some(i) = next
            .inventory
            .items
            .iter()
            .position(|it| linked_to(it, &plan.link))
        {
            // a +N line (or layer) follows the pick's level
            let line = &next.inventory.items[i];
            if line.infusion_layer_link.as_deref() == Some(plan.link.as_str()) {
                if line
                    .infusion
                    .as_ref()
                    .is_some_and(|x| !plan.items.contains(x))
                {
                    let fit = uni.get(&line.item).and_then(|base| {
                        plan.items
                            .iter()
                            .find(|c| uni.get(c).is_some_and(|c| can_adorn(c, base)))
                    });
                    if let Some(c) = fit.cloned() {
                        next.inventory.items[i].infusion = Some(c);
                        changed = true;
                    }
                }
                continue;
            }
            if plan.needs_base
                && !line.infusion_adopted
                && line
                    .adorner
                    .as_ref()
                    .is_some_and(|a| !plan.items.contains(a))
            {
                let fit = uni.get(&line.item).and_then(|base| {
                    plan.items
                        .iter()
                        .find(|c| uni.get(c).is_some_and(|c| can_adorn(c, base)))
                });
                if let Some(c) = fit.cloned() {
                    next.inventory.items[i].adorner = Some(c);
                    changed = true;
                }
            }
            continue;
        }
        // on load: the pick's own named item added by hand is adopted; a layer (only this engine
        // writes one) is re-linked; a generic +N line is never taken without asking (loot)
        if let Some(it) = next
            .inventory
            .items
            .iter_mut()
            .filter(|_| mode == Mode::Load)
            .find(|it| {
                !linked(it)
                    && ((plan.named && holds(&plan, it))
                        || it.infusion.as_ref().is_some_and(|x| plan.items.contains(x)))
            })
        {
            if it.infusion.as_ref().is_some_and(|x| plan.items.contains(x)) {
                it.infusion_layer_link = Some(plan.link.clone());
            } else {
                // a hand-added copy stays the user's: unpicking unlinks it, never removes it
                it.infusion_link = Some(plan.link.clone());
                it.infusion_adopted = true;
            }
            changed = true;
            continue;
        }
        if plan.needs_base {
            continue;
        }
        let Some(item) = plan.items.first() else {
            continue;
        };
        // the user deleted this pick's line: it stays deleted
        if next.inventory.infusion_declined.contains(&plan.link) {
            continue;
        }
        if let Ok(out) = commands::add_item(engine, &next, item, 1) {
            next = out.state;
            if let Some(line) = next.inventory.items.last_mut() {
                line.infusion_link = Some(plan.link.clone());
                let uid = line.uid.clone();
                next = place(engine, &next, &uid);
            }
            changed = true;
        }
    }
    changed.then_some(next)
}

/// Fill [`InventorySnapshot::infusion_prompts`] of a fresh snapshot.
pub(crate) fn annotate(uni: &Universe, state: &CharacterState, snap: &mut CharacterSnapshot) {
    // nothing to say without an infusion maximum or a linked line
    let max = snap.stats.get(MAX_STAT).map(|s| s.value);
    if max.is_none() && !state.inventory.items.iter().any(linked) {
        return;
    }
    let pts = parts(snap);
    snap.inventory.infusion_prompts = prompts_with(uni, state, snap, pts.as_ref());
    if let Some(pts) = &pts {
        for line in &mut snap.inventory.items {
            if state
                .inventory
                .items
                .iter()
                .any(|i| i.uid == line.uid && linked(i))
            {
                line.part_options = legal_parts(uni, state, pts, &line.uid);
            }
        }
    }
    let Some(max) = max else {
        return;
    };
    let lines: Vec<&InventoryItem> = state.inventory.items.iter().filter(|i| linked(i)).collect();
    let used = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    let bonus = pts.as_ref().map_or(0, |p| p.bonus);
    let off_parts = lines.iter().filter(|i| i.part.is_none()).count();
    let mut warn = |c: &str, m: String| {
        snap.diagnostics
            .push(Diagnostic::new(c, Severity::Warning, m));
    };
    if i64::from(used) > i64::from(max) {
        warn(
            code::INFUSIONS_OVER_LIMIT,
            format!("{used} infusion items but at most {max} at once"),
        );
    } else if bonus > 0 && i64::try_from(off_parts).unwrap_or(i64::MAX) > i64::from(max - bonus) {
        warn(
            code::INFUSIONS_OVER_LIMIT,
            format!(
                "{off_parts} infusion items are not on a part, but only {} can be: the other {bonus} go on parts",
                max - bonus
            ),
        );
    }
    let mut seen: Vec<&str> = Vec::new();
    for it in &lines {
        if let Some(p) = it.part.as_deref() {
            if seen.contains(&p) {
                warn(
                    code::INFUSIONS_PART_TAKEN,
                    format!("two infusions on the {p}: each part holds one"),
                );
            }
            seen.push(p);
        }
    }
    snap.inventory.infusions = Some(InfusionsSnapshot {
        used,
        max,
        parts: pts.as_ref().map_or_else(Vec::new, |p| p.labels.clone()),
        parts_from: pts.as_ref().map(|p| p.from.clone()),
        parts_bonus: bonus,
    });
}

#[cfg(test)]
mod tests {
    use super::{enhancement_in, is_generic_format};

    #[test]
    fn reads_the_plus_n_rule() {
        assert_eq!(
            enhancement_in("You can infuse a simple or martial weapon into a +2 weapon."),
            [("Weapon", 2)]
        );
        assert_eq!(
            enhancement_in(
                "You can infuse a suit of armor or a shield into a +1 armor or a +1 shield."
            ),
            [("Armor", 1)]
        );
        assert!(enhancement_in("You can infuse a rod into a +1 rod.").is_empty());
        assert!(enhancement_in("A +1 bonus to attack rolls.").is_empty());
        // a later "into a +N" is read too, whatever comes first
        assert_eq!(
            enhancement_in("Turn a rod into a +1 rod, or a weapon into a +2 weapon."),
            [("Weapon", 2)]
        );
        assert_eq!(
            enhancement_in("Turn a weapon into a +1 weapon, or armor into a +2 armor."),
            [("Weapon", 1), ("Armor", 2)]
        );
    }

    #[test]
    fn the_generic_item_is_any_name_format_that_only_adds_the_bonus() {
        assert!(is_generic_format("{{parent}} +{{enhancement}}"));
        assert!(is_generic_format("+{{enhancement}} {{parent}}"));
        assert!(is_generic_format("$(parent), +$(enhancement)"));
        assert!(!is_generic_format("Glowing {{parent}} +{{enhancement}}"));
        assert!(!is_generic_format("{{parent}}"));
    }
}
