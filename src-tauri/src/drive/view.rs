//! Compact JSON views of a character snapshot (the full snapshot is far larger than a tool
//! result should be; `section: full` still returns it).

use crate::dto::{CharacterSnapshot, SelectionSlot};
use borealis_content::Ruleset;
use serde_json::{json, Value};

pub fn ruleset_label(rs: Ruleset) -> &'static str {
    match rs {
        Ruleset::Dnd2014 => "2014",
        Ruleset::Dnd2024 => "2024",
        Ruleset::Any => "any",
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Name, level, classes, final scores, the headline numbers and the open picks.
pub fn summary(s: &CharacterSnapshot) -> Value {
    let classes: Vec<Value> = s
        .level
        .classes
        .iter()
        .map(|c| {
            json!({
                "classSlotId": c.slot_id,
                "classId": c.class_id,
                "name": c.name,
                "level": c.level,
                "subclass": c.subclass.as_ref().map(|r| &r.name),
            })
        })
        .collect();
    json!({
        "id": s.id,
        "name": s.name,
        "ruleset": ruleset_label(s.ruleset),
        "level": s.level.total,
        "classes": classes,
        "abilities": to_value(&s.abilities.final_score),
        "armorClass": s.stats.armor_class,
        "hpMax": s.stats.hp_max,
        "speed": s.stats.speed,
        "proficiencyBonus": s.stats.proficiency_bonus,
        "openSlots": s.progress.remaining,
        "nextSlotId": s.progress.next_slot_id,
        "canLevelUp": s.level.can_level_up,
        "canMulticlass": s.level.can_multiclass,
        "notices": s.notices,
        "unsaved": s.dirty,
    })
}

pub fn slot(slot: &SelectionSlot) -> Value {
    json!({
        "slotId": slot.id,
        "title": slot.title,
        "type": slot.element_type,
        "tab": slot.tab,
        "classSlotId": slot.class_slot_id,
        "required": slot.required,
        "locked": slot.locked,
        "level": slot.gained_on_level,
        "chosen": slot.chosen.as_ref().map(|c| json!({"id": c.id, "name": c.name})),
        "hint": slot.hint,
    })
}

fn spells(s: &CharacterSnapshot) -> Value {
    let classes: Vec<Value> = s
        .spellcasting
        .classes
        .iter()
        .map(|c| {
            let list: Vec<Value> = c
                .spells
                .iter()
                .map(|sp| {
                    json!({
                        "id": sp.id, "name": sp.name, "level": sp.level,
                        "prepared": sp.prepared, "alwaysPrepared": sp.always_prepared,
                    })
                })
                .collect();
            json!({
                "classSlotId": c.class_slot_id,
                "className": c.class_name,
                "model": c.model,
                "ability": c.ability,
                "saveDc": c.save_dc,
                "attackBonus": c.attack_bonus,
                "cantrips": {"known": c.cantrips_known, "max": c.cantrips_max},
                "prepared": {"count": c.prepared_count, "max": c.prepared_max},
                "maxSpellLevel": c.max_spell_level,
                "spells": list,
            })
        })
        .collect();
    json!({"classes": classes, "slots": to_value(&s.spellcasting.slots)})
}

fn inventory(s: &CharacterSnapshot) -> Value {
    let items: Vec<Value> = s
        .inventory
        .items
        .iter()
        .map(|i| {
            json!({
                "uid": i.id, "itemId": i.item_id, "name": i.display_name,
                "category": i.category, "quantity": i.quantity,
                "equipped": i.equipped, "attuned": i.attuned,
                "requiresAttunement": i.requires_attunement,
            })
        })
        .collect();
    json!({
        "items": items,
        "coins": to_value(&s.inventory.coins),
        "attunement": to_value(&s.inventory.attunement),
        "carriedWeight": s.inventory.carried_weight,
        "capacity": s.inventory.capacity,
        "warnings": s.inventory.warnings,
    })
}

fn stats(s: &CharacterSnapshot) -> Value {
    let features: Vec<&str> = s.stats.features.iter().map(|f| f.name.as_str()).collect();
    json!({
        "armorClass": s.stats.armor_class,
        "initiative": s.stats.initiative,
        "speed": s.stats.speed,
        "hpMax": s.stats.hp_max,
        "proficiencyBonus": s.stats.proficiency_bonus,
        "passivePerception": s.stats.passive_perception,
        "skills": to_value(&s.stats.skills),
        "languages": s.stats.languages,
        "proficiencies": to_value(&s.stats.proficiencies),
        "features": features,
    })
}

/// One section of the character (`summary` when `section` is `None`).
pub fn section(s: &CharacterSnapshot, section: Option<&str>) -> Result<Value, String> {
    Ok(match section.unwrap_or("summary") {
        "summary" => summary(s),
        "abilities" => to_value(&s.abilities),
        "stats" => stats(s),
        "slots" => json!({"slots": s.slots.iter().map(slot).collect::<Vec<_>>()}),
        "spells" => spells(s),
        "inventory" => inventory(s),
        "full" => to_value(s),
        other => return Err(format!("unknown section: {other}")),
    })
}
