//! Writer tests: field appearances, flatten vs fillable, the field tree, non-Latin text,
//! ExtGState alpha, clipping and the tagging hooks. Every PDF is parsed back with lopdf.

use super::*;
use crate::pdf::{write_document, DocRefs, ObjectSink, PdfDocument, StructureWriter};
use crate::pdf_draw::{Paint, Path};
use lopdf::{Dictionary, Document, Object};
use std::collections::HashMap;

const W: f32 = 300.0;
const H: f32 = 200.0;

fn pdf(mode: FormMode, ops: Vec<PageOp>) -> Vec<u8> {
    pdf_with(mode, ops, None, None)
}

fn pdf_with(
    mode: FormMode,
    ops: Vec<PageOp>,
    lang: Option<&str>,
    structure: Option<&dyn StructureWriter>,
) -> Vec<u8> {
    write_document(&PdfDocument {
        page_w: W,
        page_h: H,
        pages: vec![PdfPage {
            ops,
            struct_parents: structure.map(|_| 0),
        }],
        images: Vec::new(),
        title: "test".into(),
        lang: lang.map(str::to_string),
        mode,
        structure,
    })
}

fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("lopdf parses the output")
}

fn text_field(name: &str, value: &str, style: Style) -> PageOp {
    let t = TextField {
        style,
        ..TextField::new(value, 12.0)
    };
    PageOp::Field(FormField::new(
        name,
        20.0,
        20.0,
        160.0,
        18.0,
        FieldKind::Text(t),
    ))
}

fn deref<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    doc.dereference(obj).expect("reference resolves").1
}

fn dict<'a>(doc: &'a Document, obj: &'a Object) -> &'a Dictionary {
    match deref(doc, obj) {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        other => panic!("not a dictionary: {other:?}"),
    }
}

fn stream_text(doc: &Document, obj: &Object) -> String {
    match deref(doc, obj) {
        Object::Stream(s) => {
            let data = s
                .decompressed_content()
                .unwrap_or_else(|_| s.content.clone());
            String::from_utf8_lossy(&data).into_owned()
        }
        other => panic!("not a stream: {other:?}"),
    }
}

fn page(doc: &Document) -> &Dictionary {
    let id = *doc.get_pages().values().next().unwrap();
    doc.get_dictionary(id).unwrap()
}

fn page_content(doc: &Document) -> String {
    let id = *doc.get_pages().values().next().unwrap();
    String::from_utf8_lossy(&doc.get_page_content(id)).into_owned()
}

