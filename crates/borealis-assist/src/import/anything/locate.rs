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

/// Times a heading's prefix recurs as a line of its own to be junk glued to the heading.
const GLUE_RECURS: usize = 2;

/// Most lines a wrapped name spans.
const WRAP_LINES: usize = 3;
/// Fewest words of a name looked for across line breaks.
const WRAP_MIN_WORDS: usize = 4;

/// Shortest word of a name that ties a table caption to it.
const CAPTION_STEM: usize = 4;

/// A word without its plural ending ("boxes" -> "box", "lamps" -> "lamp").
fn stem(w: &str) -> &str {
    if let Some(s) = w
        .strip_suffix("es")
        .filter(|s| s.ends_with(['x', 's', 'h']))
    {
        return s;
    }
    w.strip_suffix('s')
        .filter(|s| !s.ends_with('s'))
        .unwrap_or(w)
}

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

/// Lowercase words a title may contain.
const SMALL_WORDS: [&str; 11] = [
    "of", "the", "and", "a", "an", "in", "on", "to", "for", "with", "or",
];
/// Fewest words of text after a run-in head that make the line a paragraph.
const RUN_IN_TEXT_WORDS: usize = 3;

/// "Name." / "Name:" at the start of a line, the head being Title Case and short.
fn run_in_head(t: &str) -> Option<&str> {
    let t = strip_marks(t);
    let cut = t.find(['.', ':'])?;
    let head = t[..cut].trim_end_matches(['*', '_']).trim();
    let words: Vec<&str> = head.split_whitespace().collect();
    let title = !words.is_empty()
        && words.len() <= MAX_RUN_IN_WORDS
        && words.iter().enumerate().all(|(i, w)| {
            w.chars()
                .next()
                .is_some_and(|c| c.is_uppercase() || !c.is_alphabetic())
                || (i > 0 && SMALL_WORDS.contains(&w.to_lowercase().as_str()))
        });
    title.then_some(head)
}

/// "Name. Text" (a run-in head followed by its text): a paragraph even without a final stop.
fn run_in_paragraph(t: &str) -> bool {
    let body = strip_marks(t);
    run_in_head(body).is_some_and(|head| {
        let rest: Vec<&str> = body[head.len()..]
            .trim_start_matches(['*', '_', '.', ':'])
            .split_whitespace()
            .collect();
        rest.len() >= RUN_IN_TEXT_WORDS
            && rest.iter().any(|w| {
                w.chars().next().is_some_and(char::is_lowercase)
                    && !SMALL_WORDS.contains(&w.to_lowercase().as_str())
            })
    })
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
    /// The element is equipment, defined by a stat row (see [`ROW_TYPES`]).
    rows_define: bool,
}

