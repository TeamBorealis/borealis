//! Aurora's description injection (specs/content-format.md, "Reference injection"): inside a
//! description, `<div element="ID"/>` and `<p element="ID"/>` stand for another element's text.
//! Aurora's description panel replaces them when it shows an element
//! (DescriptionPanelViewModelBase.ReplaceInjectedElementDescription):
//! - a `div` gets an `h5` heading with the referenced element's upper-cased name, then that
//!   element's description;
//! - a `p` becomes a paragraph opening with the referenced element's name in bold italics,
//!   followed by its description.
//!
//! The stored description keeps the reference (content-format.md 3.3.3); this runs when a
//! description is shown. Injected text is not expanded again (Aurora replaces the inner XML after
//! it has walked the children), so a reference cycle cannot loop. An unknown id stays as written
//! (Aurora logs it and leaves the tag).

/// A referenced element's name and description, by id.
pub trait InjectionSource {
    /// `(name, description)` of the element `id`, if it exists.
    fn injected(&self, id: &str) -> Option<(String, String)>;
}

impl<F: Fn(&str) -> Option<(String, String)>> InjectionSource for F {
    fn injected(&self, id: &str) -> Option<(String, String)> {
        self(id)
    }
}

/// `html` with every `<div element="ID">` / `<p element="ID">` replaced by the text of `ID`.
#[must_use]
pub fn inject_references(html: &str, source: &impl InjectionSource) -> String {
    if !html.contains("element=") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let tail = &rest[lt..];
        let Some(gt) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let tag = &tail[..=gt];
        rest = &tail[gt + 1..];
        let Some(r) = reference(tag) else {
            out.push_str(tag);
            continue;
        };
        let Some((name, description)) = source.injected(r.id) else {
            out.push_str(tag);
            continue;
        };
        if !r.self_closing {
            rest = after_close(rest, r.tag);
        }
        out.push_str(&injected_html(r.tag, &name, &description));
    }
    out.push_str(rest);
    out
}

/// One `<div element="...">` or `<p element="...">` opening tag.
struct Reference<'a> {
    tag: &'static str,
    id: &'a str,
    self_closing: bool,
}

/// The reference an opening tag makes, if it is a `div` or `p` with an `element` attribute.
fn reference(tag: &str) -> Option<Reference<'_>> {
    let inner = tag.strip_prefix('<')?.strip_suffix('>')?;
    let self_closing = inner.trim_end().ends_with('/');
    let name_len = inner
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .count();
    let name = match inner[..name_len].to_ascii_lowercase().as_str() {
        "div" => "div",
        "p" => "p",
        _ => return None,
    };
    let id = attribute(&inner[name_len..], "element")?;
    (!id.is_empty()).then_some(Reference {
        tag: name,
        id,
        self_closing,
    })
}

/// The value of attribute `name` in a tag's attribute text, trimmed.
fn attribute<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let body = &value[1..];
        let end = body.find(quote)?;
        return Some(body[..end].trim());
    }
}

/// The text after the `</tag>` that closes an opened `tag`, nested ones of the same name
/// counted; the end of the text when it is never closed.
fn after_close<'a>(rest: &'a str, tag: &str) -> &'a str {
    let lower = rest.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut depth = 0usize;
    let mut at = 0usize;
    while let Some(lt) = lower[at..].find('<').map(|i| at + i) {
        let gt = lower[lt..].find('>').map_or(lower.len(), |i| lt + i + 1);
        let t = &lower[lt..gt];
        let named = |prefix: &str| {
            t.starts_with(prefix)
                && t[prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_ascii_alphanumeric())
        };
        if named(&close) {
            if depth == 0 {
                return &rest[gt..];
            }
            depth -= 1;
        } else if named(&open) && !t.trim_end_matches('>').trim_end().ends_with('/') {
            depth += 1;
        }
        at = gt;
    }
    ""
}

/// What replaces a reference: the heading form for a `div`, the run-in form for a `p`.
fn injected_html(tag: &str, name: &str, description: &str) -> String {
    if tag == "div" {
        return format!("<div><h5>{}</h5>{description}</div>", name.to_uppercase());
    }
    let description = description.trim();
    match description.strip_prefix("<p>") {
        Some(body) => format!("<p><strong><em>{name}. </em></strong> {body}"),
        None => format!("<p><strong><em>{name}. </em></strong></p>{description}"),
    }
}

#[cfg(test)]
mod tests {
    use super::inject_references;

    fn source(id: &str) -> Option<(String, String)> {
        match id {
            "ID_A" => Some(("Metal Line".into(), "<p>Choose a metal.</p>".into())),
            "ID_B" => Some(("Bee".into(), "<div element=\"ID_A\"/>".into())),
            _ => None,
        }
    }

    #[test]
    fn a_div_reference_becomes_the_heading_and_the_description() {
        assert_eq!(
            inject_references("\n<div element=\"ID_A\" />\n", &source),
            "\n<div><h5>METAL LINE</h5><p>Choose a metal.</p></div>\n"
        );
        assert_eq!(
            inject_references("<div element='ID_A'>old</div><p>after</p>", &source),
            "<div><h5>METAL LINE</h5><p>Choose a metal.</p></div><p>after</p>"
        );
    }

    #[test]
    fn a_nested_reference_inside_a_plain_div_is_replaced() {
        assert_eq!(
            inject_references(
                "<p>Hint.</p><div class=\"reference\"><div element=\"ID_A\"/></div>",
                &source
            ),
            "<p>Hint.</p><div class=\"reference\"><div><h5>METAL LINE</h5><p>Choose a metal.</p></div></div>"
        );
    }

    #[test]
    fn a_p_reference_runs_the_name_into_the_description() {
        assert_eq!(
            inject_references("<p element=\"ID_A\"/>", &source),
            "<p><strong><em>Metal Line. </em></strong> Choose a metal.</p>"
        );
    }

    #[test]
    fn unknown_ids_stay_and_injected_text_is_not_expanded_again() {
        assert_eq!(
            inject_references("<div element=\"ID_X\"/>", &source),
            "<div element=\"ID_X\"/>"
        );
        assert_eq!(
            inject_references("<div element=\"ID_B\"/>", &source),
            "<div><h5>BEE</h5><div element=\"ID_A\"/></div>"
        );
        assert_eq!(
            inject_references("<p data-element=\"ID_A\">x</p>", &source),
            "<p data-element=\"ID_A\">x</p>"
        );
    }
}
