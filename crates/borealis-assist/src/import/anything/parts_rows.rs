//! Table rows a text-only entry prints for elements imported on their own (a d100 table of
//! items, a table of properties): the entry keeps each such row's roll and name cells, and the
//! row's other cells (the element's own text) stay with that element, so the text is not
//! imported twice.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use super::super::fidelity::words;
use super::super::types::{ImportPlan, PlannedElement, Role};

/// Cells (from the left) that may hold a row's name: a roll or number column, then the name.
const NAME_CELLS: usize = 2;
/// Share (percent) of the words of a row's other cells its element's own passage must print
/// for the row to be cut to its name.
const HELD_PCT: usize = 90;

/// Roles whose rows a text-only entry may cut: elements a table lists (items, options, parts),
/// never a class, a subclass or a feature a table only names.
fn item_like(role: Role) -> bool {
    matches!(role, Role::Standalone | Role::Option | Role::Part)
}

fn content(text: &str) -> HashSet<String> {
    words(text).into_iter().collect()
}

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
fn name_cell(cells: &[&str], names: &HashMap<String, HashSet<String>>) -> Option<usize> {
    (0..NAME_CELLS.min(cells.len()))
        .find(|&k| names.contains_key(&norm(cells[k])))
        .filter(|&k| cells.len() > k + 1)
}

/// `lines` of the text-only entry `own_id` (printed from the 0-based line ranges `blocks` of
/// `source`) with the rows of item- or option-like elements located on a passage of their own
/// outside it cut after their name cell, and the header row over them cut to the same columns.
/// A row is cut only when its other cells are printed in that element's own passage too (they
/// stay with the element); a row naming a class or a feature, a row whose element is located
/// on it here, and a row with text its element's passage lacks stay whole.
pub(crate) fn rows_of_others<'a>(
    lines: &[&'a str],
    own_id: &str,
    blocks: &[(usize, usize)],
    plan: &ImportPlan,
    source: &str,
) -> Vec<Cow<'a, str>> {
    let elsewhere = |e: &&PlannedElement| {
        e.excerpt.as_ref().is_some_and(|x| {
            let at = x.start_line.saturating_sub(1);
            !blocks.iter().any(|(s, end)| at >= *s && at < *end)
        })
    };
    let src: Vec<&str> = source.lines().collect();
    // norm(name) -> the words of the element's own located passage.
    let names: HashMap<String, HashSet<String>> = plan
        .elements
        .iter()
        .filter(|e| e.id != own_id && item_like(e.role))
        .filter(elsewhere)
        .filter_map(|e| {
            let x = e.excerpt.as_ref()?;
            let s = x.start_line.saturating_sub(1).min(src.len());
            let end = x.end_line.clamp(s, src.len());
            let n = norm(&e.name);
            (!n.is_empty()).then(|| (n, content(&src[s..end].join(" "))))
        })
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
        // The cut cells must be the element's own text, or the row keeps them.
        let cut = content(&c[k + 1..].join(" "));
        let passage = &names[&norm(c[k])];
        if cut.intersection(passage).count() * 100 < cut.len() * HELD_PCT {
            header = Some((i, c.len()));
            continue;
        }
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
                    Some(11),
                ),
                planned("b", "Lamp of Ash", "Magic Item", Role::Standalone, Some(13)),
                planned("c", "Lamp of Fog", "Magic Item", Role::Standalone, Some(7)),
                planned("w", "Warden", "Class", Role::Class, Some(15)),
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
            "Class | Minimum",
            "Warden | Wisdom 13",
        ];
        let source = format!(
            "{}\n\nLamp of Echoes. Its light repeats the last word spoken, twice.\n\nLamp of Ash. It burns oil.\n\n# Warden\nWardens keep the wood.",
            lines.join("\n")
        );
        // The entry prints lines 1-9; Lamp of Fog is located on its row there.
        let out = rows_of_others(&lines, "t", &[(0, 9)], &plan, &source);
        assert_eq!(out[1], "d4 | Item");
        assert_eq!(out[3], "1 | Lamp of Echoes");
        // Its passage lacks words of the row: the row keeps them.
        assert_eq!(out[5], lines[5]);
        // A row whose element is located on it keeps its text.
        assert_eq!(out[6], lines[6]);
        assert_eq!(out[0], lines[0]);
        // A row naming a class (a multiclassing table) keeps its columns.
        assert_eq!(out[7], lines[7]);
        assert_eq!(out[8], lines[8]);
    }
}
