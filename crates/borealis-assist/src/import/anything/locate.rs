//! Section locator: the passage of the source that defines a named element.
//!
//! A name is matched against headings first (a heading that IS the name), then run-in heads
//! ("Name. text...") and lines that are exactly the name, and only then any line mentioning it.
//! Table-of-contents and index lines (dot leaders, trailing page numbers, runs of such lines) and
//! name lists ("A – B – C" cover lines) are never a section. The section runs from the match to
//! the next heading of the same or a higher level, so multi-page sections and sub-headings stay
//! whole. The model's verbatim quote, when it falls inside a candidate's section, picks among
//! equal candidates.

use super::types::SourceExcerpt;
use crate::import::chunk::{
    detect, headings, looks_like_title, markdown_heading, norm_words, Heading, Hint, MARKED_UP,
};

const SNIPPET_CHARS: usize = 160;
/// Most characters one section may span (a level-1 heading can cover a whole book).
const MAX_SECTION_CHARS: usize = 40_000;
/// Consecutive page-numbered lines that make a contents or index block.
const INDEX_RUN: usize = 3;
/// Longest run-in head ("Name." at the start of a paragraph), in words.
const MAX_RUN_IN_WORDS: usize = 8;
/// Separated names on one line that make it a list (cover line, "see also").
const LIST_ITEMS: usize = 3;
/// Lines after a match searched for the opening of an entry (level line, type and rarity).
const ENTRY_LINES: usize = 8;
/// Lines of a section that make it a list when nearly all are bare names.
const LIST_LINES: usize = 3;
/// Share of bare names among a section's lines that make it a name list.
const LIST_SHARE: f32 = 0.8;
/// Most words of a bare name line.
const NAME_WORDS: usize = 6;
/// Most words of each other cell of an index table row (a rarity, a type).
const INDEX_CELL_WORDS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Mention,
    TableRow,
    PartHeading,
    ExactLine,
    RunIn,
    Heading,
}

struct Candidate {
    line: usize,
    kind: Kind,
    end: usize,
}

fn trailing_page_number(t: &str) -> bool {
    let t = t.trim_end();
    // A table row of three cells or more ending in a number is data, not a page reference.
    if t.matches(" | ").count() >= 2 {
        return false;
    }
    let digits = t.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 4 || digits == t.chars().count() {
        return false;
    }
    let before = t[..t.len() - digits].trim_end_matches([',', '-', '–', ' ', '\t']);
    !before.is_empty()
        && !before.ends_with(|c: char| c.is_ascii_digit() || c == '+')
        && !is_dice_prefix(before)
        && !before.to_lowercase().ends_with("level")
}

/// "2d" of "2d6": the number is a die size, not a page.
fn is_dice_prefix(before: &str) -> bool {
    let mut rev = before.chars().rev();
    matches!(rev.next(), Some('d' | 'D')) && rev.next().is_some_and(|c| c.is_ascii_digit())
}

fn dot_leader(t: &str) -> bool {
    t.contains("...") || t.contains('…') || t.contains(". . .") || t.contains("\t")
}

/// Lines that belong to a table of contents or an index: dot-leader entries, and runs of at
/// least [`INDEX_RUN`] consecutive non-blank lines ending in a page number.
fn index_mask(lines: &[&str]) -> Vec<bool> {
    let numbered: Vec<bool> = lines
        .iter()
        .map(|l| trailing_page_number(l.trim().trim_end_matches('.')))
        .collect();
    let mut mask: Vec<bool> = lines
        .iter()
        .zip(&numbered)
        .map(|(l, n)| *n && dot_leader(l.trim()))
        .collect();
    let filled: Vec<usize> = (0..lines.len())
        .filter(|&i| !lines[i].trim().is_empty())
        .collect();
    let mut k = 0;
    while k < filled.len() {
        let mut j = k;
        while j < filled.len() && numbered[filled[j]] {
            j += 1;
        }
        if j - k >= INDEX_RUN {
            for &i in &filled[k..j] {
                mask[i] = true;
            }
        }
        k = j.max(k + 1);
    }
    mask
}

/// A line listing several names ("Alpha – Beta – Gamma", "Alpha, Beta, Gamma and Delta").
fn is_list_line(t: &str) -> bool {
    let t = t.trim();
    if t.ends_with(['.', ':', ';']) {
        return false;
    }
    let items = t
        .split([',', '|', '•', '·', '–', '—', '/'])
        .flat_map(|p| p.split(" and "))
        .filter(|p| !p.trim().is_empty())
        .count();
    items >= LIST_ITEMS
}

