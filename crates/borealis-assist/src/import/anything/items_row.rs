//! A table-row item's printed row: the cells its source line prints under the table's header.
//! The description of a row item keeps exactly those cells, so it never carries text the row
//! does not print (core properties of a same-named SRD weapon, a model cell that disagrees with
//! the printed one, the same column twice).

use serde_json::{json, Map, Value};

use super::super::types::PlannedElement;

/// One printed column: its header label and its cell; `None` when the row's line ends before
/// it (a cell wrapped onto the next line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub label: String,
    pub text: Option<String>,
}

/// The located row of a table-row item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrintedRow {
    /// The row's cells, in header order.
    pub cells: Vec<Cell>,
    /// The lines right after the row up to the next row or blank line (a wrapped cell).
    pub after: String,
}

/// Fewest header columns that make a table.
const MIN_COLUMNS: usize = 2;
/// Most lines after a row searched for its wrapped cells.
const WRAP_LINES: usize = 3;

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The cells of a table line ("a | b | c", a leading or trailing bar allowed).
fn cells_of(line: &str) -> Vec<String> {
    let mut cells: Vec<String> = line
        .split('|')
        .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    while cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    if cells.first().is_some_and(String::is_empty) {
        cells.remove(0);
    }
    cells
}

/// The printed row of `planned` in `source`: its located line is a table row whose first cell
/// names it, under a header line of column labels. `None` for anything else (prose, a row
/// without a header, a row with more cells than its header).
pub(crate) fn printed_row(source: &str, planned: &PlannedElement) -> Option<PrintedRow> {
    let ex = planned.excerpt.as_ref()?;
    let lines: Vec<&str> = source.lines().collect();
    let row = ex.start_line.checked_sub(1)?;
    let line = lines.get(row)?;
    let cells = cells_of(line);
    let name = norm(&planned.name);
    if cells.len() < MIN_COLUMNS || name.is_empty() || !norm(&cells[0]).contains(&name) {
        return None;
    }
    let (start, end) = super::super::source::table_head(&lines, row)?;
    let header = lines[start..end].iter().rev().find(|l| l.contains('|'))?;
    let labels = cells_of(header);
    let is_header = labels.len() >= MIN_COLUMNS
        && labels.len() >= cells.len()
        && !labels.iter().any(|l| l.is_empty())
        && !header.chars().any(|c| c.is_ascii_digit())
        && !norm(&labels[0]).contains(&name);
    if !is_header {
        return None;
    }
    let after = lines[row + 1..]
        .iter()
        .take(WRAP_LINES)
        .take_while(|l| !l.trim().is_empty() && !l.contains('|'))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let cells = labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| Cell {
            label,
            text: cells.get(i).filter(|t| !t.is_empty()).cloned(),
        })
        .collect();
    Some(PrintedRow { cells, after })
}

/// `data` keys that hold a printed column, by the column's label: the column is shown by the
/// element's setters, not repeated in its description.
const HELD_BY: [(&str, &str); 9] = [
    ("cost", "cost"),
    ("price", "cost"),
    ("weight", "weight"),
    ("damage", "damage"),
    ("armor class", "base_ac"),
    ("armor class (ac)", "base_ac"),
    ("ac", "base_ac"),
    ("strength", "strength_requirement"),
    ("stealth", "stealth_disadvantage"),
];

fn held(data: &Map<String, Value>, label: &str) -> bool {
    let label = norm(label);
    HELD_BY
        .iter()
        .any(|(l, key)| *l == label && data.get(*key).is_some_and(|v| !v.is_null()))
}

/// A "<p><strong>Label:</strong> value</p>" line: `(label, value)`.
fn column_line(para: &str) -> Option<(String, String)> {
    let rest = para.trim().strip_prefix("<p><strong>")?;
    let (label, rest) = rest.split_once(":</strong>")?;
    let value = rest.strip_suffix("</p>")?.trim();
    (!label.contains('<') && !value.contains('<')).then(|| (label.to_string(), value.to_string()))
}

