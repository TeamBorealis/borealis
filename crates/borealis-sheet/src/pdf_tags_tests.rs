//! Parses a hand-built tagged document back with lopdf and checks the structure tree.

use super::*;
use crate::fonts::Style;
use crate::pdf::{write_document, FormMode, PageOp, PdfDocument, PdfPage};
use crate::pdf_form::{FieldKind, FormField, TextField};
use lopdf::{Dictionary, Document, Object};
use std::collections::BTreeMap;

fn text(s: &str, y: f32) -> PageOp {
    PageOp::Text {
        x: 20.0,
        y,
        size: 10.0,
        style: Style::Sans,
        color: (0.0, 0.0, 0.0),
        text: s.to_string(),
    }
}

fn tagged(b: &mut StructBuilder, page: usize, tag: &str, ops: &mut Vec<PageOp>, op: PageOp) {
    let mcid = b.leaf(tag, page);
    ops.push(PageOp::BeginTag {
        tag: tag.to_string(),
        mcid: Some(mcid),
    });
    ops.push(op);
    ops.push(PageOp::EndMarked);
}

fn field(name: &str) -> PageOp {
    PageOp::Field(
        FormField::new(
            name,
            20.0,
            100.0,
            80.0,
            16.0,
            FieldKind::Text(TextField::new("", 10.0)),
        )
        .with_tooltip(name),
    )
}

fn build() -> Vec<u8> {
    let mut b = StructBuilder::new();
    b.begin("Document");
    // Page 0: heading, a table, an artifact, one widget.
    let mut p0 = Vec::new();
    b.begin("Sect");
    tagged(&mut b, 0, "H1", &mut p0, text("Name", 10.0));
    b.begin("Table");
    b.begin("TR");
    for (tag, t) in [("TH", "Skill"), ("TH", "Bonus")] {
        let mcid = b.leaf_with(
            tag,
            0,
            LeafOpts {
                scope: Some(Scope::Column),
                ..LeafOpts::default()
            },
        );
        p0.push(PageOp::BeginTag {
            tag: tag.into(),
            mcid: Some(mcid),
        });
        p0.push(text(t, 30.0));
        p0.push(PageOp::EndMarked);
    }
    b.end();
    b.begin("TR");
    tagged(&mut b, 0, "TD", &mut p0, text("Stealth", 45.0));
    tagged(&mut b, 0, "TD", &mut p0, text("+5", 45.0));
    b.end();
    b.end();
    p0.push(PageOp::BeginArtifact);
    p0.push(PageOp::Line {
        x1: 0.0,
        y1: 60.0,
        x2: 100.0,
        y2: 60.0,
        color: (0.0, 0.0, 0.0),
        width: 1.0,
    });
    p0.push(PageOp::EndMarked);
    let sp = b.widget(0, 0, Some("Armor class"));
    let PageOp::Field(f) = field("ac") else {
        unreachable!()
    };
    p0.push(PageOp::Field(FormField {
        struct_parent: Some(sp),
        ..f
    }));
    b.end();
    // Page 1: paragraph with ActualText, second widget.
    let mut p1 = Vec::new();
    b.begin("Sect");
    let mcid = b.leaf_with(
        "P",
        1,
        LeafOpts {
            actual_text: Some("Hit points".into()),
            ..LeafOpts::default()
        },
    );
    p1.push(PageOp::BeginTag {
        tag: "P".into(),
        mcid: Some(mcid),
    });
    p1.push(text("HP", 10.0));
    p1.push(PageOp::EndMarked);
    let sp = b.widget(1, 0, None);
    let PageOp::Field(f) = field("hp") else {
        unreachable!()
    };
    p1.push(PageOp::Field(FormField {
        struct_parent: Some(sp),
        ..f
    }));
    b.end();
    b.end();
    let pages = vec![
        PdfPage {
            ops: p0,
            struct_parents: Some(b.page_key(0)),
        },
        PdfPage {
            ops: p1,
            struct_parents: Some(b.page_key(1)),
        },
    ];
    write_document(&PdfDocument {
        page_w: 300.0,
        page_h: 200.0,
        pages,
        images: Vec::new(),
        title: "tags".into(),
        lang: Some("en-US".into()),
        mode: FormMode::Fillable,
        structure: Some(&b),
    })
}

