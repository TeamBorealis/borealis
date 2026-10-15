//! Spell names a feature prints, read deterministically: the rows of its spell table ("3rd |
//! a, b") and the cantrips it says you gain ("you learn the a, b, and c cantrips"). The
//! outline's model-written list can drop a member (a trailing one, one after an "and", one
//! wrapped over a line or a page break) or carry a printed name the library spells another way
//! ("firebolt", a footnote mark glued to it): the printed names that are spells of the import
//! or the library fill those gaps. A printed piece that names no spell is never granted.

use std::collections::HashMap;

use borealis_content::ContentStore;

use super::super::fewshot::in_edition;
use super::super::types::{Edition, PlannedElement};

/// Most lines of a feature's own section read for its spell names.
const MAX_LINES: usize = 80;
/// Suffixes of an ordinal level ("3rd").
const ORDINALS: [&str; 4] = ["st", "nd", "rd", "th"];
/// Verbs that give a named cantrip ("you learn the ... cantrip").
const GRANT_VERBS: [&str; 6] = ["learn", "learns", "know", "knows", "gain", "gains"];
/// Footnote marks a printed spell name can carry.
const MARKS: [char; 7] = ['*', '†', '‡', '§', '¹', '²', '³'];
/// Most letters of a footnote marker glued to a name's last word ("mistSh").
const MAX_GLUED_MARKER: usize = 3;

/// Lower-case letters and digits only: "Fire Bolt", "firebolt" and "Fire-Bolt" share a key.
pub(super) fn key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The spells a printed name can stand for: the library's of the edition (with their spell
/// level) and the import's own (level not known at plan time).
pub(crate) struct SpellBook {
    spells: HashMap<String, (String, Option<u32>)>,
}

impl SpellBook {
    pub(crate) fn new(store: &ContentStore, edition: Edition, own: &[String]) -> Self {
        let mut spells: HashMap<String, (String, Option<u32>)> = store
            .by_type("Spell")
            .map(|s| &s.element)
            .filter(|e| in_edition(e, edition))
            .map(|e| {
                let level = e.setter_value("level").and_then(|v| v.trim().parse().ok());
                (key(&e.name), (e.name.clone(), level))
            })
            .collect();
        spells.extend(own.iter().map(|n| (key(n), (n.clone(), None))));
        Self { spells }
    }

    /// A book of just these spells (name, spell level).
    #[cfg(test)]
    pub(crate) fn of(names: &[(&str, u32)]) -> Self {
        Self {
            spells: names
                .iter()
                .map(|(n, l)| (key(n), ((*n).to_string(), Some(*l))))
                .collect(),
        }
    }

    /// The spell `printed` names (as printed, then without footnote marks, a parenthetical note
    /// or a marker glued to its last word): its name and spell level.
    pub(crate) fn find(&self, printed: &str) -> Option<&(String, Option<u32>)> {
        candidates(printed)
            .iter()
            .find_map(|c| self.spells.get(&key(c)).filter(|_| !key(c).is_empty()))
    }
}

fn candidates(printed: &str) -> Vec<String> {
    let t = printed
        .trim()
        .trim_matches(|c: char| MARKS.contains(&c) || c.is_whitespace() || c == '.');
    let mut out = vec![t.to_string()];
    if let Some((head, _)) = t.split_once(" (") {
        out.push(head.trim().to_string());
    }
    if let Some(cut) = glued_marker(t) {
        out.push(t[..cut].to_string());
    }
    out
}

/// Where a footnote marker glued to the last word starts ("mistSh": a capital after a
/// lower-case letter, then at most [`MAX_GLUED_MARKER`] letters).
fn glued_marker(t: &str) -> Option<usize> {
    let last = t.rsplit(' ').next()?;
    let base = t.len() - last.len();
    let chars: Vec<(usize, char)> = last.char_indices().collect();
    let at = (1..chars.len())
        .rev()
        .find(|&i| chars[i].1.is_uppercase() && chars[i - 1].1.is_lowercase())?;
    let tail = &chars[at..];
    (tail.len() <= MAX_GLUED_MARKER && tail.iter().all(|(_, c)| c.is_alphabetic()))
        .then(|| base + chars[at].0)
}

fn strip_join(piece: &str) -> &str {
    let mut p = piece.trim();
    loop {
        let next = ["and ", "or ", "& "]
            .iter()
            .find_map(|j| p.strip_prefix(j))
            .map(str::trim);
        match next {
            Some(n) => p = n,
            None => return p,
        }
    }
}

/// The spells a printed list names (name and spell level), split on commas, semicolons and
/// "and" / "or" joins; a piece that is a spell as a whole ("Food and Water") stays whole.
/// Pieces that name no spell are left out.
pub(crate) fn members(list: &str, book: &SpellBook) -> Vec<(String, Option<u32>)> {
    let mut out: Vec<(String, Option<u32>)> = Vec::new();
    let mut push = |found: &(String, Option<u32>)| {
        if !out.iter().any(|(n, _)| key(n) == key(&found.0)) {
            out.push(found.clone());
        }
    };
    for piece in list.split([',', ';']).map(strip_join) {
        if piece.is_empty() {
            continue;
        }
        if let Some(found) = book.find(piece) {
            push(found);
            continue;
        }
        for part in piece
            .split(" and ")
            .flat_map(|x| x.split(" or "))
            .flat_map(|x| x.split(" & "))
        {
            if let Some(found) = book.find(part) {
                push(found);
            }
        }
    }
    out
}

