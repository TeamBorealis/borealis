//! Content text helpers: a small allow-list HTML sanitizer for element descriptions.

const ALLOWED: &[&str] = &[
    "p",
    "br",
    "strong",
    "b",
    "em",
    "i",
    "u",
    "sup",
    "sub",
    "ul",
    "ol",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "blockquote",
    "code",
    "hr",
    "div",
    "span",
];
const DROP_WITH_CONTENT: &[&str] = &["script", "style", "iframe", "object", "embed", "template"];

/// Reduce an HTML fragment to allow-listed formatting tags without any attributes.
///
/// Scripts, styles, embeds and comments disappear with their content, unknown tags (links,
/// images, forms) are dropped but keep their text, and a stray `<` is escaped.
#[must_use]
pub fn sanitize_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut skip_until: Option<String> = None;
    while let Some(lt) = rest.find('<') {
        let (text, tail) = rest.split_at(lt);
        if skip_until.is_none() {
            out.push_str(text);
        }
        if tail.starts_with("<!--") {
            rest = tail.find("-->").map_or("", |e| &tail[e + 3..]);
            continue;
        }
        let Some(gt) = tail.find('>') else {
            if skip_until.is_none() {
                out.push_str("&lt;");
                out.push_str(&tail[1..]);
            }
            return out;
        };
        let inner = &tail[1..gt];
        rest = &tail[gt + 1..];
        let closing = inner.starts_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if name.is_empty() {
            if skip_until.is_none() {
                out.push_str("&lt;");
                out.push_str(inner);
                out.push_str("&gt;");
            }
            continue;
        }
        if let Some(until) = &skip_until {
            if closing && *until == name {
                skip_until = None;
            }
            continue;
        }
        if !closing && DROP_WITH_CONTENT.contains(&name.as_str()) && !inner.ends_with('/') {
            skip_until = Some(name);
        } else if ALLOWED.contains(&name.as_str()) {
            if closing {
                out.push_str(&format!("</{name}>"));
            } else {
                out.push_str(&format!("<{name}>"));
            }
        }
    }
    if skip_until.is_none() {
        out.push_str(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sanitize_html;

    #[test]
    fn keeps_formatting_and_drops_attributes() {
        assert_eq!(
            sanitize_html("<p class=\"x\" onclick=\"a()\">Hi <strong>you</strong></p>"),
            "<p>Hi <strong>you</strong></p>"
        );
    }

    #[test]
    fn removes_scripts_links_and_comments_but_keeps_link_text() {
        assert_eq!(
            sanitize_html("a<script>alert(1)</script>b<!-- c --><a href=\"x\">link</a><img src=x>"),
            "ablink"
        );
    }

    #[test]
    fn escapes_a_stray_angle_bracket() {
        assert_eq!(sanitize_html("1 < 2"), "1 &lt; 2");
        assert_eq!(sanitize_html("a <> b"), "a &lt;&gt; b");
    }
}
