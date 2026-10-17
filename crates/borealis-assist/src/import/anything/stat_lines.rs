//! Whether a text prints a creature's stat block (read by ingestion, to look again at the
//! pages of creatures whose stat block is missing, and by generation, which never invents one).

use crate::import::chunk::norm_words;

/// Different stat-block lines (armor class, hit points, speed, challenge, ability header) a
/// passage needs before it is a stat block.
const MIN_STAT_LINES: usize = 2;
/// Ability header abbreviations, as printed.
const ABILITIES: [&str; 6] = ["STR", "DEX", "CON", "INT", "WIS", "CHA"];
/// Of them, fewest that make an ability header.
const MIN_ABILITY_HEADS: usize = 4;

fn is_number(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// `text` prints a stat block: at least [`MIN_STAT_LINES`] of an armor class, hit points,
/// a speed in feet, a challenge rating, and an ability header (STR DEX CON ...).
pub(crate) fn has_stat_block(text: &str) -> bool {
    let words = norm_words(text);
    let t: Vec<&str> = words.split(' ').collect();
    let at = |i: usize| t.get(i).copied().unwrap_or("");
    let mut kinds = [false; 5];
    for (i, word) in t.iter().enumerate() {
        match *word {
            "armor" | "armour" if at(i + 1) == "class" && is_number(at(i + 2)) => kinds[0] = true,
            "ac" if is_number(at(i + 1)) => kinds[0] = true,
            "hit" if at(i + 1) == "points" && is_number(at(i + 2)) => kinds[1] = true,
            "hp" if is_number(at(i + 1)) => kinds[1] = true,
            "speed" if is_number(at(i + 1)) && matches!(at(i + 2), "ft" | "feet") => {
                kinds[2] = true;
            }
            "challenge" | "cr" if is_number(at(i + 1)) => kinds[3] = true,
            _ => {}
        }
    }
    let heads: std::collections::HashSet<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| ABILITIES.contains(w))
        .collect();
    kinds[4] = heads.len() >= MIN_ABILITY_HEADS;
    kinds.iter().filter(|k| **k).count() >= MIN_STAT_LINES
}
