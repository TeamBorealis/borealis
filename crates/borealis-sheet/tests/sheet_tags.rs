//! Tagged PDF of the whole sheet: every shown glyph is marked content, every MCID and
//! widget resolves through the ParentTree, the structure root is `Document > Sect`.

mod common;

use borealis_sheet::{render_pdf_with, CardKinds, PageSelection, SheetOptions, SheetTheme};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeSet;

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).expect("reference resolves"),
        other => other,
    }
}

fn dict<'a>(doc: &'a Document, o: &'a Object) -> &'a Dictionary {
    resolve(doc, o).as_dict().expect("dictionary")
}

fn name_of(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
}

fn options(edition_2024: bool, fillable: bool, legacy: bool, ink: bool) -> SheetOptions {
    SheetOptions {
        edition: if edition_2024 {
            borealis_sheet::SheetEdition::Y2024
        } else {
            borealis_sheet::SheetEdition::Y2014
        },
        theme: if ink {
            SheetTheme::Ink
        } else {
            SheetTheme::Colour
        },
        form_fillable: fillable,
        legacy_spellcasting: legacy,
        cards: CardKinds {
            spell: true,
            item: true,
            attack: true,
            feature: true,
        },
        pages: PageSelection::default(),
        ..SheetOptions::default()
    }
}

fn render(edition_2024: bool, o: &SheetOptions) -> Vec<u8> {
    let (c, s) = if edition_2024 {
        common::wizard_2024()
    } else {
        common::wizard_2014()
    };
    render_pdf_with(&c, &s, o, &borealis_sheet::SheetContent::default()).unwrap()
}

/// MCIDs shown on a page; panics when a text-showing operator is outside marked content.
fn page_mcids(doc: &Document, page: ObjectId, label: &str) -> BTreeSet<i64> {
    let bytes = doc.get_page_content(page);
    let content = Content::decode(&bytes).unwrap();
    let mut stack: Vec<Option<i64>> = Vec::new(); // Some(mcid) | None = artifact
    let mut seen = BTreeSet::new();
    for op in &content.operations {
        match op.operator.as_str() {
            "BDC" => {
                let tag = op.operands[0]
                    .as_name()
                    .map(|n| n.to_vec())
                    .unwrap_or_default();
                let mcid = match op.operands.get(1) {
                    Some(Object::Dictionary(d)) => {
                        d.get(b"MCID").ok().and_then(|o| o.as_i64().ok())
                    }
                    _ => None,
                };
                if tag == b"Artifact" {
                    stack.push(None);
                } else if let Some(m) = mcid {
                    assert!(seen.insert(m), "{label}: MCID {m} used twice");
                    stack.push(Some(m));
                } else {
                    stack.push(None); // /Span ActualText wrapper: must itself sit in marked content
                    assert!(
                        stack.len() > 1,
                        "{label}: untagged BDC {tag:?} at top level"
                    );
                }
            }
            "BMC" => panic!("{label}: BMC (untagged marked content)"),
            "EMC" => {
                stack.pop().expect("balanced EMC");
            }
            "Tj" | "TJ" | "'" | "\"" => {
                assert!(!stack.is_empty(), "{label}: text outside marked content");
            }
            "Do" => {
                // images (Figure) and art forms (Artifact) must both be marked
                assert!(!stack.is_empty(), "{label}: Do outside marked content");
            }
            _ => {}
        }
    }
    assert!(stack.is_empty(), "{label}: unbalanced BDC/EMC");
    seen
}

fn number_tree(doc: &Document, node: &Dictionary, out: &mut Vec<(i64, Object)>) {
    if let Ok(nums) = node.get(b"Nums").and_then(Object::as_array) {
        for pair in nums.chunks(2) {
            out.push((pair[0].as_i64().unwrap(), pair[1].clone()));
        }
    }
    if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
        for k in kids {
            number_tree(doc, dict(doc, k), out);
        }
    }
}

