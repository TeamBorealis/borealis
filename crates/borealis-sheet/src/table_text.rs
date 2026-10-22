//! Tables in feature descriptions, written as one sentence per row so a flattened table does not
//! print as a run-on paragraph (M6), and the raw text copy of a table dropped (it only repeats the
//! table, often cut off).

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

fn rows_text(table: &str) -> String {
    let rows: Vec<Vec<String>> = table
        .split("<tr")
        .skip(1)
        .map(cells)
        .filter(|r| !r.is_empty())
        .collect();
    let Some((head, body)) = rows.split_first() else {
        return String::new();
    };
    if body.is_empty() {
        return format!("<p>{}</p>", head.join(" | "));
    }
    body.iter()
        .map(|row| {
            let label = row.first().map_or("", String::as_str);
            let rest: Vec<String> = row
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, v)| !v.is_empty())
                .map(|(i, v)| match head.get(i) {
                    Some(h) if !h.is_empty() => format!("{h}: {v}"),
                    _ => v.clone(),
                })
                .collect();
            format!("<p>{label}. {}</p>", rest.join("; "))
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELVEN: &str = "<p>Pick one.</p><pre> Lineage  Level 1\nDrow  The range of your Darkvision increases to 120\n</pre><table><tr><th>Lineage</th><th>Level 1</th><th>Level 3</th></tr><tr><td>Drow</td><td>Darkvision 120 feet.</td><td>Faerie Fire</td></tr><tr><td>Wood Elf</td><td>Speed 35 feet.</td><td>Longstrider</td></tr></table><p>After.</p>";

    #[test]
    fn writes_each_row_as_a_labelled_sentence() {
        let t = crate::card_model::plain(&tables_as_rows(ELVEN));
        assert_eq!(
            t,
            "Pick one. Drow. Level 1: Darkvision 120 feet.; Level 3: Faerie Fire Wood Elf. Level 1: Speed 35 feet.; Level 3: Longstrider After."
        );
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
