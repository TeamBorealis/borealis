//! A companion creature's statistics: the creature chosen in a `Companion` select or added by
//! hand (an Aurora `Companion` element or a bundled `Monster` stat block), and its sheet block.
//! The list of companions is built in `companion_list`.

use crate::companion_block::{self, CompanionBlock};
use crate::content_dto::CompanionState;
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
pub(crate) struct CreatureStats {
    pub(crate) armor_class: i32,
    pub(crate) hp: i32,
    pub(crate) armor_class_note: Option<String>,
    pub(crate) hp_note: Option<String>,
    pub(crate) challenge: String,
    pub(crate) speed: i32,
    pub(crate) abilities: [i32; 6],
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
pub(crate) fn creature_block(el: &Element, input: &MapInput<'_>) -> CompanionBlock {
    if el.ty == "Monster" {
        return MonsterData::from_element(el).map_or_else(
            |_| CompanionBlock::default(),
            |m| companion_block::monster_block(&m),
        );
    }
    companion_block::element_block(el, &|id| input.hub.element(id).cloned())
}

pub(crate) fn creature_stats(el: &Element) -> CreatureStats {
    if el.ty == "Monster" {
        return MonsterData::from_element(el)
            .map_or_else(|_| CreatureStats::default(), |m| monster_stats(&m));
    }
    companion_stats(el)
}

/// The companion everything that shows one companion reads (the sheet's companion page, the
/// information slider): the granted one, else the first one added.
#[must_use]
pub fn companion(input: &MapInput<'_>, snap: &Engine) -> Option<CompanionState> {
    crate::companion_list::companions(input, snap)
        .into_iter()
        .next()
}

/// The companion's stat block for the printed sheet; `None` when no creature is chosen (a name or
/// a picture alone adds no page).
#[must_use]
pub fn sheet_block(c: &CompanionState) -> Option<borealis_sheet::CompanionSheet> {
    use borealis_sheet::{CompanionEntry, CompanionSheet, CompanionSkillLine};
    let creature = c.creature.as_ref()?;
    let lines = |v: &[crate::content_dto::CompanionSkill]| -> Vec<CompanionSkillLine> {
        v.iter()
            .map(|s| CompanionSkillLine {
                name: s.name.clone(),
                ability: s.ability.clone(),
                bonus: s.bonus,
            })
            .collect()
    };
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
        skills: lines(&c.skills),
        passive_perception: c.passive_perception,
        saves: lines(&c.saves),
        defenses: entries(&c.defenses),
        senses: c.senses.clone(),
        languages: c.languages.clone(),
        traits: entries(&c.traits),
        actions: entries(&c.actions),
        reactions: entries(&c.reactions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{AbilityRecord, ElementRef};

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
            id: crate::companion_list::GRANTED_ID.into(),
            granted: true,
            notes: String::new(),
            custom_traits: Vec::new(),
            armor_class_bonus: 0,
            hit_points_bonus: 0,
            speed_bonus: 0,
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
            saves: Vec::new(),
            senses: String::new(),
            languages: String::new(),
            defenses: Vec::new(),
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
