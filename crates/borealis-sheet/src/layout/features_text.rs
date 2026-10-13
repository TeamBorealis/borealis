//! Text measuring and splitting for the features, gear and story pages: the same greedy wrap the
//! form writer uses (so a measured height is the height the widget draws), paragraph-aware
//! splitting for continuation pages, and the unique-id registry (`@k` continuation suffixes).

use crate::fonts::Style;
use std::collections::HashSet;

/// Lines of `text` as the multi-line widget wraps them: `\n` ends a paragraph, words wider than a
/// line are hard-broken, an empty paragraph is one empty line.
pub(crate) fn wrap_field(style: Style, size: f32, max: f32, text: &str) -> Vec<String> {
    crate::pdf_form::wrap(text, style, size, max.max(1.0))
}

/// Number of lines `text` takes (at least one).
pub(crate) fn line_count(style: Style, size: f32, max: f32, text: &str) -> usize {
    wrap_field(style, size, max, text).len().max(1)
}

/// Split `text` so the head takes at most `max_lines` lines. Splits between paragraphs when the
/// head keeps at least `min_head` lines, otherwise between lines. `None` when it all fits.
pub(crate) fn split_text(
    style: Style,
    size: f32,
    max: f32,
    text: &str,
    max_lines: usize,
    min_head: usize,
) -> Option<(String, String)> {
    if line_count(style, size, max, text) <= max_lines {
        return None;
    }
    let paras: Vec<&str> = text.split('\n').collect();
    let counts: Vec<usize> = paras
        .iter()
        .map(|p| line_count(style, size, max, p))
        .collect();
    let (mut used, mut take) = (0, 0);
    while take < paras.len() && used + counts[take] <= max_lines {
        used += counts[take];
        take += 1;
    }
    if take > 0 && take < paras.len() && used >= min_head {
        return Some((paras[..take].join("\n"), paras[take..].join("\n")));
    }
    // Line split inside the paragraph that does not fit.
    let room = max_lines.max(1);
    let lines = wrap_field(style, size, max, paras.get(take).copied().unwrap_or(""));
    let before = paras[..take.min(paras.len())].join("\n");
    let before_lines = if take == 0 { 0 } else { used };
    let n = room.saturating_sub(before_lines).clamp(1, lines.len());
    // Keep a widow of at least two lines when the head can spare one.
    let n = if lines.len() - n == 1 && n > 1 {
        n - 1
    } else {
        n
    };
    let head_part = lines[..n].join(" ");
    let tail_part = lines[n..].join(" ");
    let head = if before.is_empty() {
        head_part
    } else {
        format!("{before}\n{head_part}")
    };
    let mut tail = vec![tail_part];
    tail.extend(
        paras[(take + 1).min(paras.len())..]
            .iter()
            .map(|s| s.to_string()),
    );
    Some((head, tail.join("\n")))
}

/// Registry of the field ids used so far: continuation pages suffix `@k` (k = page ordinal).
#[derive(Debug, Default)]
pub(crate) struct Ids {
    used: HashSet<String>,
}

impl Ids {
    /// Unique id for `base` on page `ordinal` (1 = the first page of a sequence).
    pub fn make(&mut self, base: &str, ordinal: u32) -> String {
        let first = if ordinal <= 1 {
            base.to_string()
        } else {
            format!("{base}@{ordinal}")
        };
        let mut id = first.clone();
        let mut n = 2;
        while !self.used.insert(id.clone()) {
            id = format!("{first}.{n}");
            n += 1;
        }
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_prefers_paragraph_boundaries() {
        let text = "one two three\nfour five six\nseven eight nine";
        let (h, t) = split_text(Style::Sans, 7.4, 400.0, text, 2, 1).unwrap();
        assert_eq!(h, "one two three\nfour five six");
        assert_eq!(t, "seven eight nine");
    }

    #[test]
    fn split_falls_back_to_lines_and_loses_nothing() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
        let (h, t) = split_text(Style::Sans, 7.4, 60.0, text, 3, 2).unwrap();
        assert_eq!(format!("{h} {t}"), text);
        assert!(line_count(Style::Sans, 7.4, 60.0, &h) <= 3);
    }

    #[test]
    fn ids_get_unique_suffixes() {
        let mut ids = Ids::default();
        assert_eq!(ids.make("feat.1.text", 1), "feat.1.text");
        assert_eq!(ids.make("feat.1.text", 2), "feat.1.text@2");
        assert_eq!(ids.make("feat.1.text", 2), "feat.1.text@2.2");
    }
}
