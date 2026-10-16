//! Named options printed as the rows of a table: where each row is, the text it prints (and
//! nothing else), and how many of them the rule lets a character pick. A row is a pipe-table
//! row whose naming cell is the option's name (a roll column before it is skipped), or a line
//! that starts with the name (a table the reader flattened into lines, a list of options).

/// Most blank lines between two rows of one table.
const MAX_ROW_GAP: usize = 2;
/// Most lines of a flattened row (the name line and its wrapped effect text).
const MAX_ROW_LINES: usize = 8;
/// Most lines above a table read for the rule's pick count.
const LEAD_LINES: usize = 15;
/// Number words a pick count is printed as.
const COUNTS: [&str; 10] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];
/// Verbs that open a pick ("choose two", "learn three of the following").
const PICK_VERBS: [&str; 7] = ["choose", "select", "pick", "learn", "gain", "take", "know"];
/// Most words between a pick verb and its count ("choose any two").
const PICK_REACH: usize = 3;

/// Lowercase alphanumeric words.
pub(crate) fn key_words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A markdown heading line (`# Title`).
pub(crate) fn is_heading(line: &str) -> bool {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    n > 0 && t[n..].starts_with(' ')
}

/// The cells of a pipe-table line (`a | b`, `| a | b |`), `None` for any other line.
pub(crate) fn cells(line: &str) -> Option<Vec<&str>> {
    let t = line.trim();
    if !t.contains('|') {
        return None;
    }
    let inner = t.trim_start_matches('|').trim_end_matches('|');
    Some(inner.split('|').map(str::trim).collect())
}

/// A table's header separator line (`|---|:--:|`).
fn is_separator(line: &str) -> bool {
    cells(line).is_some_and(|c| {
        c.iter()
            .all(|x| !x.is_empty() && x.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
    })
}

/// A roll or number cell ("1", "2-3", "d6", "01–05").
fn is_roll(cell: &str) -> bool {
    let t = cell.trim();
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | 'd' | 'D' | '+' | ' '))
}

/// The cell that names a row's option: the first cell that is not a roll, emphasis and
/// footnote marks left out.
fn name_cell<'a>(row: &[&'a str]) -> Option<(usize, &'a str)> {
    row.iter()
        .enumerate()
        .find(|(_, c)| !c.is_empty() && !is_roll(c))
        .map(|(k, c)| (k, c.trim_matches(['*', '_', '†', ' '])))
}

/// `words` start with the name `key` (as whole words).
fn led_by(words: &[String], key: &[String]) -> bool {
    !key.is_empty() && words.len() >= key.len() && words[..key.len()] == *key
}

/// One printed row of an option.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    /// 0-based first line.
    pub line: usize,
    /// 0-based end (exclusive): the row and its wrapped lines.
    pub end: usize,
    /// A pipe-table row (else a line that starts with the name).
    pub piped: bool,
}

/// The pipe row naming `key` at line `k` (its naming cell is the name, a note in brackets after
/// it allowed).
fn piped_row(lines: &[&str], k: usize, key: &[String]) -> bool {
    let Some(row) = cells(lines[k]).filter(|_| !is_separator(lines[k])) else {
        return false;
    };
    name_cell(&row).is_some_and(|(_, c)| {
        let w = key_words(c);
        w == key || key_words(c.split(" (").next().unwrap_or(c)) == key
    })
}

/// The line at `k` starts with the name `key` and is no heading or table line.
fn led_row(lines: &[&str], k: usize, key: &[String]) -> bool {
    let t = lines[k].trim();
    if t.is_empty() || is_heading(t) || cells(t).is_some() {
        return false;
    }
    let t = t.trim_start_matches(['-', '•', '*', '+', ' ']);
    led_by(&key_words(t), key)
}