/// The class level a table row or list line opens with ("3rd", "5th level", "Level 7", "9"),
/// 1 to 20.
fn row_level(cell: &str) -> Option<u32> {
    let lower = cell.trim().to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty() && !matches!(*w, "level" | "lvl" | "lvl."))
        .collect();
    let [w] = words.as_slice() else {
        return None;
    };
    let digits: String = w.chars().take_while(char::is_ascii_digit).collect();
    let rest = &w[digits.len()..];
    let n: u32 = digits.parse().ok()?;
    ((rest.is_empty() || ORDINALS.contains(&rest)) && (1..=20).contains(&n)).then_some(n)
}

/// The rows of a spell table (or level list) printed in `lines`: each row's class level and
/// its spell cells. A wrapped cell (a lower-case line right under a row) joins its row.
pub(crate) fn level_rows(lines: &[&str]) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    let mut open = false;
    for line in lines {
        let t = line.trim();
        let row = if t.contains('|') {
            let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
            cells
                .first()
                .and_then(|c| row_level(c))
                .map(|l| (l, cells[1..].join(", ")))
        } else {
            t.split_once(':')
                .and_then(|(head, rest)| Some((row_level(head)?, rest.trim().to_string())))
        };
        match row {
            Some(r) => {
                out.push(r);
                open = true;
            }
            None if open && t.starts_with(char::is_lowercase) => {
                if let Some(last) = out.last_mut() {
                    last.1.push(' ');
                    last.1.push_str(t);
                }
            }
            None => open = false,
        }
    }
    out
}

/// The printed lists of cantrips `lines` say you gain: "you learn the a, b, and c cantrips",
/// "you know the d cantrip" (a verb of [`GRANT_VERBS`], then "the", then the names, in one
/// sentence).
pub(crate) fn named_cantrips(lines: &[&str]) -> Vec<String> {
    let text = lines
        .iter()
        .flat_map(|l| l.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut out = Vec::new();
    for (at, _) in text.match_indices("cantrip") {
        if !text[..at].ends_with(' ') {
            continue;
        }
        let pre = &text[..at];
        let from = pre.rfind(['.', '!', '?']).map_or(0, |k| k + 1);
        let words: Vec<&str> = pre[from..].split_whitespace().collect();
        let Some(the) = (1..words.len())
            .rev()
            .find(|&j| words[j] == "the" && GRANT_VERBS.contains(&words[j - 1]))
        else {
            continue;
        };
        let list = words[the + 1..].join(" ");
        if !list.is_empty() {
            out.push(list);
        }
    }
    out
}

fn heading(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then_some(n)
}

/// The lines of `p`'s own section: its heading's section (to the next heading of its level
/// or higher), or its located passage without a heading; at most [`MAX_LINES`].
pub(crate) fn own_lines<'a>(source: &'a str, p: &PlannedElement) -> Vec<&'a str> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(ex) = p.excerpt.as_ref() else {
        return Vec::new();
    };
    let s = ex.start_line.saturating_sub(1);
    if s >= lines.len() {
        return Vec::new();
    }
    let end = match heading(lines[s]) {
        Some(h) => (s + 1..lines.len())
            .find(|&k| heading(lines[k]).is_some_and(|k| k <= h))
            .unwrap_or(lines.len()),
        None => ex.end_line.max(s + 1),
    };
    lines[s..end.min(lines.len()).min(s + MAX_LINES)].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(names: &[(&str, u32)]) -> SpellBook {
        SpellBook::of(names)
    }

    #[test]
    fn a_list_splits_on_commas_and_joins_but_keeps_a_spell_named_with_and() {
        let b = book(&[
            ("Mist Step", 2),
            ("Fog Cloud", 1),
            ("Bread and Salt", 3),
            ("Ember Lash", 0),
        ]);
        let names =
            |list: &str| -> Vec<String> { members(list, &b).into_iter().map(|(n, _)| n).collect() };
        assert_eq!(
            names("mist step, fog cloud, and bread and salt"),
            ["Mist Step", "Fog Cloud", "Bread and Salt"]
        );
        assert_eq!(names("fog cloud and mist step"), ["Fog Cloud", "Mist Step"]);
        assert_eq!(names("fog cloud or mist step"), ["Fog Cloud", "Mist Step"]);
        // Marks, notes and a marker glued by the PDF; a name spelled as one word.
        assert_eq!(
            names("mist step*, fog cloud (ritual); emberlashSh"),
            ["Mist Step", "Fog Cloud", "Ember Lash"]
        );
        // A piece that names no spell is never a member.
        assert_eq!(names("a pebble, mist step"), ["Mist Step"]);
    }

    #[test]
    fn spell_table_rows_keep_their_level_and_wrapped_cells() {
        let lines = [
            "Grove Level | Grove Spells",
            "3rd | mist step, fog",
            "cloud",
            "5th level | bread and salt",
            "!",
            "7 | ember lash",
            "9th: fog cloud",
            "11 apples | no",
        ];
        assert_eq!(
            level_rows(&lines),
            vec![
                (3, "mist step, fog cloud".to_string()),
                (5, "bread and salt".to_string()),
                (7, "ember lash".to_string()),
                (9, "fog cloud".to_string()),
            ]
        );
    }

    #[test]
    fn named_cantrips_are_read_over_wrapped_lines_but_choices_are_not() {
        let lines = [
            "When you choose this grove at 3rd level, you gain the",
            "ember lash, sparkle, and frost pinch cantrips and learn one language.",
            "You also know the glimmer cantrip. You learn one cantrip of your choice.",
            "When you cast the ember lash cantrip, it burns.",
        ];
        assert_eq!(
            named_cantrips(&lines),
            ["ember lash, sparkle, and frost pinch", "glimmer"]
        );
    }
}