/// `html` in pieces that each end a paragraph, with what precedes a paragraph (a table) as a
/// piece of its own.
fn paragraphs(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for chunk in html.split_inclusive("</p>") {
        match chunk.rfind("<p>") {
            Some(k) if k > 0 && chunk.ends_with("</p>") => {
                out.push(&chunk[..k]);
                out.push(&chunk[k..]);
            }
            _ => out.push(chunk),
        }
    }
    out
}

fn shows(desc: &str, label: &str) -> bool {
    let lower = desc.to_lowercase();
    let label = norm(label);
    lower.contains(&format!("{label}:")) || lower.contains(&format!("<th>{label}</th>"))
}

fn model_columns(data: &Map<String, Value>) -> Vec<(String, String)> {
    data.get("columns")
        .and_then(Value::as_object)
        .map(|c| {
            c.iter()
                .filter_map(|(k, v)| Some((k.trim().to_string(), super::cell_text(v)?)))
                .collect()
        })
        .unwrap_or_default()
}

/// The description of a table-row item rebuilt from its printed row: the column lines the
/// pipeline wrote (from `data.columns`, other `data` keys outside `known`, the normalised
/// properties) are replaced by the row's own cells, each once, in header order. A column the
/// model's prose or table already shows, the name cell and a column a setter holds are not
/// repeated. Returns notes on what changed.
pub(crate) fn keep_printed_row(
    el: &mut Value,
    name: &str,
    known: &[&str],
    row: &PrintedRow,
) -> Vec<String> {
    let data = el
        .get("data")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let ours = model_columns(&data);
    let mut written: Vec<String> = row.cells.iter().map(|c| norm(&c.label)).collect();
    written.push("properties".into());
    written.extend(ours.iter().map(|(k, _)| norm(k)));
    written.extend(
        data.keys()
            .filter(|k| !known.contains(&k.as_str()))
            .map(|k| norm(&super::label_of(k))),
    );
    let before = el
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut dropped: Vec<String> = Vec::new();
    let mut desc = String::new();
    for para in paragraphs(&before) {
        match column_line(para) {
            Some((label, _)) if written.contains(&norm(&label)) => dropped.push(label),
            _ => desc.push_str(para),
        }
    }
    let mut columns = Map::new();
    let mut notes = Vec::new();
    for (i, cell) in row.cells.iter().enumerate() {
        let model = ours
            .iter()
            .find(|(k, _)| norm(k) == norm(&cell.label))
            .map(|(_, v)| v.clone());
        let text = match (&cell.text, &model) {
            (Some(t), _) => t.clone(),
            // A wrapped cell: the model's value when the following lines print it.
            (None, Some(v)) if norm(&row.after).contains(&norm(v)) => v.clone(),
            _ => continue,
        };
        if let Some(v) = model.filter(|v| norm(v) != norm(&text)) {
            notes.push(format!(
                "description: the printed {} cell ({text}) kept, not {v}",
                cell.label
            ));
        }
        if (i == 0 && norm(&text) == norm(name)) || held(&data, &cell.label) {
            continue;
        }
        columns.insert(cell.label.clone(), json!(text));
        if !shows(&desc, &cell.label) {
            desc.push_str(&format!("<p><strong>{}:</strong> {text}</p>", cell.label));
        }
    }
    let unprinted: Vec<&String> = dropped
        .iter()
        .filter(|l| !row.cells.iter().any(|c| norm(&c.label) == norm(l)))
        .collect();
    if !unprinted.is_empty() {
        let labels: Vec<&str> = unprinted.iter().map(|l| l.as_str()).collect();
        notes.push(format!(
            "description: {} left out, the table row prints no such column",
            labels.join(", ")
        ));
    }
    if desc != before {
        super::obj(el).insert("description".into(), json!(desc));
        if notes.is_empty() {
            notes.push("description: the table row's printed cells kept".into());
        }
    }
    if !columns.is_empty() && data.get("columns") != Some(&Value::Object(columns.clone())) {
        super::data_mut(el).insert("columns".into(), Value::Object(columns));
    }
    notes
}
