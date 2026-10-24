//! The companion tab state: the creature chosen in a `Companion` select (an Aurora `Companion`
//! element or a bundled `Monster` stat block) with its statistics, plus the user's name/picture.

use crate::companion_block::{self, CompanionBlock};
use crate::content_dto::CompanionState;
use crate::dto::{AbilityRecord, ElementRef};
use crate::extras::ui_extras;
use crate::snapshot_map::MapInput;
use borealis_content::{Element, MonsterData, NO_CHALLENGE_RATING};
use borealis_engine::snapshot::CharacterSnapshot as Engine;

const DEFAULT_ABILITY: i32 = 10;
const COMPANION_ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Armor class, hit points, walking speed and ability scores of a creature. A stat block whose
/// armor class or hit points are a formula (decision IM12a) carries the text in the note and 0 in
/// the number, which the companion tab does not show.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CreatureStats {
    armor_class: i32,
    hp: i32,
    armor_class_note: Option<String>,
    hp_note: Option<String>,
    challenge: String,
    speed: i32,
    abilities: [i32; 6],
}

impl Default for CreatureStats {
    fn default() -> Self {
        Self {
            armor_class: 0,
            hp: 0,
            armor_class_note: None,
            hp_note: None,
            challenge: NO_CHALLENGE_RATING.to_string(),
            speed: 0,
            abilities: [DEFAULT_ABILITY; 6],
        }
    }
}

/// The leading integer of printed text (`11 (2d8 + 2)`, `40 ft., climb 30 ft.`, `13 (natural armor)`).
fn leading_int(text: &str) -> Option<i32> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn monster_stats(m: &MonsterData) -> CreatureStats {
    let a = &m.abilities;
    CreatureStats {
        armor_class: m
            .armor_class()
            .map_or(0, |ac| i32::try_from(ac).unwrap_or(0)),
        hp: m
            .hit_points()
            .map_or(0, |hp| i32::try_from(hp).unwrap_or(0)),
        armor_class_note: m.ac.is_empty().then(|| m.armor_class_text()),
        hp_note: m.hp.is_none().then(|| m.hit_points_text()),
        challenge: m.challenge_text(),
        speed: m.speed.walk.map_or(0, |w| i32::try_from(w).unwrap_or(0)),
        abilities: [a.str, a.dex, a.con, a.int, a.wis, a.cha].map(i32::from),
    }
}

/// Stats of an Aurora `Companion` element (`content-format.md` 6.12 setters).
fn companion_stats(el: &Element) -> CreatureStats {
    let int = |name: &str| el.setter_value(name).and_then(leading_int);
    CreatureStats {
        armor_class: int("ac").unwrap_or(0),
        hp: int("hp").unwrap_or(0),
        armor_class_note: None,
        hp_note: None,
        challenge: el
            .setter_value("challenge")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map_or_else(|| NO_CHALLENGE_RATING.to_string(), str::to_string),
        speed: int("speed").unwrap_or(0),
        abilities: COMPANION_ABILITIES.map(|n| int(n).unwrap_or(DEFAULT_ABILITY)),
    }
}

/// The slider's companion block of a chosen creature element.
fn creature_block(el: &Element, input: &MapInput<'_>) -> CompanionBlock {
    if el.ty == "Monster" {
        return MonsterData::from_element(el).map_or_else(
            |_| CompanionBlock::default(),
            |m| companion_block::monster_block(&m),
        );
    }
    companion_block::element_block(el, &|id| input.hub.element(id).cloned())
}

fn creature_stats(el: &Element) -> CreatureStats {
    if el.ty == "Monster" {
        return MonsterData::from_element(el)
            .map_or_else(|_| CreatureStats::default(), |m| monster_stats(&m));
    }
    companion_stats(el)
}

/// The companion, when a companion creature is chosen or the user named/pictured one.
#[must_use]
pub fn companion(input: &MapInput<'_>, snap: &Engine) -> Option<CompanionState> {
    let chosen = snap
        .slots
        .iter()
        .filter(|s| s.ty == "Companion")
        .find_map(|s| s.chosen.as_ref());
    let creature = chosen.map(|e| ElementRef {
        id: e.id.clone(),
        name: e.name.clone(),
        ty: "companion".to_string(),
        source: e.source.clone(),
    });
    let element = chosen.and_then(|e| input.hub.element(&e.id));
    let stats = element.map(creature_stats).unwrap_or_default();
    let block = element
        .map(|e| creature_block(e, input))
        .unwrap_or_default();
    let skills = companion_block::skills(&block, &stats.abilities);
    let c = input.character;
    let portrait = ui_extras(c).companion_portrait;
    let name = c.details.companion_name.clone();
    if creature.is_none() && portrait.is_none() && name.is_empty() {
        return None;
    }
    Some(CompanionState {
        name: if name.is_empty() {
            creature
                .as_ref()
                .map(|e| e.name.clone())
                .unwrap_or_default()
        } else {
            name
        },
        creature,
        portrait,
        armor_class: stats.armor_class,
        hp: stats.hp,
        armor_class_note: stats.armor_class_note,
        hp_note: stats.hp_note,
        challenge: stats.challenge,
        speed: stats.speed,
        abilities: AbilityRecord::from_fn(|i| stats.abilities[i]),
        proficiency: block.proficiency,
        initiative: block.initiative,
        build: block.build,
        fly_speed: block.speeds[0],
        climb_speed: block.speeds[1],
        swim_speed: block.speeds[2],
        burrow_speed: block.speeds[3],
        passive_perception: companion_block::passive_perception(&skills),
        skills,
        traits: block.traits,
        actions: block.actions,
        reactions: block.reactions,
    })
}

