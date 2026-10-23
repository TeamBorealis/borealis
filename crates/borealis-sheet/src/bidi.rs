//! Right-to-left support: reorders a logical line into left-to-right glyph (visual) order.

use unicode_bidi::{bidi_class, BidiClass, BidiInfo, Level};

fn is_rtl_letter(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '\u{AB}' => '\u{BB}',
        '\u{BB}' => '\u{AB}',
        _ => c,
    }
}

/// Reverse an RTL run, keeping combining marks after their base character (rule L3) and
/// mirroring paired punctuation (rule L4).
fn reverse_run(run: &[char], out: &mut Vec<char>) {
    let mut end = run.len();
    while end > 0 {
        let mut start = end - 1;
        while start > 0 && bidi_class(run[start]) == BidiClass::NSM {
            start -= 1;
        }
        // run[start] is the base, run[start+1..end] its marks, already in logical order.
        out.push(mirror(run[start]));
        out.extend_from_slice(&run[start + 1..end]);
        end = start;
    }
}

/// Undo [`reverse_run`] on a stretch of display-order characters (the reversal is its own
/// inverse), giving the logical order a shaper expects.
pub(crate) fn unreverse(visual: &[char]) -> String {
    let mut out = Vec::with_capacity(visual.len());
    reverse_run(visual, &mut out);
    out.into_iter().collect()
}

/// The characters of `text` in left-to-right display order, or `None` when `text` has no
/// right-to-left letters (so logical and visual order are identical).
///
/// The paragraph base direction is always left-to-right, matching the sheet's layout, so a
/// mixed name such as `אלון Ægir` keeps its Latin part on the right.
pub(crate) fn visual_order(text: &str) -> Option<Vec<char>> {
    if !text.chars().any(is_rtl_letter) {
        return None;
    }
    let info = BidiInfo::new(text, Some(Level::ltr()));
    let mut out = Vec::with_capacity(text.len());
    for para in &info.paragraphs {
        let (levels, runs) = info.visual_runs(para, para.range.clone());
        for run in runs {
            let chars: Vec<char> = text[run.clone()].chars().collect();
            if levels[run.start].is_rtl() {
                reverse_run(&chars, &mut out);
            } else {
                out.extend(chars);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_is_untouched() {
        assert!(visual_order("Ægir Šimek").is_none());
    }

    #[test]
    fn hebrew_run_is_reversed_and_latin_kept() {
        let v: String = visual_order("אלון Ægir").unwrap().into_iter().collect();
        assert_eq!(v, "ןולא Ægir");
        let v: String = visual_order("abc אב (x)").unwrap().into_iter().collect();
        assert_eq!(v, "abc בא (x)");
    }

    #[test]
    fn unreverse_restores_logical_order_with_marks() {
        // beh, fatha, teh: the mark stays after its base in both directions.
        let logical = "\u{628}\u{64E}\u{62A}";
        let visual = visual_order(logical).unwrap();
        assert_eq!(visual, ['\u{62A}', '\u{628}', '\u{64E}']);
        assert_eq!(unreverse(&visual), logical);
    }

    #[test]
    fn mixed_latin_arabic_line_orders_runs() {
        // LTR paragraph: the Arabic word is reversed in place, Latin stays left to right.
        let v: String = visual_order("Ægir \u{633}\u{644}\u{645} Šimek")
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(v, "Ægir \u{645}\u{644}\u{633} Šimek");
        // Digits after Arabic letters are Arabic numbers (rule W2): they join the RTL run and
        // keep their own left-to-right order, so they show to the left of the word.
        let v: String = visual_order("Ægir \u{633}\u{644}\u{645} 12")
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(v, "Ægir 12 \u{645}\u{644}\u{633}");
    }

    #[test]
    fn brackets_are_mirrored_inside_rtl() {
        let v: String = visual_order("א(ב)").unwrap().into_iter().collect();
        assert_eq!(v, "(ב)א");
    }
}
