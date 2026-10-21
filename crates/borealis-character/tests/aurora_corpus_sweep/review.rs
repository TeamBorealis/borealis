//! Sweep problems that are the correct outcome for the content as written (T6): the row passes
//! with the reviewed reason, and only when the claim behind the reason holds on the loaded
//! content and the corpus source. Anything else stays a failure.
//!
//! * a non-standard id (D-14): the element is in the store under exactly the id the source
//!   wrote, which breaks the `ID_UPPER_CASE` convention; Borealis keeps it and warns, as Aurora
//!   accepts any id.
//! * a reference to an unknown element: the id is defined by no element of the store (bundled
//!   packs and the swept repo) nor by any element of any corpus repo's XML (ids compare
//!   ordinally, as in Aurora); Borealis reports it as a content warning where Aurora reports or
//!   skips it the same way. An id some corpus file defines (a migration loss, a repo not loaded)
//!   still fails.
//! * a select that offers nothing: its supports is one plain tag that no element of the select's
//!   type carries (nor has as id), so no content can satisfy it in Aurora either.

use borealis_content::{ContentStore, Rule};
use std::cell::OnceCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const NON_STANDARD_ID: &str = "Warning NonStandardId:";
const UNKNOWN_ELEMENT: &str = "unknown element ";
const OFFERS_NOTHING: &str = "select of type ";

/// Reviewer of one sweep run's problems.
pub struct Review<'a> {
    store: &'a ContentStore,
    repos: Option<PathBuf>,
    corpus_ids: OnceCell<HashSet<String>>,
}

impl<'a> Review<'a> {
    pub fn new(store: &'a ContentStore, repos: Option<PathBuf>) -> Self {
        Self {
            store,
            repos,
            corpus_ids: OnceCell::new(),
        }
    }

    /// The reviewed reason when `problem`, reported for element `id`, is the correct outcome.
    pub fn accept(&self, id: &str, problem: &str) -> Option<String> {
        if problem.starts_with(NON_STANDARD_ID) {
            return self.non_standard(id);
        }
        if let Some(i) = problem.rfind(UNKNOWN_ELEMENT) {
            return self.undefined(problem[i + UNKNOWN_ELEMENT.len()..].trim());
        }
        let rest = problem.strip_prefix(OFFERS_NOTHING)?;
        let (ty, owner) = rest.split_once(" offers no options ")?;
        self.unsatisfiable(ty, owner.trim())
    }

    fn non_standard(&self, id: &str) -> Option<String> {
        let kept = self.store.get(id).is_some_and(|s| s.element.id == id);
        let breaks = !id.starts_with("ID_")
            || id
                .chars()
                .any(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'));
        (kept && breaks).then(|| {
            "by design (D-14): the non-standard id is kept as the source wrote it and reported"
                .to_string()
        })
    }

    fn undefined(&self, missing: &str) -> Option<String> {
        if missing.is_empty() || missing.contains(' ') || self.store.get(missing).is_some() {
            return None;
        }
        let ids = self
            .corpus_ids
            .get_or_init(|| corpus_ids(self.repos.as_deref()));
        (!ids.is_empty() && !ids.contains(missing)).then(|| {
            format!(
                "content defect: {missing} is defined in no corpus repo nor bundled pack; \
                 reported as a content warning"
            )
        })
    }

    fn unsatisfiable(&self, ty: &str, owner: &str) -> Option<String> {
        let el = &self.store.get(owner)?.element;
        let plain = |t: &str| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-')
        };
        let carried = |tag: &str| {
            self.store
                .by_type(ty)
                .any(|s| s.element.id == tag || s.element.supports.iter().any(|x| x == tag))
        };
        let tag = el.rules.iter().find_map(|r| match r {
            Rule::Select(s) if s.ty == ty => s
                .supports
                .as_deref()
                .map(str::trim)
                .filter(|t| plain(t) && !carried(t)),
            _ => None,
        })?;
        Some(format!(
            "content defect: the {ty} select supports only \"{tag}\", which no {ty} element \
             carries, so nothing can be offered"
        ))
    }
}

/// Every `ID_` id an element defines (`id="..."`) in the XML files under `repos`.
fn corpus_ids(repos: Option<&Path>) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut stack: Vec<PathBuf> = repos.map(Path::to_path_buf).into_iter().collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
            {
                if let Ok(bytes) = std::fs::read(&path) {
                    defined_ids(&String::from_utf8_lossy(&bytes), &mut ids);
                }
            }
        }
    }
    ids
}

/// The ids of the `<element ... id="ID_...">` tags in `xml` (a `grant`, `select` or `extend`
/// naming an id is a reference, not a definition).
fn defined_ids(xml: &str, out: &mut HashSet<String>) {
    let mut rest = xml;
    while let Some(i) = rest.find("<element") {
        rest = &rest[i + "<element".len()..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(id) = attribute(tag, "id").filter(|id| id.starts_with("ID_")) {
            out.insert(id.to_string());
        }
    }
}

/// The value of attribute `name` inside a start tag.
fn attribute<'t>(tag: &'t str, name: &str) -> Option<&'t str> {
    for quote in ['"', '\''] {
        let open = format!("{name}={quote}");
        let mut from = 0;
        while let Some(i) = tag[from..].find(&open).map(|i| i + from) {
            from = i + open.len();
            if tag[..i].ends_with(char::is_whitespace) {
                let value = &tag[from..];
                return value.find(quote).map(|end| &value[..end]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::defined_ids;
    use std::collections::HashSet;

    #[test]
    fn defined_ids_reads_element_id_attributes_only() {
        // Arrange
        let xml = r#"<element name="A" id="ID_A_ONE"><grant type="Spell" id="ID_B"/></element>
            <elements><element type="Spell" xid="ID_NOT" id='ID_Mixed_Case'/></elements>"#;
        let mut ids = HashSet::new();
        // Act
        defined_ids(xml, &mut ids);
        // Assert: the element ids, not the granted one nor another attribute ending in "id"
        let want: HashSet<String> = ["ID_A_ONE", "ID_Mixed_Case"].map(String::from).into();
        assert_eq!(ids, want);
    }
}