/// The companion's stat block for the printed sheet; `None` when no creature is chosen (a name or
/// a picture alone adds no page).
#[must_use]
pub fn sheet_block(c: &CompanionState) -> Option<borealis_sheet::CompanionSheet> {
    use borealis_sheet::{CompanionEntry, CompanionSheet, CompanionSkillLine};
    let creature = c.creature.as_ref()?;
    let entries = |v: &[crate::content_dto::CompanionFeature]| -> Vec<CompanionEntry> {
        v.iter()
            .map(|f| CompanionEntry {
                name: f.name.clone(),
                text: f.text.clone(),
            })
            .collect()
    };
    let a = &c.abilities;
    Some(CompanionSheet {
        creature: creature.name.clone(),
        name: c.name.clone(),
        build: c.build.clone(),
        armor_class: c
            .armor_class_note
            .clone()
            .unwrap_or_else(|| c.armor_class.to_string()),
        hit_points: c.hp_note.clone().unwrap_or_else(|| c.hp.to_string()),
        initiative: c.initiative,
        proficiency: c.proficiency,
        challenge: c.challenge.clone(),
        speed: c.speed,
        other_speeds: [c.fly_speed, c.climb_speed, c.swim_speed, c.burrow_speed],
        abilities: [a.str, a.dex, a.con, a.int, a.wis, a.cha],
        skills: c
            .skills
            .iter()
            .map(|s| CompanionSkillLine {
                name: s.name.clone(),
                ability: s.ability.clone(),
                bonus: s.bonus,
            })
            .collect(),
        passive_perception: c.passive_perception,
        traits: entries(&c.traits),
        actions: entries(&c.actions),
        reactions: entries(&c.reactions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_int_reads_printed_numbers() {
        assert_eq!(leading_int("11 (2d8 + 2)"), Some(11));
        assert_eq!(leading_int(" 40 ft., climb 30 ft."), Some(40));
        assert_eq!(leading_int("—"), None);
    }

    #[test]
    fn companion_setters_fill_stats() {
        let el: Element = serde_json::from_value(serde_json::json!({
            "id": "ID_T_COMP", "name": "Wolf", "type": "Companion", "source": "Test",
            "setters": [{"name": "ac", "value": "13 (natural armor)"}, {"name": "hp", "value": "11 (2d8 + 2)"},
                        {"name": "speed", "value": "40 ft."}, {"name": "strength", "value": "12"},
                        {"name": "dexterity", "value": "15"}]
        }))
        .unwrap();
        let s = creature_stats(&el);
        assert_eq!((s.armor_class, s.hp, s.speed), (13, 11, 40));
        assert_eq!(s.abilities, [12, 15, 10, 10, 10, 10]);
        assert_eq!(s.challenge, NO_CHALLENGE_RATING);
    }

    #[test]
    fn sheet_block_needs_a_chosen_creature() {
        let state = |creature: Option<ElementRef>| CompanionState {
            creature,
            name: "Rex".into(),
            portrait: None,
            armor_class: 13,
            hp: 0,
            armor_class_note: None,
            hp_note: Some("2d8 + 2".into()),
            challenge: "1/4".into(),
            speed: 40,
            abilities: AbilityRecord::from_fn(|i| 10 + i32::try_from(i).unwrap()),
            proficiency: 2,
            initiative: 1,
            build: "Medium beast".into(),
            fly_speed: 0,
            climb_speed: 30,
            swim_speed: 0,
            burrow_speed: 0,
            skills: Vec::new(),
            passive_perception: 10,
            traits: Vec::new(),
            actions: Vec::new(),
            reactions: Vec::new(),
        };
        assert!(sheet_block(&state(None)).is_none());
        let creature = ElementRef {
            id: "ID_T".into(),
            name: "Hound".into(),
            ty: "companion".into(),
            source: "Test".into(),
        };
        let b = sheet_block(&state(Some(creature))).unwrap();
        assert_eq!(
            (b.creature.as_str(), b.hit_points.as_str()),
            ("Hound", "2d8 + 2")
        );
        assert_eq!(
            (b.armor_class.as_str(), b.abilities[5], b.other_speeds[1]),
            ("13", 15, 30)
        );
    }

    #[test]
    fn companion_challenge_setter_is_shown() {
        let el: Element = serde_json::from_value(serde_json::json!({
            "id": "ID_T_COMP", "name": "Wolf", "type": "Companion", "source": "Test",
            "setters": [{"name": "challenge", "value": " 1/4 "}]
        }))
        .unwrap();
        assert_eq!(creature_stats(&el).challenge, "1/4");
    }
}
