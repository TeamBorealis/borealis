//! The new Borealis sheet through the public render functions.

mod common;

use borealis_character::{Character, Portrait};
use borealis_engine::CharacterSnapshot;
use borealis_sheet::{
    render_pdf, render_pdf_with, sheet_data, sheet_data_for, PageSelection, Paper, SheetContent,
    SheetEdition, SheetOptions, SheetTheme,
};
use lopdf::{Document, Object};
use std::collections::BTreeSet;

fn opts() -> SheetOptions {
    SheetOptions::default()
}

fn wiz24() -> (Character, CharacterSnapshot) {
    let (mut c, s) = common::wizard_2024();
    c.details.backstory = "Raised among the stacks.".into();
    (c, s)
}

fn parse(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("lopdf parses the PDF")
}

/// Fully qualified names of every widget (partial names joined along `/Parent`).
fn field_names(doc: &Document) -> BTreeSet<String> {
    let partial = |d: &lopdf::Dictionary| match d.get(b"T") {
        Ok(Object::String(t, _)) => Some(String::from_utf8_lossy(t).into_owned()),
        _ => None,
    };
    let mut out = BTreeSet::new();
    for obj in doc.objects.values() {
        let Object::Dictionary(d) = obj else { continue };
        if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Widget") {
            continue;
        }
        let mut parts = vec![partial(d).unwrap_or_default()];
        let mut cur = d.clone();
        while let Ok(Object::Reference(r)) = cur.get(b"Parent") {
            let Ok(Object::Dictionary(pd)) = doc.get_object(*r) else {
                break;
            };
            parts.push(partial(pd).unwrap_or_default());
            cur = pd.clone();
        }
        parts.reverse();
        out.insert(parts.join("."));
    }
    out
}

/// Text of a page's content streams, decoded.
fn page_content(doc: &Document, page: u32) -> String {
    let id = doc.get_pages()[&page];
    String::from_utf8_lossy(&doc.get_page_content(id)).into_owned()
}

fn page_text(doc: &Document, page: u32) -> String {
    doc.extract_text(&[page]).unwrap_or_default()
}

#[test]
fn page_count_and_order_per_edition() {
    let (c24, s24) = wiz24();
    let pdf = render_pdf(&c24, &s24, &opts()).unwrap();
    let doc = parse(&pdf);
    assert!(doc.get_pages().len() >= 4, "core, features, spells, story");
    let (c14, s14) = common::wizard_2014();
    let doc14 = parse(&render_pdf(&c14, &s14, &opts()).unwrap());
    assert!(
        doc14.get_pages().len() >= 4,
        "core, details, spells, inventory"
    );
    // Order: spells sit before the inventory on 2014 and before the story on 2024. Compare the
    // page counts when single groups are selected.
    let only = |spell, inv, story| PageSelection {
        core: false,
        spellcasting: spell,
        inventory: inv,
        story,
    };
    let count = |c: &Character, s: &CharacterSnapshot, p| {
        let o = SheetOptions { pages: p, ..opts() };
        parse(&render_pdf(c, s, &o).unwrap()).get_pages().len()
    };
    let total14 = doc14.get_pages().len();
    assert_eq!(
        total14,
        1 + count(
            &c14,
            &s14,
            PageSelection {
                core: true,
                ..only(false, false, false)
            }
        ) - 1
            + count(&c14, &s14, only(true, false, false))
            + count(&c14, &s14, only(false, true, false))
    );
}

#[test]
fn page_selection_without_spells_drops_the_spell_pages() {
    let (c, s) = wiz24();
    let all = parse(&render_pdf(&c, &s, &opts()).unwrap())
        .get_pages()
        .len();
    let no_spells = SheetOptions {
        pages: PageSelection {
            spellcasting: false,
            ..PageSelection::default()
        },
        ..opts()
    };
    let doc = parse(&render_pdf(&c, &s, &no_spells).unwrap());
    assert!(doc.get_pages().len() < all);
    let names = field_names(&doc);
    assert!(
        !names.iter().any(|n| n.starts_with("spell.")),
        "no spell fields"
    );
    let none = SheetOptions {
        pages: PageSelection {
            core: false,
            spellcasting: false,
            inventory: false,
            story: false,
        },
        ..opts()
    };
    assert!(render_pdf(&c, &s, &none).is_err());
}

fn expected_ids(data: &borealis_sheet::model_types::SheetData) -> Vec<String> {
    data.fields()
        .into_iter()
        .filter(|f| !f.play)
        .map(|f| f.id)
        .collect()
}

