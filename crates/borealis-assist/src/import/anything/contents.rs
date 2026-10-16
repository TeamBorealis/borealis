//! Contents pages: lines with a dot leader and a page number ("Spells ....... pg.5") are page
//! furniture, never a plan entry; a section that is a contents page is not imported (it is
//! navigation); the untitled text printed right after one is the section the contents names
//! first when nothing else prints that name, never part of the contents.

use super::option_rows::{is_heading, key_words};
use super::types::{PlannedElement, Role, SourceExcerpt};

/// Fewest leader lines that make a contents page.
const MIN_RUN: usize = 3;
/// Fewest dots of a dot leader.
const MIN_DOTS: usize = 3;
/// Most digits of a page number.
const MAX_PAGE_DIGITS: usize = 4;
/// Fewest words of untitled text after a contents page that make a section.
const MIN_UNTITLED_WORDS: usize = 30;
/// Characters of an excerpt's snippet.
const SNIPPET_CHARS: usize = 80;
/// Section titles that are a contents page by name.
const CONTENTS_TITLES: [&str; 3] = ["contents", "table of contents", "index"];

/// The title of a contents line: "Title ....... pg.12", "Title . . . . 12", "Title… 12".
pub(crate) fn leader_title(line: &str) -> Option<String> {
    let t = line.trim();
    let digits = t.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > MAX_PAGE_DIGITS {
        return None;
    }
    let before = t[..t.len() - digits].trim_end();
    let lower = before.to_lowercase();
    let cut = ["pg.", "pg", "p.", "page"]
        .iter()
        .find(|m| lower.ends_with(*m))
        .map_or(before.len(), |m| before.len() - m.len());
    let leader = before[..cut].trim_end();
    let title = leader.trim_end_matches(['.', '…', ' ', '\t']);
    let dots = leader[title.len()..].chars().filter(|c| *c == '.').count()
        + 3 * leader[title.len()..].matches('…').count();
    (dots >= MIN_DOTS && !title.trim().is_empty()).then(|| title.trim().to_string())
}

/// 0-based `[start, end)` line ranges of contents runs: at least [`MIN_RUN`] leader lines,
/// blank lines allowed between them.
pub(crate) fn runs(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < lines.len() {
        if leader_title(lines[k]).is_none() {
            k += 1;
            continue;
        }
        let start = k;
        let mut end = k + 1;
        let mut count = 1;
        let mut j = k + 1;
        while j < lines.len() {
            if leader_title(lines[j]).is_some() {
                count += 1;
                end = j + 1;
            } else if !lines[j].trim().is_empty() {
                break;
            }
            j += 1;
        }
        if count >= MIN_RUN {
            out.push((start, end));
        }
        k = end.max(k + 1);
    }
    out
}

/// The section `name` is a contents page: its heading's first lines are a contents run, or
/// it is called "Contents" and the source has one.
pub(crate) fn is_contents(source: &str, name: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let runs = runs(&lines);
    if runs.is_empty() {
        return false;
    }
    let key = key_words(name);
    if CONTENTS_TITLES.contains(&key.join(" ").as_str()) {
        return true;
    }
    lines.iter().enumerate().any(|(k, l)| {
        is_heading(l)
            && key_words(l) == key
            && (k + 1..lines.len())
                .find(|&j| !lines[j].trim().is_empty())
                .is_some_and(|j| runs.iter().any(|(s, _)| *s == j))
    })
}

/// The source prints `name` only as a contents line: no heading, title line or run-in names
/// it outside the contents runs.
pub(crate) fn listed_only(source: &str, name: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let runs = runs(&lines);
    let key = key_words(name);
    let inside = |k: usize| runs.iter().any(|(s, e)| k >= *s && k < *e);
    let listed = (0..lines.len())
        .any(|k| inside(k) && leader_title(lines[k]).is_some_and(|t| key_words(&t) == key));
    listed
        && !(0..lines.len()).any(|k| {
            let w = key_words(lines[k].trim_start_matches('#'));
            !inside(k) && w.len() >= key.len() && w[..key.len()] == *key
        })
}

/// The untitled text after each contents run, 0-based `[start, end)` (to the next heading),
/// with the titles the run lists.
fn untitled_after(lines: &[&str]) -> Vec<((usize, usize), Vec<String>)> {
    runs(lines)
        .into_iter()
        .filter_map(|(s, e)| {
            let start = (e..lines.len()).find(|&j| !lines[j].trim().is_empty())?;
            if is_heading(lines[start]) {
                return None;
            }
            let end = (start..lines.len())
                .find(|&j| is_heading(lines[j]))
                .unwrap_or(lines.len());
            let words: usize = lines[start..end]
                .iter()
                .map(|l| l.split_whitespace().count())
                .sum();
            let titles = lines[s..e].iter().filter_map(|l| leader_title(l)).collect();
            (words >= MIN_UNTITLED_WORDS).then_some(((start, end), titles))
        })
        .collect()
}

