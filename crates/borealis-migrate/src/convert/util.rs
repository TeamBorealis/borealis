//! Shared helpers for the XML to JSON conversion.

use crate::dom::XmlElement;
use crate::report::Stats;
use borealis_content::{Extra, Issue, IssueCode, Severity, UnknownNode};
use std::collections::BTreeMap;

/// A failure that drops the element being converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertError {
    /// Category.
    pub code: IssueCode,
    /// Message.
    pub message: String,
    /// Source line.
    pub line: Option<u32>,
    /// Path inside the element.
    pub path: Option<String>,
}

impl ConvertError {
    pub(crate) fn new(code: IssueCode, message: impl Into<String>, el: &XmlElement) -> Self {
        Self {
            code,
            message: message.into(),
            line: Some(el.line),
            path: None,
        }
    }

    pub(crate) fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

pub(crate) type CResult<T> = Result<T, ConvertError>;

/// Per-file conversion context: collects issues and counters.
#[derive(Debug, Default)]
pub struct Ctx {
    pub(crate) file: String,
    pub(crate) issues: Vec<Issue>,
    pub(crate) stats: Stats,
}

impl Ctx {
    pub(crate) fn new(file: &str) -> Self {
        Self {
            file: file.to_string(),
            ..Self::default()
        }
    }

    pub(crate) fn report(
        &mut self,
        severity: Severity,
        code: IssueCode,
        message: impl Into<String>,
        element: Option<&str>,
        line: Option<u32>,
    ) {
        let mut issue = Issue::new(severity, code, message).in_file(self.file.clone());
        if let Some(id) = element {
            issue = issue.at_element(id);
        }
        if let Some(line) = line {
            issue = issue.at_line(line);
        }
        self.issues.push(issue);
    }

    pub(crate) fn warn(&mut self, code: IssueCode, message: impl Into<String>, el: &XmlElement) {
        self.report(Severity::Warning, code, message, None, Some(el.line));
    }

    pub(crate) fn warn_line(&mut self, code: IssueCode, message: impl Into<String>, line: u32) {
        self.report(Severity::Warning, code, message, None, Some(line));
    }

    pub(crate) fn note_normalized(&mut self, what: &str) {
        *self.stats.normalized.entry(what.to_string()).or_default() += 1;
    }

    /// Preserve an unknown child node in `extra.children`, count it and surface it.
    pub(crate) fn keep_child(&mut self, extra: &mut Extra, owner: &str, el: &XmlElement) {
        self.keep_child_silent(extra, owner, el);
        let (severity, why) = classify(&format!("{owner}/{}", el.name), &el.name);
        self.report(
            severity,
            IssueCode::UnsupportedConstruct,
            format!(
                "unknown child <{}> of <{owner}> kept in extra ({why})",
                el.name
            ),
            None,
            Some(el.line),
        );
    }

    /// Like [`Ctx::keep_child`] without a diagnostic (the caller already reported).
    pub(crate) fn keep_child_silent(&mut self, extra: &mut Extra, owner: &str, el: &XmlElement) {
        *self
            .stats
            .unknown_children
            .entry(format!("{owner}/{}", el.name))
            .or_default() += 1;
        extra.children.push(unknown_node(el));
    }

