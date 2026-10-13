//! Inventory snapshot and weapon attacks.

use crate::diag::DiagSink;
use crate::graph::Graph;
use crate::ids;
use crate::inventory::{self, item_facts, item_weight};
use crate::money::Coin;
use crate::snapshot::{
    AttackSnapshot, ElementRef, InventorySnapshot, ItemSnapshot, MartialArtsSnapshot, MasteryPick,
};
use crate::state::{CharacterState, EquipSlot};
use crate::stats::StatSet;
use crate::universe::{UElem, Universe};

/// Element reference helper.
#[must_use]
pub fn element_ref(e: &UElem) -> ElementRef {
    ElementRef {
        id: e.el.id.clone(),
        name: e.el.name.clone(),
        ty: e.el.ty.clone(),
        source: e.el.source.clone(),
    }
}

/// Inventory summary with weights and attunement.
#[must_use]
pub fn inventory_snapshot(
    uni: &Universe,
    graph: &Graph<'_>,
    stats: &StatSet,
    state: &CharacterState,
    limit: u32,
    strength: i32,
    _diags: &mut DiagSink,
) -> InventorySnapshot {
    let inv = &state.inventory;
    let mut items = Vec::new();
    let mut attuned_total = 0;
    let effective_attuned: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.alive && n.attuned)
        .filter_map(|n| n.item_uid.clone())
        .collect();
    for it in &inv.items {
        let Some(e) = uni.get(&it.item) else { continue };
        let f = item_facts(e);
        let att = effective_attuned.contains(&it.uid);
        if att {
            attuned_total += 1;
        }
        let adorner = it.adorner.as_deref().and_then(|a| uni.get(a));
        let name = it.alt_name.clone().unwrap_or_else(|| match adorner {
            Some(a) => format!("{} ({})", e.el.name, a.el.name),
            None => e.el.name.clone(),
        });
        items.push(ItemSnapshot {
            uid: it.uid.clone(),
            item: element_ref(e),
            name,
            amount: it.amount,
            equipped: it.equipped,
            attuned: att,
            requires_attunement: f.requires_attunement
                || adorner.is_some_and(|a| item_facts(a).requires_attunement),
            weight: item_weight(uni, it),
            cost_gp: inventory::item_cost_gp(e),
            storage: it.storage.clone(),
            show_card: it.card && !it.hidden,
        });
    }
    let size = graph
        .nodes_of_type("Size")
        .next()
        .map(|(_, n)| n.e.el.id.as_str());
    let mult = inventory::capacity_multiplier(
        size,
        graph.has(ids::CAPACITY_LARGER),
        graph.has(ids::CAPACITY_DOUBLED),
        graph.has(ids::CAPACITY_HALVED),
    );
    let _ = stats;
    let capacity = crate::combat::carry(graph, strength)
        .map_or_else(|| f64::from(strength) * mult, |c| c.carry);
    InventorySnapshot {
        items,
        coins: inv.coins,
        gold_value: inv.coins.gold_value(),
        weight: inventory::carried_weight(uni, inv),
        capacity,
        attuned: attuned_total,
        attunement_limit: limit,
        coin_order: Coin::ALL.to_vec(),
    }
}

fn dice_with_bonus(dice: &str, bonus: i32, ty: &str) -> String {
    let sign = if bonus < 0 { "-" } else { "+" };
    let core = if bonus == 0 {
        dice.to_string()
    } else {
        format!("{dice}{sign}{}", bonus.abs())
    };
    if ty.is_empty() {
        core
    } else {
        format!("{core} {ty}")
    }
}

/// Everything the attack lines read besides the state.
pub struct AttackCtx<'a> {
    /// The universe.
    pub uni: &'a Universe,
    /// The expanded graph.
    pub graph: &'a Graph<'a>,
    /// Evaluated statistics.
    pub stats: &'a StatSet,
    /// Chosen Weapon Mastery weapons.
    pub masteries: &'a [MasteryPick],
    /// Martial Arts of the character.
    pub martial_arts: Option<&'a MartialArtsSnapshot>,
}

/// Attack lines for equipped weapons (`engine.md` 12.4; spell attacks are per class).
#[must_use]
pub fn attacks(
    uni: &Universe,
    graph: &Graph<'_>,
    stats: &StatSet,
    state: &CharacterState,
    proficiency: i32,
    mods: &[i32; 6],
) -> Vec<AttackSnapshot> {
    let cx = AttackCtx {
        uni,
        graph,
        stats,
        masteries: &[],
        martial_arts: None,
    };
    attacks_with(&cx, state, proficiency, mods)
}

fn die_sides(dice: &str) -> Option<u32> {
    let (n, d) = dice.trim().split_once('d')?;
    (n.trim() == "1").then(|| d.trim().parse().ok()).flatten()
}

