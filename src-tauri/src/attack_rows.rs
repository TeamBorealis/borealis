//! The attacks table as the UI shows it: the engine's automatic rows (weapons, the Unarmed
//! Strike, cantrips, breath weapons) with the user's edits (GitHub #27), then the hand-made rows.

use crate::dto::{AbilityLineDto, Attack};
use crate::ids::ability_key;
use borealis_character::{AttackEdit, Character, ManualAttack};
use borealis_engine::ids as engine_ids;
use borealis_engine::snapshot::{AttackSnapshot, CharacterSnapshot as Engine};

/// The id prefix of a hand-made row (`manual-<index>`).
pub const MANUAL_PREFIX: &str = "manual-";

/// Automatic and manual attacks.
#[must_use]
pub fn attacks(c: &Character, snap: &Engine) -> Vec<Attack> {
    let d20 = crate::play_rolls::d20_tests(snap);
    let mods: Vec<i32> = snap.abilities.iter().map(|a| a.modifier).collect();
    let auto = snap.attacks.iter().map(|a| auto_row(c, snap, a, d20));
    let manual = c
        .details
        .attacks
        .iter()
        .enumerate()
        // an Aurora row linked to an inventory item is the automatic row of that item (the
        // sheet prints it once, from the engine)
        .filter(|(_, a)| a.item_uid.is_none())
        .map(move |(i, a)| manual_row(i, a, d20, (&mods, snap.proficiency_bonus)));
    auto.chain(manual).collect()
}

fn ability_of(name: &str) -> Option<String> {
    Some(ability_key(name)).filter(|k| !k.is_empty())
}

fn auto_row(c: &Character, snap: &Engine, a: &AttackSnapshot, d20: i32) -> Attack {
    let edit = c
        .details
        .auto_attack_edits
        .get(&a.uid)
        .cloned()
        .unwrap_or_default();
    let typed_bonus = !edit.attack.trim().is_empty();
    Attack {
        id: a.uid.clone(),
        name: AttackEdit::text(&edit.name, &a.name),
        range: AttackEdit::text(&edit.range, &a.range),
        attack_bonus: AttackEdit::text(&edit.attack, &format!("{:+}", a.attack)),
        damage: AttackEdit::text(&edit.damage, &a.damage),
        description: edit.description.clone(),
        ability: ability_of(&a.ability),
        auto: true,
        show_in_combat: !edit.hidden,
        card: c
            .details
            .auto_attack_cards
            .get(&a.uid)
            .copied()
            .unwrap_or_else(|| {
                snap.inventory
                    .items
                    .iter()
                    .find(|i| i.uid == a.uid)
                    .is_none_or(|i| i.show_card)
            }),
        masteries: a
            .masteries
            .iter()
            .map(|m| crate::play_dto::MasteryDto {
                name: m.name.clone(),
                text: m.text.clone(),
                mastered: m.mastered,
            })
            .collect(),
        crit_range: a.crit_range,
        die_floor: a.die_floor,
        offhand_damage: a.offhand_damage.clone(),
        ammunition: a.ammunition.clone(),
        // the engine's attack already holds the D20 Test modifier; a save row has no roll
        to_hit: if typed_bonus {
            crate::play_rolls::manual_to_hit(&edit.attack, d20)
        } else {
            a.save.is_none().then_some(a.attack)
        },
        ability_lines: (!a.ability_lines.is_empty()).then(|| {
            a.ability_lines
                .iter()
                .zip(engine_ids::ABILITIES)
                .map(|(l, name)| AbilityLineDto {
                    ability: ability_key(name),
                    attack_bonus: format!("{:+}", l.attack),
                    damage: l.damage.clone(),
                })
                .collect()
        }),
        calculated: false,
    }
}

/// A hand-made row; `(mods, proficiency)` work out a calculated one (GitHub #29).
fn manual_row(i: usize, a: &ManualAttack, d20: i32, (mods, proficiency): (&[i32], i32)) -> Attack {
    let (attack_bonus, damage) = a.worked_out(mods, proficiency);
    Attack {
        id: format!("{MANUAL_PREFIX}{i}"),
        name: a.name.clone(),
        range: a.range.clone(),
        to_hit: crate::play_rolls::manual_to_hit(&attack_bonus, d20),
        attack_bonus,
        damage,
        description: a.description.clone(),
        ability: ability_of(&a.ability),
        auto: false,
        show_in_combat: a.displayed,
        card: a.as_card.unwrap_or(false),
        masteries: Vec::new(),
        crit_range: 20,
        die_floor: None,
        offhand_damage: None,
        ammunition: None,
        ability_lines: None,
        calculated: a.calculated,
    }
}
