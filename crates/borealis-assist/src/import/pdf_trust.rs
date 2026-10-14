//! Whether a page's text layer can stand for the page. A layer that decodes to nonsense (a font
//! whose codes do not map to letters), or a page that shows text painted as images, is read
//! visually instead.

use super::layout::TextLine;
use super::pdf_walk::ImageBox;

/// Latin letters a page needs before its vowel share is judged.
const GARBLE_MIN_LETTERS: usize = 200;
/// Below this share of vowels among Latin letters the text layer is nonsense (prose in
/// European languages has about 35-45%).
const GARBLE_VOWEL_SHARE: f32 = 0.2;
/// Words a page needs before its share of common words is judged.
const MIN_WORDS: usize = 60;
/// Below this share of very common English words the layer is letter-shifted nonsense. English
/// prose has 35-50%; tables and stat blocks 10-25%; a mis-decoded font close to 0.
const MIN_COMMON_SHARE: f32 = 0.05;
/// Images this flat (width / height) and at most this tall (points) look like lines of text.
const STRIP_RATIO: f32 = 6.0;
const STRIP_MAX_HEIGHT: f32 = 30.0;
/// Text-line-shaped images on a page that make it "text painted as images".
const MIN_STRIPS: usize = 3;
/// An image covering this share of the page is a background, not content.
const BACKGROUND_SHARE: f32 = 0.85;

/// Very common English words (function words and the commonest verbs).
const COMMON: [&str; 64] = [
    "the", "of", "and", "to", "a", "in", "is", "you", "that", "it", "for", "on", "with", "as",
    "are", "be", "this", "your", "or", "can", "an", "by", "at", "from", "its", "has", "have", "if",
    "each", "which", "one", "not", "when", "until", "into", "per", "all", "any", "but", "they",
    "their", "them", "he", "she", "his", "her", "we", "our", "was", "were", "will", "may", "must",
    "no", "up", "so", "than", "then", "there", "these", "those", "who", "what", "also",
];

/// What the text of a page looks like.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Trust {
    /// Reads as text.
    Plausible,
    /// Nonsense, with the reason.
    Garbled(String),
}

/// Judge the decoded text of a page.
pub(crate) fn judge_text(lines: &[TextLine]) -> Trust {
    let text: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let letters: Vec<char> = text.chars().filter(char::is_ascii_alphabetic).collect();
    let vowels = letters
        .iter()
        .filter(|c| "aeiouyAEIOUY".contains(**c))
        .count();
    if letters.len() >= GARBLE_MIN_LETTERS
        && (vowels as f32) < GARBLE_VOWEL_SHARE * letters.len() as f32
    {
        return Trust::Garbled("its text layer has almost no vowels".into());
    }
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.len() >= MIN_WORDS {
        let common = words
            .iter()
            .filter(|w| COMMON.contains(&w.as_str()))
            .count();
        if (common as f32) < MIN_COMMON_SHARE * words.len() as f32 {
            return Trust::Garbled(format!(
                "its text layer does not read as words ({common} common words of {})",
                words.len()
            ));
        }
    }
    Trust::Plausible
}

/// Why the text layer of a page (its `lines`, the `images` it paints, its `size`) cannot stand
/// for it; `None` when it can.
pub(crate) fn distrust(
    lines: &[TextLine],
    images: &[ImageBox],
    size: (f32, f32),
) -> Option<String> {
    if let Trust::Garbled(why) = judge_text(lines) {
        return Some(why);
    }
    let area = (size.0 * size.1).max(1.0);
    let content: Vec<&ImageBox> = images
        .iter()
        .filter(|b| b.width() * b.height() < BACKGROUND_SHARE * area)
        .collect();
    let strips = content
        .iter()
        .filter(|b| {
            b.height() > 1.0
                && b.height() <= STRIP_MAX_HEIGHT
                && b.width() >= STRIP_RATIO * b.height()
        })
        .count();
    (strips >= MIN_STRIPS).then(|| format!("it shows {strips} lines of text painted as images"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(t: &str) -> Vec<TextLine> {
        vec![TextLine {
            text: t.into(),
            size: 10.0,
            ..TextLine::default()
        }]
    }

    #[test]
    fn shifted_text_is_garbled_and_prose_is_not() {
        let prose = "When you take the lantern into the hollow, the moths gather and you can see \
the path. Each of them glows for an hour, and the light is dim. ";
        assert_eq!(judge_text(&lines(&prose.repeat(3))), Trust::Plausible);
        // The same prose with every letter moved one place along the alphabet.
        let shifted: String = prose
            .chars()
            .map(|c| match c {
                'a'..='y' | 'A'..='Y' => (c as u8 + 1) as char,
                c => c,
            })
            .collect();
        assert!(matches!(
            judge_text(&lines(&shifted.repeat(3))),
            Trust::Garbled(_)
        ));
    }

    #[test]
    fn text_painted_as_images_is_distrusted() {
        let strip = |y: f32| ImageBox {
            x0: 72.0,
            y0: y,
            x1: 500.0,
            y1: y + 12.0,
        };
        let few = lines("A heading");
        assert!(distrust(
            &few,
            &[strip(700.0), strip(680.0), strip(660.0)],
            (612.0, 792.0)
        )
        .is_some());
        let background = ImageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 612.0,
            y1: 792.0,
        };
        assert!(distrust(&few, &[background], (612.0, 792.0)).is_none());
    }
}
