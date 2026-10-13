//! Deterministic extraction of the numbers a balance report compares.
//!
//! Everything here reads the element JSON only; no model output is trusted for any number.

use borealis_ai::retrieval::html_to_text;
use borealis_content::monster_types::Cr;
use borealis_content::rules::Rule;
use borealis_content::setter;
use borealis_content::{Element, MonsterData};

/// Ability names a `stat` rule can raise.
const ABILITY_STATS: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];
/// A damage dice expression counts when "damage" follows within this many characters.
const DAMAGE_WINDOW: usize = 80;
/// Phrases that mark an area of effect in spell text.
const AREA_MARKERS: [&str; 8] = [
    "-foot-radius",
    "-foot cone",
    "-foot cube",
    "-foot line",
    "-foot-long line",
    "-foot sphere",
    "-foot cylinder",
    "each creature",
];

/// Numbers measured from a spell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpellM {
    /// Spell level (0 = cantrip).
    pub level: u32,
    /// Hits an area rather than a single target.
    pub area: bool,
    /// Largest average damage expression in the base effect (0 = deals no dice damage).
    pub damage: f64,
}

/// Numbers measured from a feat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatM {
    /// Sum of ability score increases.
    pub asi: f64,
    /// Granted or selectable proficiencies.
    pub proficiencies: f64,
}

/// Numbers measured from a species.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeciesM {
    /// Granted racial traits.
    pub traits: f64,
    /// Base walking speed.
    pub speed: f64,
    /// 1 when the species grants darkvision.
    pub darkvision: f64,
    /// Sum of ability score increases.
    pub asi: f64,
}

/// Numbers measured from a monster stat block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonsterM {
    /// Challenge rating as an index into the CR table (the rank, not the value).
    pub cr_rank: usize,
    /// Printed CR text, for notes.
    pub cr_label: &'static str,
    /// Average hit points.
    pub hp: f64,
    /// Highest listed armor class.
    pub ac: f64,
    /// Best attack bonus (0 when the monster has no attack roll).
    pub attack: f64,
    /// Estimated damage per round of the best routine.
    pub dpr: f64,
}

/// Average of `n`d`die` plus a flat bonus.
fn dice_average(n: u32, die: u32, bonus: i32) -> f64 {
    f64::from(n) * f64::from(die + 1) / 2.0 + f64::from(bonus)
}

/// Every `NdM` (optionally followed by `+K`) in `text` as `(end offset, average)`.
pub fn dice_expressions(text: &str) -> Vec<(usize, f64)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && b[i - 1].is_ascii_alphanumeric()) {
            i += 1;
            continue;
        }
        let (n, after_n) = read_number(b, i);
        if b.get(after_n) != Some(&b'd') || !b.get(after_n + 1).is_some_and(u8::is_ascii_digit) {
            i = after_n.max(i + 1);
            continue;
        }
        let (die, mut end) = read_number(b, after_n + 1);
        let mut bonus = 0_i32;
        let mut j = end;
        while b.get(j) == Some(&b' ') {
            j += 1;
        }
        if matches!(b.get(j), Some(b'+' | b'-')) {
            let neg = b[j] == b'-';
            j += 1;
            while b.get(j) == Some(&b' ') {
                j += 1;
            }
            if b.get(j).is_some_and(u8::is_ascii_digit) {
                let (k, e) = read_number(b, j);
                bonus = i32::try_from(k).unwrap_or(0) * if neg { -1 } else { 1 };
                end = e;
            }
        }
        out.push((end, dice_average(n, die, bonus)));
        i = end;
    }
    out
}

fn read_number(b: &[u8], start: usize) -> (u32, usize) {
    let mut v: u32 = 0;
    let mut i = start;
    while let Some(d) = b.get(i).filter(|c| c.is_ascii_digit()) {
        v = v.saturating_mul(10).saturating_add(u32::from(d - b'0'));
        i += 1;
    }
    (v, i)
}

/// The largest average of a dice expression followed by "damage", ignoring the upcast section.
pub fn damage_average(html: &str) -> f64 {
    let text = html_to_text(html);
    let lower = text.to_lowercase();
    let cut = lower.find("at higher levels").unwrap_or(lower.len());
    let body = &lower[..cut];
    dice_expressions(body)
        .into_iter()
        .filter(|(end, _)| {
            let tail: String = body[*end..].chars().take(DAMAGE_WINDOW).collect();
            tail.contains("damage")
        })
        .map(|(_, avg)| avg)
        .fold(0.0, f64::max)
}

/// Measure a `Spell` element.
pub fn spell(el: &Element) -> SpellM {
    let level = setter::find(&el.setters, "level")
        .and_then(|s| s.value.trim().parse().ok())
        .unwrap_or(0);
    let lower = html_to_text(&el.description).to_lowercase();
    SpellM {
        level,
        area: AREA_MARKERS.iter().any(|m| lower.contains(m)),
        damage: damage_average(&el.description),
    }
}

fn stat_int(value: &str) -> Option<i32> {
    value.trim().parse().ok()
}

