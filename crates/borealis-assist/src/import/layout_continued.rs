//! Continuation marks a page break leaves in a document's text: a "(continued from previous
//! page)" line, or a heading reprinted as "<Title> (Cont.)" over the rest of its section on the
//! next page. They are not sections: a marked heading would close the section it continues
//! (cutting a race's traits off its lead-in), so the marks are dropped and the lines under them
//! continue the section above.

/// Most words of a bare continuation mark ("continued from the previous page: <Title>").
const MAX_MARK_WORDS: usize = 12;
/// Suffixes that reprint a heading over its continuation.
const CONT_SUFFIXES: [&str; 6] = [
    "(cont.)",
    "(cont)",
    "(cont'd)",
    "(contd)",
    "(continued)",
    ", continued",
];
/// Words that may follow "continued" in a bare mark.
const MARK_FOLLOWERS: [&str; 4] = ["from", "on", "over", "overleaf"];

fn heading(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.trim_start_matches('#');
    (rest.len() < t.len() && (rest.is_empty() || rest.starts_with(' '))).then(|| rest.trim())
}

/// Lowercase text without wrapping marks (`(`, `[`, `*`, `_`) and outer spaces.
fn bare(t: &str) -> String {
    t.trim()
        .trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '*' | '_') || c.is_whitespace())
        .to_lowercase()
        .replace('\u{2019}', "'")
}

/// `t` is only a continuation mark: "continued", "Cont.", "(continued from previous page:
/// X)", "continued on next page" (a title such as "Continued Studies" is not).
fn is_mark(t: &str) -> bool {
    let b = bare(t);
    if b.split_whitespace().count() > MAX_MARK_WORDS {
        return false;
    }
    let rest = ["continued", "cont'd", "contd", "cont."]
        .iter()
        .find_map(|m| b.strip_prefix(m));
    let Some(rest) = rest else {
        return b == "cont";
    };
    let rest = rest.trim_start_matches(['.', ' ']);
    rest.is_empty()
        || rest.starts_with([':', '-', '\u{2014}', '\u{2013}', ')'])
        || MARK_FOLLOWERS
            .iter()
            .any(|f| rest.split_whitespace().next() == Some(*f))
}

/// A text line that is only a continuation mark: set apart (in brackets or emphasis) or naming
/// the page or column it points to. A wrapped prose line that is just "continued." stays.
fn is_plain_mark(line: &str) -> bool {
    let t = line.trim();
    let set_apart = t.starts_with(['(', '[', '*', '_']);
    let lower = t.to_lowercase();
    is_mark(t) && (set_apart || lower.contains("page") || lower.contains("column"))
}

/// The title a "<Title> (Cont.)" heading continues.
fn continued_title(t: &str) -> Option<String> {
    let lower = t.trim().to_lowercase();
    CONT_SUFFIXES.iter().find_map(|s| {
        let base = lower.strip_suffix(s)?.trim();
        (!base.is_empty()).then(|| norm(base))
    })
}

fn norm(t: &str) -> String {
    t.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn next_text(lines: &[&str], from: usize) -> Option<usize> {
    (from..lines.len()).find(|&k| !lines[k].trim().is_empty())
}

/// `text` without its continuation marks: a line or heading that is only a mark, and a
/// "<Title> (Cont.)" heading directly continuing the section titled `<Title>` (with the table
/// header row it reprints when that row repeats the section's first table row). A "(Cont.)"
/// heading after another section is left: it is that table's own section.
pub(crate) fn without_continuations(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut drop = vec![false; lines.len()];
    // The open section: its title and its first text line (a table's header row).
    let mut open: Option<(String, Option<usize>)> = None;
    for i in 0..lines.len() {
        if drop[i] {
            continue;
        }
        let Some(h) = heading(lines[i]) else {
            if is_plain_mark(lines[i]) {
                drop[i] = true;
            } else if let Some((_, first)) = open.as_mut().filter(|o| o.1.is_none()) {
                if !lines[i].trim().is_empty() {
                    *first = Some(i);
                }
            }
            continue;
        };
        if is_mark(h) {
            drop[i] = true;
            continue;
        }
        let continues = continued_title(h)
            .filter(|t| open.as_ref().is_some_and(|(title, _)| title == t))
            .is_some();
        if !continues {
            open = Some((norm(h), None));
            continue;
        }
        drop[i] = true;
        let header = open.as_ref().and_then(|(_, f)| *f).map(|f| lines[f].trim());
        if let Some(k) = next_text(&lines, i + 1)
            .filter(|&k| lines[k].contains(" | ") && Some(lines[k].trim()) == header)
        {
            drop[k] = true;
        }
    }
    if !drop.contains(&true) {
        return text.to_string();
    }
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        let after_blank = out.last().is_none_or(|p| p.trim().is_empty());
        if drop[i] || (l.trim().is_empty() && after_blank && i > 0 && drop[i - 1]) {
            continue;
        }
        out.push(l);
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_continuation_mark_heading_does_not_cut_the_section() {
        let src = "## Lantern Folk\n\nLantern Folk Traits\n\n\
                   # (continued from previous page: Lantern Folk traits)\n\n\
                   **Size.** Medium.\n**Speed.** 30 feet.";
        let out = without_continuations(src);
        assert!(!out.contains("continued"));
        assert!(out.contains("Lantern Folk Traits\n\n**Size.** Medium."));
        assert_eq!(out.matches('#').count(), 2);
    }

    #[test]
    fn bare_marks_go_and_titles_starting_with_continued_stay() {
        let src = "Some text.\n(Continued on next page)\n\n### Continued Studies\n\n[Cont.]\nMore,\ncontinued.";
        let out = without_continuations(src);
        assert_eq!(
            out,
            "Some text.\n\n### Continued Studies\n\nMore,\ncontinued."
        );
        for mark in [
            "continued",
            "Cont'd",
            "*Continued from the previous page*",
            "[cont.]",
        ] {
            assert!(is_mark(mark), "{mark}");
        }
        for title in ["Continued Studies", "Contingency", "Content Warning"] {
            assert!(!is_mark(title), "{title}");
        }
    }

    #[test]
    fn a_cont_heading_joins_the_section_it_continues_with_its_reprinted_header() {
        let src = "#### Bright Blades\n\nName | Cost\n\nGlass Knife | 2gp\n\n\
                   Page Footer Line\n\n#### Bright Blades (Cont.)\n\nName | Cost\n\n\
                   Mirror Saber | 9gp";
        let out = without_continuations(src);
        assert_eq!(out.matches("Bright Blades").count(), 1);
        assert_eq!(out.matches("Name | Cost").count(), 1);
        assert!(out.contains("Glass Knife | 2gp") && out.contains("Mirror Saber | 9gp"));
    }

    #[test]
    fn a_cont_heading_after_another_section_stays_a_heading() {
        let src = "#### Dim Bows\n\nName | Cost\n\nReed Bow | 1gp\n\n#### Bright Blades\n\n\
                   Name | Cost\n\nGlass Knife | 2gp\n\n#### Dim Bows (Cont.)\n\nName | Cost\n\n\
                   Cane Bow | 3gp";
        assert_eq!(without_continuations(src), src);
    }

    #[test]
    fn text_without_marks_is_unchanged() {
        let src = "# Title\n\nA line.\n\n## Part\n\nMore text.\n";
        assert_eq!(without_continuations(src), src);
    }
}
