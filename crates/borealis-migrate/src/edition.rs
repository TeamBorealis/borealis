//! Edition of migrated Aurora content (finding A3 of `specs/handoffs/st3-diff.md`).
//!
//! Aurora has no edition field: its 2024 books are separate sources (`Player’s Handbook (2024)`)
//! whose spells carry the same class support tags as the 2014 ones, and Aurora relies on the user
//! switching sources on or off. Borealis keeps both editions loaded, so a converted element that
//! belongs to a 2024 source is tagged `dnd2024` here, deterministically (IM1), from the `Source`
//! elements of the same run. A source is 2024 when:
//! - its `information` setter starts with `D&D 5.5e`, the marker the AuroraLegacy core and
//!   partnered repositories put on every 2024 source (Aurora shows it as the source's information
//!   message, `SourceParser.InformationMessage`), or
//! - it is official (`official` = true) and its `release` setter (`YYYYMMDD`) is on or after the
//!   2024 Player's Handbook, the IM8 rule for 5etools books without an edition flag.
//!
//! Third-party sources dated after 2024 without the marker (`The Crooked Moon 2014`) stay 2014.
//! A file whose elements all belong to 2024 sources (helpers of source `Internal`, which is not a
//! `Source` element, follow their file) becomes a `dnd2024` file; in a mixed file only the 2024
//! elements are tagged. Only a 2014 run is re-tagged: `--ruleset dnd2024` already stamps 2024.

use borealis_content::{ContentFile, Element, Ruleset};
use std::collections::BTreeMap;

/// Release date (`YYYYMMDD`) of the 2024 Player's Handbook, the IM8 cutoff.
const PHB_2024_RELEASE: &str = "20240917";
/// Start of the `information` setter of a 2024 source.
const MARKER_2024: &str = "D&D 5.5e";

/// Lower-cased source name with typographic apostrophes folded, as element `source` attributes
/// spell the same source both ways (`Player’s Handbook`, `Player's Handbook`).
fn source_key(name: &str) -> String {
    name.trim().replace('’', "'").to_lowercase()
}

fn is_release_date(d: &str) -> bool {
    d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit())
}

/// True when a `Source` element marks 2024 content (see the module docs).
#[must_use]
pub fn is_2024_source(source: &Element) -> bool {
    let setter = |name: &str| source.setter_value(name).map(str::trim);
    let marked =
        setter("information").is_some_and(|v| v.replace("&amp;", "&").starts_with(MARKER_2024));
    let official = setter("official").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let late = setter("release").is_some_and(|d| is_release_date(d) && d >= PHB_2024_RELEASE);
    marked || (official && late)
}

/// Every `Source` element of the run by name (and `alternative` name): is it 2024?
fn source_editions(files: &[(String, ContentFile)]) -> BTreeMap<String, bool> {
    let mut out = BTreeMap::new();
    for el in files
        .iter()
        .flat_map(|(_, f)| &f.elements)
        .filter(|e| e.ty == "Source")
    {
        let late = is_2024_source(el);
        out.insert(source_key(&el.name), late);
        for alt in el.setters.iter().filter(|s| s.name == "alternative") {
            out.insert(source_key(&alt.value), late);
        }
    }
    out
}

/// `Some(true)` for an element of a 2024 source, `Some(false)` for one of a known 2014 source,
/// `None` when its source is not a `Source` element of the run (`Internal`, another pack).
fn element_is_2024(el: &Element, sources: &BTreeMap<String, bool>) -> Option<bool> {
    if el.ty == "Source" {
        return Some(is_2024_source(el));
    }
    sources.get(&source_key(&el.source)).copied()
}