fn strip_marks(t: &str) -> &str {
    t.trim().trim_start_matches(['#', '*', '_', '-', ' '])
}

/// "Name." / "Name:" at the start of a line, the head being Title Case and short.
fn run_in_head(t: &str) -> Option<&str> {
    let t = strip_marks(t);
    let cut = t.find(['.', ':'])?;
    let head = t[..cut].trim_end_matches(['*', '_']).trim();
    let words: Vec<&str> = head.split_whitespace().collect();
    let small = [
        "of", "the", "and", "a", "an", "in", "on", "to", "for", "with", "or",
    ];
    let title = !words.is_empty()
        && words.len() <= MAX_RUN_IN_WORDS
        && words.iter().enumerate().all(|(i, w)| {
            w.chars()
                .next()
                .is_some_and(|c| c.is_uppercase() || !c.is_alphabetic())
                || (i > 0 && small.contains(&w.to_lowercase().as_str()))
        });
    title.then_some(head)
}

/// Words that make a heading a part of the element it follows ("<name> Traits").
const PART_WORDS: [&str; 12] = [
    "traits",
    "trait",
    "features",
    "feature",
    "names",
    "lore",
    "variant",
    "variants",
    "subraces",
    "options",
    "table",
    "characteristics",
];

/// `title` is "<name> <part word>" (a sub-section of the element called `name`).
fn is_sub_part(title: &str, name: &str) -> bool {
    let t = norm_words(title);
    !name.is_empty()
        && t.strip_prefix(name)
            .map(str::trim)
            .and_then(|rest| rest.split(' ').next_back())
            .is_some_and(|w| {
                PART_WORDS.contains(&w) && t.split(' ').count() <= name.split(' ').count() + 2
            })
}

/// Words that make a heading a structural sub-heading of the section it is in ("Tier 1",
/// "Crafting Table", "Example"), even at the section's own level.
const STRUCTURE_WORDS: [&str; 12] = [
    "tier",
    "tiers",
    "rank",
    "level",
    "stage",
    "step",
    "table",
    "tables",
    "example",
    "examples",
    "requirements",
    "cost",
];

/// A structural sub-heading: it numbers a part ("Tier 1", "Stage II") or names a table or an
/// example, so it belongs to the section above it.
fn structural_heading(title: &str) -> bool {
    let t = norm_words(title);
    let words: Vec<&str> = t.split(' ').filter(|w| !w.is_empty()).collect();
    let numbered = words.len() <= 4
        && words.iter().any(|w| {
            w.chars().all(|c| c.is_ascii_digit())
                || ["i", "ii", "iii", "iv", "v", "vi"].contains(w)
                || w.trim_end_matches(|c: char| c.is_ascii_alphabetic())
                    .parse::<u32>()
                    .is_ok()
        });
    numbered
        || words.first().is_some_and(|w| STRUCTURE_WORDS.contains(w))
        || words.last().is_some_and(|w| STRUCTURE_WORDS.contains(w))
}

/// A table row (`a | b | c`) one of whose cells is the name.
fn row_with_cell(t: &str, name: &str) -> bool {
    t.contains(" | ") && t.split(" | ").any(|c| same_name(c, name))
}

fn same_name(a: &str, b: &str) -> bool {
    !b.is_empty() && norm_words(a) == b
}

fn contains_name(hay: &str, name: &str) -> bool {
    !name.is_empty() && format!(" {} ", norm_words(hay)).contains(&format!(" {name} "))
}

struct Doc<'a> {
    /// The text marks its headings (`#` lines); else headings are guessed and a standalone
    /// title line also ends a section.
    marked: bool,
    lines: Vec<&'a str>,
    heads: Vec<Heading>,
    index: Vec<bool>,
}