/// The printed row of each name (`None` when the source prints no row for it). A name prefers
/// a pipe row; a flattened row runs to the next row, blank line or heading.
pub(crate) fn find_rows(source: &str, names: &[String]) -> Vec<Option<Row>> {
    let lines: Vec<&str> = source.lines().collect();
    let keys: Vec<Vec<String>> = names.iter().map(|n| key_words(n)).collect();
    let starts: Vec<Option<(usize, bool)>> = keys
        .iter()
        .map(|key| {
            (0..lines.len())
                .find(|&k| piped_row(&lines, k, key))
                .map(|k| (k, true))
                .or_else(|| {
                    (0..lines.len())
                        .find(|&k| led_row(&lines, k, key))
                        .map(|k| (k, false))
                })
        })
        .collect();
    let row_lines: Vec<usize> = starts.iter().flatten().map(|(k, _)| *k).collect();
    starts
        .iter()
        .map(|s| {
            let (line, piped) = (*s)?;
            let mut end = line + 1;
            while end < lines.len() && end < line + MAX_ROW_LINES && !row_lines.contains(&end) {
                let next = lines[end];
                let more = if piped {
                    // A wrapped cell: a pipe line with an empty naming cell.
                    cells(next).is_some_and(|c| c.first().is_some_and(|x| x.is_empty()))
                        && !is_separator(next)
                } else {
                    !next.trim().is_empty() && !is_heading(next) && cells(next).is_none()
                };
                if !more {
                    break;
                }
                end += 1;
            }
            Some(Row { line, end, piped })
        })
        .collect()
}

/// The rows of one printed table: a run of rows with at most [`MAX_ROW_GAP`] blank lines and
/// nothing else between them. Returns groups of indexes into `rows`, in source order.
pub(crate) fn blocks(source: &str, rows: &[Option<Row>]) -> Vec<Vec<usize>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut order: Vec<usize> = (0..rows.len()).filter(|k| rows[*k].is_some()).collect();
    order.sort_by_key(|k| rows[*k].as_ref().map_or(0, |r| r.line));
    order.dedup_by_key(|k| rows[*k].as_ref().map_or(0, |r| r.line));
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut last_end: Option<usize> = None;
    for k in order {
        let Some(r) = rows[k].as_ref() else { continue };
        let joined = last_end.is_some_and(|e| {
            r.line >= e
                && r.line - e <= MAX_ROW_GAP
                && lines[e..r.line].iter().all(|l| l.trim().is_empty())
        });
        match out.last_mut() {
            Some(b) if joined => b.push(k),
            _ => out.push(vec![k]),
        }
        last_end = Some(r.end);
    }
    out
}

/// The header cells of the pipe table a row at `line` belongs to (the line above its
/// separator), if it has one.
fn header(lines: &[&str], line: usize) -> Option<Vec<String>> {
    let mut k = line;
    while k > 0 && cells(lines[k - 1]).is_some() {
        k -= 1;
    }
    (k + 1 < lines.len() && is_separator(lines[k + 1]))
        .then(|| cells(lines[k]).map(|c| c.iter().map(|x| x.to_string()).collect()))
        .flatten()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// What a row prints: HTML of the row alone (a pipe row with its table's header, a flattened
/// row as one paragraph) and whether it prints anything beyond the name.
pub(crate) fn row_html(source: &str, name: &str, row: &Row) -> (String, bool) {
    let lines: Vec<&str> = source.lines().collect();
    let key = key_words(name);
    if !row.piped {
        let text: Vec<&str> = lines[row.line..row.end]
            .iter()
            .map(|l| l.trim().trim_start_matches(['-', '•', '*', '+', ' ']))
            .collect();
        let joined = text.join(" ");
        let effect = key_words(&joined).len() > key.len();
        return (format!("<p>{}</p>", escape(joined.trim())), effect);
    }
    let rows: Vec<Vec<&str>> = lines[row.line..row.end]
        .iter()
        .filter_map(|l| cells(l))
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    // Wrapped cells join their row's cell.
    let merged: Vec<String> = (0..width)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c).copied())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let named = name_cell(&rows[0]).map_or(0, |(k, _)| k);
    let effect = merged
        .iter()
        .enumerate()
        .any(|(k, c)| k != named && !c.is_empty() && !is_roll(c));
    let td = |c: &String| format!("<td>{}</td>", escape(c));
    let body = format!("<tr>{}</tr>", merged.iter().map(td).collect::<String>());
    let html = match header(&lines, row.line).filter(|h| h.len() == width && width > 1) {
        Some(h) => format!(
            "<table><tr>{}</tr>{body}</table>",
            h.iter()
                .map(|c| format!("<th>{}</th>", escape(c)))
                .collect::<String>()
        ),
        None if width > 1 => format!("<table>{body}</table>"),
        None => format!("<p>{}</p>", escape(&merged.join(" "))),
    };
    (html, effect)
}

