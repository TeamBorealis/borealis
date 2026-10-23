//! Tables in feature descriptions, written as one sentence per row (one paragraph each with
//! `card_model::plain_paragraphs`) so a flattened table does not print as a run-on paragraph
//! (M6), and the raw text copy of a table dropped (it only repeats the table, often cut off).
//! [`locate_tables`] keeps the table structure next to that text, so the sheet can draw a real
//! table where it has room.

const TABLE_OPEN: &str = "<table";
const TABLE_CLOSE: &str = "</table>";

/// `html` with each `<table>` as `<p>` rows ("Row. Header: value; Header: value.") and, when a
/// table is present, the `<pre>` blocks (a text copy of it) removed. Without a table, `html`
/// unchanged.
pub(crate) fn tables_as_rows(html: &str) -> String {
    if !html.contains(TABLE_OPEN) {
        return html.to_string();
    }
    let without_pre = cut_blocks(html, "<pre>", "</pre>", |_| String::new());
    cut_blocks(&without_pre, TABLE_OPEN, TABLE_CLOSE, rows_text)
}

/// Replace every `open..close` block by `f(block)`.
fn cut_blocks(html: &str, open: &str, close: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(open) {
        let Some(len) = rest[start..].find(close) else {
            break;
        };
        let end = start + len + close.len();
        out.push_str(&rest[..start]);
        out.push_str(&f(&rest[start..end]));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cells(row: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = row;
    while let Some(i) = rest.find("<t") {
        let tail = &rest[i..];
        if !(tail.starts_with("<td") || tail.starts_with("<th")) {
            rest = &rest[i + 2..];
            continue;
        }
        let Some(open_end) = tail.find('>') else {
            break;
        };
        let body = &tail[open_end + 1..];
        let end = body.find("</t").unwrap_or(body.len());
        out.push(strip_tags(&body[..end]));
        rest = &body[end..];
    }
    out
}

/// Cell text of every non-empty row of one `<table>` block, header row first.
fn table_rows(table: &str) -> Vec<Vec<String>> {
    table
        .split("<tr")
        .skip(1)
        .map(cells)
        .filter(|r| !r.is_empty())
        .collect()
}

/// One body row as the sentence the flattened text prints ("Row. Header: value; Header:
/// value.").
pub(crate) fn row_sentence(head: &[String], row: &[String]) -> String {
    let label = row.first().map_or("", String::as_str);
    let rest: Vec<String> = row
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, v)| (i, v.trim_end_matches('.')))
        .filter(|(_, v)| !v.is_empty())
        .map(|(i, v)| match head.get(i) {
            Some(h) if !h.is_empty() => format!("{h}: {v}"),
            _ => v.to_string(),
        })
        .collect();
    format!("{label}. {}.", rest.join("; "))
}

fn rows_text(table: &str) -> String {
    let rows = table_rows(table);
    let Some((head, body)) = rows.split_first() else {
        return String::new();
    };
    if body.is_empty() {
        return format!("<p>{}</p>", head.join(" | "));
    }
    body.iter()
        .map(|row| format!("<p>{}</p>", row_sentence(head, row)))
        .collect::<Vec<_>>()
        .join("")
}

/// The tables of `html` that have a header row and at least one body row, in document order:
/// `(header, body rows)`.
pub(crate) fn parse_tables(html: &str) -> Vec<(Vec<String>, Vec<Vec<String>>)> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(TABLE_OPEN) {
        let Some(len) = rest[start..].find(TABLE_CLOSE) else {
            break;
        };
        let end = start + len + TABLE_CLOSE.len();
        let mut rows = table_rows(&rest[start..end]);
        if rows.len() > 1 {
            let head = rows.remove(0);
            out.push((head, rows));
        }
        rest = &rest[end..];
    }
    out
}

/// The tables of a feature description located in its flattened `text`: each table whose row
/// sentences are consecutive paragraphs of `text` (in order), with those paragraphs' range.
/// A table whose sentences are not found (text rewritten after flattening) is left out, so
/// its rows stay plain text.
pub(crate) fn locate_tables(html: &str, text: &str) -> Vec<crate::model_types::TextTable> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut from = 0;
    let mut out = Vec::new();
    for (header, rows) in parse_tables(html) {
        let sentences: Vec<String> = rows.iter().map(|r| row_sentence(&header, r)).collect();
        let n = sentences.len();
        let found = (from..lines.len())
            .filter(|&i| i + n <= lines.len())
            .find(|&i| sentences.iter().zip(&lines[i..i + n]).all(|(s, l)| s == l));
        if let Some(start) = found {
            out.push(crate::model_types::TextTable {
                header,
                rows,
                lines: start..start + n,
            });
            from = start + n;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELVEN: &str = "<p>Pick one.</p><pre> Lineage  Level 1\nDrow  The range of your Darkvision increases to 120\n</pre><table><tr><th>Lineage</th><th>Level 1</th><th>Level 3</th></tr><tr><td>Drow</td><td>Darkvision 120 feet.</td><td>Faerie Fire</td></tr><tr><td>Wood Elf</td><td>Speed 35 feet.</td><td>Longstrider</td></tr></table><p>After.</p>";

    #[test]
    fn writes_each_row_as_a_labelled_sentence() {
        let t = crate::card_model::plain_paragraphs(&tables_as_rows(ELVEN));
        assert_eq!(
            t,
            "Pick one.\nDrow. Level 1: Darkvision 120 feet; Level 3: Faerie Fire.\nWood Elf. Level 1: Speed 35 feet; Level 3: Longstrider.\nAfter."
        );
    }

    #[test]
    fn run_in_headings_and_list_items_start_their_own_paragraph() {
        let h = "<p><strong>Spell Slots.</strong> The table shows <em>how many</em>.</p>\
                 <p><strong>Spellbook.</strong> It holds spells:</p><ul><li>one</li><li>two</li></ul>\
                 line<br>break";
        assert_eq!(
            crate::card_model::plain_paragraphs(h),
            "Spell Slots. The table shows how many.\nSpellbook. It holds spells:\n\u{2022} one\n\u{2022} two\nline\nbreak"
        );
        assert_eq!(crate::card_model::plain_paragraphs("<p> </p><p>x</p>"), "x");
    }

    #[test]
    fn locates_each_table_among_the_paragraphs() {
        let text = crate::card_model::plain_paragraphs(&tables_as_rows(ELVEN));
        let t = locate_tables(ELVEN, &text);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].header, ["Lineage", "Level 1", "Level 3"]);
        assert_eq!(t[0].rows.len(), 2);
        assert_eq!(t[0].lines, 1..3);
        assert!(locate_tables(ELVEN, "Pick one.\nAfter.").is_empty());
        assert!(locate_tables("<p>No table.</p>", "No table.").is_empty());
    }

    #[test]
    fn drops_the_text_copy_of_a_table() {
        assert!(!tables_as_rows(ELVEN).contains("increases to 120"));
    }

    #[test]
    fn leaves_text_without_a_table_alone() {
        let h = "<p>A</p><pre>x  y</pre>";
        assert_eq!(tables_as_rows(h), h);
    }
}
