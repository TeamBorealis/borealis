//! Lines with typography cues (font size, bold, explicit outline level) from any document format,
//! rendered as Markdown-like text whose headings are `#` lines. PDF, ODT and DOCX readers all
//! produce [`TextLine`]s, so the section locator and the chunker see one convention.

/// One line (or paragraph) of a document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLine {
    /// The text.
    pub text: String,
    /// Font size in points; 0 when unknown.
    pub size: f32,
    /// The whole line is bold.
    pub bold: bool,
    /// Outline level the document declares (1 = top), when it does.
    pub level: Option<u8>,
    /// A paragraph break (vertical gap) precedes the line.
    pub gap_before: bool,
}

/// Deepest heading level emitted.
pub const MAX_LEVEL: u8 = 4;
/// A line at least this much larger than body text is a heading.
const HEADING_RATIO: f32 = 1.12;
/// Longest line considered a heading.
const MAX_HEADING_CHARS: usize = 90;
/// Most consecutive lines one heading wraps over.
const MAX_HEADING_LINES: usize = 2;
/// Sizes closer than this (points) are one heading level.
const SIZE_STEP: f32 = 0.6;

fn round(size: f32) -> i32 {
    (size * 2.0).round() as i32
}

/// Heading levels for a document: body size and the distinct larger sizes, largest first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadingScale {
    body: f32,
    sizes: Vec<f32>,
    /// (size, level) of headings whose level the document declares.
    declared: Vec<(f32, u8)>,
}

fn heading_text_ok(t: &str) -> bool {
    let t = t.trim();
    let n = t.chars().count();
    n > 0
        && n <= MAX_HEADING_CHARS
        && !t.ends_with([',', ';'])
        && t.chars().any(char::is_alphabetic)
}

impl HeadingScale {
    /// The scale of `lines`: body = the size carrying the most characters.
    pub fn of<'a>(lines: impl IntoIterator<Item = &'a TextLine>) -> Self {
        let mut weight: std::collections::BTreeMap<i32, usize> = Default::default();
        let mut all: Vec<&TextLine> = Vec::new();
        let mut declared = Vec::new();
        for l in lines {
            if let Some(level) = l.level {
                declared.push((l.size, level));
            }
            if l.size > 0.0 {
                *weight.entry(round(l.size)).or_default() += l.text.chars().count();
                all.push(l);
            }
        }
        let Some((&body, _)) = weight.iter().max_by_key(|(k, w)| (**w, -**k)) else {
            return Self::default();
        };
        let body = body as f32 / 2.0;
        let mut sizes: Vec<f32> = all
            .iter()
            .filter(|l| l.size >= body * HEADING_RATIO && heading_text_ok(&l.text))
            .map(|l| l.size)
            .collect();
        sizes.sort_by(|a, b| b.total_cmp(a));
        sizes.dedup_by(|a, b| (*b - *a).abs() < SIZE_STEP);
        Self {
            body,
            sizes,
            declared,
        }
    }

    /// Below every declared heading at least as large, else by size rank.
    fn size_level(&self, size: f32, rank: usize) -> u8 {
        self.declared
            .iter()
            .filter(|(s, _)| *s + SIZE_STEP > size)
            .map(|(_, l)| l + 1)
            .max()
            .unwrap_or_else(|| level_of(rank))
            .min(MAX_LEVEL)
    }

    /// Heading level of `line`, if it is a heading.
    pub fn level(&self, line: &TextLine) -> Option<u8> {
        if let Some(l) = line.level {
            return Some(l.clamp(1, MAX_LEVEL));
        }
        if self.body <= 0.0 || !heading_text_ok(&line.text) {
            return None;
        }
        if line.size >= self.body * HEADING_RATIO {
            let rank = self
                .sizes
                .iter()
                .position(|s| (s - line.size).abs() < SIZE_STEP)
                .unwrap_or(self.sizes.len().saturating_sub(1));
            return Some(self.size_level(line.size, rank));
        }
        // A short bold line at body size standing alone: the lowest heading level.
        let t = line.text.trim();
        let standalone = !t.ends_with(['.', ':']) && t.chars().count() <= 60;
        (line.bold && standalone && (line.size - self.body).abs() < SIZE_STEP)
            .then(|| self.size_level(line.size, self.sizes.len()))
    }
}

fn level_of(rank: usize) -> u8 {
    u8::try_from(rank + 1).unwrap_or(MAX_LEVEL).min(MAX_LEVEL)
}

/// Render `lines` with `scale`: headings become `#` lines surrounded by blank lines, paragraph
/// breaks become blank lines.
pub fn render(lines: &[TextLine], scale: &HeadingScale) -> String {
    let lines: Vec<&TextLine> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let levels = heading_levels(&lines, scale);
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let blank = out.is_empty() || out.ends_with("\n\n");
        let Some(level) = levels[i] else {
            if l.gap_before && !blank {
                out.push('\n');
            }
            out.push_str(l.text.trim_end());
            out.push('\n');
            i += 1;
            continue;
        };
        // A heading wrapped over two lines is one heading.
        let mut text = l.text.trim().trim_start_matches('#').trim().to_string();
        if levels.get(i + 1) == Some(&Some(level))
            && l.level.is_none()
            && lines[i + 1].level.is_none()
        {
            text.push(' ');
            text.push_str(lines[i + 1].text.trim());
            i += 1;
        }
        if !blank {
            out.push('\n');
        }
        out.push_str(&"#".repeat(usize::from(level)));
        out.push(' ');
        out.push_str(&text);
        out.push_str("\n\n");
        i += 1;
    }
    out.trim_end().to_string()
}

/// Heading level per line. More than [`MAX_HEADING_LINES`] consecutive lines at one heading
/// size (and no declared level) are a passage set in a larger face, not headings.
fn heading_levels(lines: &[&TextLine], scale: &HeadingScale) -> Vec<Option<u8>> {
    let mut levels: Vec<Option<u8>> = lines.iter().map(|l| scale.level(l)).collect();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i + 1;
        if levels[i].is_some() && lines[i].level.is_none() {
            while j < lines.len()
                && levels[j].is_some()
                && lines[j].level.is_none()
                && (lines[j].size - lines[i].size).abs() < SIZE_STEP
                && lines[j].bold == lines[i].bold
            {
                j += 1;
            }
            if j - i > MAX_HEADING_LINES {
                levels[i..j].iter_mut().for_each(|l| *l = None);
            }
        }
        i = j;
    }
    levels
}

/// Render with the document's own scale.
pub fn to_markdown(lines: &[TextLine]) -> String {
    render(lines, &HeadingScale::of(lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, size: f32, bold: bool) -> TextLine {
        TextLine {
            text: text.into(),
            size,
            bold,
            ..TextLine::default()
        }
    }

    #[test]
    fn sizes_and_bold_become_heading_levels() {
        let lines = vec![
            line("Chapter of Things", 24.0, true),
            line(
                "Some body text that is long enough to be the body size.",
                10.0,
                false,
            ),
            line("A Section", 16.0, false),
            line(
                "More body text, again long enough to dominate the weights.",
                10.0,
                false,
            ),
            line("Bold Item", 10.0, true),
            line("Body sentence ends here.", 10.0, false),
        ];
        let md = to_markdown(&lines);
        assert!(md.starts_with("# Chapter of Things\n\n"), "{md}");
        assert!(md.contains("\n## A Section\n"), "{md}");
        assert!(md.contains("\n### Bold Item\n"), "{md}");
        assert!(md.contains("Body sentence ends here."));
    }
}
