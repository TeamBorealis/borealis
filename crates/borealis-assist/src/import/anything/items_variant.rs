//! A per-type version of an item ("<Item>, <version>") prints its own row of the item's roll
//! or version table, not the whole table. The model copies the table into every version, or
//! leaves it out; the row whose cell names the version (any cell: the first one is often the
//! roll) is what the version keeps.

use serde_json::{json, Value};

/// Most characters of an entity name decoded in a cell ("&amp;" ... "&#8217;").
const MAX_ENTITY: usize = 8;

/// One `<tr>` of a table: its byte range in the table's html and whether it is a header row.
struct Row {
    start: usize,
    end: usize,
    header: bool,
    cells: Vec<String>,
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => name
            .strip_prefix("#x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
            .and_then(char::from_u32),
    }
}

/// The visible text of an html fragment, entities decoded, whitespace collapsed, lowercase.
fn visible(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '<' => tag = true,
            '>' if tag => {
                tag = false;
                out.push(' ');
            }
            _ if tag => {}
            '&' => match rest.find(';').filter(|&k| k <= MAX_ENTITY) {
                Some(k) if entity(&rest[..k]).is_some() => {
                    out.extend(entity(&rest[..k]));
                    rest = &rest[k + 1..];
                }
                _ => out.push('&'),
            },
            _ => out.push(c),
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The `[start, end)` byte ranges of `<open ...>...</close>` blocks in `html`.
fn blocks(html: &str, open: &str, close: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(s) = html[from..].find(open).map(|k| from + k) {
        let Some(e) = html[s..].find(close).map(|k| s + k + close.len()) else {
            break;
        };
        out.push((s, e));
        from = e;
    }
    out
}

/// The visible text of each `<td>` / `<th>` of a row.
fn cells(tr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(s) = tr[from..]
        .match_indices("<t")
        .map(|(k, _)| from + k)
        .find(|&k| tr[k..].starts_with("<td") || tr[k..].starts_with("<th"))
    {
        let end = tr[s..]
            .find("</td>")
            .into_iter()
            .chain(tr[s..].find("</th>"))
            .min()
            .map_or(tr.len(), |k| s + k);
        out.push(visible(&tr[s..end]));
        from = end.max(s + 1);
    }
    out
}

fn rows(table: &str) -> Vec<Row> {
    let head_end = table.find("</thead>");
    blocks(table, "<tr", "</tr>")
        .into_iter()
        .map(|(start, end)| {
            let tr = &table[start..end];
            let cells = cells(tr);
            Row {
                start,
                end,
                header: head_end.is_some_and(|h| start < h) || tr.contains("<th"),
                cells,
            }
        })
        .collect()
}

/// A cell that names the version: the version itself, the full name, or "<Item> (<version>)",
/// a footnote marker after it allowed.
fn names_version(cell: &str, item: &str, version: &str) -> bool {
    let cell = cell.trim_end_matches(['*', '†']).trim();
    cell == version || cell == format!("{item}, {version}") || cell == format!("{item} ({version})")
}

/// `table` with only its header rows and the body rows naming the version; `None` when no
/// body row names it. The second value counts the rows left out.
fn own_rows(table: &str, item: &str, version: &str) -> Option<(String, usize)> {
    let rows = rows(table);
    let own = |r: &Row| r.cells.iter().any(|c| names_version(c, item, version));
    if !rows.iter().any(|r| !r.header && own(r)) {
        return None;
    }
    let mut out = String::new();
    let mut at = 0;
    let mut cut = 0;
    for r in rows.iter().filter(|r| !r.header && !own(r)) {
        out.push_str(&table[at..r.start]);
        at = r.end;
        cut += 1;
    }
    out.push_str(&table[at..]);
    Some((out, cut))
}

/// The heading right before `at` in `html` (`<h1>`..`<h6>`), as its byte start.
fn caption_before(html: &str, at: usize) -> Option<usize> {
    let before = html[..at].trim_end();
    let close = before.rfind("</h")?;
    let tail = &before[close..];
    let is_heading = tail.len() == 5 && tail.as_bytes()[3].is_ascii_digit() && tail.ends_with('>');
    is_heading.then(|| before[..close].rfind("<h"))?
}

/// The version's table rebuilt from its own description, else from the item's: header rows plus
/// the row naming the version. Returns the new description and a note, `None` when nothing
/// changes.
pub(crate) fn own_row(desc: &str, name: &str, item_desc: Option<&str>) -> Option<(String, String)> {
    let (item, version) = name.rsplit_once(", ")?;
    let (item, version) = (visible(item), visible(version));
    if item.is_empty() || version.is_empty() {
        return None;
    }
    for (s, e) in blocks(desc, "<table", "</table>") {
        let Some((table, cut)) = own_rows(&desc[s..e], &item, &version) else {
            continue;
        };
        if cut == 0 {
            return None;
        }
        let out = format!("{}{table}{}", &desc[..s], &desc[e..]);
        let note = format!("description: only its own row of the table kept ({cut} other row(s) of the other versions left out)");
        return Some((out, note));
    }
    let item_desc = item_desc?;
    let (s, table) = blocks(item_desc, "<table", "</table>")
        .into_iter()
        .find_map(|(s, e)| Some((s, own_rows(&item_desc[s..e], &item, &version)?.0)))?;
    let lead = caption_before(item_desc, s).unwrap_or(s);
    let added = format!("{}{table}", &item_desc[lead..s]);
    // After the paragraph the item prints right before the table, when the version has it too.
    let prev = item_desc[..lead]
        .trim_end()
        .strip_suffix("</p>")
        .and_then(|b| b.rfind("<p").map(|k| &item_desc[k..lead]))
        .map(str::trim_end)
        .filter(|p| !p.is_empty());
    let out = match prev.and_then(|p| desc.find(p).map(|k| k + p.len())) {
        Some(k) => format!("{}{added}{}", &desc[..k], &desc[k..]),
        None => format!("{desc}{added}"),
    };
    Some((
        out,
        "description: its own row of the item's table added".into(),
    ))
}

/// [`own_row`] on an element: `items` maps item names (visible text) to their descriptions.
pub(crate) fn own_row_fix(el: &mut Value, items: &[(String, String)]) -> Option<String> {
    let name = el.get("name").and_then(Value::as_str)?.to_string();
    let item = visible(name.rsplit_once(", ")?.0);
    let item_desc = items
        .iter()
        .find(|(n, _)| *n == item)
        .map(|(_, d)| d.as_str())?;
    let desc = el.get("description").and_then(Value::as_str).unwrap_or("");
    let (out, note) = own_row(desc, &name, Some(item_desc))?;
    super::obj(el).insert("description".into(), json!(out));
    Some(note)
}

/// Item names (visible text) with their descriptions, for [`own_row_fix`].
pub(crate) fn item_descriptions(raw: &[Value]) -> Vec<(String, String)> {
    raw.iter()
        .filter_map(|el| {
            let name = el.get("name").and_then(Value::as_str)?;
            let desc = el.get("description").and_then(Value::as_str)?;
            Some((visible(name), desc.to_string()))
        })
        .collect()
}
