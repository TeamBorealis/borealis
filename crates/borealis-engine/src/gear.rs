//! Inventory snapshot and weapon attacks.

use crate::attunement;
use crate::diag::{code, DiagSink, Diagnostic, Severity};
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
    diags: &mut DiagSink,
) -> InventorySnapshot {
    let inv = &state.inventory;
    let mut items = Vec::new();
    let mut attuned_total = 0;
    let effective_attuned: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.alive && n.attuned)
        .filter_map(|n| n.item_uid.clone())
        .chain(
            graph
                .carried_items()
                .iter()
                .filter(|c| c.attuned)
                .map(|c| c.uid.clone()),
        )
        .collect();
    for it in &inv.items {
        let Some(e) = uni.get(&it.item) else { continue };
        let f = item_facts(e);
        let att = effective_attuned.contains(&it.uid);
        if att {
            attuned_total += 1;
        }
        let adorner = it.adorner.as_deref().and_then(|a| uni.get(a));
        let (restriction, barred) =
            attunement::line_restriction(uni, graph, &e.el, adorner.map(|a| &a.el));
        if att && barred {
            diags.push(
                Diagnostic::new(
                    code::ATTUNEMENT_RESTRICTED,
                    Severity::Warning,
                    format!(
                        "{} is attuned although it requires attunement {}",
                        e.el.name,
                        restriction.as_ref().map_or("", |r| r.text.as_str())
                    ),
                )
                .at(e.el.id.clone()),
            );
        }
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
            attunement_restriction: restriction.map(|r| r.text).filter(|t| !t.is_empty()),
            attunement_barred: barred,
            proficient_by_item: crate::item_prof::self_proficient(graph, &it.uid, e, adorner),
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

/// `damage` followed by each per-hit extra (`1d8+5 slashing + 1d8 radiant`).
fn with_extra(damage: String, extra: Option<&Vec<String>>) -> String {
    extra
        .into_iter()
        .flatten()
        .fold(damage, |acc, x| format!("{acc} + {x}"))
}

pub(crate) fn dice_with_bonus(dice: &str, bonus: i32, ty: &str) -> String {
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

/// Attack lines for wielded weapons and for the weapons listed in
/// [`crate::state::Inventory::listed_attacks`] (`engine.md` 12.4; spell attacks are per class).
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
    let bond = crate::pact_weapon::bond(uni, graph, state);
    let mut out = Vec::new();
    for it in &state.inventory.items {
        let Some((e, adorner)) = crate::item_base::resolve(uni, it) else {
            continue;
        };
        let wielded = matches!(
            it.equipped,
            Some(EquipSlot::PrimaryHand | EquipSlot::SecondaryHand)
        );
        if !item_facts(e).weapon || !(wielded || state.inventory.listed_attacks.contains(&it.uid)) {
            continue;
        }
        let versatile = it.versatile && wielded;
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
        let own_ability = if ranged || dexterous { 1 } else { 0 };
        let pact = bond
            .as_ref()
            .filter(|(uid, _)| *uid == it.uid)
            .map(|(_, a)| *a);
        let ability = match pact {
            Some(a) if mods[a] > mods[own_ability] => a,
            _ => own_ability,
        };
        let loaded = crate::ammo::loaded(uni, state, e);
        let enh = weapon_enhancement(&e.el, adorner.map(|a| &a.el))
            + loaded.as_ref().map_or(0, |l| l.enhancement);
        let proficient = pact.is_some()
            || crate::item_prof::weapon_proficient(graph, e)
            || crate::item_prof::self_proficient(graph, &it.uid, e, adorner);
        let lname = e.el.name.to_lowercase();
        let group = if melee { "melee" } else { "ranged" };
        let two_handed = has("ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED") || versatile;
        let am = crate::attack_rules::attack_mods(
            graph,
            &crate::attack_rules::AttackFacts {
                item: Some((it.uid.as_str(), e.el.id.as_str())),
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
        let extra = crate::attack_rules::per_hit_extra_damage(graph, false);
        let dmg = e.el.setter("damage");
        let mut dice = dmg.map(|d| d.value.clone()).unwrap_or_default();
        let ty = dmg
            .and_then(|d| d.attribute("type"))
            .unwrap_or("")
            .to_string();
        if versatile {
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
            name: it.alt_name.clone().unwrap_or_else(|| {
                uni.get(&it.item)
                    .map_or_else(|| e.el.name.clone(), |own| own.el.name.clone())
            }),
            range: e.el.setter_value("range").unwrap_or("").to_string(),
            attack: atk,
            damage: if dice.is_empty() {
                String::new()
            } else {
                with_extra(dice_with_bonus(&dice, bonus, &ty), melee.then_some(&extra))
            },
            ability: ids::ABILITIES[ability].to_string(),
            proficient,
            mastery: crate::play_rules::attack_mastery(uni, &e.el, cx.masteries),
            ammunition: loaded.map(|l| l.name),
            save: None,
        });
    }
    // 2024: every character has the row (SRD 5.2.1 Unarmed Strike); 2014: a character with
    // Martial Arts (SRD 5.1 Monk). Aurora lists no automatic row (its attacks are user-added).
    if state.ruleset == borealis_content::Ruleset::Dnd2024 || cx.martial_arts.is_some() {
        out.push(unarmed_strike(cx, arts, proficiency, mods, d20));
    }
    out
}

/// The `+N` of a weapon: its adorner's enhancement when the adorner has one (a `+1 Weapon`
/// put on a plain Longsword), else the weapon's own (a concrete `+1 Longsword`).
fn weapon_enhancement(
    weapon: &borealis_content::Element,
    adorner: Option<&borealis_content::Element>,
) -> i32 {
    adorner
        .and_then(crate::ammo::enhancement_of)
        .or_else(|| crate::ammo::enhancement_of(weapon))
        .unwrap_or(0)
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
    let damage = with_extra(
        damage,
        Some(&crate::attack_rules::per_hit_extra_damage(cx.graph, true)),
    );
    let am = crate::attack_rules::attack_mods(
        cx.graph,
        &crate::attack_rules::AttackFacts {
            item: None,
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
        ammunition: None,
        save: None,
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

    fn with_enhancement(value: Option<&str>) -> borealis_content::Element {
        let mut el = borealis_content::Element {
            id: "ID_TEST_BLADE".into(),
            name: "Test Blade".into(),
            ty: "Weapon".into(),
            ..borealis_content::Element::default()
        };
        if let Some(v) = value {
            el.setters
                .push(borealis_content::Setter::new("enhancement", v));
        }
        el
    }

    #[test]
    fn weapon_reads_its_own_enhancement_when_not_adorned() {
        let plain = with_enhancement(None);
        let own = with_enhancement(Some("+2"));
        let adorner = with_enhancement(Some("1"));
        assert_eq!(weapon_enhancement(&plain, None), 0);
        assert_eq!(weapon_enhancement(&own, None), 2);
        assert_eq!(weapon_enhancement(&plain, Some(&adorner)), 1);
        // the adorner's bonus replaces the weapon's own, they do not stack
        assert_eq!(weapon_enhancement(&own, Some(&adorner)), 1);
        assert_eq!(weapon_enhancement(&own, Some(&plain)), 2);
    }
}
