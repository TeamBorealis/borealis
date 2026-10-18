//! Attack rows of carried weapons. Borealis lists an attack only for an equipped weapon, while an
//! expected sheet lists every carried weapon (one-handed, plus the two-handed damage of versatile
//! weapons). After the main facts are read, each weapon line is equipped in turn through
//! `equip_item` (main hand, then both hands for versatile) and its attack row read: the numbers
//! are always the engine's own.

use super::compare::{Facts, Fv};
use super::norm;
use borealis_lib::dto::CharacterSnapshot;
use borealis_lib::dto_ext::EquipRequest;
use borealis_lib::state::AppState;

fn row(snap: &CharacterSnapshot, name: &str) -> Option<(i64, String)> {
    snap.attacks
        .iter()
        .find(|a| norm::label(&a.name) == name)
        .and_then(|a| {
            let hit = a.attack_bonus.trim().trim_start_matches('+').parse().ok()?;
            Some((hit, norm::damage(&a.damage)))
        })
}

fn free_hands(app: &AppState, id: &str, snap: &CharacterSnapshot) -> Option<CharacterSnapshot> {
    let mut cur = None;
    for line in snap.inventory.items.iter().filter(|i| i.equipped.is_some()) {
        if line.armor.is_none()
            || line.category.to_ascii_lowercase().contains("shield")
            || line.name.eq_ignore_ascii_case("shield")
        {
            if let Ok(s) = app.unequip_item(id, &line.id) {
                cur = Some(s);
            }
        }
    }
    cur
}

/// Facts `attack [<weapon>] to hit|damage|damage two handed` for every weapon line.
pub fn probe(app: &AppState, snap: &CharacterSnapshot) -> Facts {
    let mut out = Facts::new();
    let id = snap.id.clone();
    let lines: Vec<(String, String)> = snap
        .inventory
        .items
        .iter()
        .filter(|i| i.is_weapon)
        .map(|i| (i.id.clone(), norm::label(&i.name)))
        .collect();
    let mut cur = snap.clone();
    for (uid, name) in lines {
        let one = app
            .equip_item(&id, &uid, EquipRequest::Primary)
            .or_else(|_| app.equip_item(&id, &uid, EquipRequest::TwoHanded));
        if let Ok(s) = one {
            if let Some((hit, dmg)) = row(&s, &name) {
                out.insert(format!("attack [{name}] to hit"), Fv::Num(hit));
                out.insert(format!("attack [{name}] damage"), Fv::Text(dmg));
            }
            cur = s;
        }
        let two = app
            .equip_item(&id, &uid, EquipRequest::Versatile)
            .or_else(|_| {
                if let Some(s) = free_hands(app, &id, &cur) {
                    cur = s;
                }
                app.equip_item(&id, &uid, EquipRequest::Versatile)
            });
        if let Ok(s) = two {
            if let Some((_, dmg)) = row(&s, &name) {
                out.insert(format!("attack [{name}] damage two handed"), Fv::Text(dmg));
            }
            cur = s;
        }
        if let Ok(s) = app.unequip_item(&id, &uid) {
            cur = s;
        }
    }
    out
}
