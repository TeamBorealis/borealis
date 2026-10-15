//! Lead-in paragraphs: the rules prose under a heading that introduces a list of entries
//! planned on their own ("Each charm below can be bought once ..."). With no planned element on
//! that heading, the prose lands in no description; it becomes a text-only entry named after the
//! heading, so the rule that says how the entries are used is kept.

use super::types::ImportPlan;

/// Fewest entries planned under a heading for its prose to be their lead-in.
const MIN_LISTED: usize = 2;
/// Fewest words of prose that make a lead-in (a caption or a stray line is none).
const MIN_LEAD_WORDS: usize = 12;

/// Marks a PDF glues to the end of a heading title.
const TITLE_MARKS: [char; 3] = ['!', '*', '†'];

fn norm(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Level and title of a markdown heading line.
fn heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    let title = t[n..].trim();
    (n > 0 && t[n..].starts_with(' ') && !title.is_empty()).then_some((n, title))
}

/// 0-based first lines of the planned passages, sorted.
fn starts(plan: &ImportPlan) -> Vec<usize> {
    let mut s: Vec<usize> = plan
        .elements
        .iter()
        .filter_map(|e| e.excerpt.as_ref())
        .map(|x| x.start_line.saturating_sub(1))
        .collect();
    s.sort_unstable();
    s.dedup();
    s
}

/// 0-based `[start, end)` each planned passage covers: up to its end or the next planned
/// passage, whichever comes first (a passage planned inside it carries its own text).
fn covered(plan: &ImportPlan, starts: &[usize]) -> Vec<(usize, usize)> {
    plan.elements
        .iter()
        .filter_map(|e| e.excerpt.as_ref())
        .map(|x| {
            let s = x.start_line.saturating_sub(1);
            let next = starts
                .iter()
                .copied()
                .find(|l| *l > s)
                .unwrap_or(usize::MAX);
            (s, x.end_line.min(next).max(s + 1))
        })
        .collect()
}

/// Whether the uncovered lines `body` are a paragraph of prose: enough words, a sentence, and
/// no table rows.
fn is_prose(body: &[&str]) -> bool {
    let words: usize = body.iter().map(|l| l.split_whitespace().count()).sum();
    words >= MIN_LEAD_WORDS
        && body.iter().any(|l| l.trim_end().ends_with(['.', '!', '?']))
        && !body.iter().any(|l| l.contains(" | "))
}

/// Titles of the headings whose own prose introduces at least [`MIN_LISTED`] entries planned
/// under them while no planned passage carries that prose (see the module docs). Headings a
/// group is already named after are left out.
pub(crate) fn lead_ins(plan: &ImportPlan, source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let starts = starts(plan);
    let ranges = covered(plan, &starts);
    let heads: Vec<(usize, usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| heading(l).map(|(n, t)| (i, n, t)))
        .collect();
    let named = |t: &str| {
        let key = norm(t);
        plan.groups.iter().any(|g| norm(&g.name) == key)
            || plan.elements.iter().any(|e| norm(&e.name) == key)
    };
    let mut out: Vec<String> = Vec::new();
    for (k, &(h, level, printed)) in heads.iter().enumerate() {
        // A mark glued to the title by the PDF ("Callings!") is no part of its name.
        let title = printed.trim_end_matches(TITLE_MARKS).trim_end();
        if title.is_empty() || starts.contains(&h) || named(title) || out.iter().any(|o| o == title)
        {
            continue;
        }
        let end = heads[k + 1..]
            .iter()
            .find(|(_, n, _)| *n <= level)
            .map_or(lines.len(), |(i, _, _)| *i);
        let inner: Vec<usize> = starts
            .iter()
            .copied()
            .filter(|s| *s > h && *s < end)
            .collect();
        if inner.len() < MIN_LISTED {
            continue;
        }
        let next_heading = heads.get(k + 1).map_or(end, |(i, _, _)| *i);
        let stop = inner[0].min(next_heading);
        let body: Vec<&str> = (h + 1..stop)
            .filter(|i| !ranges.iter().any(|(s, e)| i >= s && i < e))
            .map(|i| lines[i])
            .filter(|l| !l.trim().is_empty())
            .collect();
        if is_prose(&body) {
            out.push(title.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::types::{Edition, PlannedElement, Role, SourceExcerpt};

    fn planned(name: &str, start: usize, end: usize) -> PlannedElement {
        PlannedElement {
            id: format!("ID_{}", name.to_uppercase().replace(' ', "_")),
            ty: "Rules Glossary Entry".into(),
            name: name.into(),
            role: Role::Text,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: Some(SourceExcerpt {
                start_line: start,
                end_line: end,
                snippet: String::new(),
            }),
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        }
    }

    fn plan(elements: Vec<PlannedElement>) -> ImportPlan {
        ImportPlan {
            edition: Edition::Y2014,
            groups: Vec::new(),
            elements,
            notes: Vec::new(),
        }
    }

    const SOURCE: &str = "# Hearth Charms\n\
\n\
A hearth charm is bought once per season; you may keep up to three of the charms below at any time.\n\
\n\
## Ember Knot\n\
\n\
You never feel cold.\n\
\n\
## Ash Ring\n\
\n\
Smoke does not blind you.\n\
\n\
# Credits\n\
\n\
Written by nobody in particular for testing only, with thanks to everyone here.\n";

    #[test]
    fn a_rules_paragraph_introducing_planned_entries_is_a_lead_in() {
        // Arrange: the two charms are planned (lines 5-8 and 9-12, 1-based), the heading is not.
        let p = plan(vec![
            planned("Ember Knot", 5, 8),
            planned("Ash Ring", 9, 12),
        ]);

        // Act
        let leads = lead_ins(&p, SOURCE);

        // Assert: the prose under "Credits" introduces nothing planned.
        assert_eq!(leads, vec!["Hearth Charms".to_string()]);
    }

    #[test]
    fn a_heading_planned_itself_or_carrying_its_prose_is_no_lead_in() {
        let own = plan(vec![
            planned("Hearth Charms", 1, 4),
            planned("Ember Knot", 5, 8),
            planned("Ash Ring", 9, 12),
        ]);
        assert!(lead_ins(&own, SOURCE).is_empty());
        // One planned entry under it is no list.
        let one = plan(vec![planned("Ember Knot", 5, 12)]);
        assert!(lead_ins(&one, SOURCE).is_empty());
    }
}
