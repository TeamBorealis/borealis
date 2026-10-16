//! The table rows check: every data row of a table inside an element's own passage is carried
//! by the element (most of the row's words are in it), and every feature a class or subclass
//! progression table names has an element.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::super::fidelity::plain;
use super::super::furniture::Furniture;
use super::{
    cells, failed, heading, is_row, leading_number, norm, passed, quoted_list, span, tokens,
    Checker,
};
use super::{DraftElement, ElementCheck, ImportDraft, PlannedElement, Role, TABLE_ROW_MISSING};

/// Share of a table row's words the element must carry.
const ROW_COVERED: f64 = 0.75;
/// Lines (either way) within which a row needs another row to be part of a table: a lone row
/// in prose is a label ("Name | 3rd") or a running header, not a table.
const TABLE_NEIGHBOUR_LINES: usize = 2;
/// Words of a row that say nothing (left out of its coverage).
const ROW_STOP: [&str; 12] = [
    "the", "and", "for", "with", "per", "you", "your", "that", "this", "are", "has", "not",
];
/// Feature cells of a progression table that name no element of their own.
const GENERIC_FEATURES: [&str; 3] = ["ability score improvement", "feature", "improvement"];

/// Every word the element carries: its strings (description as text, ids as words) and numbers.
fn carried(el: &Value, out: &mut HashSet<String>) {
    match el {
        Value::String(s) => {
            let text = if s.starts_with("ID_") {
                s.replace('_', " ")
            } else {
                plain(s)
            };
            out.extend(split_words(&text));
        }
        Value::Number(n) => {
            out.insert(n.to_string());
        }
        Value::Array(a) => a.iter().for_each(|v| carried(v, out)),
        Value::Object(m) => m.values().for_each(|v| carried(v, out)),
        Value::Bool(_) | Value::Null => {}
    }
}

/// Tokens split where digits meet letters ("25gp" = "25", "gp"; "1d4x5" = "1", "d", "4", ...).
fn split_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for t in tokens(text) {
        let mut cur = String::new();
        for ch in t.chars() {
            let boundary = cur
                .chars()
                .last()
                .is_some_and(|l| l.is_ascii_digit() != ch.is_ascii_digit());
            if boundary {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(ch);
        }
        out.push(cur);
    }
    out
}

fn row_words(line: &str) -> Vec<String> {
    split_words(line)
        .into_iter()
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()) || t.chars().count() >= 3)
        .filter(|t| !ROW_STOP.contains(&t.as_str()))
        .collect()
}

/// A row of a table of contents: a name and a page number ("Old Ways | 12").
fn contents_row(line: &str) -> bool {
    let cs = cells(line);
    cs.len() == 2
        && !cs[1].is_empty()
        && cs[1].chars().all(|c| c.is_ascii_digit())
        && cs[0].chars().any(char::is_alphabetic)
        && !cs[0].chars().any(|c| c.is_ascii_digit())
}

/// The data rows of the tables in `[s, e)`: header rows (before a block's first row printing a
/// number other than a die, so "d6 | Effect" is a header), lone rows, rows of a table of
/// contents and page furniture left out.
fn data_rows(lines: &[String], s: usize, e: usize, furniture: &Furniture) -> Vec<usize> {
    let row =
        |i: usize| lines.get(i).is_some_and(|l| is_row(l) && !contents_row(l)) && !furniture.has(i);
    let mut out = Vec::new();
    let mut numbered_block = false;
    for (i, line) in lines.iter().enumerate().take(e).skip(s) {
        if !row(i) {
            if !line.trim().is_empty() {
                numbered_block = false;
            }
            continue;
        }
        let lone = !(i.saturating_sub(TABLE_NEIGHBOUR_LINES)..=i + TABLE_NEIGHBOUR_LINES)
            .any(|j| j != i && row(j));
        if lone {
            continue;
        }
        numbered_block |= tokens(line).iter().any(|t| leading_number(t).is_some());
        if numbered_block {
            out.push(i);
        }
    }
    out
}

/// The element each table row belongs to: the planned element starting nearest above it, when
/// its located passage holds the row (a row after another element's start is never an earlier
/// element's, even when that one's passage was located too long).
pub(super) fn row_owners(draft: &ImportDraft, lines: &[String]) -> HashMap<String, Vec<usize>> {
    let furniture = super::super::furniture::furniture(&draft.source_text);
    let spans: Vec<(&str, usize, usize)> = draft
        .plan
        .elements
        .iter()
        .filter_map(|p| span(p).map(|(s, e)| (p.id.as_str(), s, e)))
        .collect();
    let mut out: HashMap<String, Vec<usize>> = HashMap::new();
    let mut taken = HashSet::new();
    for &(_, s, e) in &spans {
        for i in data_rows(lines, s, e, &furniture) {
            if !taken.insert(i) {
                continue;
            }
            let nearest = spans
                .iter()
                .filter(|(_, s, _)| *s <= i)
                .max_by_key(|(_, s, e)| (*s, usize::MAX - *e));
            if let Some((id, _, _)) = nearest.filter(|(_, _, e)| i < *e) {
                out.entry((*id).to_string()).or_default().push(i);
            }
        }
    }
    out
}