fn check(edition_2024: bool, fillable: bool, legacy: bool, ink: bool) {
    let label = format!("2024={edition_2024} fillable={fillable} legacy={legacy} ink={ink}");
    let pdf = render(edition_2024, &options(edition_2024, fillable, legacy, ink));
    let doc = Document::load_mem(&pdf).unwrap();
    let catalog = doc.catalog().unwrap();
    assert!(catalog.get(b"Lang").is_ok(), "{label}: /Lang");
    let mark = dict(&doc, catalog.get(b"MarkInfo").expect("MarkInfo"));
    assert_eq!(mark.get(b"Marked").unwrap(), &Object::Boolean(true));
    let root = dict(
        &doc,
        catalog.get(b"StructTreeRoot").expect("StructTreeRoot"),
    );

    // Root: a single Document whose kids are all Sect.
    let top = resolve(&doc, root.get(b"K").unwrap());
    let top_dict = match top {
        Object::Array(a) => {
            assert_eq!(a.len(), 1, "{label}: one top element");
            dict(&doc, &a[0])
        }
        other => other.as_dict().unwrap(),
    };
    assert_eq!(name_of(top_dict, b"S").as_deref(), Some("Document"));
    let sects = resolve(&doc, top_dict.get(b"K").unwrap())
        .as_array()
        .unwrap()
        .clone();
    assert!(!sects.is_empty());
    for s in &sects {
        assert_eq!(
            name_of(dict(&doc, s), b"S").as_deref(),
            Some("Sect"),
            "{label}"
        );
    }

    // Parent tree.
    let mut nums = Vec::new();
    number_tree(
        &doc,
        dict(&doc, root.get(b"ParentTree").unwrap()),
        &mut nums,
    );
    let tree: std::collections::BTreeMap<i64, Object> = nums.into_iter().collect();

    let pages = doc.get_pages();
    assert_eq!(sects.len(), pages.len(), "{label}: one Sect per page");
    let mut forms = 0usize;
    for (no, &page_id) in &pages {
        let pdict = doc.get_dictionary(page_id).unwrap();
        let key = pdict
            .get(b"StructParents")
            .expect("StructParents")
            .as_i64()
            .unwrap();
        let mcids = page_mcids(&doc, page_id, &format!("{label} page {no}"));
        let arr = resolve(&doc, tree.get(&key).expect("page key in ParentTree"))
            .as_array()
            .unwrap();
        for m in &mcids {
            let slot = arr
                .get(*m as usize)
                .unwrap_or_else(|| panic!("{label}: mcid {m} unresolved"));
            let el = dict(&doc, slot);
            assert!(
                el.get(b"S").is_ok(),
                "{label}: ParentTree entry is a StructElem"
            );
        }
        assert_eq!(
            arr.len(),
            mcids.len(),
            "{label} page {no}: ParentTree covers exactly the MCIDs"
        );
        // Widgets.
        if let Ok(annots) = pdict.get(b"Annots").and_then(Object::as_array) {
            assert_eq!(
                name_of(pdict, b"Tabs").as_deref(),
                Some("S"),
                "{label}: /Tabs /S"
            );
            for a in annots {
                let w = dict(&doc, a);
                if name_of(w, b"Subtype").as_deref() != Some("Widget") {
                    continue;
                }
                forms += 1;
                let sp = w
                    .get(b"StructParent")
                    .expect("widget StructParent")
                    .as_i64()
                    .unwrap();
                let el = dict(&doc, tree.get(&sp).expect("widget key in ParentTree"));
                assert_eq!(name_of(el, b"S").as_deref(), Some("Form"), "{label}");
                assert!(w.get(b"TU").is_ok(), "{label}: widget without /TU");
            }
        }
    }
    // The character name heading is a group (H1) on page 1; its text is in the widget or a P.
    assert!(h1_on_first_page(&doc, &sects), "{label}: H1 on page 1");
    assert_eq!(forms > 0, fillable, "{label}: widgets iff fillable");
}

