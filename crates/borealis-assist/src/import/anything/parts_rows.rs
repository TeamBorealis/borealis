//! Table rows a text-only entry prints for elements imported on their own (a d100 table of
//! items, a table of properties): the entry keeps each such row's roll and name cells, and the
//! row's other cells (the element's own text) stay with that element, so the text is not
//! imported twice.

use std::borrow::Cow;
use std::collections::HashSet;

use super::super::types::{ImportPlan, Role};

/// Cells (from the left) that may hold a row's name: a roll or number column, then the name.
const NAME_CELLS: usize = 2;

fn norm(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn cells(line: &str) -> Option<Vec<&str>> {
    if !line.contains(" | ") {
        return None;
    }
    let t = line.trim().trim_start_matches('|').trim_end_matches('|');
    Some(t.split('|').map(str::trim).collect())
}

/// Index of the cell naming an element of `names`, when cells follow it.
fn name_cell(cells: &[&str], names: &HashSet<String>) -> Option<usize> {
    (0..NAME_CELLS.min(cells.len()))
        .find(|&k| names.contains(&norm(cells[k])))
        .filter(|&k| cells.len() > k + 1)
}

/// `lines` of the text-only entry `own_id` (printed from the 0-based line ranges `blocks`) with
/// the rows of elements located on a passage of their own outside it cut after their name
/// cell, and the header row over them cut to the same columns. An element located on its row
/// here keeps the row whole (the row is its only printing).
pub(crate) fn rows_of_others<'a>(
    lines: &[&'a str],
    own_id: &str,
    blocks: &[(usize, usize)],
    plan: &ImportPlan,
) -> Vec<Cow<'a, str>> {
    let elsewhere = |e: &&super::super::types::PlannedElement| {
        e.excerpt.as_ref().is_some_and(|x| {
            let at = x.start_line.saturating_sub(1);
            !blocks.iter().any(|(s, end)| at >= *s && at < *end)
        })
    };
    let names: HashSet<String> = plan
        .elements
        .iter()
        .filter(|e| e.id != own_id && e.role != Role::Text)
        .filter(elsewhere)
        .map(|e| norm(&e.name))
        .filter(|n| !n.is_empty())
        .collect();
    let mut out: Vec<Cow<'a, str>> = lines.iter().map(|l| Cow::Borrowed(*l)).collect();
    if names.is_empty() {
        return out;
    }
    let mut header: Option<(usize, usize)> = None; // (line, cell count) of the last table line
    for (i, l) in lines.iter().enumerate() {
        let Some(c) = cells(l) else {
            if !l.trim().is_empty() {
                header = None;
            }
            continue;
        };
        let Some(k) = name_cell(&c, &names) else {
            header = Some((i, c.len()));
            continue;
        };
        out[i] = Cow::Owned(c[..=k].join(" | "));
        if let Some((h, n)) = header.take().filter(|(_, n)| *n == c.len()) {
            let hc = cells(lines[h]).unwrap_or_default();
            if hc.len() == n {
                out[h] = Cow::Owned(hc[..=k].join(" | "));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::fidelity::tests::planned;

    #[test]
    fn rows_of_separately_imported_elements_keep_their_roll_and_name() {
        let plan = ImportPlan {
            elements: vec![
                planned(
                    "t",
                    "Odd Lamps",
                    "Rules Glossary Entry",
                    Role::Text,
                    Some(1),
                ),
                planned(
                    "a",
                    "Lamp of Echoes",
                    "Magic Item",
                    Role::Standalone,
                    Some(9),
                ),
                planned("b", "Lamp of Ash", "Magic Item", Role::Standalone, Some(12)),
                planned("c", "Lamp of Fog", "Magic Item", Role::Standalone, Some(7)),
            ],
            edition: crate::import::anything::types::Edition::Y2024,
            groups: Vec::new(),
            notes: Vec::new(),
        };
        let lines = [
            "Roll on the table.",
            "d4 | Item | Description",
            "",
            "1 | Lamp of Echoes | Its light repeats the last word spoken.",
            "",
            "2 | Lamp of Ash | It burns ash for oil.",
            "3 | Lamp of Fog | Its light is grey.",
        ];
        // The entry prints lines 1-7; Lamp of Fog is located on its row there.
        let out = rows_of_others(&lines, "t", &[(0, 7)], &plan);
        assert_eq!(out[1], "d4 | Item");
        assert_eq!(out[3], "1 | Lamp of Echoes");
        assert_eq!(out[5], "2 | Lamp of Ash");
        // A row whose element is located on it keeps its text.
        assert_eq!(out[6], lines[6]);
        assert_eq!(out[0], lines[0]);
    }
}