    /// Collect attributes not in `known` into `extra.attributes`, count and surface them.
    pub(crate) fn keep_attributes(
        &mut self,
        extra: &mut Extra,
        owner: &str,
        el: &XmlElement,
        known: &[&str],
    ) {
        for (k, v) in &el.attrs {
            if known.contains(&k.as_str()) {
                continue;
            }
            let key = format!("{owner}@{k}");
            let (severity, why) = classify(&key, k);
            self.report(
                severity,
                IssueCode::UnsupportedConstruct,
                format!(
                    "unknown attribute `{k}` on <{}> kept in extra ({why})",
                    el.name
                ),
                None,
                Some(el.line),
            );
            *self.stats.unknown_attributes.entry(key).or_default() += 1;
            extra.attributes.insert(k.clone(), v.clone());
        }
    }
}

/// Constructs that have no effect in the legacy format. They are harmless to drop, so they are reported at
/// Info level; every other unknown name is a Warning (possible typo or newer extension).
const LEGACY_IGNORED: &[&str] = &[
    "grant@known",
    "grant@supports",
    "grant@equipped",
    "sheet@name",
    "elements/p",
    "elements/b",
    "append/description",
    "append/requirements",
];

fn classify(key: &str, name: &str) -> (Severity, &'static str) {
    if LEGACY_IGNORED.contains(&key) {
        (Severity::Info, "has no effect in the legacy format")
    } else if name.contains(':') {
        (Severity::Info, "vendor-namespaced annotation")
    } else {
        (Severity::Warning, "not part of the format; ignored")
    }
}

/// Convert an unknown XML node to `{tag, attributes, text}`.
pub(crate) fn unknown_node(el: &XmlElement) -> UnknownNode {
    let text = if el.has_element_children() {
        el.inner_xml()
    } else {
        el.text().trim().to_string()
    };
    UnknownNode {
        tag: el.name.clone(),
        attributes: el.attrs.iter().cloned().collect::<BTreeMap<_, _>>(),
        text: if text.is_empty() { None } else { Some(text) },
    }
}

/// Required attribute, trimmed.
pub(crate) fn required_attr(el: &XmlElement, name: &str) -> CResult<String> {
    el.attr(name).map(|v| v.trim().to_string()).ok_or_else(|| {
        ConvertError::new(
            IssueCode::MissingAttribute,
            format!("<{}> is missing required attribute `{name}`", el.name),
            el,
        )
    })
}

/// Optional attribute, verbatim.
pub(crate) fn opt_attr(el: &XmlElement, name: &str) -> Option<String> {
    el.attr(name).map(ToString::to_string)
}

/// Optional boolean attribute with .NET `Convert.ToBoolean` semantics.
pub(crate) fn bool_attr(el: &XmlElement, name: &str) -> CResult<Option<bool>> {
    match el.attr(name) {
        None => Ok(None),
        Some(v) => borealis_content::setter::parse_bool(v)
            .map(Some)
            .ok_or_else(|| {
                ConvertError::new(
                    IssueCode::InvalidBoolean,
                    format!("attribute `{name}` has invalid boolean value {v:?}"),
                    el,
                )
            }),
    }
}

/// Optional non-negative integer attribute.
pub(crate) fn u32_attr(el: &XmlElement, name: &str) -> CResult<Option<u32>> {
    match el.attr(name) {
        None => Ok(None),
        Some(v) => v.trim().parse::<u32>().map(Some).map_err(|_| {
            ConvertError::new(
                IssueCode::InvalidInteger,
                format!("attribute `{name}` has invalid non-negative integer value {v:?}"),
                el,
            )
        }),
    }
}

/// Split a comma separated tag list: trimmed, empty pieces and later duplicates dropped.
pub(crate) fn split_tags(text: &str) -> (Vec<String>, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut duplicates = 0;
    for piece in text.split(',') {
        let tag = piece.trim();
        if tag.is_empty() {
            continue;
        }
        if out.iter().any(|t| t == tag) {
            duplicates += 1;
        } else {
            out.push(tag.to_string());
        }
    }
    (out, duplicates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    #[test]
    fn attribute_helpers() {
        let el = parse_document(r#"<a n="3" b="TRUE" bad="x" id=" X "/>"#).unwrap();
        assert_eq!(u32_attr(&el, "n").unwrap(), Some(3));
        assert_eq!(u32_attr(&el, "missing").unwrap(), None);
        assert_eq!(
            u32_attr(&el, "bad").unwrap_err().code,
            IssueCode::InvalidInteger
        );
        assert_eq!(bool_attr(&el, "b").unwrap(), Some(true));
        assert_eq!(
            bool_attr(&el, "n").unwrap_err().code,
            IssueCode::InvalidBoolean
        );
        assert_eq!(required_attr(&el, "id").unwrap(), "X");
        assert_eq!(
            required_attr(&el, "zz").unwrap_err().code,
            IssueCode::MissingAttribute
        );
        assert_eq!(opt_attr(&el, "id").as_deref(), Some(" X "));
    }

    #[test]
    fn tags_are_trimmed_and_deduplicated() {
        assert_eq!(
            split_tags(" A, B ,,A, C"),
            (vec!["A".into(), "B".into(), "C".into()], 1)
        );
        assert_eq!(split_tags(""), (vec![], 0));
    }

    #[test]
    fn unknown_nodes() {
        let el = parse_document(r#"<x k="v"><y>t</y></x>"#).unwrap();
        let n = unknown_node(&el);
        assert_eq!(n.text.as_deref(), Some("<y>t</y>"));
        assert_eq!(n.attributes["k"], "v");
        let empty = parse_document("<z/>").unwrap();
        assert_eq!(unknown_node(&empty).text, None);
    }
}
