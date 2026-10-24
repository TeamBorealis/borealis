//! Printed source lines as description HTML, the shape the bundled descriptions use: a
//! paragraph per block, bold headings, list lines apart, a pipe table as a `<table>`, and the
//! extraction's `**bold**` runs as bold.

/// A line of a bulleted or numbered list.
fn is_list_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("• ")
        || t.split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn is_heading(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then_some(n)
}

/// The cells of a pipe-table line ("a | b | c" or "| a | b |"), `None` for any other line.
fn table_cells(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.contains(" | ") && !(t.starts_with('|') && t.len() > 1) {
        return None;
    }
    let inner = t.trim_start_matches('|').trim_end_matches('|');
    Some(inner.split('|').map(|c| c.trim().to_string()).collect())
}

/// A markdown separator row ("--- | :---:").
fn separator(cells: &[String]) -> bool {
    cells
        .iter()
        .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escaped text with paired `**` runs as bold (an unpaired `**` stays as printed).
fn inline(s: &str) -> String {
    let parts: Vec<&str> = s.split("**").collect();
    if parts.len() < 3 || parts.len().is_multiple_of(2) {
        return escape(s);
    }
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i % 2 == 1 && !p.trim().is_empty() {
                format!("<strong>{}</strong>", escape(p))
            } else {
                escape(p)
            }
        })
        .collect()
}

/// `next` continues `prev` (a wrapped line): it opens in lower case, or `prev` stops mid-word
/// on a hyphen. Returns the joined text.
fn continued(prev: &str, next: &str) -> Option<String> {
    let next = next.trim();
    let lower = next.starts_with(|c: char| c.is_lowercase());
    let hyphen = prev.ends_with('-') && prev.chars().rev().nth(1).is_some_and(char::is_alphabetic);
    if !lower {
        return None;
    }
    Some(if hyphen {
        format!("{}{next}", &prev[..prev.len() - 1])
    } else {
        format!("{prev} {next}")
    })
}

/// One block of lines (no blank line inside) being written.
#[derive(Default)]
struct Block {
    out: String,
    para: Vec<String>,
    rows: Vec<Vec<String>>,
    list: bool,
}

impl Block {
    fn flush_para(&mut self) {
        if !self.para.is_empty() {
            let sep = if self.list { "<br>" } else { " " };
            let text: Vec<String> = self.para.iter().map(|p| inline(p)).collect();
            self.out.push_str(&format!("<p>{}</p>", text.join(sep)));
            self.para.clear();
        }
    }

    fn flush_table(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let row = |cells: &[String], tag: &str| {
            let cells: String = cells
                .iter()
                .map(|c| format!("<{tag}>{}</{tag}>", inline(c)))
                .collect();
            format!("<tr>{cells}</tr>")
        };
        let rows = std::mem::take(&mut self.rows);
        let (head, body) = if rows.len() > 1 {
            (Some(&rows[0]), &rows[1..])
        } else {
            (None, &rows[..])
        };
        self.out.push_str("<table>");
        if let Some(h) = head {
            self.out
                .push_str(&format!("<thead>{}</thead>", row(h, "th")));
        }
        let body: String = body.iter().map(|r| row(r, "td")).collect();
        self.out.push_str(&format!("<tbody>{body}</tbody></table>"));
    }

    fn line(&mut self, l: &str) {
        if let Some(n) = is_heading(l) {
            self.flush_para();
            self.flush_table();
            let text = l.trim_start()[n..].trim();
            self.out
                .push_str(&format!("<p><strong>{}</strong></p>", escape(text)));
            return;
        }
        if let Some(cells) = table_cells(l) {
            self.flush_para();
            if !separator(&cells) {
                self.rows.push(cells);
            }
            return;
        }
        // A wrapped cell runs on in its row.
        if let Some(cell) = self.rows.last_mut().and_then(|r| r.last_mut()) {
            if let Some(joined) = continued(cell, l) {
                *cell = joined;
                return;
            }
            self.flush_table();
        }
        // A wrapped list line runs on in its item; a word broken over two lines is one word.
        let hyphen = self.para.last().is_some_and(|p| {
            p.ends_with('-') && p.chars().rev().nth(1).is_some_and(char::is_alphabetic)
        });
        if (self.list && !is_list_line(l)) || hyphen {
            if let Some(joined) = self.para.last().and_then(|p| continued(p, l)) {
                *self.para.last_mut().expect("a last line") = joined;
                return;
            }
        }
        self.para.push(l.trim().to_string());
    }
}

