//! The companion statistics block of the information slider's companion context: proficiency,
//! initiative, the build line, the speed variants, the skills and the traits, actions and
//! reactions. Read from a bundled `Monster` stat block or from an Aurora `Companion` element.

use crate::content_dto::{CompanionFeature, CompanionSkill};
use crate::elements::plain_text;
use borealis_content::monster_types::Feature;
use borealis_content::{Element, MonsterData};

const DEFAULT_PROFICIENCY: i32 = 2;
const DEFAULT_SCORE: i32 = 10;
const PASSIVE_BASE: i32 = 10;
/// Skill name, key-ability abbreviation and index into the six scores (STR..CHA), in sheet order.
const SKILLS: [(&str, &str, usize); 18] = [
    ("Acrobatics", "DEX", 1),
    ("Animal Handling", "WIS", 4),
    ("Arcana", "INT", 3),
    ("Athletics", "STR", 0),
    ("Deception", "CHA", 5),
    ("History", "INT", 3),
    ("Insight", "WIS", 4),
    ("Intimidation", "CHA", 5),
    ("Investigation", "INT", 3),
    ("Medicine", "WIS", 4),
    ("Nature", "INT", 3),
    ("Perception", "WIS", 4),
    ("Performance", "CHA", 5),
    ("Persuasion", "CHA", 5),
    ("Religion", "INT", 3),
    ("Sleight of Hand", "DEX", 1),
    ("Stealth", "DEX", 1),
    ("Survival", "WIS", 4),
];

/// Everything the companion context shows besides armor class, hit points, walking speed and scores.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompanionBlock {
    pub proficiency: i32,
    pub initiative: i32,
    pub build: String,
    /// Fly, climb, swim, burrow (0 when the creature has none).
    pub speeds: [i32; 4],
    /// Skill name (lower case) to printed bonus.
    pub skill_bonuses: Vec<(String, i32)>,
    pub traits: Vec<CompanionFeature>,
    pub actions: Vec<CompanionFeature>,
    pub reactions: Vec<CompanionFeature>,
}

