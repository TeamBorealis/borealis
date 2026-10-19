//! Stage-2 hosts and checks beyond the class / origin / feat / option hosts of [`super`]:
//! an item host for an element only an item's select offers (an artifact's adjusted ability
//! scores, a lycanthropy "item"), with the character options a requirement names turned on.
//! Every host is derived from the content: no id is named here.

use super::{needed_groups, Edition};
use crate::support::app::{Driver, Explore, Host, Offers, TEST_SCORES};
use crate::{metric, outcome, Installed, SUITE};
use borealis_content::{ContentStore, Element, Rule};
use std::collections::BTreeSet;

/// Element types a character carries (`gear_info::is_item`).
const ITEM_TYPES: [&str; 4] = ["Item", "Weapon", "Armor", "Magic Item"];

/// Element type of a character option (the options dialog's custom toggles).
const OPTION_TYPE: &str = "Option";

/// Host label prefix of an item host.
pub(super) const ITEM_LABEL: &str = "item:";

/// Items an item host tries for one element before giving up.
const MAX_ITEMS: usize = 3;

/// Ability stat names, in snapshot order (str .. cha).
const ABILITY_STATS: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

pub(super) fn is_item_type(ty: &str) -> bool {
    ITEM_TYPES.contains(&ty)
}

/// The items whose selects offer `id`: an item owning an offering select, or an item that
/// grants (unconditionally) the element owning one; only items the edition's item search lists
/// (`searchable`), as a user can add no other. In store order, at most [`MAX_ITEMS`].
pub(super) fn host_items(
    e: &Edition<'_>,
    id: &str,
    el: &Element,
    searchable: &BTreeSet<String>,
) -> Vec<String> {
    let store = &e.ins.store;
    let is_item =
        |x: &str| searchable.contains(x) && store.element(x).is_some_and(|x| is_item_type(&x.ty));
    let mut out: Vec<String> = Vec::new();
    for (owner, _, _) in e.selects.offering(id, &el.ty, Some(el)) {
        if owner == id {
            continue;
        }
        let mut cands: Vec<String> = Vec::new();
        if is_item(owner) {
            cands.push(owner.clone());
        }
        if let Some(granters) = e.selects.granted_by.get(owner) {
            cands.extend(granters.iter().filter(|g| is_item(g)).cloned());
        }
        for c in cands {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out.truncate(MAX_ITEMS);
    out
}

/// Splits needed id groups into the character options to turn on (a group naming an `Option`
/// element: its first one) and the groups still to pick in the builder.
pub(super) fn split_options(
    store: &ContentStore,
    groups: &[Vec<String>],
) -> (Vec<String>, Vec<Vec<String>>) {
    let is_option = |id: &String| store.element(id).is_some_and(|x| x.ty == OPTION_TYPE);
    let mut options = Vec::new();
    let mut rest = Vec::new();
    for g in groups {
        match g.iter().find(|id| is_option(id)) {
            Some(o) => {
                if !options.contains(o) {
                    options.push(o.clone());
                }
            }
            None => rest.push(g.clone()),
        }
    }
    (options, rest)
}

/// The sign of the change each ability stat rule of `el` makes (unconditional integer stats).
fn ability_effects(el: &Element) -> [i32; 6] {
    let mut out = [0; 6];
    for r in &el.rules {
        let Rule::Stat(s) = r else { continue };
        if s.requirements.is_some() || s.level.is_some_and(|l| l > 1) {
            continue;
        }
        let (Some(i), Ok(v)) = (
            ABILITY_STATS.iter().position(|a| *a == s.name),
            s.value.trim().parse::<i32>(),
        ) else {
            continue;
        };
        out[i] += v;
    }
    out.map(i32::signum)
}

fn finals(h: &Host) -> [i32; 6] {
    let f = &h.snap.abilities.final_score;
    [f.str, f.dex, f.con, f.int, f.wis, f.cha]
}

fn offered_in(d: &Driver<'_>, h: &Host, target: &str) -> bool {
    h.snap
        .slots
        .iter()
        .any(|s| s.chosen.is_none() && d.options(h, &s.id).iter().any(|(id, _)| id == target))
}

/// An item host: a L1 fighter (human) holding one of `items` (added, equipped, attuned when the
/// item needs it), with the options the element's requirements name turned on. When the element
/// is offered it is picked: it applies when a slot holds it and every ability score its stats
/// change moves that way. True when offered.
pub(super) fn item_host(
    d: &Driver<'_>,
    e: &Edition<'_>,
    target: &str,
    items: &[String],
    ex: &mut Explore<'_>,
    offers: &mut Offers,
) -> bool {
    let store = &e.ins.store;
    let Some(el) = store.element(target) else {
        return false;
    };
    let (options, _) = split_options(store, &needed_groups(e, target, el));
    items.iter().any(|item| {
        let Some(mut h) = d.create(e.rs, TEST_SCORES) else {
            return false;
        };
        let label = format!("{ITEM_LABEL}{item}");
        for id in [&e.ids.fighter, &e.ids.human, &e.ids.variant]
            .into_iter()
            .flatten()
        {
            d.pick(&mut h, id);
        }
        d.enable_options(&mut h, &options);
        let held = d.hold_item(&mut h, item);
        d.collect(&h, &label, offers);
        let offered = held && offered_in(d, &h, target);
        if offered {
            let before = finals(&h);
            let picked = d.pick(&mut h, target);
            let chosen = h
                .snap
                .slots
                .iter()
                .any(|s| s.chosen.as_ref().is_some_and(|c| c.id == target));
            let after = finals(&h);
            let wrong: Vec<usize> = ability_effects(el)
                .iter()
                .enumerate()
                .filter(|(i, sign)| **sign != 0 && (after[*i] - before[*i]).signum() != **sign)
                .map(|(i, _)| i)
                .collect();
            let why = if !picked || !chosen {
                format!("offered by {label} but the pick was refused")
            } else if !wrong.is_empty() {
                format!(
                    "picked on {label}; ability scores {wrong:?} did not change as its stats say"
                )
            } else {
                format!("picked on {label}")
            };
            offers
                .applied
                .entry(target.to_string())
                .or_insert((picked && chosen && wrong.is_empty(), why));
            d.collect(&h, &label, offers);
            d.explore(&mut h, ex, &label, offers);
        }
        d.drop_host(h);
        offered
    })
}

/// Outcomes beyond "offered": every item-host pick applies (`s2/applies/<id>`). Counts and ids
/// only.
pub(super) fn record_extra(src: &str, _ins: &Installed, offers: &Offers) {
    let mut applied = 0;
    for (id, (ok, why)) in &offers.applied {
        applied += i64::from(*ok);
        outcome::record(SUITE, &format!("{src}/s2/applies/{id}"), *ok, why);
    }
    metric(src, "s2", "*", "item_picks", offers.applied.len() as i64);
    metric(src, "s2", "*", "item_picks_applied", applied);
}
