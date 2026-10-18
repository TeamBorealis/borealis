//! Aurora's custom-file rule for elements its type parsers reject (content-format 9.1).
//!
//! Aurora parses every `<element>` of a custom file into a temporary list before merging any of
//! them; an exception from a type parser discards the WHOLE file, its elements and its appends
//! (`DataManager.InitializeElementDataAsync`). The exception modelled here is the missing-setter
//! one: `ElementParser.ValidateElementSetters` throws `MissingSetterException` when a `Level`,
//! `Class`, `Spell`, `Deity`, `Source` or `Companion` element lacks a setter its parser requires.
//!
//! So an identity-only stub such as `<element type="Level" id="ID_LEVEL_1" />` never replaces
//! the real level in Aurora: its file is not loaded at all. Borealis does the same for custom
//! content and names every discarded element in the report.

use crate::convert::element_lines;
use crate::report::Stats;
use borealis_content::{setter, ContentFile, Issue, IssueCode};

/// Setters Aurora's type parsers require, by exact element type (`ValidateElementSetters`).
pub const AURORA_REQUIRED_SETTERS: [(&str, &[&str]); 6] = [
    ("Level", &["level", "experience"]),
    ("Class", &["hd"]),
    ("Spell", &["level", "school", "time", "duration", "range"]),
    ("Deity", &["alignment", "domains", "symbol"]),
    ("Source", &["url", "abbreviation"]),
    (
        "Companion",
        &[
            "strength",
            "dexterity",
            "constitution",
            "intelligence",
            "wisdom",
            "charisma",
            "type",
            "size",
            "challenge",
        ],
    ),
];

/// An element whose Aurora type parser would throw for a missing setter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSetter {
    /// Element id.
    pub id: String,
    /// Element type.
    pub ty: String,
    /// The first required setter it lacks (the one Aurora reports).
    pub setter: &'static str,
}

/// Elements of `file` that make Aurora discard it, in file order.
#[must_use]
pub fn missing_setters(file: &ContentFile) -> Vec<MissingSetter> {
    file.elements
        .iter()
        .filter_map(|e| {
            let (_, required) = AURORA_REQUIRED_SETTERS
                .iter()
                .find(|(ty, _)| *ty == e.ty.as_str())?;
            let setter = required
                .iter()
                .find(|name| setter::find(&e.setters, name).is_none())?;
            Some(MissingSetter {
                id: e.id.clone(),
                ty: e.ty.clone(),
                setter,
            })
        })
        .collect()
}

/// The report entries of a discarded file: one `missing_required_setter` error per culprit, one
/// `element_dropped` error per element (with its line) and a `file_skipped` error that counts the
/// discarded appends.
#[must_use]
pub fn discard_issues(
    rel: &str,
    text: &str,
    file: &ContentFile,
    culprits: &[MissingSetter],
) -> Vec<Issue> {
    let Some(first) = culprits.first() else {
        return Vec::new();
    };
    let lines = element_lines(text);
    let locate = |issue: Issue, id: &str| {
        let issue = issue.in_file(rel.to_string()).at_element(id.to_string());
        match lines.get(id) {
            Some(&line) => issue.at_line(line),
            None => issue,
        }
    };
    let cause = format!(
        "{} ({}) has no {:?} setter",
        first.id, first.ty, first.setter
    );
    let mut out: Vec<Issue> = culprits
        .iter()
        .map(|c| {
            locate(
                Issue::error(
                    IssueCode::MissingRequiredSetter,
                    format!(
                        "{} element requires setter {:?}; Aurora does not load a custom file \
                         holding such an element, so this whole file is not imported",
                        c.ty, c.setter
                    ),
                ),
                &c.id,
            )
        })
        .collect();
    out.extend(file.elements.iter().map(|e| {
        locate(
            Issue::error(
                IssueCode::ElementDropped,
                format!("element dropped: its file is not loaded because {cause}"),
            ),
            &e.id,
        )
    }));
    out.push(
        Issue::error(
            IssueCode::FileSkipped,
            format!(
                "file not loaded ({} element(s), {} append(s) discarded): {cause}, and Aurora \
                 discards a custom file whose elements fail to parse",
                file.elements.len(),
                file.appends.len()
            ),
        )
        .in_file(rel.to_string()),
    );
    out
}

/// Counts of a discarded file: nothing converted, every element dropped.
#[must_use]
pub fn discarded_stats(stats: &Stats, file: &ContentFile) -> Stats {
    Stats {
        dropped_elements: stats.dropped_elements + file.elements.len(),
        dropped_rules: stats.dropped_rules,
        ..Stats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{Element, Setter};

    fn file(elements: Vec<Element>) -> ContentFile {
        ContentFile {
            elements,
            ..ContentFile::default()
        }
    }

    #[test]
    fn only_aurora_parser_types_with_a_missing_setter_are_culprits() {
        let stub = Element::new("ID_LEVEL_1", "1", "Level", "Internal");
        let mut level = Element::new("ID_LEVEL_2", "2", "Level", "Internal");
        level.setters.push(Setter::new("Level", "2"));
        level.setters.push(Setter::new("EXPERIENCE", "300"));
        let grants = Element::new("ID_G", "G", "Grants", "Internal");
        let mut half = Element::new("ID_LEVEL_3", "3", "Level", "Internal");
        half.setters.push(Setter::new("level", "3"));
        let lower = Element::new("ID_X", "x", "level", "Internal");
        let got = missing_setters(&file(vec![stub, level, grants, half, lower]));
        let ids: Vec<_> = got.iter().map(|m| (m.id.as_str(), m.setter)).collect();
        assert_eq!(ids, [("ID_LEVEL_1", "level"), ("ID_LEVEL_3", "experience")]);
    }

    #[test]
    fn discard_names_every_element_with_its_line() {
        let text = "<elements>\n<element type=\"Level\" id=\"ID_A\" />\n<element type=\"Feat\" id=\"ID_B\" />\n</elements>";
        let f = file(vec![
            Element::new("ID_A", "a", "Level", "Internal"),
            Element::new("ID_B", "b", "Feat", "Hb"),
        ]);
        let culprits = missing_setters(&f);
        let issues = discard_issues("x/sys.xml", text, &f, &culprits);
        let dropped: Vec<_> = issues
            .iter()
            .filter(|i| i.code == IssueCode::ElementDropped)
            .map(|i| (i.element.as_deref(), i.line))
            .collect();
        assert_eq!(dropped, [(Some("ID_A"), Some(2)), (Some("ID_B"), Some(3))]);
        assert!(issues
            .iter()
            .all(|i| i.file.as_deref() == Some("x/sys.xml")));
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.code == IssueCode::MissingRequiredSetter)
                .count(),
            1
        );
        assert!(issues.iter().any(|i| i.code == IssueCode::FileSkipped));
        let stats = discarded_stats(&Stats::default(), &f);
        assert_eq!((stats.elements, stats.dropped_elements), (0, 2));
    }
}
