//! Aurora's custom-file rule for files its loader rejects (content-format 9.1, D-20).
//!
//! Aurora parses every `<element>` of a custom file into a temporary list before merging any of
//! them; an exception anywhere in that parse discards the WHOLE file, its elements and its
//! appends (`DataManager.InitializeElementDataAsync`, `Aurora_Builder/Builder.Presentation.
//! Services.Data/DataManager.cs:257-317`: one `try` around `ElementsFile.FromFile`, the header
//! and type parsers and the merge). [`culprits`] models every such throw (the list, with the
//! Aurora line of each, is in `model.rs`): a `Level`, `Class`, `Spell`, `Deity`, `Source` or
//! `Companion` without a setter its parser requires (`ValidateElementSetters`), an `<info>`
//! without `<update>`, an element without one of its four header attributes, a rule tag
//! Aurora does not know, a bad integer or boolean that a parser converts, an `Item` without
//! `<setters>`, and the rest.
//!
//! So an identity-only stub such as `<element type="Level" id="ID_LEVEL_1" />` never replaces
//! the real level in Aurora: its file is not loaded at all. Borealis does the same for custom
//! content and names every discarded element in the report. Core (embedded) content is not
//! affected: Aurora catches per element there.

mod model;

use crate::convert::element_lines;
use crate::dom::XmlElement;
use crate::report::Stats;
use borealis_content::{ContentFile, Issue, IssueCode};

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

/// Something in a custom file that makes Aurora's loader throw (and so discard the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Culprit {
    /// Element id (`None` for a file-level cause such as `<info>`).
    pub id: Option<String>,
    /// Element type, when the element has one.
    pub ty: Option<String>,
    /// 1-based line of the element (or `<info>`) start tag.
    pub line: u32,
    /// The issue category reported for it.
    pub code: IssueCode,
    /// What is wrong, for the report.
    pub cause: String,
    /// Where Aurora throws (decompiled source path and line).
    pub aurora: String,
}

/// Everything in this custom file (its raw XML root, before typo repair) that makes Aurora
/// discard it: the file-level cause first, then the first throw of each element, in file order.
#[must_use]
pub fn culprits(root: &XmlElement) -> Vec<Culprit> {
    let file = model::file_culprit(root);
    let elements = root
        .elements()
        .filter(|e| e.name == "element")
        .filter_map(model::element_culprit);
    file.into_iter().chain(elements).collect()
}

