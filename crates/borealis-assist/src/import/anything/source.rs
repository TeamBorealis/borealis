//! The source text each element is generated from: its own located section, or, when no
//! section was found, every passage that mentions it. Nothing is cut silently: what an element
//! without a section was given is stated in the import report.

use super::types::{PlannedElement, Role};
use crate::import::chunk::{markdown_heading, norm_words};

/// Lines of context kept around each mention of an element that has no section.
const MENTION_CONTEXT: usize = 12;
/// An element without a section and without any mention gets the whole source when it is at
/// most this long, else its first this many characters (reported).
pub(crate) const WHOLE_SOURCE_LIMIT: usize = 120_000;

/// 0-based line ranges (start, end exclusive) around the lines mentioning `name`, merged.
fn mention_ranges(source: &str, name: &str) -> Vec<(usize, usize)> {
    let key = format!(" {} ", norm_words(name));
    if key.trim().is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !format!(" {} ", norm_words(l)).contains(&key) {
            continue;
        }
        let (s, e) = (
            i.saturating_sub(MENTION_CONTEXT),
            (i + MENTION_CONTEXT + 1).min(lines.len()),
        );
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// What an element without a located section is generated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unlocated {
    /// The passages mentioning it (count).
    Mentions(usize),
    /// The whole source.
    Whole(usize),
    /// The first `.0` of `.1` characters (no mention, source too long).
    Start(usize, usize),
}

/// The source for an element with no section of its own, and what it is.
pub(crate) fn unlocated_source(source: &str, name: &str) -> (String, Unlocated) {
    let n = source.chars().count();
    let ranges = mention_ranges(source, name);
    if n > WHOLE_SOURCE_LIMIT && !ranges.is_empty() {
        let lines: Vec<&str> = source.lines().collect();
        let text = ranges
            .iter()
            .map(|(s, e)| lines[*s..*e].join("\n"))
            .collect::<Vec<_>>()
            .join("\n...\n");
        return (text, Unlocated::Mentions(ranges.len()));
    }
    if n <= WHOLE_SOURCE_LIMIT {
        return (source.to_string(), Unlocated::Whole(n));
    }
    (
        source.chars().take(WHOLE_SOURCE_LIMIT).collect(),
        Unlocated::Start(WHOLE_SOURCE_LIMIT, n),
    )
}

/// The report line for an element generated without a section of its own.
pub(crate) fn unlocated_message(source: &str, p: &PlannedElement) -> String {
    let sent = match unlocated_source(source, &p.name).1 {
        Unlocated::Mentions(k) => format!("the {k} passage(s) that mention it"),
        Unlocated::Whole(n) => format!("all {n} characters"),
        Unlocated::Start(k, n) => {
            format!("its first {k} of {n} characters (the name appears nowhere)")
        }
    };
    format!(
        "no section for \"{}\" was found in the source; it was generated from {sent}",
        p.name
    )
}