/// `lines` without the blank lines between two rows of one table (rows with the same number of
/// cells, a wrapped cell allowed after the upper one): an extraction that prints a blank line
/// after every row still printed one table, not a table per row.
fn rows_together<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let width = |l: &str| table_cells(l).map(|c| c.len());
    let mut out = Vec::with_capacity(lines.len());
    let mut open: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        if l.trim().is_empty() {
            let next = lines[i + 1..]
                .iter()
                .find(|n| !n.trim().is_empty())
                .and_then(|n| width(n));
            if open.is_some() && next == open {
                continue;
            }
            open = None;
        } else if let Some(w) = width(l) {
            open = Some(w);
        } else if !l.trim_start().starts_with(char::is_lowercase) {
            open = None;
        }
        out.push(*l);
    }
    out
}

/// See the module doc.
pub(crate) fn to_html(lines: &[&str]) -> String {
    let lines = rows_together(lines);
    let mut out = String::new();
    for block in lines.split(|l| l.trim().is_empty()) {
        if block.is_empty() {
            continue;
        }
        let mut b = Block {
            list: block.iter().any(|l| is_list_line(l)),
            ..Block::default()
        };
        for l in block {
            b.line(l);
        }
        b.flush_para();
        b.flush_table();
        out.push_str(&b.out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_table_becomes_a_table_with_its_header_and_bold_cells() {
        let lines = [
            "d4 | Omen",
            "1 | **Ash.** The wind turns",
            "grey.",
            "2 | **Salt.** Milk sours.",
        ];
        assert_eq!(
            to_html(&lines),
            "<table><thead><tr><th>d4</th><th>Omen</th></tr></thead><tbody>\
             <tr><td>1</td><td><strong>Ash.</strong> The wind turns grey.</td></tr>\
             <tr><td>2</td><td><strong>Salt.</strong> Milk sours.</td></tr></tbody></table>"
        );
    }

    #[test]
    fn prose_after_a_table_and_separator_rows_are_kept_apart() {
        let lines = ["| a | b |", "|---|---|", "| 1 | 2 |", "Roll again."];
        assert_eq!(
            to_html(&lines),
            "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table><p>Roll again.</p>"
        );
    }

    #[test]
    fn rows_printed_with_a_blank_line_after_each_are_one_table() {
        let lines = [
            "d4 | Hide | Ground",
            "",
            "1 | Newt | Marsh, bog",
            "",
            "2 | Stag | Forest",
            "and glade",
            "",
            "3 | Goat | Hills",
            "",
            "Roll again on a 4.",
            "",
            "a | b",
        ];
        let html = to_html(&lines);
        assert_eq!(html.matches("<table>").count(), 2, "{html}");
        assert!(html.starts_with(
            "<table><thead><tr><th>d4</th><th>Hide</th><th>Ground</th></tr></thead><tbody>\
             <tr><td>1</td><td>Newt</td><td>Marsh, bog</td></tr>\
             <tr><td>2</td><td>Stag</td><td>Forest and glade</td></tr>\
             <tr><td>3</td><td>Goat</td><td>Hills</td></tr></tbody></table><p>Roll again on a 4.</p>"
        ));
    }

    #[test]
    fn tables_of_different_widths_stay_apart() {
        let lines = ["a | b", "1 | 2", "", "x | y | z", "3 | 4 | 5"];
        assert_eq!(to_html(&lines).matches("<table>").count(), 2);
    }

    #[test]
    fn a_wrapped_list_line_stays_one_item() {
        let lines = ["- 1st: mist step, cinder", "  ward", "- 2nd: ember hold"];
        assert_eq!(
            to_html(&lines),
            "<p>- 1st: mist step, cinder ward<br>- 2nd: ember hold</p>"
        );
    }

    #[test]
    fn a_word_broken_over_two_lines_is_one_word() {
        let lines = ["You learn ember ward and cinder-", "lash, and mist step."];
        assert_eq!(
            to_html(&lines),
            "<p>You learn ember ward and cinderlash, and mist step.</p>"
        );
    }

    #[test]
    fn an_unpaired_bold_mark_stays_as_printed() {
        assert_eq!(to_html(&["a ** b"]), "<p>a ** b</p>");
        assert_eq!(
            to_html(&["**Bold.** text **x**"]),
            "<p><strong>Bold.</strong> text <strong>x</strong></p>"
        );
    }
}