fn asi_total(el: &Element) -> f64 {
    el.rules
        .iter()
        .filter_map(|r| match r {
            Rule::Stat(s) if ABILITY_STATS.contains(&s.name.to_lowercase().as_str()) => {
                stat_int(&s.value)
            }
            _ => None,
        })
        .filter(|v| *v > 0)
        .map(f64::from)
        .sum()
}

/// Measure a `Feat` element.
pub fn feat(el: &Element) -> FeatM {
    let proficiencies: f64 = el
        .rules
        .iter()
        .map(|r| match r {
            Rule::Grant(g) if g.ty.as_deref() == Some("Proficiency") => 1.0,
            Rule::Select(s) if s.ty == "Proficiency" => f64::from(s.number.unwrap_or(1)),
            _ => 0.0,
        })
        .sum();
    FeatM {
        asi: asi_total(el),
        proficiencies,
    }
}

/// The enchantment bonus of an item: a `+N` in the name, else the largest small positive stat value.
pub fn item_bonus(el: &Element) -> f64 {
    let b = el.name.as_bytes();
    if let Some(p) = el.name.find('+') {
        if b.get(p + 1).is_some_and(u8::is_ascii_digit) {
            return f64::from(read_number(b, p + 1).0);
        }
    }
    el.rules
        .iter()
        .filter_map(|r| match r {
            Rule::Stat(s) => stat_int(&s.value),
            _ => None,
        })
        .filter(|v| (1..=10).contains(v))
        .map(f64::from)
        .fold(0.0, f64::max)
}

/// The `rarity` setter of an item, when present.
pub fn rarity(el: &Element) -> Option<String> {
    setter::find(&el.setters, "rarity").map(|s| s.value.trim().to_string())
}

/// Measure a race/species element.
pub fn species(el: &Element) -> SpeciesM {
    let mut traits = 0.0;
    let mut darkvision = 0.0;
    let mut speed = 30.0;
    for r in &el.rules {
        match r {
            Rule::Grant(g) if g.ty.as_deref() == Some("Racial Trait") => traits += 1.0,
            Rule::Grant(g) if g.id.to_uppercase().contains("DARKVISION") => darkvision = 1.0,
            Rule::Stat(s) if s.name.eq_ignore_ascii_case("innate speed") => {
                if let Some(v) = stat_int(&s.value) {
                    speed = f64::from(v);
                }
            }
            _ => {}
        }
    }
    SpeciesM {
        traits,
        speed,
        darkvision,
        asi: asi_total(el),
    }
}

fn count_in_text(text: &str) -> usize {
    const WORDS: [(&str, usize); 5] = [
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
    ];
    let lower = text.to_lowercase();
    WORDS
        .iter()
        .filter(|(w, _)| lower.split(|c: char| !c.is_alphabetic()).any(|t| t == *w))
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(1)
}

/// Estimated damage per round: the best save-based action, or the Multiattack routine.
fn damage_per_round(m: &MonsterData) -> f64 {
    let mut hits: Vec<f64> = m
        .actions
        .iter()
        .filter_map(|a| a.attack.as_ref())
        .map(|a| a.damage.iter().map(|d| f64::from(d.average)).sum())
        .collect();
    hits.sort_by(|a, b| b.total_cmp(a));
    let count = m
        .actions
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("multiattack"))
        .map_or(1, |a| count_in_text(&a.text));
    let routine: f64 = if hits.is_empty() {
        0.0
    } else {
        (0..count).map(|i| hits.get(i).unwrap_or(&hits[0])).sum()
    };
    let save_best = m
        .actions
        .iter()
        .filter_map(|a| a.save.as_ref())
        .map(|s| s.damage.iter().map(|d| f64::from(d.average)).sum::<f64>())
        .fold(0.0, f64::max);
    routine.max(save_best)
}

/// Measure a `Monster` element; `None` when its data does not parse.
pub fn monster(el: &Element) -> Option<MonsterM> {
    let m = MonsterData::from_element(el).ok()?;
    let text = m.cr.as_text();
    let (cr_rank, (label, _)) = Cr::XP_TABLE
        .iter()
        .enumerate()
        .find(|(_, (t, _))| *t == text)
        .map(|(i, e)| (i, *e))?;
    Some(MonsterM {
        cr_rank,
        cr_label: label,
        hp: f64::from(m.hp.average),
        ac: m.ac.iter().map(|a| f64::from(a.value)).fold(0.0, f64::max),
        attack: m
            .actions
            .iter()
            .filter_map(|a| a.attack.as_ref())
            .map(|a| f64::from(a.to_hit))
            .fold(0.0, f64::max),
        dpr: damage_per_round(&m),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_average_handles_bonus_and_ignores_words() {
        let v = dice_expressions("deals 2d8 + 4 and 10d10 damage; add 1d4");
        let avgs: Vec<f64> = v.iter().map(|x| x.1).collect();
        assert_eq!(avgs, vec![13.0, 55.0, 2.5]);
    }

    #[test]
    fn damage_average_skips_the_upcast_section() {
        let html =
            "<p>It takes 3d6 fire damage.</p><p><strong>At Higher Levels.</strong> 9d6 damage.</p>";
        assert!((damage_average(html) - 10.5).abs() < f64::EPSILON);
    }
}