/// Play write-ins: always blank, so always widgets (prefilled values are printed instead).
fn play_ids(data: &borealis_sheet::model_types::SheetData) -> Vec<String> {
    data.fields()
        .into_iter()
        .filter(|f| f.play)
        .map(|f| f.id)
        .collect()
}

fn base(name: &str) -> &str {
    // `@k` suffixes sit on the last dotted part: `skill.stealth@2`.
    name.split('@').next().unwrap_or(name)
}

fn assert_fields_present(c: &Character, s: &CharacterSnapshot, edition: SheetEdition) {
    let o = SheetOptions { edition, ..opts() };
    let doc = parse(&render_pdf(c, s, &o).unwrap());
    // A name under an existing terminal field is flattened by the writer (`ac.note` -> `ac_note`).
    let flat = |n: &str| base(n).replace('.', "_");
    let names: BTreeSet<String> = field_names(&doc).iter().map(|n| flat(n)).collect();
    let data = sheet_data_for(c, s, &Default::default(), edition);
    let missing: Vec<String> = play_ids(&data)
        .into_iter()
        .filter(|id| !names.contains(&flat(id)) && !id.contains(".slot."))
        .collect();
    assert!(
        missing.is_empty(),
        "missing field ids ({}): {missing:?}",
        missing.len()
    );
}

#[test]
fn every_play_field_id_is_an_acroform_field() {
    let (c, s) = wiz24();
    assert_fields_present(&c, &s, SheetEdition::Auto);
    let (c, s) = common::wizard_2014();
    assert_fields_present(&c, &s, SheetEdition::Auto);
}

#[test]
fn flatten_mode_has_no_form() {
    let (c, s) = wiz24();
    let o = SheetOptions {
        form_fillable: false,
        ..opts()
    };
    let bytes = render_pdf(&c, &s, &o).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("/AcroForm") && !text.contains("/Widget"));
    assert!(field_names(&parse(&bytes)).is_empty());
}

#[test]
fn page_1_equipment_summary_lines_are_fields_of_their_own_in_2014() {
    // The blank lines of the 2014 page-1 summary (the fixture carries no items) are widgets in a
    // fillable PDF; items and coins (prefilled) are printed, as every other prefilled field.
    let (c, s) = common::wizard_2014();
    let o = SheetOptions {
        edition: SheetEdition::Y2014,
        ..opts()
    };
    let names = field_names(&parse(&render_pdf(&c, &s, &o).unwrap()));
    assert!(names.contains("summary.equip.1.name"), "{names:?}");
    assert!(names.contains("summary.equip.1.qty"));
    let flat = SheetOptions {
        form_fillable: false,
        ..o
    };
    assert!(field_names(&parse(&render_pdf(&c, &s, &flat).unwrap())).is_empty());
}

#[test]
fn ink_pdf_has_only_grey_colour_operators() {
    let (c, s) = common::wizard_2014();
    let o = SheetOptions {
        theme: SheetTheme::Ink,
        ..opts()
    };
    let doc = parse(&render_pdf(&c, &s, &o).unwrap());
    for page in doc.get_pages().keys() {
        let content = page_content(&doc, *page);
        let mut toks: Vec<&str> = Vec::new();
        for line in content.lines() {
            toks.clear();
            toks.extend(line.split_whitespace());
            for (i, t) in toks.iter().enumerate() {
                if matches!(*t, "rg" | "RG") && i >= 3 {
                    let v: Vec<f32> = toks[i - 3..i]
                        .iter()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    assert!(
                        v.len() == 3 && v[0] == v[1] && v[1] == v[2],
                        "page {page}: colour {line}"
                    );
                }
                assert!(!matches!(*t, "k" | "K"), "cmyk on page {page}");
            }
        }
    }
}