fn dict<'a>(doc: &'a Document, o: &'a Object) -> &'a Dictionary {
    match doc.dereference(o).expect("resolves").1 {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        other => panic!("not a dict: {other:?}"),
    }
}

fn mcids(content: &str) -> Vec<u32> {
    content
        .split("/MCID ")
        .skip(1)
        .map(|r| {
            r.split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

/// Walk the tree: collect (page object, mcid) kids and OBJR targets; check /P links.
fn walk(
    doc: &Document,
    id: (u32, u16),
    parent: (u32, u16),
    mcs: &mut Vec<((u32, u16), u32)>,
    objrs: &mut Vec<(u32, u16)>,
) {
    let d = doc.get_dictionary(id).unwrap();
    assert_eq!(d.get(b"P").unwrap().as_reference().unwrap(), parent);
    let pg = d.get(b"Pg").ok().and_then(|o| o.as_reference().ok());
    match d.get(b"K").unwrap() {
        Object::Integer(m) => mcs.push((pg.expect("leaf has Pg"), *m as u32)),
        Object::Dictionary(o) => {
            assert_eq!(o.get(b"Type").unwrap().as_name().unwrap(), b"OBJR");
            objrs.push(o.get(b"Obj").unwrap().as_reference().unwrap());
        }
        Object::Array(kids) => {
            for k in kids {
                walk(doc, k.as_reference().unwrap(), id, mcs, objrs);
            }
        }
        other => panic!("unexpected /K {other:?}"),
    }
}

#[test]
fn structure_tree_is_consistent() {
    let doc = Document::load_mem(&build()).expect("parses");
    let root = dict(&doc, doc.trailer.get(b"Root").unwrap());
    assert_eq!(root.get(b"Lang").unwrap().as_str().unwrap(), b"en-US");
    assert!(dict(&doc, root.get(b"MarkInfo").unwrap())
        .get(b"Marked")
        .unwrap()
        .as_bool()
        .unwrap());
    let str_root_id = root.get(b"StructTreeRoot").unwrap().as_reference().unwrap();
    let str_root = doc.get_dictionary(str_root_id).unwrap();

    let mut mcs = Vec::new();
    let mut objrs = Vec::new();
    for k in str_root.get(b"K").unwrap().as_array().unwrap() {
        walk(
            &doc,
            k.as_reference().unwrap(),
            str_root_id,
            &mut mcs,
            &mut objrs,
        );
    }
    assert_eq!(objrs.len(), 2);

    // ParentTree.
    let pt = dict(&doc, str_root.get(b"ParentTree").unwrap());
    let nums = pt.get(b"Nums").unwrap().as_array().unwrap();
    let mut tree: BTreeMap<i64, &Object> = BTreeMap::new();
    for pair in nums.chunks(2) {
        tree.insert(pair[0].as_i64().unwrap(), &pair[1]);
    }
    let next = str_root
        .get(b"ParentTreeNextKey")
        .unwrap()
        .as_i64()
        .unwrap();
    assert!(tree.keys().all(|&k| k < next));

    // Content streams: every MCID used exactly once in the tree, resolving via the parent tree.
    let mut seen = Vec::new();
    for (&pid, _) in doc.get_pages().iter().map(|(n, id)| (id, n)) {
        let page = doc.get_dictionary(pid).unwrap();
        let sp = page
            .get(b"StructParents")
            .expect("page has StructParents")
            .as_i64()
            .unwrap();
        let content = String::from_utf8_lossy(&doc.get_page_content(pid)).into_owned();
        let used = mcids(&content);
        assert!(
            !content.contains("BDC")
                || content.matches("BDC").count() == content.matches("EMC").count()
        );
        let arr = tree[&sp].as_array().unwrap();
        assert_eq!(arr.len(), used.len());
        for m in used {
            let elem = arr[m as usize].as_reference().unwrap();
            let d = doc.get_dictionary(elem).unwrap();
            assert_eq!(d.get(b"K").unwrap().as_i64().unwrap(), i64::from(m));
            assert_eq!(d.get(b"Pg").unwrap().as_reference().unwrap(), pid);
            seen.push((pid, m));
        }
        // Widgets: /StructParent resolves to a Form element holding the OBJR.
        for a in page.get(b"Annots").unwrap().as_array().unwrap() {
            let w = dict(&doc, a);
            let key = w.get(b"StructParent").unwrap().as_i64().unwrap();
            let elem = doc
                .get_dictionary(tree[&key].as_reference().unwrap())
                .unwrap();
            assert_eq!(elem.get(b"S").unwrap().as_name().unwrap(), b"Form");
            let o = dict(&doc, elem.get(b"K").unwrap());
            assert_eq!(o.get(b"Obj").unwrap(), a);
            assert!(w.get(b"TU").is_ok());
        }
    }
    let mut from_tree = mcs.clone();
    from_tree.sort_unstable();
    seen.sort_unstable();
    assert_eq!(from_tree, seen, "each MCID referenced exactly once");
    assert_eq!(seen.len(), 6);
}

#[test]
fn custom_tags_get_role_map_and_scope_is_written() {
    let mut b = StructBuilder::new();
    b.map_role("Stat", "Div");
    b.leaf("Odd", 0);
    let bytes = {
        let pages = vec![PdfPage {
            ops: Vec::new(),
            struct_parents: Some(b.page_key(0)),
        }];
        write_document(&PdfDocument {
            page_w: 100.0,
            page_h: 100.0,
            pages,
            images: Vec::new(),
            title: "t".into(),
            lang: None,
            mode: FormMode::Flatten,
            structure: Some(&b),
        })
    };
    let doc = Document::load_mem(&bytes).unwrap();
    let root = dict(&doc, doc.trailer.get(b"Root").unwrap());
    let sr = dict(&doc, root.get(b"StructTreeRoot").unwrap());
    let rm = dict(&doc, sr.get(b"RoleMap").unwrap());
    assert_eq!(rm.get(b"Stat").unwrap().as_name().unwrap(), b"Div");
    assert_eq!(rm.get(b"Odd").unwrap().as_name().unwrap(), b"Span");
}

#[test]
fn list_items_wrap_loose_content_in_lbody_and_short_rows_span_the_table() {
    // PDF/UA-1 7.2-20 (an LI holds Lbl/LBody only) and 7.2-42 (rows of equal width).
    let mut b = StructBuilder::new();
    b.begin("L");
    b.begin("LI");
    b.leaf("Lbl", 0);
    b.leaf("P", 0);
    b.widget(0, 0, None);
    b.end();
    b.end();
    b.begin("Table");
    b.begin("TR");
    for _ in 0..3 {
        b.leaf("TD", 0);
    }
    b.end();
    b.begin("TR");
    b.leaf("TH", 0);
    b.end();
    b.end();
    let (nodes, spans) = b.normalized();
    let tags =
        |ids: &[usize]| -> Vec<String> { ids.iter().map(|&k| nodes[k].tag.clone()).collect() };
    let li = nodes.iter().position(|n| n.tag == "LI").unwrap();
    assert_eq!(tags(&nodes[li].kids), ["Lbl", "LBody"]);
    assert_eq!(tags(&nodes[nodes[li].kids[1]].kids), ["P", "Form"]);
    let th = nodes.iter().position(|n| n.tag == "TH").unwrap();
    assert_eq!(spans.get(&th), Some(&3));
    assert_eq!(spans.len(), 1, "full rows keep one column per cell");
}