/// End (exclusive) of the section a passage starting at `s` heads: the next heading of its
/// level or higher, else `end`.
fn section_end(lines: &[String], s: usize, end: usize) -> usize {
    let Some((level, _)) = lines.get(s).and_then(|l| heading(l)) else {
        return end;
    };
    (s + 1..end)
        .find(|j| heading(&lines[*j]).is_some_and(|(n, _)| n <= level))
        .unwrap_or(end)
}

/// The rows of a table inside an element's own section that text entries planned inside that
/// section own (a table split into entries of their own): element id -> rows. Class and
/// subclass roots, whose sections hold their features, and text entries are left out.
pub(super) fn piece_rows(
    draft: &ImportDraft,
    lines: &[String],
    owners: &HashMap<String, Vec<usize>>,
) -> HashMap<String, Vec<usize>> {
    let plan = &draft.plan.elements;
    let text_starts: Vec<(&str, usize)> = plan
        .iter()
        .filter(|p| p.role == Role::Text)
        .filter_map(|p| span(p).map(|(s, _)| (p.id.as_str(), s)))
        .collect();
    let mut out: HashMap<String, Vec<usize>> = HashMap::new();
    for p in plan.iter().filter(|p| {
        !matches!(
            p.role,
            Role::Text | Role::Class | Role::Subclass | Role::SubclassGate
        )
    }) {
        let Some((s, e)) = span(p) else {
            continue;
        };
        let next_element = plan
            .iter()
            .filter(|o| o.role != Role::Text && o.id != p.id)
            .filter_map(span)
            .map(|(o, _)| o)
            .filter(|o| *o > s)
            .min()
            .unwrap_or(lines.len());
        // The owner's heading section, even past its located passage (a passage located up
        // to its own text leaves out the table the split entries hold), never past the next
        // non-text element.
        let headed = lines.get(s).is_some_and(|l| heading(l).is_some());
        let end = section_end(
            lines,
            s,
            if headed {
                next_element
            } else {
                e.min(next_element)
            },
        );
        let rows: Vec<usize> = text_starts
            .iter()
            .filter(|(_, t)| *t > s && *t < end)
            .filter_map(|(id, _)| owners.get(*id))
            .flatten()
            .copied()
            .filter(|i| *i > s && *i < end)
            .collect();
        if !rows.is_empty() {
            out.insert(p.id.clone(), rows);
        }
    }
    out
}

/// The rows of `rows` with less than [`ROW_COVERED`] of their words in `el`: their first
/// cells, and whether that first cell is carried (only the other cells are lost: the table was
/// flattened into text).
fn missing_rows(el: &Value, name: &str, rows: &[usize], lines: &[String]) -> Vec<MissingRow> {
    let mut have = HashSet::new();
    carried(el, &mut have);
    have.extend(split_words(name));
    rows.iter()
        .filter(|i| {
            let w = row_words(&lines[**i]);
            let found = w.iter().filter(|t| have.contains(*t)).count();
            !w.is_empty() && (found as f64) < ROW_COVERED * w.len() as f64
        })
        .map(|i| {
            let first = cells(&lines[*i])[0];
            let head = row_words(first);
            let kept = !head.is_empty() && head.iter().all(|t| have.contains(t));
            (first.to_string(), kept)
        })
        .collect()
}

/// A missing row: its first cell, and whether that cell is carried (see [`missing_rows`]).
type MissingRow = (String, bool);

fn first_cells(rows: &[MissingRow]) -> Vec<String> {
    rows.iter().map(|(c, _)| c.clone()).collect()
}

/// A level cell of a progression table ("1st", "Level 3", "3").
fn level_cell(cell: &str) -> Option<u32> {
    let t = cell.to_lowercase();
    let t = t.strip_prefix("level").unwrap_or(&t).trim();
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    let rest = &t[digits.len()..];
    let n: u32 = digits.parse().ok()?;
    (["", "st", "nd", "rd", "th"].contains(&rest) && (1..=20).contains(&n)).then_some(n)
}

fn names_feature(name: &str, known: &[String]) -> bool {
    let n = norm(name);
    if n.is_empty() || GENERIC_FEATURES.iter().any(|g| n.ends_with(g)) {
        return true;
    }
    let mine: HashSet<&str> = n.split(' ').collect();
    known.iter().any(|k| {
        let theirs: HashSet<&str> = k.split(' ').collect();
        *k == n || mine.intersection(&theirs).count() * 2 >= mine.len().max(theirs.len())
    })
}