/// The report entries of a discarded file: one error per culprit (its own code), one
/// `element_dropped` error per converted element (with its line) and a `file_skipped` error
/// that counts the discarded appends.
#[must_use]
pub fn discard_issues(
    rel: &str,
    text: &str,
    file: &ContentFile,
    culprits: &[Culprit],
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
    let cause = match &first.id {
        Some(id) => format!(
            "{id} ({}): {}",
            first.ty.as_deref().unwrap_or("?"),
            first.cause
        ),
        None => first.cause.clone(),
    };
    let mut out: Vec<Issue> = culprits
        .iter()
        .map(|c| {
            let issue = Issue::error(
                c.code,
                format!(
                    "{}; Aurora's loader throws here ({}) and does not load a custom file \
                     holding this, so this whole file is not imported",
                    c.cause, c.aurora
                ),
            )
            .in_file(rel.to_string())
            .at_line(c.line);
            match &c.id {
                Some(id) => issue.at_element(id.clone()),
                None => issue,
            }
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
    use crate::dom::parse_document;
    use borealis_content::Element;

    fn found(xml: &str) -> Vec<(Option<String>, IssueCode)> {
        culprits(&parse_document(xml).unwrap())
            .into_iter()
            .map(|c| (c.id, c.code))
            .collect()
    }

    fn one(body: &str) -> Option<IssueCode> {
        let xml = format!("<elements>{body}</elements>");
        let got = found(&xml);
        assert!(got.len() <= 1, "{got:?}");
        got.first().map(|(_, code)| *code)
    }

    const LEVEL_OK: &str =
        r#"<setters><set name="Level">2</set><set name="EXPERIENCE">300</set></setters>"#;

    fn el(ty: &str, inner: &str) -> String {
        format!(r#"<element name="N" type="{ty}" source="S" id="ID_X">{inner}</element>"#)
    }

    #[test]
    fn missing_required_setters_name_the_element() {
        let xml = format!(
            "<elements>{}{}{}{}</elements>",
            r#"<element name="1" type="Level" source="Internal" id="ID_LEVEL_1" />"#,
            el("Level", LEVEL_OK).replace("ID_X", "ID_LEVEL_2"),
            r#"<element name="G" type="Grants" source="Internal" id="ID_G" />"#,
            r#"<element name="x" type="level" source="Internal" id="ID_LOWER" />"#,
        );
        assert_eq!(
            found(&xml),
            [(Some("ID_LEVEL_1".into()), IssueCode::MissingRequiredSetter)]
        );
    }

    #[test]
    fn clean_elements_of_every_modelled_type_pass() {
        for (ty, inner) in [
            ("Level", LEVEL_OK),
            (
                "Spell",
                r#"<setters><set name="level">0</set><set name="school">Evocation</set><set name="time">1 action</set><set name="duration">x</set><set name="range">x</set><set name="hasVerbalComponent">true</set><set name="isRitual"></set></setters>"#,
            ),
            (
                "Item",
                r#"<setters><set name="stackable">TRUE</set><set name="weight" excludeEncumbrance="false">1</set></setters>"#,
            ),
            (
                "Race",
                r#"<setters><set name="names" type="male">A, B</set></setters>"#,
            ),
            (
                "Class",
                r#"<setters><set name="hd">d8</set></setters><multiclass id="ID_MC"/>"#,
            ),
            (
                "Multiclass",
                r#"<setters><set name="hd">d8</set></setters><multiclass id="ID_MC"><setters><set name="multiclass proficiencies">-</set></setters></multiclass>"#,
            ),
            (
                "Feat",
                r#"<rules><!-- c --><grant type="Feat" id="ID_A" level="2"/><statistic name="x" value="1" inline="true"/><inline/><select type="List" name="L" number=" 1 "><item id="1">a</item></select></rules><spellcasting name="x" prepare="false"><list known="True">x</list></spellcasting><compendium display="false"/>"#,
            ),
        ] {
            assert_eq!(one(&el(ty, inner)), None, "{ty}");
        }
    }

    #[test]
    fn each_aurora_throw_is_a_culprit() {
        use IssueCode as C;
        let cases: &[(&str, &str, IssueCode)] = &[
            (
                "Feat",
                r#"<setters><set>no name</set></setters>"#,
                C::MissingAttribute,
            ),
            (
                "Feat",
                r#"<setters>stray text</setters>"#,
                C::MissingAttribute,
            ),
            (
                "Feat",
                r#"<setters><set name="allow duplicate">yes</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Feat",
                r#"<rules><Grant type="Feat" id="ID_A"/></rules>"#,
                C::UnknownRuleTag,
            ),
            ("Feat", r#"<rules>text</rules>"#, C::UnknownRuleTag),
            (
                "Feat",
                r#"<rules><grant type="Feat" id="ID_A" level="1st"/></rules>"#,
                C::InvalidInteger,
            ),
            (
                "Feat",
                r#"<rules><select name="s" number="two" type="Feat"/></rules>"#,
                C::InvalidInteger,
            ),
            (
                "Feat",
                r#"<rules><select name="s" type="Feat" optional="yes"/></rules>"#,
                C::InvalidBoolean,
            ),
            (
                "Feat",
                r#"<rules><select name="s" supports="x"/></rules>"#,
                C::MissingAttribute,
            ),
            (
                "Feat",
                r#"<rules><select name="s" type="List"><!-- c --><item id="1">a</item></select></rules>"#,
                C::InvalidInteger,
            ),
            (
                "Feat",
                r#"<rules><select name="s" type="List"><item id="a">a</item></select></rules>"#,
                C::InvalidInteger,
            ),
            (
                "Feat",
                r#"<rules><stat name="x" value="1" merge="1"/></rules>"#,
                C::InvalidBoolean,
            ),
            (
                "Feat",
                r#"<rules><grant type="Feat" id="ID_A"><set>v</set></grant></rules>"#,
                C::MissingAttribute,
            ),
            (
                "Feat",
                r#"<spellcasting name="x" extend="no"/>"#,
                C::InvalidBoolean,
            ),
            (
                "Feat",
                r#"<spellcasting name="x"><list known="1">x</list></spellcasting>"#,
                C::InvalidBoolean,
            ),
            (
                "Feat",
                r#"<equipment><description/></equipment>"#,
                C::UnsupportedConstruct,
            ),
            ("Feat", r#"<compendium display="hide"/>"#, C::InvalidBoolean),
            (
                "Level",
                r#"<setters><set name="level">first</set><set name="experience">0</set></setters>"#,
                C::InvalidInteger,
            ),
            (
                "Class",
                r#"<setters><set name="hd">d8</set></setters><multiclass/>"#,
                C::MissingAttribute,
            ),
            (
                "Multiclass",
                r#"<setters><set name="hd">d8</set></setters>"#,
                C::UnsupportedConstruct,
            ),
            (
                "Multiclass",
                r#"<setters><set name="hd">d8</set></setters><multiclass id="ID_X"/>"#,
                C::MulticlassIdSameAsClass,
            ),
            (
                "Spell",
                r#"<setters><set name="level">0</set><set name="school">E</set><set name="time">t</set><set name="duration">d</set><set name="range">r</set><set name="isConcentration">maybe</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Source",
                r#"<setters><set name="url">u</set><set name="abbreviation">A</set><set name="official">yes</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Companion",
                r#"<setters><set name="strength">10</set><set name="dexterity">10</set><set name="constitution">10</set><set name="intelligence">10</set><set name="wisdom">10</set><set name="charisma">10</set><set name="type">beast</set><set name="size">Tiny</set><set name="challenge">0</set></setters>"#,
                C::MissingRequiredSetter,
            ),
            (
                "Monster",
                r#"<setters><set name="size">M</set></setters>"#,
                C::MissingRequiredSetter,
            ),
            (
                "Item",
                r#"<rules><grant type="Proficiency" id="ID_P"/></rules>"#,
                C::MissingRequiredSetter,
            ),
            (
                "Weapon",
                r#"<setters><set name="weight" excludeEncumbrance="1">1</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Magic Item",
                r#"<setters><set name="cost" override="y">1</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Race",
                r#"<setters><set name="names">A, B</set></setters>"#,
                C::MissingRequiredSetter,
            ),
            (
                "Language",
                r#"<setters><set name="exotic">x</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Option",
                r#"<setters><set name="default">on</set></setters>"#,
                C::InvalidBoolean,
            ),
            (
                "Background Feature",
                r#"<setters><set name="primary">1</set></setters>"#,
                C::InvalidBoolean,
            ),
        ];
        for (ty, inner, code) in cases {
            assert_eq!(one(&el(ty, inner)), Some(*code), "{ty}: {inner}");
        }
        let headless = r#"<element name="N" type="Feat" id="ID_X" />"#;
        assert_eq!(one(headless), Some(C::MissingAttribute));
    }

    #[test]
    fn info_without_update_or_update_without_file_is_a_file_culprit() {
        for (info, bad) in [
            ("<info><name>x</name></info>", true),
            (r#"<info><update version="1.0"/></info>"#, true),
            (
                r#"<info><update version="1.0"><file name="a" url="u"/></update></info>"#,
                false,
            ),
            ("", false),
        ] {
            let got = found(&format!("<elements>{info}</elements>"));
            assert_eq!(!got.is_empty(), bad, "{info}");
            assert!(got.iter().all(|(id, _)| id.is_none()));
        }
    }

    #[test]
    fn discard_names_every_element_with_its_line() {
        let text = "<elements>\n<element name=\"a\" type=\"Level\" source=\"I\" id=\"ID_A\" />\n<element name=\"b\" type=\"Feat\" source=\"I\" id=\"ID_B\" />\n</elements>";
        let f = ContentFile {
            elements: vec![
                Element::new("ID_A", "a", "Level", "Internal"),
                Element::new("ID_B", "b", "Feat", "Hb"),
            ],
            ..ContentFile::default()
        };
        let culprits = culprits(&parse_document(text).unwrap());
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
        let culprit: Vec<_> = issues
            .iter()
            .filter(|i| i.code == IssueCode::MissingRequiredSetter)
            .collect();
        assert_eq!(culprit.len(), 1);
        assert_eq!(culprit[0].line, Some(2));
        assert!(culprit[0].message.contains("ElementParser.cs:277-279"));
        assert!(issues.iter().any(|i| i.code == IssueCode::FileSkipped));
        let stats = discarded_stats(&Stats::default(), &f);
        assert_eq!((stats.elements, stats.dropped_elements), (0, 2));
    }
}
