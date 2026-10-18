//! When a content file is not well formed, none of its elements convert. So that none of them
//! disappears silently, a lenient scan finds every `<element ... id= type=>` start tag outside
//! comments and reports each one as dropped, with its id, type and line.

use super::util::Ctx;
use borealis_content::{Issue, IssueCode};

/// Report every `<element>` start tag of an unparsable file as a dropped element.
pub(crate) fn report_unparsed_elements(ctx: &mut Ctx, text: &str) {
    for (offset, attrs) in element_tags(text) {
        let id = attr(attrs, "id").filter(|i| !i.is_empty());
        let ty = attr(attrs, "type").filter(|t| !t.is_empty());
        if let (Some(id), Some(ty)) = (&id, &ty) {
            ctx.types.insert(id.clone(), ty.clone());
        }
        ctx.stats.dropped_elements += 1;
        let line = line_of(text, offset);
        let mut issue = Issue::error(
            IssueCode::ElementDropped,
            "element dropped: the file is not well-formed XML (see the parse error)",
        )
        .in_file(ctx.file.clone())
        .at_line(line);
        if let Some(id) = id {
            issue = issue.at_element(id);
        }
        ctx.issues.push(issue);
    }
}

/// The 1-based line of the first `<element>` start tag of every id, outside comments.
pub(crate) fn element_lines(text: &str) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for (offset, attrs) in element_tags(text) {
        if let Some(id) = attr(attrs, "id").filter(|i| !i.is_empty()) {
            out.entry(id).or_insert_with(|| line_of(text, offset));
        }
    }
    out
}

fn line_of(text: &str, offset: usize) -> u32 {
    let n = text[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `(offset, attribute text)` of every `<element` start tag outside comments.
fn element_tags(text: &str) -> Vec<(usize, &str)> {
    const OPEN: &str = "<element";
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(rel) = text[at..].find('<') {
        let start = at + rel;
        let rest = &text[start..];
        if rest.starts_with("<!--") {
            at = rest
                .find("-->")
                .map_or(text.len(), |end| start + end + "-->".len());
            continue;
        }
        at = start + 1;
        let after = &rest[OPEN.len().min(rest.len())..];
        if !rest.starts_with(OPEN) || !after.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let end = tag_end(after).unwrap_or(after.len());
        out.push((start, &after[..end]));
        at = start + OPEN.len() + end;
    }
    out
}

/// Index of the `>` that ends a start tag, skipping quoted values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

/// The unescaped value of attribute `name` in a start tag's attribute text.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        let value_part = rest[eq + 1..].trim_start();
        let quote = value_part
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')?;
        let body = &value_part[1..];
        let close = body.find(quote)?;
        if key.rsplit(char::is_whitespace).next() == Some(name) {
            return Some(unescape(body[..close].trim()));
        }
        rest = &body[close + 1..];
    }
}

fn unescape(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tags_outside_comments_with_either_quote() {
        let text = "<elements>\n<!-- <element id=\"ID_NO\" type=\"Feat\"> -->\n<element name='a>b' id='ID_A' type=\"Spell\">\n<elementx id=\"ID_X\">\n<element\tid=\"ID_O&apos;B\" type=\"Feat\"/>";
        let tags = element_tags(text);
        let ids: Vec<_> = tags.iter().filter_map(|(_, a)| attr(a, "id")).collect();
        assert_eq!(ids, ["ID_A", "ID_O'B"]);
        assert_eq!(attr(tags[0].1, "type").as_deref(), Some("Spell"));
        assert_eq!(line_of(text, tags[0].0), 3);
        assert_eq!(attr(" xid=\"1\" id=\"2\"", "id").as_deref(), Some("2"));
    }
}