fn widgets(doc: &Document) -> Vec<&Dictionary> {
    match page(doc).get(b"Annots") {
        Ok(arr) => deref(doc, arr)
            .as_array()
            .unwrap()
            .iter()
            .map(|o| dict(doc, o))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// `ToUnicode` maps of the fonts in a resource dictionary, by resource name.
fn cmaps(doc: &Document, resources: &Dictionary) -> HashMap<String, HashMap<u16, String>> {
    let fonts = dict(doc, resources.get(b"Font").unwrap());
    fonts
        .iter()
        .map(|(name, font)| {
            let font = dict(doc, font);
            assert_eq!(font.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
            let cmap = stream_text(doc, font.get(b"ToUnicode").unwrap());
            let map = cmap
                .lines()
                .skip_while(|l| !l.ends_with("beginbfchar"))
                .filter_map(|l| {
                    let mut it = l.split(['<', '>']).filter(|s| !s.trim().is_empty());
                    let cid = u16::from_str_radix(it.next()?, 16).ok()?;
                    let hex = it.next()?;
                    let units: Vec<u16> = (0..hex.len() / 4)
                        .filter_map(|i| u16::from_str_radix(&hex[i * 4..i * 4 + 4], 16).ok())
                        .collect();
                    Some((cid, String::from_utf16(&units).ok()?))
                })
                .collect();
            (String::from_utf8_lossy(name).into_owned(), map)
        })
        .collect()
}

/// Decode every `/Fn size Tf <hex> Tj` in a content stream through the fonts' `ToUnicode`.
fn shown_text(content: &str, maps: &HashMap<String, HashMap<u16, String>>) -> String {
    let mut out = String::new();
    let mut font = String::new();
    let tokens: Vec<&str> = content.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if *t == "Tf" && i >= 2 {
            font = tokens[i - 2].trim_start_matches('/').to_string();
        }
        if *t == "Tj" && i >= 1 {
            let hex = tokens[i - 1].trim_matches(['<', '>']);
            let map = &maps[&font];
            for k in 0..hex.len() / 4 {
                let cid = u16::from_str_radix(&hex[k * 4..k * 4 + 4], 16).unwrap();
                out.push_str(map.get(&cid).map_or("\u{FFFD}", String::as_str));
            }
        }
    }
    out
}

fn appearance<'a>(doc: &'a Document, widget: &'a Dictionary) -> (&'a Object, &'a Dictionary) {
    let n = dict(doc, widget.get(b"AP").unwrap()).get(b"N").unwrap();
    (n, dict(doc, n))
}

/// The text a page draws, decoded through the page fonts' `ToUnicode` maps.
fn page_text(doc: &Document) -> String {
    let res = dict(doc, page(doc).get(b"Resources").unwrap());
    shown_text(&page_content(doc), &cmaps(doc, res))
}

#[test]
fn prefilled_values_are_page_text_and_empty_fields_are_widgets() {
    let value = "Brunhild \u{2212}1 «ok»";
    let bytes = pdf(
        FormMode::Fillable,
        vec![
            text_field("char.name", value, Style::SerifBold),
            text_field("char.title", "", Style::SerifBold),
        ],
    );
    let doc = load(&bytes);
    // The value is vector text in the embedded subset, the same in every viewer (PDFKit
    // ignores a text widget's /AP); only the empty play field is a widget.
    let content = page_content(&doc);
    assert!(content.contains("/F4 12.00 Tf <"), "{content}");
    assert!(!content.contains('?'));
    assert_eq!(page_text(&doc), value);
    let cap = 0.714 * 12.0;
    let expected_y = 18.0 - (18.0 + cap) / 2.0;
    assert!(
        content.contains(&format!(" {expected_y:.1}")),
        "baseline near {expected_y}: {content}"
    );
    let ws = widgets(&doc);
    assert_eq!(ws.len(), 1);
    let w = ws[0];
    assert_eq!(w.get(b"FT").unwrap().as_name().unwrap(), b"Tx");
    assert_eq!(w.get(b"T").unwrap().as_str().unwrap(), b"title");
    let da = String::from_utf8_lossy(w.get(b"DA").unwrap().as_str().unwrap()).into_owned();
    assert!(
        da.starts_with("/TiBo 12.00 Tf"),
        "DA names the standard serif bold face: {da}"
    );
    // No regeneration request; /DR resolves the /DA name to a face every viewer has.
    let raw = String::from_utf8_lossy(&bytes);
    assert!(!raw.contains("NeedAppearances"));
    let acro = dict(&doc, doc.catalog().unwrap().get(b"AcroForm").unwrap());
    let dr_fonts = dict(
        &doc,
        dict(&doc, acro.get(b"DR").unwrap()).get(b"Font").unwrap(),
    );
    let tibo = dict(&doc, dr_fonts.get(b"TiBo").unwrap());
    assert_eq!(
        tibo.get(b"BaseFont").unwrap().as_name().unwrap(),
        b"Times-Bold"
    );
    assert!(dr_fonts.get(b"Helv").is_ok(), "AcroForm /DA default");
    let (ap, _) = appearance(&doc, w);
    assert!(!stream_text(&doc, ap).contains("Tj"), "empty appearance");
}

#[test]
fn fillable_and_flatten_embed_only_the_drawn_glyphs() {
    let count = |mode| {
        let doc = load(&pdf(mode, vec![text_field("a", "1", Style::Sans)]));
        let res = dict(&doc, page(&doc).get(b"Resources").unwrap());
        cmaps(&doc, res)["F1"].len()
    };
    assert_eq!(count(FormMode::Fillable), 1);
    assert_eq!(count(FormMode::Flatten), 1);
}

#[test]
fn one_line_tall_fields_are_written_single_line() {
    let field = |h| {
        let t = TextField {
            multiline: true,
            ..TextField::new("", 7.4)
        };
        PageOp::Field(FormField::new(
            "notes",
            10.0,
            10.0,
            200.0,
            h,
            FieldKind::Text(t),
        ))
    };
    let doc = load(&pdf(FormMode::Fillable, vec![field(10.6), field(20.2)]));
    let ws = widgets(&doc);
    let ff = |w: &Dictionary| w.get(b"Ff").unwrap().as_i64().unwrap() & (1 << 12);
    assert_eq!(
        ff(ws[0]),
        0,
        "one line tall: single-line (PDFKit centres it)"
    );
    assert_eq!(ff(ws[1]), 1 << 12, "two lines tall: multi-line");
}

#[test]
fn flatten_draws_values_into_the_page_and_omits_widgets() {
    let frame = FieldFrame {
        shape: Shape::Square,
        paint: Paint::fill((0.95, 0.94, 0.9)),
        radius: 2.0,
    };
    let ops = |checked| {
        vec![
            PageOp::Field(
                FormField::new(
                    "hp.max",
                    10.0,
                    10.0,
                    60.0,
                    20.0,
                    FieldKind::Text(TextField::new("32", 14.0)),
                )
                .with_frame(frame),
            ),
            PageOp::Field(FormField::new(
                "death.success.1",
                10.0,
                40.0,
                8.0,
                8.0,
                FieldKind::Check(CheckField::new(checked)),
            )),
        ]
    };
    let fill_bytes = pdf(FormMode::Fillable, ops(true));
    let flat_bytes = pdf(FormMode::Flatten, ops(true));
    let fill = load(&fill_bytes);
    let flat = load(&flat_bytes);
    assert_eq!(
        widgets(&fill).len(),
        1,
        "the check box; the value is printed"
    );
    assert!(widgets(&flat).is_empty());
    assert!(fill.catalog().unwrap().get(b"AcroForm").is_ok());
    assert!(flat.catalog().unwrap().get(b"AcroForm").is_err());
    assert!(!String::from_utf8_lossy(&flat_bytes).contains("/Widget"));
    let res = dict(&flat, page(&flat).get(b"Resources").unwrap());
    let flat_content = page_content(&flat);
    assert_eq!(shown_text(&flat_content, &cmaps(&flat, res)), "32");
    let fill_content = page_content(&fill);
    assert_eq!(
        page_text(&fill),
        "32",
        "prefilled values are printed in both modes"
    );
    // Frames and values are page content in both modes; flattening adds the check marks.
    assert!(
        flat_content.starts_with(fill_content.trim_end()),
        "{fill_content}\n--\n{flat_content}"
    );
    // Flattened check mark is drawn; an unchecked box adds nothing.
    assert!(flat_content.contains(" S Q"));
    let unchecked = page_content(&load(&pdf(FormMode::Flatten, ops(false))));
    assert!(!unchecked.contains(" S Q"));
}

#[test]
fn check_boxes_have_on_and_off_appearances() {
    let check = |name: &str, checked, mark, shape| {
        PageOp::Field(FormField::new(
            name,
            10.0,
            10.0,
            10.0,
            10.0,
            FieldKind::Check(CheckField {
                mark,
                shape,
                ..CheckField::new(checked)
            }),
        ))
    };
    let doc = load(&pdf(
        FormMode::Fillable,
        vec![
            check("cond.poisoned", true, Mark::Tick, Shape::Square),
            check("insp", false, Mark::Fill, Shape::Diamond),
            check("exh.1", true, Mark::Cross, Shape::Square),
            check("death.fail.1", true, Mark::Fill, Shape::Circle),
        ],
    ));
    let ws = widgets(&doc);
    assert_eq!(ws.len(), 4);
    for (w, on) in ws.iter().zip([true, false, true, true]) {
        assert_eq!(w.get(b"FT").unwrap().as_name().unwrap(), b"Btn");
        let state: &[u8] = if on { b"Yes" } else { b"Off" };
        assert_eq!(w.get(b"V").unwrap().as_name().unwrap(), state);
        assert_eq!(w.get(b"AS").unwrap().as_name().unwrap(), state);
        let n = dict(&doc, dict(&doc, w.get(b"AP").unwrap()).get(b"N").unwrap());
        let yes = stream_text(&doc, n.get(b"Yes").unwrap());
        let off = stream_text(&doc, n.get(b"Off").unwrap());
        assert!(
            yes.starts_with("q ") && yes.trim_end().ends_with('Q'),
            "{yes}"
        );
        assert!(off.is_empty());
    }
    // Fill marks use the field shape: the diamond is a polygon, the circle uses curves.
    let n = |i: usize| {
        dict(
            &doc,
            dict(&doc, ws[i].get(b"AP").unwrap()).get(b"N").unwrap(),
        )
    };
    assert!(stream_text(&doc, n(1).get(b"Yes").unwrap()).contains(" f Q"));
    assert!(stream_text(&doc, n(3).get(b"Yes").unwrap()).contains(" c "));
}

#[test]
fn dotted_names_build_a_field_hierarchy() {
    let tree = FieldTree::build([
        "abil.str.score",
        "abil.str.mod",
        "abil.dex.score",
        "ac",
        "ac",
        "ac.basis",
        "",
        "x..y",
    ]);
    let names: Vec<String> = (0..tree.leaves.len()).map(|l| tree.full_name(l)).collect();
    assert_eq!(
        names,
        [
            "abil.str.score",
            "abil.str.mod",
            "abil.dex.score",
            "ac",
            "ac@2",
            "ac_basis",
            "field",
            "x._.y"
        ]
    );
    assert_eq!(tree.roots.len(), 6, "abil, ac, ac@2, ac_basis, field, x");
    assert_eq!(tree.nodes[0].kids.len(), 2, "abil has str and dex");

    let doc = load(&pdf(
        FormMode::Fillable,
        vec![
            text_field("abil.str.score", "", Style::SerifBold),
            text_field("abil.str.mod", "", Style::SerifBold),
            text_field("ac", "", Style::SerifBold),
        ],
    ));
    let acro = dict(&doc, doc.catalog().unwrap().get(b"AcroForm").unwrap());
    let roots = deref(&doc, acro.get(b"Fields").unwrap())
        .as_array()
        .unwrap();
    assert_eq!(roots.len(), 2);
    let abil = dict(&doc, &roots[0]);
    assert_eq!(abil.get(b"T").unwrap().as_str().unwrap(), b"abil");
    let str_node = dict(&doc, &abil.get(b"Kids").unwrap().as_array().unwrap()[0]);
    assert_eq!(str_node.get(b"Kids").unwrap().as_array().unwrap().len(), 2);
    let ws = widgets(&doc);
    assert_eq!(ws[0].get(b"T").unwrap().as_str().unwrap(), b"score");
    let parent = dict(&doc, ws[0].get(b"Parent").unwrap());
    assert_eq!(parent.get(b"T").unwrap().as_str().unwrap(), b"str");
    assert!(acro.get(b"DR").is_ok() && acro.get(b"NeedAppearances").is_err());
}

#[test]
fn non_latin_values_round_trip_through_to_unicode() {
    for value in [
        "Ἀθηνᾶ Παλλάς",
        "Жанна д'Арк",
        "Šárka Dvořáková",
        "1d6\u{2212}1 · ½",
    ] {
        let doc = load(&pdf(
            FormMode::Fillable,
            vec![text_field("n", value, Style::Serif)],
        ));
        let content = page_content(&doc);
        assert_eq!(page_text(&doc), value);
        assert!(!content.contains("<0000"), "no .notdef for {value}");
    }
    // Hebrew is drawn in visual order with the logical text as /ActualText.
    let doc = load(&pdf(
        FormMode::Fillable,
        vec![text_field("n", "אלון", Style::Sans)],
    ));
    let content = page_content(&doc);
    assert_eq!(page_text(&doc), "ןולא");
    assert!(content.contains("/ActualText <FEFF05D005DC05D505DF>"));
    // CJK has no embedded face: it comes from a system font when the machine has one, else it
    // draws .notdef boxes (never '?').
    let doc = load(&pdf(
        FormMode::Fillable,
        vec![text_field("n", "日本", Style::Sans)],
    ));
    let content = page_content(&doc);
    if crate::fonts::Face::Cjk.available() {
        assert_eq!(page_text(&doc), "日本");
        assert!(!content.contains("<0000"), "{content}");
    } else {
        eprintln!("no system CJK font; checking the .notdef fallback");
        assert!(content.contains("<00000000> Tj") && !content.contains('?'));
    }
}

#[test]
fn multiline_fields_wrap_and_clip() {
    let t = TextField {
        multiline: true,
        ..TextField::new("one two three four five six seven eight nine ten", 9.0)
    };
    let doc = load(&pdf(
        FormMode::Fillable,
        vec![PageOp::Field(FormField::new(
            "notes",
            10.0,
            10.0,
            60.0,
            40.0,
            FieldKind::Text(t),
        ))],
    ));
    assert!(widgets(&doc).is_empty(), "a prefilled value is printed");
    let content = page_content(&doc);
    assert!(content.matches(" Tm ").count() >= 3, "{content}");
    assert!(content.contains("0 0 60.00 40.00 re W n"));
}

#[test]
fn paths_alpha_and_clip_use_ext_gstate() {
    let ribbon = Path::rounded_rect(10.0, 10.0, 200.0, 30.0, 8.0);
    let bytes = pdf(
        FormMode::Flatten,
        vec![
            PageOp::Save,
            PageOp::Clip {
                path: Path::rect(0.0, 0.0, 150.0, 100.0),
                even_odd: false,
            },
            PageOp::Opacity(0.5),
            PageOp::Path {
                path: ribbon,
                paint: Paint::fill((0.19, 0.34, 0.33)).with_opacity(0.12),
            },
            PageOp::Restore,
        ],
    );
    let doc = load(&bytes);
    let res = dict(&doc, page(&doc).get(b"Resources").unwrap());
    let gs = dict(&doc, res.get(b"ExtGState").unwrap());
    let ga = dict(&doc, gs.get(b"GA120").unwrap());
    assert_eq!(ga.get(b"ca").unwrap().as_float().unwrap(), 0.12);
    assert!(gs.get(b"GA500").is_ok());
    let content = page_content(&doc);
    assert!(content.contains("150.00 200.00 l") && content.contains("W n"));
    assert!(content.contains("/GA500 gs") && content.contains("/GA120 gs"));
    assert!(content.contains(" c ") && content.contains(" f Q"));
}

struct FakeTags;

impl StructureWriter for FakeTags {
    fn write(&self, sink: &mut ObjectSink<'_>, refs: &DocRefs) -> String {
        let root = sink.alloc();
        sink.object(
            root,
            &format!(
                "<< /Type /StructTreeRoot /K [] /Pg {} 0 R /W {} >>",
                refs.pages[0],
                refs.widgets[0].len()
            ),
        );
        format!("/StructTreeRoot {root} 0 R /MarkInfo << /Marked true >>")
    }
}

#[test]
fn tagging_hooks_mark_content_and_extend_the_catalog() {
    let mut field = FormField::new(
        "ac",
        10.0,
        10.0,
        30.0,
        20.0,
        FieldKind::Text(TextField::new("", 12.0)),
    )
    .with_tooltip("Armor class");
    field.struct_parent = Some(1);
    let bytes = pdf_with(
        FormMode::Fillable,
        vec![
            PageOp::BeginArtifact,
            PageOp::Path {
                path: Path::circle(50.0, 50.0, 10.0),
                paint: Paint::stroke((0.7, 0.5, 0.0), 0.6),
            },
            PageOp::EndMarked,
            PageOp::BeginTag {
                tag: "P".into(),
                mcid: Some(3),
            },
            PageOp::EndMarked,
            PageOp::Field(field),
        ],
        Some("en-US"),
        Some(&FakeTags),
    );
    let doc = load(&bytes);
    let cat = doc.catalog().unwrap();
    assert_eq!(cat.get(b"Lang").unwrap().as_str().unwrap(), b"en-US");
    assert!(cat.get(b"StructTreeRoot").is_ok() && cat.get(b"MarkInfo").is_ok());
    let root = dict(&doc, cat.get(b"StructTreeRoot").unwrap());
    assert_eq!(root.get(b"W").unwrap().as_i64().unwrap(), 1);
    assert_eq!(
        page(&doc).get(b"StructParents").unwrap().as_i64().unwrap(),
        0
    );
    let w = widgets(&doc)[0];
    assert_eq!(w.get(b"StructParent").unwrap().as_i64().unwrap(), 1);
    assert_eq!(w.get(b"TU").unwrap().as_str().unwrap(), b"Armor class");
    let content = page_content(&doc);
    assert!(content.contains("/Artifact << /Type /Layout >> BDC"));
    assert!(content.contains("/P << /MCID 3 >> BDC\nEMC"));
}

#[test]
fn output_is_deterministic() {
    let ops = || vec![text_field("a.b", "x", Style::Sans)];
    assert_eq!(
        pdf(FormMode::Fillable, ops()),
        pdf(FormMode::Fillable, ops())
    );
}

/// Writes a sample form page when `BOREALIS_FORM_SAMPLE` names an output path (visual check).
#[test]
fn sample_form_page() {
    let Ok(path) = std::env::var("BOREALIS_FORM_SAMPLE") else {
        return;
    };
    std::fs::write(path, sample_bytes(FormMode::Fillable)).unwrap();
    if let Ok(flat) = std::env::var("BOREALIS_FORM_SAMPLE_FLAT") {
        std::fs::write(flat, sample_bytes(FormMode::Flatten)).unwrap();
    }
}

fn sample_bytes(mode: FormMode) -> Vec<u8> {
    let tint = FieldFrame {
        shape: Shape::Square,
        paint: Paint::fill((0.953, 0.937, 0.902)).with_stroke((0.81, 0.79, 0.74), 0.5),
        radius: 2.0,
    };
    let ring = |shape| FieldFrame {
        shape,
        paint: Paint::stroke((0.70, 0.50, 0.0), 0.7),
        radius: 1.0,
    };
    let text = |name: &str, (x, y, w, h): (f32, f32, f32, f32), t: TextField| {
        PageOp::Field(FormField::new(name, x, y, w, h, FieldKind::Text(t)).with_frame(tint))
    };
    let check = |name: &str, x: f32, y: f32, mark, shape, on| {
        PageOp::Field(
            FormField::new(
                name,
                x,
                y,
                9.0,
                9.0,
                FieldKind::Check(CheckField {
                    mark,
                    shape,
                    ..CheckField::new(on)
                }),
            )
            .with_frame(ring(shape)),
        )
    };
    let label = |x: f32, y: f32, s: &str| PageOp::Text {
        x,
        y,
        size: 6.0,
        style: Style::SansBold,
        color: (0.31, 0.35, 0.36),
        text: s.into(),
    };
    let ops = vec![
        PageOp::Path {
            path: Path::rounded_rect(0.0, 0.0, W, 60.0, 0.0),
            paint: Paint::fill((0.19, 0.34, 0.33)).with_opacity(0.10),
        },
        label(20.0, 12.0, "NAME"),
        text(
            "char.name",
            (20.0, 20.0, 170.0, 26.0),
            TextField {
                style: Style::SerifBold,
                ..TextField::new("Ysolde Vantor אלון", 18.0)
            },
        ),
        label(200.0, 12.0, "STRENGTH"),
        text(
            "abil.str.mod",
            (200.0, 20.0, 40.0, 26.0),
            TextField {
                style: Style::SerifBold,
                align: TextAlign::Center,
                ..TextField::new("\u{2212}1", 18.0)
            },
        ),
        text(
            "abil.str.score",
            (246.0, 24.0, 30.0, 18.0),
            TextField {
                align: TextAlign::Center,
                ..TextField::new("8", 10.0)
            },
        ),
        label(20.0, 70.0, "DEATH SAVES"),
        check(
            "death.success.1",
            20.0,
            80.0,
            Mark::Fill,
            Shape::Circle,
            true,
        ),
        check(
            "death.success.2",
            32.0,
            80.0,
            Mark::Fill,
            Shape::Circle,
            false,
        ),
        check("insp", 50.0, 80.0, Mark::Fill, Shape::Diamond, true),
        check("cond.poisoned", 68.0, 80.0, Mark::Tick, Shape::Square, true),
        check("exh.1", 86.0, 80.0, Mark::Cross, Shape::Square, true),
        check(
            "cond.blinded",
            104.0,
            80.0,
            Mark::Tick,
            Shape::Square,
            false,
        ),
        label(20.0, 100.0, "NOTES"),
        text(
            "notes",
            (20.0, 108.0, 260.0, 60.0),
            TextField {
                multiline: true,
                style: Style::SerifItalic,
                ..TextField::new(
                    "Ἀθηνᾶ, Жанна and Šárka: Fey Ancestry gives advantage on saves against \
                     being charmed; 1d6\u{2212}1 bludgeoning. Ignored: 日本.",
                    8.0,
                )
            },
        ),
    ];
    pdf(mode, ops)
}
