//! The source text each element is generated from: its own located section, or, when no
//! section was found, every passage that mentions it. Nothing is cut silently: what an element
//! without a section was given is stated in the import report.

use super::types::{PlannedElement, Role};
use crate::import::chunk::norm_words;

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
/// Most characters sent for a class or subclass root, counted from its header.
const ROOT_SOURCE_CHARS: usize = 9000;

/// The source one element is generated from on its own: its located section (a class or
/// subclass root from its header on), else the passages mentioning it.
pub(crate) fn element_window(source: &str, planned: &PlannedElement) -> String {
    let Some(ex) = planned.excerpt.as_ref() else {
        return unlocated_source(source, &planned.name).0;
    };
    let from_header = source.lines().skip(ex.start_line.saturating_sub(1));
    if matches!(planned.role, Role::Class | Role::Subclass) {
        return from_header
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .take(ROOT_SOURCE_CHARS)
            .collect();
    }
    from_header
        .take((ex.end_line - ex.start_line + 1).max(MIN_WINDOW_LINES))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

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