/// Text entries the plan could not locate that a contents page names and nothing else prints:
/// the one the untitled text after that page is (when exactly one such entry is listed there)
/// gets that text; the others are dropped (they have no section). Returns the plan notes and
/// the ids dropped.
pub(crate) fn place_listed(
    elements: &mut Vec<PlannedElement>,
    source: &str,
) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut notes = Vec::new();
    let unlocated = |e: &PlannedElement| e.role == Role::Text && e.excerpt.is_none();
    for ((start, end), titles) in untitled_after(&lines) {
        let listed: Vec<usize> = (0..elements.len())
            .filter(|&k| unlocated(&elements[k]) && listed_only(source, &elements[k].name))
            .filter(|&k| {
                titles
                    .iter()
                    .any(|t| key_words(t) == key_words(&elements[k].name))
            })
            .collect();
        if let [k] = listed.as_slice() {
            let e = &mut elements[*k];
            e.excerpt = Some(SourceExcerpt {
                start_line: start + 1,
                end_line: end,
                snippet: lines[start].chars().take(SNIPPET_CHARS).collect(),
            });
            notes.push(format!(
                "{}: the untitled text after the contents page, which lists it",
                e.name
            ));
        }
    }
    let dropped: Vec<String> = elements
        .iter()
        .filter(|e| unlocated(e) && listed_only(source, &e.name))
        .map(|e| e.id.clone())
        .collect();
    for e in elements.iter().filter(|e| dropped.contains(&e.id)) {
        notes.push(format!(
            "{}: listed only on a contents page, no section of its own; not imported",
            e.name
        ));
    }
    elements.retain(|e| !dropped.contains(&e.id));
    (notes, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "# Book\n\n## Table of Contents\n\nForeword..........pg.2\n\
Spells . . . . . . 5\nAppendix… 9\nDear reader, this book was written over many long winters by a tired author who wanted \
to share every spell he ever learned with the world, and he hopes you enjoy reading it as much as he enjoyed writing it.\n\n\
## Spells\n\nFirst spell.\n\n## Appendix\n\nEnd.\n";

    #[test]
    fn leader_lines_are_contents_lines() {
        assert_eq!(
            leader_title("Foreword..........pg.2").as_deref(),
            Some("Foreword")
        );
        assert_eq!(
            leader_title("Spells . . . . . . 5").as_deref(),
            Some("Spells")
        );
        assert_eq!(leader_title("Appendix… 9").as_deref(), Some("Appendix"));
        assert_eq!(leader_title("Deal 2d6 damage. Range 30"), None);
        assert_eq!(leader_title("1st | 2 | 3"), None);
        let lines: Vec<&str> = SRC.lines().collect();
        assert_eq!(runs(&lines), vec![(4, 7)]);
    }

    #[test]
    fn a_contents_section_and_names_listed_only_there_are_told_apart() {
        assert!(is_contents(SRC, "Table of Contents"));
        assert!(!is_contents(SRC, "Spells"));
        assert!(listed_only(SRC, "Foreword"));
        assert!(!listed_only(SRC, "Spells"));
    }

    fn text(id: &str, name: &str) -> PlannedElement {
        PlannedElement {
            id: id.into(),
            ty: "Rules Glossary Entry".into(),
            name: name.into(),
            role: Role::Text,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: None,
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        }
    }

    #[test]
    fn the_untitled_text_after_a_contents_page_is_the_entry_it_lists() {
        let mut elements = vec![text("a", "Foreword"), text("b", "Spells")];
        let (notes, dropped) = place_listed(&mut elements, SRC);
        assert!(dropped.is_empty());
        let ex = elements[0].excerpt.as_ref().expect("placed");
        assert_eq!((ex.start_line, ex.end_line), (8, 9));
        assert_eq!(notes.len(), 1);
        // Two unlocated entries listed only there: neither takes the text, both are dropped.
        let mut two = vec![text("a", "Foreword"), text("c", "Appendix Notes")];
        let src = SRC.replace("Appendix… 9", "Appendix Notes… 9");
        let src = src.replace("## Appendix", "## Closing");
        let (_, dropped) = place_listed(&mut two, &src);
        assert_eq!(dropped, vec!["a".to_string(), "c".to_string()]);
        assert!(two.is_empty());
    }
}
