/// The shared sanitizer applied before any boolean expression is parsed.
///
/// * every `,` becomes `&&`
/// * `&amp;` becomes `&`
/// * every run of `&` becomes `&&`, every run of `|` becomes `||`
/// * the result is trimmed
#[must_use]
pub fn sanitize(input: &str) -> String {
    let with_and = input.replace(',', "&&").replace("&amp;", "&");
    let mut out = String::with_capacity(with_and.len());
    let mut chars = with_and.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' || c == '|' {
            while chars.peek() == Some(&c) {
                chars.next();
            }
            out.push(c);
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn comma_amp_and_pipe_runs_are_normalized() {
        assert_eq!(sanitize("A,B"), "A&&B");
        assert_eq!(sanitize("A&B"), "A&&B");
        assert_eq!(sanitize("A&&B"), "A&&B");
        assert_eq!(sanitize("A&&&B"), "A&&B");
        assert_eq!(sanitize("A|B"), "A||B");
        assert_eq!(sanitize("A||B"), "A||B");
        assert_eq!(sanitize("A|||B"), "A||B");
        assert_eq!(sanitize("A &amp; B"), "A && B");
        assert_eq!(sanitize("  A, B  "), "A&& B");
        assert_eq!(sanitize(""), "");
    }
}