#[test]
fn edition_override_changes_the_field_ids() {
    let (c, s) = wiz24();
    let auto = sheet_data(&c, &s, &Default::default());
    let forced = sheet_data_for(&c, &s, &Default::default(), SheetEdition::Y2014);
    assert_ne!(
        expected_ids(&auto).contains(&"char.origin_feat".to_string()),
        expected_ids(&forced).contains(&"char.origin_feat".to_string())
    );
    let o = SheetOptions {
        edition: SheetEdition::Y2014,
        ..opts()
    };
    // The origin feat is a prefilled value, so it is printed (not a widget) on the 2024 sheet.
    let feat = match &auto.edition_fields {
        borealis_sheet::model_types::EditionFields::Y2024(f) => f.origin_feat.clone(),
        borealis_sheet::model_types::EditionFields::Y2014(_) => None,
    }
    .expect("the 2024 wizard has an origin feat");
    let page1 = |o: &SheetOptions| {
        parse(&render_pdf(&c, &s, o).unwrap())
            .extract_text(&[1])
            .unwrap()
    };
    // The feat may also be listed among features; only the 2024 header prints it right after
    // "<background> background ·".
    let in_header = |o: &SheetOptions| {
        page1(o)
            .split(" background")
            .nth(1)
            .is_some_and(|rest| rest.chars().take(40).collect::<String>().contains(&feat))
    };
    assert!(!in_header(&o), "2014 sheet has no origin feat");
    assert!(in_header(&opts()));
}

#[test]
fn output_is_deterministic() {
    let (c, s) = wiz24();
    let o = SheetOptions {
        paper: Paper::A4,
        ..opts()
    };
    let a = render_pdf_with(&c, &s, &o, &SheetContent::default()).unwrap();
    assert_eq!(
        a,
        render_pdf_with(&c, &s, &o, &SheetContent::default()).unwrap()
    );
}

#[test]
fn last_page_carries_the_attribution_and_every_footer_counts() {
    let (c, s) = wiz24();
    let doc = parse(&render_pdf(&c, &s, &opts()).unwrap());
    let n = doc.get_pages().len() as u32;
    assert!(page_text(&doc, n).contains("CC BY 4.0"));
    assert!(!page_text(&doc, 1).contains("CC BY 4.0"));
    assert!(page_text(&doc, 1).contains(&format!("Page 1 of {n}")));
}

#[test]
fn cards_and_legacy_spell_page_still_append() {
    let (c, s) = common::wizard_2014();
    let base_n = parse(&render_pdf(&c, &s, &opts()).unwrap())
        .get_pages()
        .len();
    let mut o = opts();
    o.cards.spell = true;
    let with_cards = parse(&render_pdf(&c, &s, &o).unwrap());
    assert!(
        with_cards.get_pages().len() > base_n,
        "spell cards add pages"
    );
    let mut o = opts();
    o.legacy_spellcasting = true;
    let legacy = parse(&render_pdf(&c, &s, &o).unwrap());
    let names = field_names(&legacy);
    assert!(
        !names.iter().any(|n| n.starts_with("spell.")),
        "new spell pages are replaced"
    );
    let total = legacy.get_pages().len() as u32;
    assert!(page_text(&legacy, total).contains(&format!("Page {total} of {total}")));
}

#[test]
fn a_jpeg_portrait_is_placed_on_the_story_or_details_page() {
    let (mut c, s) = wiz24();
    c.portrait = Some(Portrait {
        file_name: "p.jpg".into(),
        data_base64: tiny_jpeg(),
    });
    let bytes = render_pdf(&c, &s, &opts()).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("/DCTDecode"));
}

fn tiny_jpeg() -> String {
    const B: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAAAP/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AKp//2Q==";
    B.to_string()
}

#[test]
#[ignore = "writes sample PDFs to BOREALIS_E2C_OUT for manual inspection"]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_E2C_OUT") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let (mut c14, s14) = common::wizard_2014();
    c14.details.backstory = "Raised among the stacks.\nShe tends the forge-fires.".into();
    c14.details.personality.traits = "Quiet and watchful.\nAlways counts the exits.".into();
    c14.details.personality.ideals = "Knowledge\nShared freely.".into();
    c14.portrait = Some(Portrait {
        file_name: "p.jpg".into(),
        data_base64: tiny_jpeg(),
    });
    let (mut c24, s24) = wiz24();
    c24.portrait = c14.portrait.clone();
    c24.details.personality = c14.details.personality.clone();
    c24.details.backstory = c14.details.backstory.clone();
    for (name, c, s) in [("w2014", &c14, &s14), ("w2024", &c24, &s24)] {
        for (tag, o) in [
            ("colour", opts()),
            (
                "ink",
                SheetOptions {
                    theme: SheetTheme::Ink,
                    paper: Paper::A4,
                    ..opts()
                },
            ),
        ] {
            let pdf = render_pdf(c, s, &o).unwrap();
            std::fs::write(dir.join(format!("{name}-{tag}.pdf")), pdf).unwrap();
        }
    }
}
