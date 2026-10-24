//! Named options printed as the rows of a table: where each row is, the text it prints (and
//! nothing else), and how many of them the rule lets a character pick. A row is a pipe-table
//! row whose naming cell is the option's name (a roll column before it is skipped), or a line
//! that starts with the name (a table the reader flattened into lines, a list of options).

/// Most lines between two rows of one table (blank lines, rows the detection did not name,
/// a caption such as a tier or page line).
const MAX_ROW_GAP: usize = 6;
/// Most blank lines between two lines led by names that keep them in one table.
const MAX_FLAT_GAP: usize = 2;
/// Most caption lines between two rows of one table ("2nd Tier (base cost: 1,000 gp)").
const MAX_GAP_CAPTIONS: usize = 2;
/// Most words of a caption line inside a table.
const CAPTION_WORDS: usize = 8;
/// Most lines of one pipe row (the row line and its wrapped text).
const MAX_ROW_LINES: usize = 20;
/// Most lines of one row of a flattened table (the name line and its wrapped text).
const MAX_FLAT_ROW_LINES: usize = 8;
/// Most lines above a table read for the rule's pick count.
const LEAD_LINES: usize = 15;
/// Number words a pick count is printed as.
const COUNTS: [&str; 10] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];
/// Verbs that open a pick ("choose two", "learn three of the following").
const PICK_VERBS: [&str; 7] = ["choose", "select", "pick", "learn", "gain", "take", "know"];
/// Words that say a character picks from a list even without a count ("of your choice").
const PICK_WORDS: [&str; 7] = [
    "choose", "chosen", "choice", "select", "selected", "pick", "picked",
];
/// Words that say a table is rolled on, not picked from.
const ROLL_WORDS: [&str; 5] = ["roll", "rolls", "random", "randomly", "determine"];
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