/// Tag the 2024 content of a 2014 run. Returns the indices of the files that changed.
pub fn tag_2024(files: &mut [(String, ContentFile)], run_ruleset: Ruleset) -> Vec<usize> {
    if run_ruleset != Ruleset::Dnd2014 {
        return Vec::new();
    }
    let sources = source_editions(files);
    if !sources.values().any(|late| *late) {
        return Vec::new();
    }
    let mut changed = Vec::new();
    for (i, (_, file)) in files.iter_mut().enumerate() {
        let kinds: Vec<Option<bool>> = file
            .elements
            .iter()
            .map(|e| element_is_2024(e, &sources))
            .collect();
        if !kinds.contains(&Some(true)) {
            continue;
        }
        if kinds.contains(&Some(false)) {
            for (el, kind) in file.elements.iter_mut().zip(&kinds) {
                if *kind == Some(true) {
                    el.ruleset = Some(Ruleset::Dnd2024);
                }
            }
        } else {
            file.ruleset = Some(Ruleset::Dnd2024);
        }
        changed.push(i);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Setter;

    fn source(name: &str, setters: &[(&str, &str)]) -> Element {
        let mut e = Element::new(format!("ID_SRC_{}", name.len()), name, "Source", "Core");
        e.setters = setters.iter().map(|(n, v)| Setter::new(*n, *v)).collect();
        e
    }

    fn el(id: &str, source: &str) -> Element {
        Element::new(id, id, "Spell", source)
    }

    fn file(elements: Vec<Element>) -> ContentFile {
        let mut f = ContentFile::new(elements);
        f.ruleset = Some(Ruleset::Dnd2014);
        f
    }

    #[test]
    fn marker_or_official_release_date_marks_2024() {
        assert!(is_2024_source(&source("A", &[("information", "D&D 5.5e")])));
        assert!(is_2024_source(&source(
            "B",
            &[("information", "D&amp;D 5.5e. Only SRD 5.2.1 content.")]
        )));
        assert!(is_2024_source(&source(
            "C",
            &[("official", "true"), ("release", "20240917")]
        )));
        // a day before the 2024 handbook, third-party, malformed date or no setters: 2014
        assert!(!is_2024_source(&source(
            "D",
            &[("official", "true"), ("release", "20240916")]
        )));
        assert!(!is_2024_source(&source(
            "E",
            &[("third-party", "true"), ("release", "20250613")]
        )));
        assert!(!is_2024_source(&source(
            "F",
            &[("official", "true"), ("release", "2025-01-01")]
        )));
        assert!(!is_2024_source(&source("G", &[])));
    }

    #[test]
    fn whole_file_of_a_2024_source_becomes_a_2024_file() {
        let new = source("Player’s Handbook (2024)", &[("information", "D&D 5.5e")]);
        let old = source(
            "Player’s Handbook",
            &[("official", "true"), ("release", "20140819")],
        );
        let mut files = vec![
            ("a/source.xml".into(), file(vec![new])),
            ("b/source.xml".into(), file(vec![old])),
            (
                "a/spells.xml".into(),
                file(vec![
                    el("NEW", "Player's Handbook (2024)"),
                    el("HELPER", "Internal"),
                ]),
            ),
            (
                "b/spells.xml".into(),
                file(vec![el("OLD", "Player’s Handbook")]),
            ),
            ("c.xml".into(), file(vec![el("HELPER2", "Internal")])),
        ];
        assert_eq!(tag_2024(&mut files, Ruleset::Dnd2014), vec![0, 2]);
        assert_eq!(files[0].1.ruleset, Some(Ruleset::Dnd2024));
        assert_eq!(files[1].1.ruleset, Some(Ruleset::Dnd2014));
        assert_eq!(files[2].1.ruleset, Some(Ruleset::Dnd2024));
        assert!(files[2].1.elements.iter().all(|e| e.ruleset.is_none()));
        assert_eq!(files[3].1.ruleset, Some(Ruleset::Dnd2014));
        assert_eq!(files[4].1.ruleset, Some(Ruleset::Dnd2014));
    }

    #[test]
    fn mixed_file_tags_only_its_2024_elements() {
        let mut alt = source(
            "Dungeon Master’s Guide (2024)",
            &[("information", "D&D 5.5e")],
        );
        alt.setters.push(Setter::new("alternative", "DMG 2024"));
        let old = source("Dungeon Master’s Guide", &[("official", "true")]);
        let mut files = vec![(
            "mixed.xml".into(),
            file(vec![
                alt,
                old,
                el("NEW", "DMG 2024"),
                el("OLD", "Dungeon Master’s Guide"),
                el("HELPER", "Internal"),
            ]),
        )];
        assert_eq!(tag_2024(&mut files, Ruleset::Dnd2014), vec![0]);
        let f = &files[0].1;
        assert_eq!(f.ruleset, Some(Ruleset::Dnd2014));
        let tags: Vec<_> = f.elements.iter().map(|e| e.ruleset).collect();
        let n = Some(Ruleset::Dnd2024);
        assert_eq!(tags, [n, None, n, None, None]);
    }

    #[test]
    fn a_2024_run_or_a_run_without_2024_sources_is_unchanged() {
        let new = source("New", &[("information", "D&D 5.5e")]);
        let mut files = vec![("a.xml".into(), file(vec![new, el("X", "New")]))];
        assert!(tag_2024(&mut files, Ruleset::Dnd2024).is_empty());
        let old = source("Old", &[("release", "20260101")]);
        let mut files = vec![("b.xml".into(), file(vec![old, el("Y", "Old")]))];
        assert!(tag_2024(&mut files, Ruleset::Dnd2014).is_empty());
        assert_eq!(files[0].1.ruleset, Some(Ruleset::Dnd2014));
    }
}
