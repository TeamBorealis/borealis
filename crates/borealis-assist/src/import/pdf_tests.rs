//! Synthetic-PDF tests of the text-layer reader (invented text only).

use lopdf::{dictionary, Document, Object, Stream};

use super::pdf::{extract_pdf_text, read_pdf_layout};

/// A one-page PDF with `content` and fonts `F1` (custom codes: `Differences` with unknown glyph
/// names plus a `ToUnicode` map over `alphabet`), `F2` (Helvetica, WinAnsi) and `F3` (oldstyle
/// digit names, no `ToUnicode`).
fn pdf(alphabet: &str, content: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut diffs = vec![Object::Integer(1)];
    let mut cmap = String::from("1 begincodespacerange <00> <FF> endcodespacerange\n");
    for (i, c) in alphabet.chars().enumerate() {
        diffs.push(Object::Name(format!("g{}", i + 1).into_bytes()));
        let mut units = [0u16; 2];
        let hex: String = c
            .encode_utf16(&mut units)
            .iter()
            .map(|u| format!("{u:04X}"))
            .collect();
        cmap.push_str(&format!(
            "1 beginbfchar <{:02X}> <{hex}> endbfchar\n",
            i + 1
        ));
    }
    let tu = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    let f1 = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "ABCDEF+Lanternface",
        "Encoding" => dictionary! {"Type" => "Encoding", "Differences" => diffs}, "ToUnicode" => tu});
    let f2 = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let digits: Vec<Object> = std::iter::once(Object::Integer(48))
        .chain(
            [
                "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
            ]
            .iter()
            .map(|d| Object::Name(format!("{d}.oldstyle").into_bytes())),
        )
        .collect();
    let f3 = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => dictionary! {"Type" => "Encoding", "Differences" => digits}},
    );
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! {"Font" => dictionary! {"F1" => f1, "F2" => f2, "F3" => f3}},
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1},
        ),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// `text` as a hex string of `alphabet` codes (1-based).
fn coded(alphabet: &str, text: &str) -> String {
    let codes: String = text
        .chars()
        .map(|c| format!("{:02X}", alphabet.chars().position(|a| a == c).unwrap() + 1))
        .collect();
    format!("<{codes}>")
}

fn show(font: &str, size: f32, x: f32, y: f32, s: &str) -> String {
    format!("BT /{font} {size} Tf {x} {y} Td {s} Tj ET\n")
}

#[test]
fn to_unicode_wins_over_unknown_glyph_names_and_ligatures_and_digits_survive() {
    let alpha = "Mothsgaerlnd \u{FB01}";
    let mut c = show(
        "F1",
        11.0,
        72.0,
        700.0,
        &coded(alpha, "Moths gather at the lantern"),
    );
    c += &show(
        "F1",
        11.0,
        72.0,
        680.0,
        &coded(alpha, "\u{FB01}nd the \u{FB01}re"),
    );
    c += &show("F3", 11.0, 72.0, 660.0, "(Year 1984)");
    let t = extract_pdf_text(&pdf(alpha, &c)).unwrap().text;
    assert!(t.contains("Moths gather at the lantern"), "{t}");
    assert!(t.contains("find the fire"), "{t}");
    assert!(t.contains("1984"), "{t}");
}

#[test]
fn overprinted_headings_are_read_once_and_superscripts_stay_in_the_word() {
    let mut c = String::new();
    for dx in [0.0, 0.4] {
        c += &show("F2", 18.0, 72.0 + dx, 720.0, "(Glimmer Lore)");
    }
    c += "BT /F2 10 Tf 72 690 Td (Reach the 4) Tj ET\n";
    // Helvetica without a `Widths` array: every glyph is half an em wide.
    let four_end = 72.0 + 11.0 * 5.0;
    c += &format!("BT /F2 6 Tf {four_end} 694 Td (th) Tj ET\n");
    c += &format!(
        "BT /F2 10 Tf {} 690 Td ( level of the spire before dusk.) Tj ET\n",
        four_end + 5.0
    );
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    assert_eq!(t.matches("Glimmer Lore").count(), 1, "{t}");
    assert!(t.contains("4th level"), "{t}");
}

#[test]
fn letter_shifted_text_layer_is_not_trusted() {
    let prose =
        "the moths gather at the lantern and you can see the path as it winds into the hollow ";
    let shifted: String = prose
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() && c != 'z' {
                (c as u8 + 1) as char
            } else {
                c
            }
        })
        .collect();
    let mut c = String::new();
    for i in 0..8 {
        c += &show(
            "F2",
            10.0,
            72.0,
            700.0 - 14.0 * i as f32,
            &format!("({shifted})"),
        );
    }
    let layout = read_pdf_layout(&pdf("a", &c)).unwrap();
    assert!(!layout.pages[0].has_text());
    assert!(layout.pages[0].distrust.is_some());
}

#[test]
fn catalogue_rows_keep_the_item_name_whole_beside_its_text() {
    // Three cells per row: number, item name (wrapped, vertically centred), description;
    // the name cell ends close to the description column.
    let items = [
        (
            "Moth Jar of",
            "Dusk",
            ["Glows for an hour", "when shaken twice", "at night."],
        ),
        (
            "Dew Bottle of",
            "Plenty",
            ["Fills with water", "each dawn and never", "runs dry."],
        ),
        (
            "Ember Ring of",
            "Warmth",
            ["Keeps the wearer", "warm in snow and", "biting wind."],
        ),
        (
            "Lantern Hook of",
            "Holding",
            ["Holds any lantern", "steady on a moving", "cart or boat."],
        ),
    ];
    let mut c = show("F2", 10.0, 72.0, 740.0, "(d4)") + &show("F2", 10.0, 100.0, 740.0, "(Item)");
    c += &show("F2", 10.0, 182.0, 740.0, "(Description)");
    for (k, (a, b, desc)) in items.iter().enumerate() {
        let top = 720.0 - 48.0 * k as f32;
        c += &show("F2", 10.0, 72.0, top - 12.0, &format!("({})", k + 1));
        c += &show("F2", 10.0, 100.0, top - 6.0, &format!("({a})"));
        c += &show("F2", 10.0, 100.0, top - 18.0, &format!("({b})"));
        for (j, l) in desc.iter().enumerate() {
            c += &show("F2", 10.0, 182.0, top - 12.0 * j as f32, &format!("({l})"));
        }
    }
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    for (a, b, desc) in items {
        let row = t
            .lines()
            .find(|l| l.contains(&format!("{a} {b}")))
            .unwrap_or_else(|| panic!("{t}"));
        assert!(row.contains(desc[0]) && row.contains(desc[2]), "{row}\n{t}");
    }
}
