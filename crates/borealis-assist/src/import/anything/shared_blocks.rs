//! Several stat blocks in one passage: a rider and its mount printed in one shared block, or
//! variants printed one after the other. Each stat block opens at its armor class line under
//! its own sub-heading (a heading, a bold line or a short name line, above the size and type
//! line); a creature is written from the sub-heading that names it, so traits printed under the
//! mount's sub-heading never land on the rider.

use crate::import::chunk::norm_words;

/// Lines looked back from an armor class line for the stat block's name.
const TITLE_REACH: usize = 4;
/// Most words of a plain line that names a stat block.
const TITLE_WORDS: usize = 6;
/// Creature sizes, which open a stat block's size and type line.
const SIZES: [&str; 6] = ["tiny", "small", "medium", "large", "huge", "gargantuan"];

/// The line opens a stat block: "Armor Class 15", "AC 15".
fn armor_line(line: &str) -> bool {
    let w = norm_words(line);
    let mut it = w.split(' ');
    let first = it.next().unwrap_or("");
    let second = it.next().unwrap_or("");
    let number = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
    match first {
        "armor" | "armour" => second == "class" && it.next().is_some_and(number),
        "ac" => number(second),
        _ => false,
    }
}

/// The size and type line under a stat block's name ("*Large beast, unaligned*").
fn size_line(line: &str) -> bool {
    let w = norm_words(line);
    w.split(' ').next().is_some_and(|f| SIZES.contains(&f)) && line.contains(',')
}

/// The text of a line that names a stat block: a heading, a bold line, or a short line that is
/// no sentence.
fn title_of(line: &str) -> Option<String> {
    let t = line.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(h) = t.strip_prefix('#') {
        return Some(h.trim_start_matches('#').trim().to_string());
    }
    let bare = t.trim_matches(|c| c == '*' || c == '_').trim();
    let short = bare.split_whitespace().count() <= TITLE_WORDS;
    (short && !bare.ends_with(['.', ':', ';', ',']) && !bare.contains(" | "))
        .then(|| bare.to_string())
}

/// The name line (index, name) of the stat block opening at `armor`.
fn title_above(lines: &[&str], armor: usize) -> Option<(usize, String)> {
    let mut i = armor;
    let mut seen_size = false;
    for _ in 0..TITLE_REACH {
        i = i.checked_sub(1)?;
        let l = lines[i];
        if l.trim().is_empty() {
            continue;
        }
        if !seen_size && size_line(l) {
            seen_size = true;
            continue;
        }
        return title_of(l).map(|t| (i, t));
    }
    None
}

/// How well a sub-heading names a creature: shared words over all words (1 is the same name).
fn likeness(title: &str, name: &str) -> f32 {
    let a: std::collections::BTreeSet<String> =
        norm_words(title).split(' ').map(str::to_string).collect();
    let b: std::collections::BTreeSet<String> =
        norm_words(name).split(' ').map(str::to_string).collect();
    let shared = a.intersection(&b).filter(|w| !w.is_empty()).count();
    shared as f32 / a.union(&b).count().max(1) as f32
}

/// `text` cut to the stat block of `name` when it prints two or more named stat blocks: the
/// lines before the first one, then the one whose sub-heading names `name` best (a unique best
/// with a word in common). `None` when the passage prints one stat block, or no sub-heading
/// names the creature.
pub(super) fn own_stat_block(text: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks: Vec<(usize, String)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !armor_line(l) {
            continue;
        }
        let (at, title) = title_above(&lines, i)?;
        if blocks.last().is_some_and(|(prev, _)| *prev >= at) {
            return None;
        }
        blocks.push((at, title));
    }
    if blocks.len() < 2 {
        return None;
    }
    let scores: Vec<f32> = blocks.iter().map(|(_, t)| likeness(t, name)).collect();
    let best = scores.iter().copied().fold(0.0, f32::max);
    let winners: Vec<usize> = (0..blocks.len()).filter(|&k| scores[k] == best).collect();
    let [k] = winners.as_slice() else {
        return None;
    };
    if best <= 0.0 {
        return None;
    }
    let start = blocks[*k].0;
    let end = blocks.get(k + 1).map_or(lines.len(), |(at, _)| *at);
    let mut out: Vec<&str> = lines[..blocks[0].0].to_vec();
    out.extend(&lines[start..end]);
    Some(out.join("\n").trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARED: &str = "### Reed Rider\n\nReed riders patrol the marsh on great herons.\n\n**Reed Rider**\n*Small humanoid, neutral*\nArmor Class 14\nHit Points 11 (2d6 + 4)\nSpeed 25 ft.\n**Sure Seat.** The rider can't be knocked from its mount.\n\n**Great Heron**\n*Large beast, unaligned*\nArmor Class 12\nHit Points 19 (3d10 + 3)\nSpeed 10 ft., fly 60 ft.\n**Keen Sight.** The heron has advantage on checks that rely on sight.\n**Dive.** If the heron flies 20 feet straight toward a target, it deals an extra 1d6 damage.";

    #[test]
    fn each_creature_of_a_shared_block_gets_the_traits_under_its_own_sub_heading() {
        let heron = own_stat_block(SHARED, "Great Heron").unwrap();
        assert!(
            heron.starts_with("### Reed Rider\n\nReed riders patrol"),
            "{heron}"
        );
        assert!(
            heron.contains("Keen Sight") && heron.contains("Dive."),
            "{heron}"
        );
        assert!(
            !heron.contains("Sure Seat") && !heron.contains("Hit Points 11"),
            "{heron}"
        );
        let rider = own_stat_block(SHARED, "Reed Rider").unwrap();
        assert!(
            rider.contains("Sure Seat") && rider.contains("Hit Points 11"),
            "{rider}"
        );
        assert!(
            !rider.contains("Keen Sight") && !rider.contains("Great Heron"),
            "{rider}"
        );
    }

    #[test]
    fn one_stat_block_or_no_naming_sub_heading_leaves_the_passage_whole() {
        let one =
            "### Great Heron\n*Large beast, unaligned*\nArmor Class 12\nHit Points 19 (3d10 + 3)";
        assert_eq!(own_stat_block(one, "Great Heron"), None);
        assert_eq!(own_stat_block(SHARED, "Marsh Lantern"), None);
        // Variants printed one after the other under headings.
        let variants = "## Bog Hag\n*Medium fey, evil*\nArmor Class 15\nHit Points 30\n\n## Elder Bog Hag\n*Medium fey, evil*\nArmor Class 17\nHit Points 80";
        let elder = own_stat_block(variants, "Elder Bog Hag").unwrap();
        assert!(elder.starts_with("## Elder Bog Hag") && elder.contains("Hit Points 80"));
        let hag = own_stat_block(variants, "Bog Hag").unwrap();
        assert!(
            hag.contains("Hit Points 30") && !hag.contains("Hit Points 80"),
            "{hag}"
        );
    }
}