impl Doc<'_> {
    fn heading_at(&self, line: usize) -> Option<&Heading> {
        self.heads.iter().find(|h| h.line == line)
    }

    /// End (exclusive) of the section of the heading at `line` with `level`.
    fn heading_end(&self, line: usize, level: u8, name: &str) -> usize {
        self.heads
            .iter()
            .find(|h| {
                h.line > line
                    && (h.level < level
                        || (h.level == level
                            && !is_sub_part(&h.title, name)
                            && !structural_heading(&h.title)))
            })
            .map_or(self.lines.len(), |h| h.line)
    }

    /// End (exclusive) of a section without a marked heading level, starting at `line`: the
    /// next heading or standalone title line after a paragraph break (unless it is a part of
    /// `name`, such as "<name> Traits"), and for a run-in head also the next run-in head.
    fn paragraph_end(&self, line: usize, run_in: bool, name: &str) -> usize {
        let mut i = line + 1;
        while i < self.lines.len() {
            let l = self.lines[i];
            let starts = self.heading_at(i).is_some()
                || self.standalone_title(i)
                || (run_in && run_in_head(l).is_some());
            let sub = is_sub_part(strip_marks(l), name) || structural_heading(strip_marks(l));
            if starts && (run_in || !sub) {
                return i;
            }
            i += 1;
        }
        self.lines.len()
    }

    /// A title-like line alone in its paragraph (a wrapped body line has a continuation).
    fn standalone_title(&self, i: usize) -> bool {
        let blank = |k: usize| self.lines.get(k).map_or(true, |l| l.trim().is_empty());
        i > 0 && blank(i - 1) && blank(i + 1) && looks_like_title(self.lines[i])
    }

    fn candidates(&self, name: &str) -> Vec<Candidate> {
        let mut out = Vec::new();
        for (i, raw) in self.lines.iter().enumerate() {
            if self.index[i] || !contains_name(raw, name) {
                continue;
            }
            let t = strip_marks(raw);
            let (kind, end) = if let Some(h) = self.heading_at(i) {
                let kind = if same_name(&h.title, name) {
                    Kind::Heading
                } else {
                    Kind::PartHeading
                };
                let end = if self.marked {
                    self.heading_end(i, h.level, name)
                } else {
                    self.paragraph_end(i, false, name)
                };
                (kind, end)
            } else if run_in_head(t).is_some_and(|h| same_name(h, name)) {
                (Kind::RunIn, self.paragraph_end(i, true, name))
            } else if same_name(t, name) {
                (Kind::ExactLine, self.paragraph_end(i, false, name))
            } else if row_with_cell(t, name) {
                (Kind::TableRow, self.row_end(i))
            } else if is_list_line(t) && !t.contains(" | ") {
                continue;
            } else {
                (Kind::Mention, self.paragraph_block_end(i))
            };
            out.push(Candidate { line: i, kind, end });
        }
        out
    }

    /// A table row and its wrapped lines (to the next row, blank line or heading).
    fn row_end(&self, line: usize) -> usize {
        let mut end = line + 1;
        while end < self.lines.len()
            && !self.lines[end].trim().is_empty()
            && !self.lines[end].contains(" | ")
            && self.heading_at(end).is_none()
        {
            end += 1;
        }
        end
    }

    /// The paragraph (to the next blank line) around a mention.
    fn paragraph_block_end(&self, line: usize) -> usize {
        let mut end = line + 1;
        while end < self.lines.len() && !self.lines[end].trim().is_empty() {
            end += 1;
        }
        end
    }

    /// The lines after `line` (to the next heading) open an entry: a spell's level line, an
    /// item's type and rarity, a stat block, a feat's prerequisite.
    fn opens_entry(&self, line: usize) -> bool {
        let stop = (line + 1..self.lines.len())
            .find(|&k| self.heading_at(k).is_some())
            .unwrap_or(self.lines.len())
            .min(line + 1 + ENTRY_LINES);
        detect(&self.lines[line + 1..stop]) != Hint::Unknown
    }

    /// The candidate is unlikely to be the element's definition (it loses to any that can be): a heading, name line or mention with
    /// no text under it, or an entry in a list of bare names (contents, a class spell list, an
    /// index, an index table row: the name and a rarity or type). A catalogue or stat row
    /// (numbers, a description) is a definition.
    fn not_a_definition(&self, c: &Candidate, name: &str) -> bool {
        if c.kind == Kind::RunIn {
            return false;
        }
        if c.kind == Kind::TableRow {
            // An index row: besides the name only a word or two (a rarity, a type).
            return self.lines[c.line].split(" | ").all(|cell| {
                same_name(cell, name)
                    || (!cell.chars().any(|ch| ch.is_ascii_digit())
                        && cell.split_whitespace().count() <= INDEX_CELL_WORDS)
            });
        }
        let body: Vec<&str> = self.lines[c.line..c.end]
            .iter()
            .map(|l| strip_marks(l))
            .filter(|l| !l.is_empty())
            .collect();
        if body.len() <= 1 {
            return true;
        }
        let names = body
            .iter()
            .filter(|l| !l.ends_with(['.', ':', ';']) && l.split_whitespace().count() <= NAME_WORDS)
            .count();
        c.kind != Kind::Heading
            && body.len() >= LIST_LINES
            && names as f32 >= LIST_SHARE * body.len() as f32
    }

    fn contains_quote(&self, c: &Candidate, quote: &str) -> bool {
        !quote.is_empty()
            && self.lines[c.line..c.end]
                .iter()
                .any(|l| norm_words(l).contains(quote))
    }

    fn excerpt(&self, start: usize, end: usize) -> SourceExcerpt {
        let mut end = end.max(start + 1).min(self.lines.len());
        while end > start + 1 && self.lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        let mut chars = 0;
        let mut cut = start;
        while cut < end && chars < MAX_SECTION_CHARS {
            chars += self.lines[cut].chars().count() + 1;
            cut += 1;
        }
        let snippet: String = self.lines[start..cut]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(SNIPPET_CHARS)
            .collect();
        SourceExcerpt {
            start_line: start + 1,
            end_line: cut,
            snippet,
        }
    }
}