/// The located section of `p` (`None` without one), whole.
pub(crate) fn section_text(source: &str, p: &PlannedElement) -> Option<String> {
    let ex = p.excerpt.as_ref()?;
    Some(
        source
            .lines()
            .skip(ex.start_line.saturating_sub(1))
            .take(ex.end_line + 1 - ex.start_line.min(ex.end_line + 1))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Fewest lines sent for a located element (a one-line excerpt still gets its context).
const MIN_WINDOW_LINES: usize = 12;
/// Fewest level rows ("1st | ...") that make a class progression table.
const MIN_TABLE_ROWS: usize = 5;
/// Lines above a progression table kept as its title and header rows.
const TABLE_HEAD_LINES: usize = 4;
/// Most non-row lines inside a progression table (blank lines, a wrapped cell).
const TABLE_GAP_LINES: usize = 2;
/// Headings that belong to a class's hit point and proficiency block.
const PROFICIENCY_HEADINGS: [&str; 4] = ["hit points", "proficiencies", "equipment", "hit dice"];
/// Most lines of a hit point and proficiency block after its "Hit Dice" line.
const PROFICIENCY_MAX_LINES: usize = 60;

/// The source one element is generated from on its own: its located section, whole; a class
/// root also gets its progression table and its hit point and proficiency block when they lie
/// outside that section; an element without a section gets the passages mentioning it.
pub(crate) fn element_window(source: &str, planned: &PlannedElement) -> String {
    let Some(ex) = planned.excerpt.as_ref() else {
        return unlocated_source(source, &planned.name).0;
    };
    let lines: Vec<&str> = source.lines().collect();
    let start = ex.start_line.saturating_sub(1).min(lines.len());
    let end = ex.end_line.max(ex.start_line).min(lines.len());
    if planned.role != Role::Class {
        let short = (end - start).max(MIN_WINDOW_LINES);
        return lines[start..(start + short).min(lines.len())].join("\n");
    }
    let mut blocks = vec![(start, end)];
    blocks.extend(progression_table(&lines, start));
    blocks.extend(proficiency_block(&lines, start));
    blocks.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in blocks {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
        .iter()
        .map(|(s, e)| lines[*s..*e].join("\n"))
        .collect::<Vec<_>>()
        .join("\n...\n")
}

/// A class table row: its first cell is a level ("1st", "12th", "3").
fn level_row(line: &str) -> bool {
    let Some((first, _)) = line.trim().split_once(" | ") else {
        return false;
    };
    let digits = first.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    first.len() - digits.len() <= 2 && digits.parse::<u32>().is_ok_and(|n| (1..=20).contains(&n))
}

/// The progression table nearest to line `near` (0-based line range with its header lines).
fn progression_table(lines: &[&str], near: usize) -> Option<(usize, usize)> {
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !level_row(lines[i]) {
            i += 1;
            continue;
        }
        let (first, mut last, mut rows) = (i, i, 1);
        let mut j = i + 1;
        while j < lines.len() && j - last <= TABLE_GAP_LINES + 1 {
            if level_row(lines[j]) {
                (last, rows) = (j, rows + 1);
            }
            j += 1;
        }
        runs.push((first, last + 1, rows));
        i = last + 1;
    }
    runs.into_iter()
        .filter(|(_, _, rows)| *rows >= MIN_TABLE_ROWS)
        .min_by_key(|(s, _, _)| s.abs_diff(near))
        .map(|(s, e, _)| (s.saturating_sub(TABLE_HEAD_LINES), e))
}

/// The hit point and proficiency block nearest to line `near`: from the heading above its "Hit
/// Dice" line to the first heading that is not one of [`PROFICIENCY_HEADINGS`] (at most
/// [`PROFICIENCY_MAX_LINES`] lines on).
fn proficiency_block(lines: &[&str], near: usize) -> Option<(usize, usize)> {
    let heading = |l: &str| markdown_heading(l).map(|(_, t)| t.to_lowercase());
    let at = (0..lines.len())
        .filter(|&i| lines[i].to_lowercase().contains("hit dice"))
        .min_by_key(|i| i.abs_diff(near))?;
    let start = (at.saturating_sub(TABLE_HEAD_LINES)..=at)
        .find(|&i| heading(lines[i]).is_some())
        .unwrap_or(at);
    let limit = (at + PROFICIENCY_MAX_LINES).min(lines.len());
    let end = (at + 1..limit)
        .find(|&i| {
            heading(lines[i]).is_some_and(|t| !PROFICIENCY_HEADINGS.iter().any(|h| t.contains(h)))
        })
        .unwrap_or(limit);
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::types::SourceExcerpt;

    fn root(role: Role, start_line: usize, end_line: usize) -> PlannedElement {
        PlannedElement {
            id: "ID_T_ROOT".into(),
            ty: "Class".into(),
            name: "Lamplighter".into(),
            role,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: Some(SourceExcerpt {
                start_line,
                end_line,
                snippet: String::new(),
            }),
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        }
    }

    #[test]
    fn a_class_root_gets_its_whole_section_table_and_proficiencies() {
        let mut src = String::from("# Lamplighter\n\nKeepers of the night roads.\n\n");
        src.push_str("## The Lamplighter\n\nLevel | Proficiency Bonus | Features\n");
        for l in 1..=20 {
            src.push_str(&format!("{l}th | +2 | Feature {l}\n"));
        }
        src.push_str(&"\nA long tale of the road and its lamps.\n".repeat(400));
        src.push_str("\n## Class Features\n\n### Hit Points\n\nHit Dice: 1d8 per level\n");
        src.push_str("\n### Proficiencies\n\nSaving Throws: Wisdom, Charisma\n");
        src.push_str("\n### Wick Sense\n\nYou smell oil.\n");
        let p = root(Role::Class, 1, 3);
        let w = element_window(&src, &p);
        assert!(w.starts_with("# Lamplighter"), "{w}");
        assert!(w.contains("20th | +2 | Feature 20") && w.contains("Level | Proficiency"));
        assert!(
            w.contains("Hit Dice: 1d8") && w.contains("Saving Throws: Wisdom"),
            "{w}"
        );
        assert!(
            !w.contains("Wick Sense") && !w.contains("tale of the road"),
            "{w}"
        );
        // A subclass root or a feature keeps its own section, whole.
        let lines = src.lines().count();
        let w = element_window(&src, &root(Role::Subclass, 1, lines));
        assert_eq!(w.chars().count(), src.trim_end().chars().count());
    }

    #[test]
    fn long_sources_send_the_mentions_of_an_unlocated_element() {
        let filler = "Plain words about the weather and the road.\n".repeat(4000);
        let src = format!("{filler}The Moth Lantern hangs by the door.\n{filler}");
        let (text, what) = unlocated_source(&src, "Moth Lantern");
        assert_eq!(what, Unlocated::Mentions(1));
        assert!(text.contains("The Moth Lantern hangs"));
        assert!(text.lines().count() <= 2 * MENTION_CONTEXT + 1);
        let (text, what) = unlocated_source("short source", "Nowhere");
        assert_eq!(
            (text.as_str(), what),
            ("short source", Unlocated::Whole(12))
        );
    }
}
