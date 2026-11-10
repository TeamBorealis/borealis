use super::*;
use crate::ids::IdGenerator;
use borealis_content::{ContentFile, Element, GrantRule, Rule, Ruleset};
use std::collections::HashSet;

fn settings(code: &str) -> PackSettings {
    PackSettings {
        name: "  Gloom   Hollow ".into(),
        code: code.into(),
        edition: Ruleset::Any,
        author: Some(" Mira ".into()),
        description: Some("Dark <things> & more".into()),
        license: Some("".into()),
    }
}

fn generator(code: &str) -> IdGenerator {
    IdGenerator::new(code, Vec::<String>::new(), Vec::<String>::new()).unwrap()
}

#[test]
fn settings_are_normalized_and_checked() {
    let s = settings("GH").checked(&HashSet::new()).unwrap();
    assert_eq!(s.name, "Gloom Hollow");
    assert_eq!(s.author.as_deref(), Some("Mira"));
    assert_eq!(s.license, None, "an empty licence is no licence");
    assert_eq!(s.pack_id(), "homebrew-gh");
    let bad = |code: &str, taken: &[&str]| {
        settings(code)
            .checked(&taken.iter().map(|t| (*t).to_string()).collect())
            .unwrap_err()
            .0
    };
    assert_eq!(
        bad("g", &[]),
        "The pack code is 2 to 8 capital letters or digits."
    );
    assert_eq!(bad("GH", &["GH"]), "Another book already uses the code GH.");
    let mut unnamed = settings("GH");
    unnamed.name = " \u{7} ".into();
    assert_eq!(
        unnamed.checked(&HashSet::new()).unwrap_err().0,
        "Give the pack a name."
    );
}

#[test]
fn a_new_pack_has_its_book_first_and_both_editions() {
    let s = settings("GH").checked(&HashSet::new()).unwrap();
    let m = new_manifest(&s);
    assert_eq!(m.id, "homebrew-gh");
    assert_eq!(m.version, FIRST_VERSION);
    assert_eq!(m.rulesets, vec![Ruleset::Dnd2014, Ruleset::Dnd2024]);
    assert_eq!(m.files[0].path, CONTENT_FILE);
    let src = source_element(&s, &mut generator("GH"), "20261110");
    assert_eq!(src.id, "ID_GH_SOURCE_GLOOM_HOLLOW");
    assert_eq!(src.ty, SOURCE_TYPE);
    assert_eq!(src.source, "Gloom Hollow");
    assert_eq!(src.setter_value("abbreviation"), Some("GH"));
    assert_eq!(src.setter_value("release"), Some("20261110"));
    assert_eq!(
        src.setter_value("url"),
        Some(""),
        "the validator requires it"
    );
    assert_eq!(src.description, "<p>Dark &lt;things&gt; &amp; more</p>");
    let c = new_content(&s, src);
    assert_eq!(c.ruleset, Some(Ruleset::Any));
    assert_eq!(pack_edition(&m, &c), Ruleset::Any);
    assert_eq!(source_name(&m, &c), "Gloom Hollow");
}

#[test]
fn renaming_a_pack_renames_its_book_and_the_sources_that_name_it() {
    let s = settings("GH").checked(&HashSet::new()).unwrap();
    let mut src = source_element(&s, &mut generator("GH"), "20261110");
    src.setters
        .push(borealis_content::Setter::new("information", "x"));
    let mut c = new_content(&s, src);
    c.elements.push(Element::new(
        "ID_GH_LANGUAGE_A",
        "A",
        "Language",
        "Gloom Hollow",
    ));
    c.elements.push(Element::new(
        "ID_GH_LANGUAGE_B",
        "B",
        "Language",
        "Elsewhere",
    ));
    let mut renamed = s.clone();
    renamed.name = "Bright Hollow".into();
    renamed.edition = Ruleset::Dnd2014;
    renamed.author = None;
    let next = content_with(&c, &renamed);
    let book = source_of(&next).unwrap();
    assert_eq!(book.name, "Bright Hollow");
    assert_eq!(book.id, "ID_GH_SOURCE_GLOOM_HOLLOW", "ids never change");
    assert_eq!(book.setter_value("author"), None);
    assert_eq!(book.setter_value("release"), Some("20261110"));
    assert_eq!(
        book.setter_value("information"),
        Some("x"),
        "other setters kept"
    );
    assert_eq!(next.elements[1].source, "Bright Hollow");
    assert_eq!(next.elements[2].source, "Elsewhere");
    assert_eq!(next.ruleset, Some(Ruleset::Dnd2014));
    let m = manifest_with(&new_manifest(&s), &renamed);
    assert_eq!(
        (m.name.as_str(), m.rulesets.clone()),
        ("Bright Hollow", vec![Ruleset::Dnd2014])
    );
}

#[test]
fn versions_bump_their_patch_number() {
    assert_eq!(bump_patch("1.0.0"), "1.0.1");
    assert_eq!(bump_patch("2.3.9"), "2.3.10");
    assert_eq!(bump_patch("1.0"), "1.0.1");
    assert_eq!(bump_patch(""), "1.0.0");
}

