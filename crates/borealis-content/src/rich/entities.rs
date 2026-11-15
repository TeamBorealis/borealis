//! HTML character references: decoding for the parser and the escape lint.

/// Named references the parser and the lint understand (the ones text pasted from the web or word processors
/// carries). Anything else is left as written.
const NAMED: [(&str, char); 31] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("rsquo", '\u{2019}'),
    ("lsquo", '\u{2018}'),
    ("rdquo", '\u{201d}'),
    ("ldquo", '\u{201c}'),
    ("sbquo", '\u{201a}'),
    ("bdquo", '\u{201e}'),
    ("ndash", '\u{2013}'),
    ("mdash", '\u{2014}'),
    ("hellip", '\u{2026}'),
    ("minus", '\u{2212}'),
    ("times", '\u{d7}'),
    ("divide", '\u{f7}'),
    ("frac12", '\u{bd}'),
    ("frac14", '\u{bc}'),
    ("frac34", '\u{be}'),
    ("deg", '\u{b0}'),
    ("plusmn", '\u{b1}'),
    ("copy", '\u{a9}'),
    ("reg", '\u{ae}'),
    ("trade", '\u{2122}'),
    ("bull", '\u{2022}'),
    ("middot", '\u{b7}'),
    ("eacute", '\u{e9}'),
    ("prime", '\u{2032}'),
    ("Prime", '\u{2033}'),
];

/// Longest reference body we look at (`#x10FFFF` is 8 characters).
const MAX_BODY: usize = 10;

/// Decode the character reference at the start of `s` (which starts with `&`).
/// Returns the character and the reference's length in bytes.
#[must_use]
pub(crate) fn decode_at(s: &str) -> Option<(char, usize)> {
    let body = s.strip_prefix('&')?;
    let end = body
        .char_indices()
        .take(MAX_BODY + 1)
        .find(|(_, c)| *c == ';')?
        .0;
    let name = &body[..end];
    let ch = if let Some(num) = name.strip_prefix('#') {
        let (digits, radix) = match num.strip_prefix(['x', 'X']) {
            Some(hex) => (hex, 16),
            None => (num, 10),
        };
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return None;
        }
        let code = u32::from_str_radix(digits, radix).ok()?;
        char::from_u32(code).filter(|c| *c != '\0')?
    } else {
        NAMED.iter().find(|(n, _)| *n == name)?.1
    };
    Some((ch, end + 2))
}

/// Decode every reference [`decode_at`] knows; anything else stays as written.
#[must_use]
pub(crate) fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some((c, n)) = decode_at(rest) {
            out.push(c);
            rest = &rest[n..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_named_and_numeric() {
        assert_eq!(decode("a &amp; b &lt;c&gt;"), "a & b <c>");
        assert_eq!(
            decode("it&#8217;s &#x2014; &rsquo;"),
            "it\u{2019}s \u{2014} \u{2019}"
        );
        assert_eq!(decode("&nbsp;"), "\u{a0}");
    }

    #[test]
    fn leaves_unknown_and_broken_references() {
        assert_eq!(
            decode("AT&T &unknown; &#; &#xZZ; & ;"),
            "AT&T &unknown; &#; &#xZZ; & ;"
        );
        assert_eq!(decode("&#0;"), "&#0;");
        assert_eq!(decode("&#+65; &#x+41;"), "&#+65; &#x+41;");
        assert_eq!(decode("&amp"), "&amp");
    }
}