/// A die cell of a table's header ("d4", "1d20", "d100", "d%").
fn is_die(cell: &str) -> bool {
    let t = cell.trim().to_lowercase();
    let t = t.trim_start_matches(|c: char| c.is_ascii_digit());
    t.strip_prefix('d')
        .is_some_and(|n| n == "%" || (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
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

/// The cell names `key`: the name alone, or the name and a note or its effect after a mark
/// ("Name (2nd level)", "Name. Effect", "Name: effect", "Name*").
fn cell_names(cell: &str, key: &[String]) -> bool {
    let c = cell.trim().trim_start_matches(['*', '_', '•', '-', ' ']);
    if key_words(c) == key {
        return true;
    }
    if !led_by(&key_words(c), key) {
        return false;
    }
    // The text right after the name starts with a mark, never another word of a longer name.
    let mut rest = c;
    for w in key {
        let Some(at) = rest.to_lowercase().find(w.as_str()) else {
            return false;
        };
        rest = &rest[(at + w.len()).min(rest.len())..];
    }
    rest.trim_start_matches(['*', '_', '†'])
        .trim_start()
        .starts_with(['(', '.', ':', '—', '–', '-', ','])
}

/// The index of the cell of `row` that names `key`.
fn named_at(row: &[&str], key: &[String]) -> Option<usize> {
    row.iter().position(|c| cell_names(c, key))
}

/// The line ends a sentence ("... weapon.", "... size.8" with a footnote mark).
fn ends_sentence(line: &str) -> bool {
    let t = line.trim_end();
    // A closed table row ("| a | b |") is finished.
    if t.ends_with('|') {
        return true;
    }
    let bare = t.trim_end_matches(|c: char| c.is_ascii_digit());
    let t = if bare.ends_with(['.', '!', '?', ')']) {
        bare
    } else {
        t
    };
    t.ends_with(['.', '!', '?', ':', ';', ')'])
}

/// A short caption line inside a table ("2nd Tier (base cost: 1,000 gp)", "Uncommon"): no
/// heading, no table line, at most [`CAPTION_WORDS`] words, no sentence end.
fn is_caption(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty()
        && !is_heading(t)
        && cells(t).is_none()
        && t.split_whitespace().count() <= CAPTION_WORDS
        && !t.ends_with(['.', '!', '?', ':', ';'])
}

/// The line after a row's line `prev` continues that row's text (a wrapped cell): a plain
/// line that is no caption after a finished sentence.
fn continues_row(prev: &str, next: &str) -> bool {
    let t = next.trim();
    !t.is_empty() && !is_heading(t) && cells(t).is_none() && !(is_caption(t) && ends_sentence(prev))
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

/// The pipe row naming `key` at line `k` (any of its cells names it).
fn piped_row(lines: &[&str], k: usize, key: &[String]) -> bool {
    !is_separator(lines[k]) && cells(lines[k]).is_some_and(|row| named_at(&row, key).is_some())
}

/// The line at `k` starts with the name `key` (capitalised) and is no heading or table line.
fn led_row(lines: &[&str], k: usize, key: &[String]) -> bool {
    let t = lines[k].trim();
    if t.is_empty() || is_heading(t) || cells(t).is_some() {
        return false;
    }
    let t = t.trim_start_matches(['-', '•', '*', '+', ' ']);
    // A row opens with the name as a name (capitalised), never a sentence's lowercase words.
    t.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase)
        && led_by(&key_words(t), key)
}

/// The lines that may be the row of `key`: its pipe rows when it has any, else the lines
/// that start with it.
fn candidates(lines: &[&str], key: &[String]) -> (Vec<usize>, bool) {
    let piped: Vec<usize> = (0..lines.len())
        .filter(|&k| piped_row(lines, k, key))
        .collect();
    if !piped.is_empty() {
        return (piped, true);
    }
    let led = (0..lines.len())
        .filter(|&k| led_row(lines, k, key))
        .collect();
    (led, false)
}

/// The line of each name among its candidates: a name printed several times (the same name
/// in two tables) takes the occurrence of its rank among the entries of that name; a name
/// the source prints once as a row but again elsewhere takes the occurrence nearest the other
/// names' rows (the table, not a mention).
fn choose_lines(keys: &[Vec<String>], cands: &[(Vec<usize>, bool)]) -> Vec<Option<usize>> {
    let first: Vec<Option<usize>> = cands.iter().map(|(c, _)| c.first().copied()).collect();
    (0..keys.len())
        .map(|k| {
            let (lines, _) = &cands[k];
            let same: Vec<usize> = (0..keys.len()).filter(|&j| keys[j] == keys[k]).collect();
            if same.len() > 1 {
                let rank = same.iter().position(|&j| j == k).unwrap_or(0);
                return lines.get(rank).or(lines.last()).copied();
            }
            if lines.len() < 2 {
                return lines.first().copied();
            }
            let others: Vec<usize> = (0..keys.len())
                .filter(|&j| keys[j] != keys[k])
                .filter_map(|j| first[j])
                .collect();
            lines
                .iter()
                .copied()
                .min_by_key(|&l| others.iter().map(|&o| o.abs_diff(l)).min().unwrap_or(0))
        })
        .collect()
}

/// The printed row of each name (`None` when the source prints no row for it). A name prefers
/// a pipe row; a row runs over its wrapped lines (a pipe line with an empty first cell, or
/// plain text that continues the cell) to the next row, blank line, caption or heading.
pub(crate) fn find_rows(source: &str, names: &[String]) -> Vec<Option<Row>> {
    let lines: Vec<&str> = source.lines().collect();
    let keys: Vec<Vec<String>> = names.iter().map(|n| key_words(n)).collect();
    let cands: Vec<(Vec<usize>, bool)> = keys.iter().map(|k| candidates(&lines, k)).collect();
    let chosen = choose_lines(&keys, &cands);
    let row_lines: Vec<usize> = chosen.iter().flatten().copied().collect();
    chosen
        .iter()
        .zip(&cands)
        .map(|(at, (_, piped))| {
            let (line, piped) = ((*at)?, *piped);
            let mut end = line + 1;
            let most = if piped {
                MAX_ROW_LINES
            } else {
                MAX_FLAT_ROW_LINES
            };
            while end < lines.len() && end < line + most && !row_lines.contains(&end) {
                let next = lines[end];
                let wrapped_cell = piped
                    && cells(next).is_some_and(|c| c.first().is_some_and(|x| x.is_empty()))
                    && !is_separator(next);
                if !(wrapped_cell || continues_row(lines[end - 1], next)) {
                    break;
                }
                end += 1;
            }
            Some(Row { line, end, piped })
        })
        .collect()
}

/// The lines between two rows keep them in one table: blank lines, pipe lines of the rows'
/// `width` (rows the detection did not name, a repeated header) and at most
/// [`MAX_GAP_CAPTIONS`] captions; between lines led by names (no `width`), blank lines only.
fn same_table(gap: &[&str], width: Option<usize>) -> bool {
    // Lines led by the names (a flattened table, a list) are one table only with nothing but
    // a blank line or two between them.
    if width.is_none() {
        return gap.len() <= MAX_FLAT_GAP && gap.iter().all(|l| l.trim().is_empty());
    }
    let same_width = |l: &str| cells(l).is_some_and(|c| width.is_none_or(|w| c.len() == w));
    gap.len() <= MAX_ROW_GAP
        && gap
            .iter()
            .all(|l| l.trim().is_empty() || same_width(l) || is_caption(l))
        && gap.iter().filter(|l| is_caption(l)).count() <= MAX_GAP_CAPTIONS
}

/// The rows of one printed table: a run of rows of one kind (pipe rows, or lines led by the
/// names) with only blank lines, unnamed table lines and short captions between them. Returns groups of indexes into `rows`, in source order.
pub(crate) fn blocks(source: &str, rows: &[Option<Row>]) -> Vec<Vec<usize>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut order: Vec<usize> = (0..rows.len()).filter(|k| rows[*k].is_some()).collect();
    order.sort_by_key(|k| rows[*k].as_ref().map_or(0, |r| r.line));
    order.dedup_by_key(|k| rows[*k].as_ref().map_or(0, |r| r.line));
    let mut out: Vec<Vec<usize>> = Vec::new();
    let width = |r: &Row| r.piped.then(|| cells(lines[r.line]).map_or(0, |c| c.len()));
    let mut last: Option<(usize, bool, Option<usize>)> = None;
    for k in order {
        let Some(r) = rows[k].as_ref() else { continue };
        // Pipe rows and lines led by a name are never rows of one table; nor are pipe rows of
        // another width (the next table, its header naming a row).
        let joined = last.is_some_and(|(e, piped, w)| {
            piped == r.piped && w == width(r) && r.line >= e && same_table(&lines[e..r.line], w)
        });
        match out.last_mut() {
            Some(b) if joined => b.push(k),
            _ => out.push(vec![k]),
        }
        last = Some((r.end, r.piped, width(r)));
    }
    out
}

/// The header cells of the pipe table a row at `line` belongs to: the line above the table's
/// separator, found reading the table upwards over its rows, blank lines and captions.
fn header(lines: &[&str], line: usize) -> Option<Vec<String>> {
    let mut k = line;
    while k > 0 {
        let l = lines[k - 1];
        if is_separator(l) {
            let h = (0..k - 1).rev().find(|&j| !lines[j].trim().is_empty())?;
            return cells(lines[h]).map(|c| c.iter().map(|x| x.to_string()).collect());
        }
        if cells(l).is_some() || l.trim().is_empty() || is_caption(l) {
            k -= 1;
            continue;
        }
        // Plain lines are the wrapped text of the row above them, else the table ends.
        let top = (k.saturating_sub(MAX_ROW_LINES)..k).rev().find(|&j| {
            cells(lines[j]).is_some() || lines[j].trim().is_empty() || is_heading(lines[j])
        })?;
        cells(lines[top])?;
        k = top + 1;
    }
    None
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
    let mut rows: Vec<Vec<String>> = Vec::new();
    for l in &lines[row.line..row.end] {
        match cells(l) {
            Some(c) => rows.push(c.iter().map(|x| x.to_string()).collect()),
            // Plain text that wraps the row's last cell.
            None => {
                if let Some(cell) = rows.last_mut().and_then(|r| r.last_mut()) {
                    cell.push(' ');
                    cell.push_str(l.trim());
                }
            }
        }
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    // Wrapped cells join their row's cell.
    let merged: Vec<String> = (0..width)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c).map(|x| x.trim()))
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let first: Vec<&str> = rows
        .first()
        .map(|r| r.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let named = named_at(&first, &key)
        .or_else(|| name_cell(&first).map(|(k, _)| k))
        .unwrap_or(0);
    let effect = merged
        .iter()
        .enumerate()
        .any(|(k, c)| k != named && !c.is_empty() && !is_roll(c))
        || merged
            .get(named)
            .is_some_and(|c| key_words(c).len() > key.len());
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

/// The rule text printed above the table starting at `line`, from `from` (the rule's heading
/// line, or the nearest heading within [`LEAD_LINES`]) down, table lines left out.
pub(crate) fn lead_text(source: &str, from: Option<usize>, line: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line = line.min(lines.len());
    let near = (line.saturating_sub(LEAD_LINES)..line)
        .rev()
        .find(|&k| is_heading(lines[k]))
        .map_or(line.saturating_sub(LEAD_LINES), |k| k + 1);
    let from = from.filter(|&f| f < line).map_or(near, |f| f + 1);
    lines[from..line]
        .iter()
        .filter(|l| cells(l).is_none())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text says a character picks ("choose", "of your choice", "select"), or prints a pick
/// count ("learn two").
pub(crate) fn says_pick(text: &str) -> bool {
    printed_count(text).is_some()
        || key_words(text)
            .iter()
            .any(|w| PICK_WORDS.contains(&w.as_str()))
}

/// The text says the table is rolled on ("roll", "at random", "to determine").
pub(crate) fn says_roll(text: &str) -> bool {
    key_words(text)
        .iter()
        .any(|w| ROLL_WORDS.contains(&w.as_str()))
}

/// The rows are those of a roll table: their table's header opens with a die ("d4", "d20"),
/// or most of them open with a roll number while their name is in a later cell.
pub(crate) fn rolled_rows(source: &str, rows: &[&Row]) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let piped: Vec<Vec<&str>> = rows
        .iter()
        .filter(|r| r.piped)
        .filter_map(|r| cells(lines[r.line]))
        .collect();
    if piped.is_empty() {
        return false;
    }
    let die_header = rows
        .iter()
        .filter(|r| r.piped)
        .any(|r| die_head(&lines, r.line).is_some_and(|h| h.first().is_some_and(|c| is_die(c))));
    let numbered = piped
        .iter()
        .filter(|c| c.len() > 1 && c.first().is_some_and(|x| is_roll(x)))
        .count();
    die_header || numbered * 2 > piped.len()
}

/// The first pipe line above the rows of a table (its header, separator or not), read over
/// the table's rows, blank lines and captions.
fn die_head<'a>(lines: &[&'a str], line: usize) -> Option<Vec<&'a str>> {
    let mut k = line;
    let mut top = None;
    while k > 0 {
        let l = lines[k - 1];
        if cells(l).is_some() && !is_separator(l) {
            top = Some(k - 1);
        } else if !(l.trim().is_empty() || is_separator(l) || is_caption(l)) {
            break;
        }
        k -= 1;
    }
    top.and_then(|t| cells(lines[t]))
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
        let count = |src: &str, line: usize| printed_count(&lead_text(src, None, line));
        assert_eq!(count(PIPED, 6), Some(2));
        assert_eq!(count(FLAT, 4), Some(3));
        assert_eq!(printed_count("You learn 4 of the following."), Some(4));
        assert_eq!(printed_count("The table lists the tricks."), None);
    }

    /// A table the reader printed with a level column first, a name with its effect in one
    /// cell, a cell wrapped onto a plain line, blank lines between rows and a tier caption.
    const LEVELLED: &str = "# Gambits\n\n## Gambit Table\n\n\
| Level | Gambit | Effect |\n|---|---|---|\n\
| 1st | Quick Step | You move 5 feet and the target\n\
cannot take reactions until your next turn.\n\n\
| 1st | Low Blow: the target is knocked prone. | |\n\
Second Tier\n\
| 3rd | Iron Feint | You gain advantage. |\n\n# Next\n";

    #[test]
    fn rows_are_found_by_any_cell_over_wrapped_lines_blank_lines_and_captions() {
        let n = names(&["Quick Step", "Low Blow", "Iron Feint"]);
        let rows = find_rows(LEVELLED, &n);
        let spans: Vec<(usize, usize)> = rows.iter().flatten().map(|r| (r.line, r.end)).collect();
        assert_eq!(spans, vec![(6, 8), (9, 10), (11, 12)]);
        assert_eq!(blocks(LEVELLED, &rows), vec![vec![0, 1, 2]]);
        let (html, effect) = row_html(LEVELLED, "Quick Step", rows[0].as_ref().unwrap());
        assert_eq!(
            html,
            "<table><tr><th>Level</th><th>Gambit</th><th>Effect</th></tr><tr><td>1st</td><td>Quick Step</td><td>You move 5 feet and the target cannot take reactions until your next turn.</td></tr></table>"
        );
        assert!(effect);
        // The name and its effect in one cell is a row printing an effect; the header is
        // found across the blank line and the caption.
        let (html, effect) = row_html(LEVELLED, "Low Blow", rows[1].as_ref().unwrap());
        assert!(effect);
        assert!(html.contains("<th>Gambit</th>"), "{html}");
        let (html, _) = row_html(LEVELLED, "Iron Feint", rows[2].as_ref().unwrap());
        assert!(html.contains("<th>Gambit</th>"), "{html}");
        // The header of the next, wider table is no row of this one, even when it names one.
        let two = "Tag | Effect\nKeen | Sharp.\nLight | Quick.\nGrip | Firm.\n\
Fittings | Cost | Effect\nHood | 1 gp | Quiet.\n";
        let rows = find_rows(two, &names(&["Keen", "Light", "Grip", "Fittings"]));
        assert_eq!(blocks(two, &rows), vec![vec![0, 1, 2], vec![3]]);
        // A longer name is no row of a shorter one.
        assert!(find_rows("| Quick Stepper | x |", &names(&["Quick Step"]))[0].is_none());
    }

    #[test]
    fn a_name_printed_twice_takes_the_row_of_its_own_table() {
        let src = "Quick Step is a gambit.\n\n| d4 | Beast |\n|---|---|\n| 1 | Owl |\n\
| 2 | Rat |\n\nText.\n\n| d6 | Beast |\n|---|---|\n| 1 | Rat |\n| 2 | Toad |\n";
        let rows = find_rows(src, &names(&["Owl", "Rat", "Rat", "Toad"]));
        let lines: Vec<usize> = rows.iter().flatten().map(|r| r.line).collect();
        assert_eq!(lines, vec![4, 5, 11, 12]);
        // A mention in prose is never chosen over the row near the other names' rows.
        let flat = "Low Blow hurts.\n\n# Gambits\n\nQuick Step moves.\nLow Blow trips.\n";
        let rows = find_rows(flat, &names(&["Quick Step", "Low Blow"]));
        assert_eq!(rows[1].as_ref().map(|r| r.line), Some(5));
    }

    #[test]
    fn roll_tables_and_pick_wording_are_told_apart() {
        let src = "| d4 | Beast |\n|---|---|\n| 1 | Owl |\n| 2 | Rat |\n| 3 | Toad |\n";
        let rows = find_rows(src, &names(&["Owl", "Rat", "Toad"]));
        let refs: Vec<&Row> = rows.iter().flatten().collect();
        assert!(rolled_rows(src, &refs));
        let rows = find_rows(LEVELLED, &names(&["Quick Step", "Low Blow", "Iron Feint"]));
        let refs: Vec<&Row> = rows.iter().flatten().collect();
        assert!(!rolled_rows(LEVELLED, &refs));
        assert!(says_pick("You know gambits of your choice."));
        assert!(says_pick("Learn two of them."));
        assert!(!says_pick("The table lists the gambits."));
        assert!(says_roll("Roll on the table to determine the beast."));
        assert!(!says_roll("Choose a beast."));
    }
}