/// The number a pick phrase prints ("choose two", "learn 3 of the following"), if any.
pub(crate) fn printed_count(text: &str) -> Option<u32> {
    let w = key_words(text);
    w.iter().enumerate().find_map(|(k, word)| {
        if !PICK_VERBS.iter().any(|v| word.starts_with(v)) {
            return None;
        }
        w[k + 1..].iter().take(PICK_REACH).find_map(|x| {
            COUNTS
                .iter()
                .position(|c| c == x)
                .map(|p| p as u32 + 1)
                .or_else(|| x.parse::<u32>().ok().filter(|n| (1..=20).contains(n)))
        })
    })
}

/// The pick count the rule prints above the table starting at `line` (up to its heading).
pub(crate) fn lead_count(source: &str, line: usize) -> Option<u32> {
    let lines: Vec<&str> = source.lines().collect();
    let from = (line.saturating_sub(LEAD_LINES)..line)
        .rev()
        .find(|&k| is_heading(lines[k]))
        .map_or(line.saturating_sub(LEAD_LINES), |k| k + 1);
    let lead: Vec<&str> = lines[from..line]
        .iter()
        .filter(|l| cells(l).is_none())
        .copied()
        .collect();
    printed_count(&lead.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIPED: &str = "# Tricks\n\nLearn two tricks from the table.\n\n\
| d4 | Trick | Effect |\n|---|---|---|\n| 1 | Low Sweep | The target falls prone. |\n\
| | | It can stand at once. |\n| 2 | High Feint | |\n\n# Next\n";

    const FLAT: &str = "# Gambits\n\nChoose any three gambits.\n\n\
Quick Step\nDart Away You move 10 feet\nwithout provoking.\nLow Blow\n\nAfter the table.\n";

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_pipe_row_is_found_past_its_roll_cell_with_its_wrapped_line() {
        let rows = find_rows(PIPED, &names(&["Low Sweep", "High Feint", "Missing"]));
        assert_eq!(rows[0].as_ref().map(|r| (r.line, r.end)), Some((6, 8)));
        assert_eq!(rows[1].as_ref().map(|r| r.line), Some(8));
        assert!(rows[2].is_none());
        let (html, effect) = row_html(PIPED, "Low Sweep", rows[0].as_ref().unwrap());
        assert_eq!(
            html,
            "<table><tr><th>d4</th><th>Trick</th><th>Effect</th></tr><tr><td>1</td><td>Low Sweep</td><td>The target falls prone. It can stand at once.</td></tr></table>"
        );
        assert!(effect);
        // A row printing only its name has no effect text.
        assert!(!row_html(PIPED, "High Feint", rows[1].as_ref().unwrap()).1);
        assert_eq!(blocks(PIPED, &rows), vec![vec![0, 1]]);
    }

    #[test]
    fn a_flattened_table_is_rows_led_by_the_names() {
        let n = names(&["Quick Step", "Dart Away", "Low Blow"]);
        let rows = find_rows(FLAT, &n);
        let spans: Vec<(usize, usize)> = rows.iter().flatten().map(|r| (r.line, r.end)).collect();
        assert_eq!(spans, vec![(4, 5), (5, 7), (7, 8)]);
        assert_eq!(blocks(FLAT, &rows), vec![vec![0, 1, 2]]);
        let (html, effect) = row_html(FLAT, "Dart Away", rows[1].as_ref().unwrap());
        assert_eq!(html, "<p>Dart Away You move 10 feet without provoking.</p>");
        assert!(effect);
        assert!(!row_html(FLAT, "Low Blow", rows[2].as_ref().unwrap()).1);
    }

    #[test]
    fn the_pick_count_is_read_from_the_rule_above_the_table() {
        assert_eq!(lead_count(PIPED, 6), Some(2));
        assert_eq!(lead_count(FLAT, 4), Some(3));
        assert_eq!(printed_count("You learn 4 of the following."), Some(4));
        assert_eq!(printed_count("The table lists the tricks."), None);
    }
}