/// The column of a progression table's features: the header cell naming features, in the row
/// line above the first level row, when the header has as many cells as the rows.
fn feature_column(lines: &[String], first: usize, width: usize) -> Option<usize> {
    let header = (0..first)
        .rev()
        .map(|j| &lines[j])
        .find(|l| !l.trim().is_empty())?;
    let hs = cells(header);
    (is_row(header) && hs.len() == width)
        .then(|| hs.iter().position(|h| norm(h).contains("feature")))
        .flatten()
}

/// `"<name>" (level N` for each feature of the root's progression table no element has.
fn missing_features(draft: &ImportDraft, p: &PlannedElement, lines: &[String]) -> Vec<String> {
    let Some((s, e)) = span(p) else {
        return Vec::new();
    };
    let known: Vec<String> = draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null())
        .map(|e| norm(&e.name))
        .chain(draft.plan.groups.iter().map(|g| norm(&g.name)))
        .collect();
    let mut out = Vec::new();
    let mut column: Option<usize> = None;
    let mut in_table = false;
    for i in s..e.min(lines.len()) {
        let l = &lines[i];
        if heading(l).is_some() {
            in_table = false;
        }
        let cs = cells(l);
        let Some(level) = is_row(l).then(|| level_cell(cs[0])).flatten() else {
            continue;
        };
        if !in_table {
            column = feature_column(lines, i, cs.len());
            in_table = true;
        }
        let Some(cell) = column.and_then(|c| cs.get(c)) else {
            continue;
        };
        for name in cell.split([',', ';']).map(str::trim) {
            if !names_feature(name, &known) {
                out.push(format!("{name}\" (level {level}"));
            }
        }
    }
    out
}

/// The table row checks of `json` as element `e`, given the rows it owns.
pub(super) fn row_checks(
    c: &Checker,
    draft: &ImportDraft,
    e: &DraftElement,
    json: &Value,
) -> Vec<ElementCheck> {
    let root = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id && matches!(p.role, Role::Class | Role::Subclass));
    // A class or subclass root carries its progression table as data and feature elements,
    // never as text: only the features its table names are checked.
    let rows = match root {
        Some(_) => &[][..],
        None => c.rows.get(&e.id).map_or(&[][..], Vec::as_slice),
    };
    let mut out = Vec::new();
    let pieces = match root {
        Some(_) => &[][..],
        None => c.pieces.get(&e.id).map_or(&[][..], Vec::as_slice),
    };
    let split = missing_rows(json, &e.name, pieces, &c.lines);
    if !split.is_empty() {
        out.push(failed(
            TABLE_ROW_MISSING,
            format!(
                "{} of its own table's rows are separate entries, not in it: {}",
                split.len(),
                quoted_list(&first_cells(&split))
            ),
        ));
    }
    let missing = missing_rows(json, &e.name, rows, &c.lines);
    let (flattened, lost): (Vec<MissingRow>, Vec<MissingRow>) =
        missing.into_iter().partition(|(_, kept)| *kept);
    if !lost.is_empty() {
        let lost = first_cells(&lost);
        let message = if lost.len() == 1 {
            format!("The table row {} is missing", quoted_list(&lost))
        } else {
            format!(
                "{} table rows are missing: {}",
                lost.len(),
                quoted_list(&lost)
            )
        };
        out.push(failed(TABLE_ROW_MISSING, message));
    }
    if !flattened.is_empty() {
        let names = first_cells(&flattened);
        out.push(failed(
            TABLE_ROW_MISSING,
            format!(
                "Its table was flattened into text: {} {} only by {} first cell, the other columns are lost. Keep the table's rows and columns",
                quoted_list(&names),
                if names.len() == 1 { "is kept" } else { "are kept" },
                if names.len() == 1 { "its" } else { "their" }
            ),
        ));
    }
    let mut features = 0;
    if let Some(p) = root {
        for m in missing_features(draft, p, &c.lines) {
            features += 1;
            out.push(failed(
                TABLE_ROW_MISSING,
                format!("No element for \"{m} in the table)"),
            ));
        }
    }
    if out.is_empty()
        && (!rows.is_empty()
            || !pieces.is_empty()
            || root.is_some() && features == 0 && has_table(c, root))
    {
        out.push(passed(TABLE_ROW_MISSING, "Every table row is imported"));
    }
    out
}

fn has_table(c: &Checker, root: Option<&PlannedElement>) -> bool {
    root.and_then(span)
        .is_some_and(|(s, e)| c.lines.iter().take(e).skip(s).any(|l| is_row(l)))
}
