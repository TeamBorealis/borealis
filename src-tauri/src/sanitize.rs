//! Allow-list HTML sanitiser for content descriptions shown in the compendium.
//!
//! Only formatting tags survive, without attributes (except numeric `colspan`/`rowspan` on cells);
//! scripts, styles, frames and the like are removed with their content, and unknown tags are
//! dropped while their text is kept. Stray `<`, `>` and `&` are escaped.

const ALLOWED: [&str; 31] = [
    "p",
    "br",
    "hr",
    "b",
    "strong",
    "i",
    "em",
    "u",
    "s",
    "sup",
    "sub",
    "small",
    "code",
    "span",
    "div",
    "blockquote",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "h1",
    "h2",
    "h3",
];
const EXTRA_ALLOWED: [&str; 4] = ["h4", "h5", "h6", "caption"];
const VOID: [&str; 2] = ["br", "hr"];
const DROP_WITH_CONTENT: [&str; 10] = [
    "script", "style", "iframe", "object", "embed", "template", "noscript", "svg", "math", "head",
];
const MAX_TAG_CHARS: usize = 2000;

fn is_allowed(name: &str) -> bool {
    ALLOWED.contains(&name) || EXTRA_ALLOWED.contains(&name)
}

/// Sanitise `html` for display.
#[must_use]
pub fn sanitize_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        match c {
            '<' => rest = tag(rest, &mut out),
            '>' => {
                out.push_str("&gt;");
                rest = &rest[1..];
            }
            '&' => {
                if entity_len(rest).is_some_and(|n| n > 0) {
                    out.push('&');
                } else {
                    out.push_str("&amp;");
                }
                rest = &rest[1..];
            }
            _ => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// Length of a well-formed entity (`&amp;`, `&#39;`) at the start of `s`.
fn entity_len(s: &str) -> Option<usize> {
    let body = s.strip_prefix('&')?;
    let end = body.find(';')?;
    let name = &body[..end];
    let valid = !name.is_empty()
        && name.len() <= 10
        && name
            .strip_prefix('#')
            .map_or(name.chars().all(|c| c.is_ascii_alphanumeric()), |n| {
                !n.is_empty() && n.chars().all(|c| c.is_ascii_hexdigit() || c == 'x')
            });
    valid.then_some(end + 2)
}

/// Handle the tag starting at `s[0] == '<'`; returns the remaining input.
fn tag<'a>(s: &'a str, out: &mut String) -> &'a str {
    if let Some(after) = s.strip_prefix("<!--") {
        return after.find("-->").map_or("", |i| &after[i + 3..]);
    }
    let Some(end) = s.find('>').filter(|e| *e <= MAX_TAG_CHARS) else {
        out.push_str("&lt;");
        return &s[1..];
    };
    let inner = &s[1..end];
    let after = &s[end + 1..];
    let closing = inner.starts_with('/');
    let body = inner.trim_start_matches('/').trim_end_matches('/');
    let name: String = body
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase();
    if name.is_empty() {
        out.push_str("&lt;");
        return &s[1..];
    }
    if DROP_WITH_CONTENT.contains(&name.as_str()) {
        if closing {
            return after;
        }
        return skip_element(after, &name);
    }
    if is_allowed(&name) {
        if closing {
            if !VOID.contains(&name.as_str()) {
                out.push_str("</");
                out.push_str(&name);
                out.push('>');
            }
        } else {
            out.push('<');
            out.push_str(&name);
            if name == "td" || name == "th" {
                span_attrs(body, out);
            }
            out.push('>');
        }
    }
    after
}

/// Skip to just after the matching closing tag (or the end of input).
fn skip_element<'a>(s: &'a str, name: &str) -> &'a str {
    let lower = s.to_ascii_lowercase();
    let needle = format!("</{name}");
    match lower.find(&needle) {
        Some(i) => lower[i..].find('>').map_or("", |j| &s[i + j + 1..]),
        None => "",
    }
}

/// Keep `colspan` / `rowspan` when they are small numbers.
fn span_attrs(body: &str, out: &mut String) {
    for attr in ["colspan", "rowspan"] {
        let lower = body.to_ascii_lowercase();
        let Some(i) = lower.find(attr) else { continue };
        let value: String = lower[i + attr.len()..]
            .trim_start()
            .trim_start_matches('=')
            .trim_start()
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = value.parse::<u32>() {
            if (1..=20).contains(&n) {
                out.push_str(&format!(" {attr}=\"{n}\""));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_html;

    #[test]
    fn keeps_formatting_and_drops_attributes() {
        assert_eq!(
            sanitize_html("<p class=\"x\" onclick=\"evil()\">Hi <b>there</b><br/></p>"),
            "<p>Hi <b>there</b><br></p>"
        );
    }

    #[test]
    fn removes_scripts_with_their_content() {
        assert_eq!(
            sanitize_html("a<script>alert(1)</script>b<STYLE>p{}</STYLE>c"),
            "abc"
        );
    }

    #[test]
    fn unknown_tags_keep_text_and_links_lose_href() {
        assert_eq!(
            sanitize_html("<a href=\"javascript:x\">click</a> <img src=x onerror=y>"),
            "click "
        );
    }

    #[test]
    fn escapes_stray_markup_and_keeps_entities() {
        assert_eq!(
            sanitize_html("1 < 2 > 0 & &amp; &#39;"),
            "1 &lt; 2 &gt; 0 &amp; &amp; &#39;"
        );
    }

    #[test]
    fn table_cells_keep_numeric_spans_only() {
        assert_eq!(
            sanitize_html("<td colspan=\"2\" style=\"x\">c</td><td colspan=\"abc\">d</td>"),
            "<td colspan=\"2\">c</td><td>d</td>"
        );
    }

    #[test]
    fn unterminated_comment_and_tag_do_not_panic() {
        assert_eq!(sanitize_html("a<!-- b"), "a");
        assert_eq!(sanitize_html("a <b"), "a &lt;b");
    }

    #[test]
    fn length_changing_lowercase_does_not_misalign_skipped_elements() {
        // `İ` lower-cases to three bytes; offsets must still index the original text.
        let html = "<p>a</p><script>İİİ ß ẞ Σ</script><p>b</p>";
        assert_eq!(sanitize_html(html), "<p>a</p><p>b</p>");
        let styled = "<STYLE>İİ</STYLE>ok<td colspan=\"2\" İ>x</td>";
        assert!(sanitize_html(styled).starts_with("ok"));
    }
}