/// The section of `source` that defines `name`; `quote` (a short verbatim quote from the model)
/// breaks ties and is the fallback when the name is not found.
pub(crate) fn locate_section(source: &str, quote: &str, name: &str) -> Option<SourceExcerpt> {
    let lines: Vec<&str> = source.lines().collect();
    let marked = lines
        .iter()
        .filter(|l| markdown_heading(l).is_some())
        .count()
        >= MARKED_UP;
    let doc = Doc {
        marked,
        heads: headings(source),
        index: index_mask(&lines),
        lines,
    };
    let name = norm_words(name);
    let quote = norm_words(quote);
    let found = doc.candidates(&name);
    // The quote first, then an exact heading, run-in head or name line that can be a
    // definition (not a bare name in a list, an index row or a line with nothing under it),
    // then a match opening an entry body, then any that can be a definition, then the kind.
    let best = found.iter().max_by(|a, b| {
        let key = |c: &Candidate| {
            let definition = !doc.not_a_definition(c, &name);
            (
                doc.contains_quote(c, &quote),
                definition && c.kind >= Kind::ExactLine,
                doc.opens_entry(c.line),
                definition,
                c.kind,
                std::cmp::Reverse(c.line),
            )
        };
        key(a).cmp(&key(b))
    });
    if let Some(c) = best {
        let quoted = doc.contains_quote(c, &quote);
        // A bare mention loses to a quote found elsewhere (the definition's own text).
        if c.kind != Kind::Mention || quoted || quote.is_empty() {
            return Some(doc.excerpt(c.line, c.end));
        }
    }
    // The quote alone (when it is more than the name): its paragraph to the next heading.
    if quote.is_empty() || quote == name {
        return best.map(|c| doc.excerpt(c.line, c.end));
    }
    let at = (0..doc.lines.len()).find(|&i| {
        !doc.index[i] && !is_list_line(doc.lines[i]) && norm_words(doc.lines[i]).contains(&quote)
    })?;
    let mut start = at;
    while start > 0 && !doc.lines[start - 1].trim().is_empty() && doc.heading_at(start).is_none() {
        start -= 1;
    }
    Some(doc.excerpt(start, doc.paragraph_end(start, true, "")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section<'a>(src: &'a str, name: &str, quote: &str) -> Vec<&'a str> {
        let ex = locate_section(src, quote, name).expect("located");
        src.lines()
            .skip(ex.start_line - 1)
            .take(ex.end_line - ex.start_line + 1)
            .collect()
    }

    const BOOK: &str = "# Wanderers of the Mist\n\nEmberkin – Tidewalker – Glasswing\n\n## Contents\n\nEmberkin ........ 4\nTidewalker ........ 6\nGlasswing ........ 9\nIndex ........ 12\n\n## Emberkin\n\nEmberkin carry a coal in their chest.\n\n### Emberkin Traits\n\nDarkvision. You see in dim light within 60 feet.\nEmber Heart. You resist fire damage.\n\nThe coal never cools.\n\n## Tidewalker\n\nTidewalkers breathe water.\n\nDarkvision. Tidewalkers see 30 feet in murk.\n\n## Index\n\nEmberkin 4\nGlasswing 9\nTidewalker 6\n";

    #[test]
    fn skips_cover_and_contents_and_keeps_subsections() {
        let s = section(BOOK, "Emberkin", "");
        assert_eq!(s[0], "## Emberkin");
        assert!(s.contains(&"### Emberkin Traits"));
        assert!(s.contains(&"The coal never cools."));
        assert!(!s.iter().any(|l| l.contains("Tidewalker")), "{s:?}");
        // Glasswing has no section of its own: only the cover, the contents and the index name it.
        assert!(locate_section(BOOK, "", "Glasswing").is_none());
    }

    #[test]
    fn run_in_traits_stop_at_the_next_trait_and_quotes_pick_the_right_one() {
        let s = section(BOOK, "Ember Heart", "");
        assert_eq!(
            s.first().copied(),
            Some("Ember Heart. You resist fire damage.")
        );
        assert!(s.contains(&"The coal never cools."));
        let s = section(BOOK, "Darkvision", "see 30 feet in murk");
        assert_eq!(s, vec!["Darkvision. Tidewalkers see 30 feet in murk."]);
        let s = section(BOOK, "Darkvision", "");
        assert!(s[0].contains("dim light"), "{s:?}");
    }

    #[test]
    fn unmarked_sections_keep_their_own_parts() {
        let src = "Glimmerkin\n\nSmall folk who glow at dusk and hum.\n\nGlimmerkin Traits\n\n\
Ability Score Increase. Your Charisma rises by 2.\nSize. You are Small.\n\nDuskling\n\nA shy folk.\n";
        let s = section(src, "Glimmerkin", "");
        assert!(s.contains(&"Size. You are Small."), "{s:?}");
        assert!(!s.contains(&"Duskling"), "{s:?}");
        let s = section(src, "Ability Score Increase", "");
        assert_eq!(s, vec!["Ability Score Increase. Your Charisma rises by 2."]);
    }

    #[test]
    fn numbered_sub_headings_and_tables_stay_in_their_section() {
        let src = "# Workshop\n\n## Glass Lanterns\n\nLanterns are blown in stages.\n\n## Tier 1\n\n\
Cost | Time\n5 gp | 1 day\n\n## Lantern Table\n\nd6 | Colour\n1 | Amber\n\n## Rope Bridges\n\nBridges sway.\n\n\
## Price List\n\nd4 | Item | Effect\n1 | Moth Jar | Glows faintly for an hour.\n2 | Dew Bottle | Never runs dry.\n";
        let s = section(src, "Glass Lanterns", "");
        assert!(s.contains(&"5 gp | 1 day"), "{s:?}");
        assert!(s.contains(&"1 | Amber"), "{s:?}");
        assert!(!s.iter().any(|l| l.contains("Bridges")), "{s:?}");
        let s = section(src, "Moth Jar", "");
        assert_eq!(s, vec!["1 | Moth Jar | Glows faintly for an hour."]);
    }

    #[test]
    fn an_entry_body_beats_a_name_in_a_list_and_an_exact_heading_beats_both() {
        let src = "# Spell Lists\n\n## Bard\n\nGlimmer Ward\nMoth Swarm\nDew Lance\nReed Song\n\n\
## Spells\n\n### ~ Margin Note Moth Swarm\n\n2nd-level conjuration\n\nCasting Time: 1 action\n\
Range: 60 feet\n\nMoths fill a cube.\n\n### Dew Lance\n\nEvocation cantrip\n\nA lance of dew.\n";
        let s = section(src, "Moth Swarm", "");
        assert_eq!(s[0], "### ~ Margin Note Moth Swarm", "{s:?}");
        assert!(s.contains(&"Moths fill a cube."), "{s:?}");
        let src =
            "# Lamplighter\n\nKeepers of lamps.\n\n## Wick Paths\n\n#### Moth Lamplighter\n\n\
Prerequisite: 5th level\n\nYou befriend moths.\n";
        assert_eq!(section(src, "Lamplighter", "")[0], "# Lamplighter");
    }

    #[test]
    fn index_lines_are_recognised() {
        let lines = [
            "Alpha 3",
            "Beta 7",
            "Gamma 12",
            "",
            "Level 3",
            "Deal 2d6 damage",
            "Body.",
        ];
        let m = index_mask(&lines);
        assert_eq!(m, vec![true, true, true, false, false, false, false]);
        assert!(is_list_line("Alpha – Beta – Gamma"));
        assert!(!is_list_line("Alpha, the first of them, wakes."));
        // Table rows ending in a number (a DC, a weight) are data, not page references.
        let rows = [
            "Moth Club | 1 sp | 10",
            "Reed Pike | 5 gp | 25",
            "Dew Axe | 2 gp | 25",
        ];
        assert_eq!(index_mask(&rows), vec![false; 3]);
        let src = "Weapons\n\nName | Cost | DC\nMoth Club | 1 sp | 10\nReed Pike | 5 gp | 25\nDew Axe | 2 gp | 25";
        let ex = locate_section(src, "", "Reed Pike").unwrap();
        assert_eq!((ex.start_line, ex.end_line), (5, 5));
    }
}
