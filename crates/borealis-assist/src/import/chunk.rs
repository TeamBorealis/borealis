//! Split imported text into chunks at headings and stat-block patterns.

use borealis_ai::task::ContentKind;

/// What a chunk looks like, from its first lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// Stat block (size/type line, Armor Class, Hit Points).
    Monster,
    /// Spell (level and school line, Casting Time).
    Spell,
    /// Magic item or equipment (kind and rarity line).
    Item,
    /// Feat (Prerequisite line or an ability increase of 1).
    Feat,
    /// Race or species (Ability Score Increase, Size, Speed).
    Species,
    /// No recognised pattern.
    Unknown,
}

impl Hint {
    /// The element types a chunk of this kind should produce.
    pub fn types(self) -> &'static [&'static str] {
        match self {
            Hint::Monster => &["Monster"],
            Hint::Spell => ContentKind::Spell.element_types(),
            Hint::Item => ContentKind::Item.element_types(),
            Hint::Feat => ContentKind::Feat.element_types(),
            Hint::Species => ContentKind::Race.element_types(),
            Hint::Unknown => &[],
        }
    }
}

/// One slice of the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Position in the document.
    pub index: usize,
    /// Heading text, when the chunk starts at one.
    pub heading: Option<String>,
    /// Chunk text including its heading line.
    pub text: String,
    /// 1-based first line.
    pub start_line: usize,
    /// 1-based last line.
    pub end_line: usize,
    /// Detected pattern.
    pub hint: Hint,
}

const SIZES: [&str; 6] = ["tiny", "small", "medium", "large", "huge", "gargantuan"];
const ITEM_KINDS: [&str; 9] = [
    "wondrous item",
    "weapon (",
    "armor (",
    "ring,",
    "potion,",
    "rod,",
    "staff,",
    "wand,",
    "scroll,",
];
const RARITIES: [&str; 5] = ["common", "uncommon", "rare", "very rare", "legendary"];
/// Longest line that can be a heading.
const MAX_HEADING_CHARS: usize = 60;
/// Lines scanned after a candidate heading for a type signature.
const SIGNATURE_LINES: usize = 6;

fn is_level_line(l: &str) -> bool {
    let first = l.split_whitespace().next().unwrap_or_default();
    (first.ends_with("-level") && first.chars().next().is_some_and(|c| c.is_ascii_digit()))
        || l.ends_with(" cantrip")
}

/// Detect the pattern in the lines that follow a heading.
pub fn detect(lines: &[&str]) -> Hint {
    let body: Vec<String> = lines
        .iter()
        .map(|l| l.trim().trim_matches(['*', '_']).to_lowercase())
        .filter(|l| !l.is_empty())
        .take(SIGNATURE_LINES)
        .collect();
    let has = |p: &str| body.iter().any(|l| l.starts_with(p));
    if has("armor class") && has("hit points")
        || body.iter().any(|l| {
            SIZES.iter().any(|s| l.starts_with(s))
                && l.contains(',')
                && l.split_whitespace().count() <= 8
        })
    {
        return Hint::Monster;
    }
    if has("casting time") || body.iter().any(|l| is_level_line(l)) {
        return Hint::Spell;
    }
    if body.iter().any(|l| {
        ITEM_KINDS.iter().any(|k| l.starts_with(k)) && RARITIES.iter().any(|r| l.contains(r))
    }) {
        return Hint::Item;
    }
    if has("prerequisite") {
        return Hint::Feat;
    }
    if has("ability score increase") || (has("size.") && has("speed.")) {
        return Hint::Species;
    }
    Hint::Unknown
}

pub(crate) fn looks_like_title(l: &str) -> bool {
    let t = l.trim();
    !t.is_empty()
        && t.chars().count() <= MAX_HEADING_CHARS
        && !t.ends_with(['.', ':', ',', ';'])
        && t.chars().next().is_some_and(char::is_alphanumeric)
}

fn all_caps(t: &str) -> bool {
    let letters: Vec<char> = t.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= 3 && letters.iter().all(|c| c.is_uppercase())
}

fn is_heading(lines: &[&str], i: usize) -> bool {
    let t = lines[i].trim();
    if t.starts_with('#') {
        return true;
    }
    let prev_blank = i == 0 || lines[i - 1].trim().is_empty();
    if !prev_blank || !looks_like_title(t) {
        return false;
    }
    all_caps(t) || detect(&lines[i + 1..]) != Hint::Unknown
}