#[test]
fn saving_replaces_in_place_and_adds_new_elements_last() {
    let s = settings("GH").checked(&HashSet::new()).unwrap();
    let c = new_content(&s, source_element(&s, &mut generator("GH"), ""));
    let a = Element::new("ID_A", "A", "Language", "Gloom Hollow");
    let b = Element::new("ID_B", "B", "Language", "Gloom Hollow");
    let c = with_elements(&c, &[a.clone(), b]);
    let mut a2 = a;
    a2.name = "A2".into();
    let c = with_elements(
        &c,
        &[a2, Element::new("ID_C", "C", "Language", "Gloom Hollow")],
    );
    let names: Vec<&str> = c.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Gloom Hollow", "A2", "B", "C"]);
    let gone: HashSet<&str> = ["ID_B"].into_iter().collect();
    let (left, removed) = without_elements(&c, &gone);
    assert_eq!(left.elements.len(), 3);
    assert_eq!(removed[0].id, "ID_B");
}

#[test]
fn creator_json_round_trips_and_refuses_other_files() {
    let mut c = CreatorFile::new("GH", "1.5.0");
    c = c.with_saved(
        "ID_A",
        CreatorElement {
            kind: "language".into(),
            created_at: "2026-11-10T10:00:00Z".into(),
            copied_from: Some(CopiedFrom {
                id: "ID_X".into(),
                pack_id: "imp".into(),
                srd: false,
            }),
            non_srd_text: true,
        },
    );
    let text = c.to_json_pretty();
    assert!(text.contains("\"retiredIds\"") && text.contains("\"nonSrdText\": true"));
    assert_eq!(CreatorFile::parse(&text).unwrap(), c);
    assert!(c.has_non_srd_text());
    assert!(matches!(
        CreatorFile::parse("{ not json"),
        Err(CreatorFileError::Unreadable(_))
    ));
    assert!(matches!(
        CreatorFile::parse(r#"{"format":"other/1","code":"GH"}"#),
        Err(CreatorFileError::UnknownFormat(_))
    ));
    // a later save keeps the first save's time and provenance
    let again = c.with_saved(
        "ID_A",
        CreatorElement {
            kind: "language".into(),
            created_at: "2026-11-11T10:00:00Z".into(),
            copied_from: None,
            non_srd_text: false,
        },
    );
    assert_eq!(again.elements["ID_A"], c.elements["ID_A"]);
}

#[test]
fn creator_json_follows_a_pack_edited_outside_the_app() {
    let mut content = ContentFile::new(vec![
        Element::new("ID_GH_SOURCE_X", "X", "Source", "X"),
        Element::new("ID_A", "A", "Language", "X"),
        Element::new("ID_C", "C", "Spell", "X"),
    ]);
    let adopted = CreatorFile::adopt("GH", "1.5.0", &content);
    assert_eq!(
        adopted.elements.keys().collect::<Vec<_>>(),
        ["ID_A", "ID_C"],
        "the book element is managed, not listed"
    );
    assert_eq!(adopted.elements["ID_A"].kind, "language");
    assert_eq!(adopted.elements["ID_C"].kind, "other");
    let retired = adopted.with_retired(["ID_C"]);
    assert_eq!(retired.retired_ids, ["ID_C"]);
    assert!(!retired.elements.contains_key("ID_C"));
    assert_eq!(retired.with_retired(["ID_C"]).retired_ids, ["ID_C"]);
    // C was deleted; then someone removed A and added B by hand
    content
        .elements
        .retain(|e| e.id != "ID_A" && e.id != "ID_C");
    content
        .elements
        .push(Element::new("ID_B", "B", "Language", "X"));
    let next = retired.reconciled(&content);
    assert_eq!(next.elements.keys().collect::<Vec<_>>(), ["ID_B"]);
    assert_eq!(next.retired_ids, ["ID_C"], "retired ids stay retired");
}

#[test]
fn children_are_granted_elements_of_the_child_types() {
    let mut class = Element::new("ID_CL", "Cl", "Class", "X");
    let grant = |id: &str| {
        Rule::Grant(GrantRule {
            id: id.into(),
            ty: Some("Class Feature".into()),
            ..GrantRule::default()
        })
    };
    class.rules = vec![
        grant("ID_F1"),
        grant("ID_LANG"),
        grant("ID_F1"),
        grant("ID_F2"),
    ];
    let pack = vec![
        class.clone(),
        Element::new("ID_F2", "F2", "Class Feature", "X"),
        Element::new("ID_F1", "F1", "Class Feature", "X"),
        Element::new("ID_LANG", "L", "Language", "X"),
    ];
    let kids: Vec<&str> = granted_children(&class, &pack)
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(kids, ["ID_F1", "ID_F2"]);
    assert_eq!(
        parent_of(&pack[1], &pack).map(|p| p.id.as_str()),
        Some("ID_CL")
    );
    assert_eq!(parent_of(&pack[3], &pack), None);
}

#[test]
fn digests_are_stable_and_see_any_change() {
    let a = Element::new("ID_A", "A", "Language", "X");
    let mut b = a.clone();
    assert_eq!(digest(&[&a]), digest(&[&b]));
    b.name = "B".into();
    assert_ne!(digest(&[&a]), digest(&[&b]));
    assert_ne!(digest(&[&a]), digest(&[]));
}