/// Ability modifier of a score.
#[must_use]
pub fn modifier(score: i32) -> i32 {
    (score - DEFAULT_SCORE).div_euclid(2)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// `Medium beast, unaligned`: size, type and alignment as the slider prints them (upper-cased by CSS).
fn build_line(size: &str, ty: &str, alignment: &str) -> String {
    let head = [capitalize(size.trim()), ty.trim().to_lowercase()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let tail = alignment.trim().to_lowercase();
    match (head.is_empty(), tail.is_empty()) {
        (_, true) => head,
        (true, false) => tail,
        (false, false) => format!("{head}, {tail}"),
    }
}

/// Proficiency bonus of a challenge rating (`1/4`, `5`), 2 for anything below 5 or unreadable.
#[must_use]
pub fn proficiency_for_challenge(challenge: &str) -> i32 {
    let Ok(cr) = challenge.trim().parse::<u32>() else {
        return DEFAULT_PROFICIENCY;
    };
    match cr {
        5..=8 => 3,
        9..=12 => 4,
        13..=16 => 5,
        17..=20 => 6,
        21..=24 => 7,
        25..=28 => 8,
        29..=30 => 9,
        _ => DEFAULT_PROFICIENCY,
    }
}

fn feature(f: &Feature) -> CompanionFeature {
    CompanionFeature {
        name: f.name.clone(),
        text: plain_text(&f.text),
    }
}

/// The block of a bundled stat block.
#[must_use]
pub fn monster_block(m: &MonsterData) -> CompanionBlock {
    let size = m
        .size
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    let dex = i32::from(m.abilities.dex);
    let speed = |v: Option<u32>| v.map_or(0, |n| i32::try_from(n).unwrap_or(0));
    CompanionBlock {
        proficiency: m.proficiency_bonus.map_or(DEFAULT_PROFICIENCY, i32::from),
        initiative: m.initiative.as_ref().map_or(modifier(dex), |i| i.modifier),
        build: build_line(&size, &m.creature_type.to_string(), &m.alignment),
        speeds: [
            speed(m.speed.fly),
            speed(m.speed.climb),
            speed(m.speed.swim),
            speed(m.speed.burrow),
        ],
        skill_bonuses: m.skills.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        traits: m.traits.iter().map(feature).collect(),
        actions: m.actions.iter().map(feature).collect(),
        reactions: m.reactions.iter().map(feature).collect(),
    }
}

/// The number after `keyword` in printed text such as `40 ft., climb 30 ft., fly 60 ft.`
fn printed_speed(text: &str, keyword: &str) -> i32 {
    text.split(',')
        .map(|p| p.trim().to_lowercase())
        .find_map(|part| {
            let rest = part.strip_prefix(keyword)?;
            let digits: String = rest
                .trim()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .unwrap_or(0)
}

/// `Perception +3, Stealth -1` to (`perception`, 3), (`stealth`, -1).
fn printed_skills(text: &str) -> Vec<(String, i32)> {
    text.split(',')
        .filter_map(|part| {
            let (name, bonus) = part.trim().rsplit_once(' ')?;
            let bonus = bonus.trim_start_matches('+').parse().ok()?;
            Some((name.trim().to_lowercase(), bonus))
        })
        .collect()
}

fn linked(
    el: &Element,
    setter: &str,
    lookup: &dyn Fn(&str) -> Option<Element>,
) -> Vec<CompanionFeature> {
    el.setter_value(setter)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .filter_map(lookup)
        .map(|e| CompanionFeature {
            text: e
                .sheet
                .as_ref()
                .and_then(|s| s.descriptions.first())
                .map_or_else(|| plain_text(&e.description), |d| plain_text(&d.text)),
            name: e.name,
        })
        .collect()
}

/// The block of an Aurora `Companion` element; `lookup` finds its linked trait, action and
/// reaction elements by id.
#[must_use]
pub fn element_block(el: &Element, lookup: &dyn Fn(&str) -> Option<Element>) -> CompanionBlock {
    let text = |name: &str| el.setter_value(name).unwrap_or_default();
    let dex = text("dexterity").trim().parse().unwrap_or(DEFAULT_SCORE);
    let speed = text("speed");
    CompanionBlock {
        proficiency: text("proficiency")
            .trim()
            .parse()
            .unwrap_or_else(|_| proficiency_for_challenge(text("challenge"))),
        initiative: modifier(dex),
        build: build_line(text("size"), text("type"), text("alignment")),
        speeds: ["fly", "climb", "swim", "burrow"].map(|k| printed_speed(speed, k)),
        skill_bonuses: printed_skills(text("skills")),
        traits: linked(el, "traits", lookup),
        actions: linked(el, "actions", lookup),
        reactions: linked(el, "reactions", lookup),
    }
}

/// The 18 skills with their bonus: the printed one, else the ability modifier.
#[must_use]
pub fn skills(block: &CompanionBlock, abilities: &[i32; 6]) -> Vec<CompanionSkill> {
    SKILLS
        .iter()
        .map(|(name, abbr, idx)| CompanionSkill {
            name: (*name).to_string(),
            ability: (*abbr).to_string(),
            bonus: block
                .skill_bonuses
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map_or_else(|| modifier(abilities[*idx]), |(_, v)| *v),
        })
        .collect()
}

/// Passive Perception: 10 plus the Perception bonus.
#[must_use]
pub fn passive_perception(skills: &[CompanionSkill]) -> i32 {
    PASSIVE_BASE
        + skills
            .iter()
            .find(|s| s.name == "Perception")
            .map_or(0, |s| s.bonus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_line_is_size_type_alignment() {
        assert_eq!(
            build_line("medium", "Beast", "Unaligned"),
            "Medium beast, unaligned"
        );
        assert_eq!(build_line("", "", "Neutral"), "neutral");
    }

    #[test]
    fn proficiency_follows_challenge() {
        assert_eq!(proficiency_for_challenge("1/4"), 2);
        assert_eq!(proficiency_for_challenge("5"), 3);
        assert_eq!(proficiency_for_challenge("30"), 9);
    }

    #[test]
    fn printed_speed_reads_variants() {
        let t = "40 ft., climb 30 ft., fly 60 ft.";
        assert_eq!(printed_speed(t, "climb"), 30);
        assert_eq!(printed_speed(t, "fly"), 60);
        assert_eq!(printed_speed(t, "swim"), 0);
    }

    #[test]
    fn skills_use_printed_bonus_else_modifier() {
        let block = CompanionBlock {
            skill_bonuses: printed_skills("Perception +3, Stealth -1"),
            ..CompanionBlock::default()
        };
        let all = skills(&block, &[10, 14, 10, 10, 10, 10]);
        assert_eq!(all.len(), 18);
        let bonus = |n: &str| all.iter().find(|s| s.name == n).map(|s| s.bonus);
        assert_eq!(bonus("Perception"), Some(3));
        assert_eq!(bonus("Stealth"), Some(-1));
        assert_eq!(bonus("Acrobatics"), Some(2));
        assert_eq!(passive_perception(&all), 13);
    }
}