/// Attack lines including masteries, Martial Arts, Rage-style damage and the 2024 Unarmed Strike.
#[must_use]
pub fn attacks_with(
    cx: &AttackCtx<'_>,
    state: &CharacterState,
    proficiency: i32,
    mods: &[i32; 6],
) -> Vec<AttackSnapshot> {
    let (uni, graph, stats) = (cx.uni, cx.graph, cx.stats);
    let d20 = stats.sum(crate::play_rules::D20_GROUP);
    let arts = cx.martial_arts.filter(|m| m.active);
    let mut out = Vec::new();
    for it in &state.inventory.items {
        let Some(e) = uni.get(&it.item) else { continue };
        if !item_facts(e).weapon
            || !matches!(
                it.equipped,
                Some(EquipSlot::PrimaryHand | EquipSlot::SecondaryHand)
            )
        {
            continue;
        }
        let has = |tag: &str| e.supports.iter().any(|s| s == tag);
        let ranged = has("ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED")
            || has("ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED")
            || has("ID_INTERNAL_WEAPON_CATEGORY_FIREARM")
            || has("ID_INTERNAL_WEAPON_CATEGORY_RANGED");
        let melee = !ranged;
        let finesse = has("ID_INTERNAL_WEAPON_PROPERTY_FINESSE");
        let monk_weapon = melee
            && (has("ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE")
                || (has("ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE")
                    && has("ID_INTERNAL_WEAPON_PROPERTY_LIGHT")));
        let dexterous = (finesse || (arts.is_some() && monk_weapon)) && mods[1] > mods[0];
        let ability = if ranged || dexterous { 1 } else { 0 };
        let adorner = it.adorner.as_deref().and_then(|a| uni.get(a));
        let enh = adorner
            .and_then(|a| a.el.setter_value("enhancement"))
            .and_then(|v| v.trim().trim_start_matches('+').parse::<i32>().ok())
            .unwrap_or(0);
        let proficient =
            e.el.setter_value("proficiency")
                .is_some_and(|p| graph.has(p));
        let lname = e.el.name.to_lowercase();
        let group = if melee { "melee" } else { "ranged" };
        let two_handed = has("ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED") || it.versatile;
        let am = crate::attack_rules::attack_mods(
            graph,
            &crate::attack_rules::AttackFacts {
                melee,
                unarmed: false,
                light: has("ID_INTERNAL_WEAPON_PROPERTY_LIGHT"),
                two_handed,
            },
        );
        let atk = am.attack
            + if proficient { proficiency } else { 0 }
            + mods[ability]
            + stats.sum(&format!("{group}:attack"))
            + stats.sum(&format!("{lname}:attack"))
            + enh
            + d20;
        let dmg = e.el.setter("damage");
        let mut dice = dmg.map(|d| d.value.clone()).unwrap_or_default();
        let ty = dmg
            .and_then(|d| d.attribute("type"))
            .unwrap_or("")
            .to_string();
        if it.versatile {
            if let Some(v) =
                e.el.setter_value("versatile")
                    .filter(|v| !v.trim().is_empty())
            {
                dice = v.trim().to_string();
            }
        }
        if let Some(m) = arts.filter(|_| monk_weapon) {
            if die_sides(&m.die) > die_sides(&dice) {
                dice.clone_from(&m.die);
            }
        }
        let bonus = mods[ability]
            + stats.sum(&format!("{group}:damage"))
            + stats.sum(&format!("{lname}:damage"))
            + strength_melee_bonus(stats, ability, melee)
            + enh;
        let offhand_damage = (state.ruleset == borealis_content::Ruleset::Dnd2024
            && has("ID_INTERNAL_WEAPON_PROPERTY_LIGHT")
            && !dice.is_empty())
        .then(|| {
            let keep = if am.offhand_modifier {
                bonus
            } else {
                bonus - mods[ability]
            };
            dice_with_bonus(&dice, keep, &ty)
        });
        out.push(AttackSnapshot {
            crit_range: am.crit,
            die_floor: am.die_floor,
            offhand_damage,
            uid: it.uid.clone(),
            name: it.alt_name.clone().unwrap_or_else(|| e.el.name.clone()),
            range: e.el.setter_value("range").unwrap_or("").to_string(),
            attack: atk,
            damage: if dice.is_empty() {
                String::new()
            } else {
                dice_with_bonus(&dice, bonus, &ty)
            },
            ability: ids::ABILITIES[ability].to_string(),
            proficient,
            mastery: crate::play_rules::attack_mastery(uni, &e.el, cx.masteries),
        });
    }
    if state.ruleset == borealis_content::Ruleset::Dnd2024 {
        out.push(unarmed_strike(cx, arts, proficiency, mods, d20));
    }
    out
}

/// Extra damage of toggles such as Rage: the group `damage:strength melee`.
fn strength_melee_bonus(stats: &StatSet, ability: usize, melee: bool) -> i32 {
    if ability == 0 && melee {
        stats.sum("damage:strength melee")
    } else {
        0
    }
}

fn unarmed_strike(
    cx: &AttackCtx<'_>,
    arts: Option<&MartialArtsSnapshot>,
    proficiency: i32,
    mods: &[i32; 6],
    d20: i32,
) -> AttackSnapshot {
    let ability = if arts.is_some() && mods[1] > mods[0] {
        1
    } else {
        0
    };
    let bonus = mods[ability] + strength_melee_bonus(cx.stats, ability, true);
    let damage = match arts {
        Some(m) => dice_with_bonus(&m.die, bonus, "bludgeoning"),
        None => format!("{} bludgeoning", 1 + bonus),
    };
    let am = crate::attack_rules::attack_mods(
        cx.graph,
        &crate::attack_rules::AttackFacts {
            melee: true,
            unarmed: true,
            light: false,
            two_handed: false,
        },
    );
    AttackSnapshot {
        crit_range: am.crit,
        die_floor: None,
        offhand_damage: None,
        uid: "unarmed".to_string(),
        name: "Unarmed Strike".to_string(),
        range: "5 ft.".to_string(),
        attack: am.attack + proficiency + mods[ability] + cx.stats.sum("melee:attack") + d20,
        damage,
        ability: ids::ABILITIES[ability].to_string(),
        proficient: true,
        mastery: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_text() {
        assert_eq!(dice_with_bonus("1d8", 3, "slashing"), "1d8+3 slashing");
        assert_eq!(dice_with_bonus("1d4", -1, "piercing"), "1d4-1 piercing");
        assert_eq!(dice_with_bonus("1d6", 0, ""), "1d6");
    }
}