/// Split `text` into chunks; text before the first heading becomes a chunk without a heading.
pub fn chunk_text(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&i| is_heading(&lines, i))
        .collect();
    let mut bounds: Vec<usize> = Vec::new();
    if starts.first().is_none_or(|&s| s > 0) {
        bounds.push(0);
    }
    bounds.extend(&starts);
    let mut out = Vec::new();
    for (k, &start) in bounds.iter().enumerate() {
        let end = bounds.get(k + 1).copied().unwrap_or(lines.len());
        let slice = &lines[start..end];
        let body = slice.join("\n").trim().to_string();
        if body.is_empty() {
            continue;
        }
        let is_head = starts.contains(&start);
        let heading = is_head.then(|| slice[0].trim().trim_start_matches('#').trim().to_string());
        let after = if is_head { &slice[1..] } else { slice };
        out.push(Chunk {
            index: out.len(),
            heading,
            text: body,
            start_line: start + 1,
            end_line: end,
            hint: detect(after),
        });
    }
    out
}

/// A heading of a source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 0-based line index.
    pub line: usize,
    /// 1 = top level; larger is deeper.
    pub level: u8,
    /// Heading text without markup.
    pub title: String,
}

/// Level of a heuristic (unmarked) heading in ALL CAPS.
const CAPS_LEVEL: u8 = 2;
/// Level of other heuristic headings (title lines over a stat block, spell or item line).
const TITLE_LEVEL: u8 = 3;
/// With at least this many `#` headings the text is taken as marked up: only `#` lines count.
pub(crate) const MARKED_UP: usize = 3;

/// `#`-heading level and title of a line.
pub fn markdown_heading(line: &str) -> Option<(u8, &str)> {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    let rest = &t[hashes..];
    (1..=6).contains(&hashes).then_some(())?;
    rest.starts_with(' ').then_some(())?;
    let title = rest.trim().trim_end_matches('#').trim();
    (!title.is_empty()).then(|| (u8::try_from(hashes).unwrap_or(6), title))
}

/// Headings of `text`: `#` lines, and in unmarked text also ALL-CAPS title lines and title
/// lines over a recognised stat block / spell / item signature.
pub fn headings(text: &str) -> Vec<Heading> {
    let lines: Vec<&str> = text.lines().collect();
    let marked: Vec<Heading> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            markdown_heading(l).map(|(level, title)| Heading {
                line: i,
                level,
                title: title.to_string(),
            })
        })
        .collect();
    if marked.len() >= MARKED_UP {
        return marked;
    }
    (0..lines.len())
        .filter_map(|i| {
            if let Some((level, title)) = markdown_heading(lines[i]) {
                return Some(Heading {
                    line: i,
                    level,
                    title: title.to_string(),
                });
            }
            if !is_heading(&lines, i) {
                return None;
            }
            let t = lines[i].trim();
            Some(Heading {
                line: i,
                level: if all_caps(t) { CAPS_LEVEL } else { TITLE_LEVEL },
                title: t.to_string(),
            })
        })
        .collect()
}

/// Lower-case words of `s`, punctuation dropped.
pub fn norm_words(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The `expected` headings that appear nowhere in `text` (case and punctuation ignored): the
/// coverage check of an extraction.
pub fn missing_headings(expected: &[String], text: &str) -> Vec<String> {
    let hay = format!(" {} ", norm_words(text));
    expected
        .iter()
        .filter(|h| {
            let n = norm_words(h);
            !n.is_empty() && !hay.contains(&format!(" {n} "))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_headings_and_detects_patterns() {
        let doc = "# Title\n\nIntro.\n\nEmber Ward\n2nd-level abjuration\nCasting Time: 1 action\n\nGLOOM STALKER\nMedium monstrosity, neutral evil\nArmor Class 13\nHit Points 30\n";
        let c = chunk_text(doc);
        let kinds: Vec<(Option<&str>, Hint)> =
            c.iter().map(|c| (c.heading.as_deref(), c.hint)).collect();
        assert_eq!(
            kinds,
            vec![
                (Some("Title"), Hint::Unknown),
                (Some("Ember Ward"), Hint::Spell),
                (Some("GLOOM STALKER"), Hint::Monster)
            ]
        );
        assert_eq!(c[1].start_line, 5);
    }

    #[test]
    fn headings_and_coverage() {
        let doc = "# Book\n\n## Part One\ntext\n### Deep\n";
        let h = headings(doc);
        let got: Vec<(u8, &str)> = h.iter().map(|h| (h.level, h.title.as_str())).collect();
        assert_eq!(got, vec![(1, "Book"), (2, "Part One"), (3, "Deep")]);
        let plain =
            headings("Intro.\n\nGLOOM STALKER\nMedium monstrosity, neutral evil\nArmor Class 13\n");
        assert_eq!((plain[0].level, plain[0].line), (2, 2));
        let missing = missing_headings(&["Part one".into(), "Gone Away".into()], doc);
        assert_eq!(missing, vec!["Gone Away".to_string()]);
    }
}