impl Doc<'_> {
    fn heading_at(&self, line: usize) -> Option<&Heading> {
        self.heads.iter().find(|h| h.line == line)
    }

    /// End (exclusive) of the section of the heading at `line` with `level`. A section that is
    /// itself a table (its heading directly over a table row) has no caption below it: a
    /// same-level table title after it is a sibling table ("Medium Armor" after "Light Armor").
    fn heading_end(&self, line: usize, level: u8, name: &str) -> usize {
        let own_table = self.table_follows(line);
        self.heads
            .iter()
            .find(|h| {
                h.line > line
                    && (h.level < level
                        || (h.level == level
                            && !is_sub_part(&h.title, name)
                            && !structural_heading(&h.title)
                            && (own_table || !self.caption_of(h, name))))
            })
            .map_or(self.lines.len(), |h| h.line)
    }

    /// The first non-blank line after `line` is a table row.
    fn table_follows(&self, line: usize) -> bool {
        self.lines
            .get(line + 1..)
            .and_then(|rest| rest.iter().find(|l| !l.trim().is_empty()))
            .is_some_and(|l| l.contains(" | "))
    }

    /// `h` is the caption of a table of the element `name`: a table row follows it and its
    /// title shares a word with the name ("Kinds of moth lamps" over a lamp's table).
    fn caption_of(&self, h: &Heading, name: &str) -> bool {
        let table_next = self.table_follows(h.line);
        let stems = |t: &str| -> Vec<String> {
            norm_words(t)
                .split(' ')
                .filter(|w| w.chars().count() >= CAPTION_STEM)
                .map(|w| stem(w).to_string())
                .collect()
        };
        let named = stems(name);
        table_next && stems(&h.title).iter().any(|w| named.contains(w))
    }

    /// End (exclusive) of a section without a marked heading level, starting at `line`: the
    /// next heading or standalone title line after a paragraph break (unless it is a part of
    /// `name`, such as "<name> Traits"), and for a run-in head also the next run-in head.
    fn paragraph_end(&self, line: usize, run_in: bool, name: &str) -> usize {
        let mut i = line + 1;
        while i < self.lines.len() {
            let l = self.lines[i];
            let starts = self.section_heading(i)
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

    /// `title` is the name with junk glued in front of it: a margin mark ("~ Signature Name")
    /// or a line that recurs on its own elsewhere (a sidebar signature, a running header).
    fn glued_title(&self, title: &str, name: &str) -> bool {
        let t = norm_words(title);
        let Some(prefix) = t
            .strip_suffix(name)
            .map(str::trim)
            .filter(|p| !p.is_empty())
        else {
            return false;
        };
        if t.len() == prefix.len() + name.len() {
            return false; // the name is the end of a longer word
        }
        let marked = title
            .trim()
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric());
        marked
            || self
                .lines
                .iter()
                .filter(|l| norm_words(strip_marks(l)) == prefix)
                .take(GLUE_RECURS)
                .count()
                >= GLUE_RECURS
    }

    /// The bare name line at `line` continues a run of bare name lines (a list, not an entry).
    fn in_name_run(&self, line: usize) -> bool {
        let above = line.checked_sub(1).map(|k| (k, self.lines[k].trim()));
        above.is_some_and(|(k, l)| {
            !l.is_empty()
                && self.heading_at(k).is_none()
                && !l.contains(" | ")
                && !l.ends_with(['.', ':', ';', ','])
                && l.split_whitespace().count() <= NAME_WORDS
        })
    }

    /// The bare line at `line` is printed again elsewhere as a line of its own (not a heading):
    /// a running page header, not an entry's title.
    fn recurs(&self, line: usize) -> bool {
        let own = norm_words(strip_marks(self.lines[line]));
        !own.is_empty()
            && (0..self.lines.len()).any(|k| {
                k != line
                    && self.heading_at(k).is_none()
                    && norm_words(strip_marks(self.lines[k])) == own
            })
    }

    /// Candidates whose name is wrapped over 2..=[`WRAP_LINES`] lines (blank lines between
    /// them skipped): the joined lines are exactly the name. A heading among them makes it a
    /// heading; the section runs from the first line to the end of the last line's section.
    fn wrapped_candidates(&self, name: &str) -> Vec<Candidate> {
        let mut out = Vec::new();
        let first = name.split(' ').next().unwrap_or_default();
        if name.split(' ').count() < WRAP_MIN_WORDS {
            return out;
        }
        for i in 0..self.lines.len() {
            let head = norm_words(strip_marks(self.lines[i]));
            if self.index[i] || !head.starts_with(first) || !name.starts_with(&head) || head == name
            {
                continue;
            }
            let mut joined = head;
            let mut k = i;
            for _ in 1..WRAP_LINES {
                let Some(next) = (k + 1..self.lines.len().min(k + 3))
                    .find(|&j| !self.lines[j].trim().is_empty())
                else {
                    break;
                };
                k = next;
                joined = format!("{joined} {}", norm_words(strip_marks(self.lines[k])));
                if joined == name {
                    let heading = (i..=k).find_map(|j| self.heading_at(j).map(|h| (j, h.level)));
                    let (kind, end) = match heading {
                        Some((_, level)) if self.marked => {
                            let last = self.heading_at(k).map_or(level, |h| h.level);
                            (Kind::Heading, self.heading_end(k, last, name))
                        }
                        Some(_) => (Kind::Heading, self.paragraph_end(k, false, name)),
                        None => (Kind::ExactLine, self.paragraph_end(k, false, name)),
                    };
                    out.push(Candidate { line: i, kind, end });
                    break;
                }
                if !name.starts_with(&joined) {
                    break;
                }
            }
        }
        out
    }

    /// Lines from `line` up to the nearest heading naming `parent` whose section encloses it
    /// (structural sub-headings such as "Tier 2" stay inside a section, so the nearest wins).
    fn under(&self, line: usize, parent: &str) -> Option<usize> {
        self.heads
            .iter()
            .filter(|h| h.line < line && contains_name(&h.title, parent))
            .filter(|h| {
                let end = if self.marked {
                    self.heading_end(h.line, h.level, "")
                } else {
                    self.paragraph_end(h.line, false, "")
                };
                end > line
            })
            .map(|h| line - h.line)
            .min()
    }

    /// A title-like line alone in its paragraph (a wrapped body line has a continuation). A
    /// run-in paragraph whose sentence lacks its final stop ("Size. You are Small") is no title.
    fn standalone_title(&self, i: usize) -> bool {
        let blank = |k: usize| self.lines.get(k).map_or(true, |l| l.trim().is_empty());
        i > 0
            && blank(i - 1)
            && blank(i + 1)
            && looks_like_title(self.lines[i])
            && !run_in_paragraph(self.lines[i])
    }

    /// A heading of the section structure: a marked heading, or (unmarked text) a guessed one
    /// that is not a run-in paragraph missing its final stop.
    fn section_heading(&self, i: usize) -> bool {
        self.heading_at(i)
            .is_some_and(|_| self.marked || !run_in_paragraph(self.lines[i]))
    }

    fn candidates(&self, name: &str) -> Vec<Candidate> {
        let mut out = Vec::new();
        for (i, raw) in self.lines.iter().enumerate() {
            if self.index[i] || !contains_name(raw, name) {
                continue;
            }
            let t = strip_marks(raw);
            let (kind, end) =
                if let Some(h) = self.heading_at(i).filter(|_| self.section_heading(i)) {
                    let kind = if same_name(&h.title, name) || self.glued_title(&h.title, name) {
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
        out.extend(self.wrapped_candidates(name));
        out
    }

    /// The row at `line` gives the stats of `name`: its first cell is the name and another
    /// cell holds a number (a cost, a die, an armor class).
    fn stat_row(&self, line: usize, name: &str) -> bool {
        let mut cells = self.lines[line].split(" | ");
        cells
            .next()
            .is_some_and(|c| same_name(strip_marks(c), name))
            && cells.any(|c| c.chars().any(|ch| ch.is_ascii_digit()))
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
        // A question ("Can a <name> ...?", an FAQ entry) is about the element, not its definition.
        if self.lines[c.line].trim_end().ends_with('?') {
            return true;
        }
        if c.kind == Kind::TableRow {
            // An index row: besides the name only a word or two (a rarity, a type).
            return self.lines[c.line].split(" | ").all(|cell| {
                same_name(cell, name)
                    || (!cell.chars().any(|ch| ch.is_ascii_digit())
                        && cell.split_whitespace().count() <= INDEX_CELL_WORDS)
            });
        }
        if c.kind == Kind::ExactLine && (self.in_name_run(c.line) || self.recurs(c.line)) {
            return true;
        }
        // Page numbers between the name and the next block are no text of it.
        let body: Vec<&str> = self.lines[c.line..c.end]
            .iter()
            .map(|l| strip_marks(l))
            .filter(|l| !l.is_empty() && !l.chars().all(|ch| ch.is_ascii_digit()))
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
/// breaks ties and is the fallback when the name is not found. A variant ("<Item>, <kind>")
/// found only in a list or index takes its item's section when that section names the kind
/// (the item's table of kinds).
pub(crate) fn locate_section(source: &str, quote: &str, name: &str) -> Option<SourceExcerpt> {
    locate_section_as(source, quote, name, "")
}

/// Element types whose definition is a table row (cost, damage, armor class): a stat row with
/// the name in its first cell beats a same-named heading that opens no entry (a prose
/// sub-section, a modification of the same name).
const ROW_TYPES: [&str; 3] = ["Weapon", "Armor", "Item"];

/// [`locate_section`] for an element of type `ty` (see [`ROW_TYPES`]).
pub(crate) fn locate_section_as(
    source: &str,
    quote: &str,
    name: &str,
    ty: &str,
) -> Option<SourceExcerpt> {
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
        rows_define: ROW_TYPES.contains(&ty),
    };
    let own = find(&doc, &norm_words(name), &norm_words(quote));
    if own.as_ref().is_some_and(|(_, definition)| *definition) {
        return own.map(|(e, _)| e);
    }
    if let Some(found) = qualified(name).and_then(|(base, parent)| {
        find_under(
            &doc,
            &norm_words(base),
            &norm_words(parent),
            &norm_words(quote),
        )
    }) {
        return Some(found);
    }
    let parent = name.rsplit_once(", ").and_then(|(base, kind)| {
        let (ex, definition) = find(&doc, &norm_words(base), "")?;
        let kind = norm_words(kind);
        let names_kind = doc.lines[ex.start_line - 1..ex.end_line]
            .iter()
            .any(|l| contains_name(l, &kind));
        (definition && names_kind).then_some(ex)
    });
    parent.or(own.map(|(e, _)| e))
}

/// "<Name> (<Parent>)": a name the plan qualified with the section it is printed in.
fn qualified(name: &str) -> Option<(&str, &str)> {
    let (base, rest) = name.trim().strip_suffix(')')?.rsplit_once('(')?;
    let (base, parent) = (base.trim(), rest.trim());
    (!base.is_empty() && !parent.is_empty()).then_some((base, parent))
}

/// The definition of `name` printed under a heading that names `parent` (the qualifier of
/// "<Name> (<Parent>)"); a definition elsewhere is a same-named entry of another section.
fn find_under(doc: &Doc, name: &str, parent: &str, quote: &str) -> Option<SourceExcerpt> {
    doc.candidates(name)
        .into_iter()
        .filter(|c| c.kind >= Kind::ExactLine && !doc.not_a_definition(c, name))
        .filter_map(|c| doc.under(c.line, parent).map(|d| (c, d)))
        .max_by_key(|(c, d)| (doc.contains_quote(c, quote), std::cmp::Reverse(*d), c.kind))
        .map(|(c, _)| doc.excerpt(c.line, c.end))
}

/// The best section for the normalised `name` and `quote`, and whether it can be the
/// definition (not a bare name in a list or index).
fn find(doc: &Doc, name: &str, quote: &str) -> Option<(SourceExcerpt, bool)> {
    let found = doc.candidates(name);
    // The quote first, then an exact heading, run-in head or name line that can be a
    // definition (not a bare name in a list, an index row or a line with nothing under it),
    // then a match opening an entry body, then any that can be a definition, then the kind.
    let key = |c: &Candidate| {
        let definition = !doc.not_a_definition(c, name);
        let opens = doc.opens_entry(c.line);
        // Equipment: its stat row, or a heading opening an entry, first.
        let stats = doc.rows_define
            && ((c.kind == Kind::TableRow && doc.stat_row(c.line, name))
                || (c.kind >= Kind::PartHeading && opens));
        (
            doc.contains_quote(c, quote),
            stats,
            definition && (c.kind >= Kind::ExactLine || c.kind == Kind::TableRow),
            opens,
            definition,
            c.kind,
            std::cmp::Reverse(c.line),
        )
    };
    let best = found.iter().max_by(|a, b| key(a).cmp(&key(b)));
    let defines = |c: &Candidate| doc.contains_quote(c, quote) || !doc.not_a_definition(c, name);
    if let Some(c) = best {
        let quoted = doc.contains_quote(c, quote);
        // A bare mention loses to a quote found elsewhere (the definition's own text).
        if c.kind != Kind::Mention || quoted || quote.is_empty() {
            return Some((doc.excerpt(c.line, c.end), defines(c)));
        }
    }
    // The quote alone (when it is more than the name): its paragraph to the next heading.
    if quote.is_empty() || quote == name {
        return best.map(|c| (doc.excerpt(c.line, c.end), defines(c)));
    }
    let at = (0..doc.lines.len()).find(|&i| {
        !doc.index[i] && !is_list_line(doc.lines[i]) && norm_words(doc.lines[i]).contains(quote)
    })?;
    let mut start = at;
    while start > 0 && !doc.lines[start - 1].trim().is_empty() && doc.heading_at(start).is_none() {
        start -= 1;
    }
    Some((doc.excerpt(start, doc.paragraph_end(start, true, "")), true))
}

#[cfg(test)]
#[path = "locate_tests.rs"]
mod tests;
