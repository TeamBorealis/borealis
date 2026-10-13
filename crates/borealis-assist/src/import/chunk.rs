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

fn looks_like_title(l: &str) -> bool {
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
    if starts.first().map_or(true, |&s| s > 0) {
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
}
