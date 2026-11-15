//! Plain text of description HTML.

/// Strip HTML tags and decode the common entities; block tags become spaces, whitespace runs collapse.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::html_to_text;

    #[test]
    fn strips_tags_decodes_entities_and_collapses_whitespace() {
        assert_eq!(
            html_to_text("<p>Fire &amp; <b>ice</b></p>\n<p>3 &lt; 4&nbsp;ft.</p>"),
            "Fire & ice 3 < 4 ft."
        );
        assert_eq!(html_to_text("&amp;lt;"), "&lt;", "decoded once");
        assert_eq!(html_to_text(""), "");
    }
}