/// Does the first Sect contain an element with /S /H1 (at any depth)?
fn h1_on_first_page(doc: &Document, sects: &[Object]) -> bool {
    fn walk(doc: &Document, o: &Object, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        match resolve(doc, o) {
            Object::Dictionary(d) => {
                name_of(d, b"S").as_deref() == Some("H1")
                    || d.get(b"K").is_ok_and(|k| walk(doc, k, depth + 1))
            }
            Object::Array(a) => a.iter().any(|k| walk(doc, k, depth + 1)),
            _ => false,
        }
    }
    walk(doc, &sects[0], 0)
}

#[test]
fn tagged_2014_fillable() {
    check(false, true, false, false);
    check(false, true, true, true);
}

#[test]
fn tagged_2014_flatten() {
    check(false, false, false, true);
    check(false, false, true, false);
}

#[test]
fn tagged_2024_fillable() {
    check(true, true, false, true);
    check(true, true, true, false);
}

#[test]
fn tagged_2024_flatten() {
    check(true, false, false, false);
    check(true, false, true, true);
}

#[test]
fn headings_labels_and_tables_exist() {
    let pdf = render(true, &options(true, true, false, false));
    let doc = Document::load_mem(&pdf).unwrap();
    let mut roles = BTreeSet::new();
    for obj in doc.objects.values() {
        if let Object::Dictionary(d) = obj {
            if let Some(s) = name_of(d, b"S") {
                roles.insert(s);
            }
        }
    }
    for want in [
        "Document", "Sect", "H1", "H2", "P", "Lbl", "Form", "Table", "TR", "TH", "TD", "L", "LI",
    ] {
        assert!(roles.contains(want), "role {want} missing: {roles:?}");
    }
}

/// Table rows only hold cells; table and list parents hold rows and items.
#[test]
fn table_and_list_children_are_well_formed() {
    let pdf = render(true, &options(true, true, false, false));
    let doc = Document::load_mem(&pdf).unwrap();
    let role = |o: &Object| name_of(dict(&doc, o), b"S").unwrap_or_default();
    let mut checked = 0;
    for obj in doc.objects.values() {
        let Object::Dictionary(d) = obj else { continue };
        let Some(s) = name_of(d, b"S") else { continue };
        let allowed: &[&str] = match s.as_str() {
            "Table" => &["TR"],
            "TR" => &["TH", "TD"],
            "L" => &["LI"],
            _ => continue,
        };
        let Ok(k) = d.get(b"K") else { continue };
        let kids: Vec<&Object> = match resolve(&doc, k) {
            Object::Array(a) => a.iter().collect(),
            other => vec![other],
        };
        for kid in kids {
            if matches!(kid, Object::Reference(_)) {
                assert!(
                    allowed.contains(&role(kid).as_str()),
                    "{s} has a {} child",
                    role(kid)
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 20, "tables and lists were checked ({checked})");
}

/// Equipment is a table with an ITEM header cell, and skills are list items, in both editions.
#[test]
fn equipment_table_and_skills_list_are_tagged() {
    for is24 in [false, true] {
        let pdf = render(is24, &options(is24, true, false, false));
        let doc = Document::load_mem(&pdf).unwrap();
        let (mut item_th, mut li) = (0, 0);
        for obj in doc.objects.values() {
            let Object::Dictionary(d) = obj else { continue };
            match name_of(d, b"S").as_deref() {
                Some("TH") if matches!(d.get(b"ActualText"), Ok(Object::String(t, _)) if t.ends_with(b"ITEM") || t.windows(4).any(|w| w == b"I\0T\0".as_slice() || w == b"ITEM".as_slice())) =>
                {
                    item_th += 1;
                }
                Some("LI") => li += 1,
                _ => {}
            }
        }
        assert!(item_th >= 1, "equipment header (2024: {is24})");
        assert!(
            li >= 18 + 6 * usize::from(is24),
            "skill items {li} (2024: {is24})"
        );
    }
}
