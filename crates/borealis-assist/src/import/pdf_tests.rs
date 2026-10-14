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

/// Prose lines of a column: `words` repeated to about `chars` characters per line.
fn prose(x: f32, top: f32, lines: usize, words: &str) -> String {
    (0..lines)
        .map(|i| {
            show(
                "F2",
                10.0,
                x,
                top - 14.0 * i as f32,
                &format!("({words} {i})"),
            )
        })
        .collect()
}

#[test]
fn a_table_across_the_column_gutter_is_read_row_by_row() {
    let mut c = prose(72.0, 740.0, 8, "the hollow lantern road winds on");
    c += &prose(320.0, 740.0, 8, "a quiet bell rings over the moor");
    let rows = [
        ["Name", "Cost", "Damage", "Weight", "Properties"],
        [
            "Lantern rod",
            "12 gp",
            "1d6 fire",
            "3 lb.",
            "Glows (range 20/60), loud",
        ],
        [
            "Bell staff",
            "30 gp",
            "1d8 thunder",
            "4 lb.",
            "Rings (range 30/90), heavy",
        ],
        [
            "Moth sling",
            "2 gp",
            "1d4 piercing",
            "1 lb.",
            "Quiet (range 40/120), light",
        ],
    ];
    for (k, row) in rows.iter().enumerate() {
        let y = 600.0 - 14.0 * k as f32;
        for (cell, x) in row.iter().zip([72.0, 140.0, 180.0, 250.0, 290.0]) {
            c += &show("F2", 10.0, x, y, &format!("({cell})"));
        }
    }
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    for row in &rows[1..] {
        assert!(t.lines().any(|l| l == row.join(" | ")), "{t}");
    }
    let left = t.find("road winds on 7").unwrap();
    let right = t.find("over the moor 0").unwrap();
    assert!(
        left < right && right < t.find("Lantern rod").unwrap(),
        "{t}"
    );
}

#[test]
fn a_table_inside_one_column_keeps_its_rows_beside_the_other_columns_prose() {
    let mut c = prose(72.0, 740.0, 4, "the hollow lantern road winds on");
    c += &prose(320.0, 740.0, 14, "a quiet bell rings over the moor");
    for (k, row) in [
        ["d4", "Moss", "Terrain"],
        ["1", "Fern", "Bog, fen"],
        ["2", "Reed", "Marsh"],
        ["3", "Pine", "Hills"],
    ]
    .iter()
    .enumerate()
    {
        let y = 670.0 - 14.0 * k as f32;
        for (cell, x) in row.iter().zip([72.0, 100.0, 180.0]) {
            c += &show("F2", 10.0, x, y, &format!("({cell})"));
        }
    }
    c += &prose(72.0, 600.0, 4, "past the old mill and the ford");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    for row in [
        "1 | Fern | Bog, fen",
        "2 | Reed | Marsh",
        "3 | Pine | Hills",
    ] {
        assert!(t.lines().any(|l| l == row), "{t}");
    }
    assert!(
        !t.lines().any(|l| l.contains("Fern") && l.contains("moor")),
        "{t}"
    );
    let column = t.find("the ford 3").unwrap();
    assert!(column < t.find("over the moor 0").unwrap(), "{t}");
}

#[test]
fn a_repeated_two_word_label_is_not_split_into_cells() {
    let mut c = String::new();
    for k in 0..5 {
        let top = 740.0 - 70.0 * k as f32;
        c += &show("F2", 10.0, 72.0, top, &format!("(Spell number {k})"));
        c += &show("F2", 10.0, 72.0, top - 14.0, "(Casting)");
        c += &show("F2", 10.0, 110.0, top - 14.0, "(Time: 1 action)");
        c += &show("F2", 10.0, 72.0, top - 28.0, "(Range: 30 feet)");
    }
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    assert_eq!(t.matches("Casting Time: 1 action").count(), 5, "{t}");
    assert!(!t.contains(" | "), "{t}");
}

#[test]
fn a_numbered_two_column_table_inside_a_column_keeps_its_rows() {
    let mut c = prose(72.0, 740.0, 3, "the hollow lantern road winds on");
    c += &prose(320.0, 740.0, 16, "a quiet bell rings over the moor");
    let names = ["Moss", "Fern", "Reed", "Pine", "Sedge", "Heath"];
    for (k, name) in names.iter().enumerate() {
        let y = 690.0 - 14.0 * k as f32;
        c += &show("F2", 10.0, 72.0, y, &format!("({})", k + 1));
        c += &show("F2", 10.0, 150.0, y, &format!("({name})"));
    }
    c += &prose(72.0, 590.0, 3, "past the old mill and the ford");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    for (k, name) in names.iter().enumerate() {
        assert!(t.lines().any(|l| l == format!("{} | {name}", k + 1)), "{t}");
    }
}

/// A list column: a heading of `size` 13 at `top`, then `items` 13 points apart.
fn list(x: f32, top: f32, head: &str, items: &[&str]) -> String {
    let mut c = show("F2", 13.0, x, top, &format!("({head})"));
    for (i, item) in items.iter().enumerate() {
        c += &show(
            "F2",
            10.0,
            x,
            top - 13.0 * (i + 1) as f32,
            &format!("({item})"),
        );
    }
    c
}

/// One block: a big name over four list columns (the middle two share baselines).
fn list_block(top: f32, name: &str, tag: &str) -> String {
    let item = |n: usize| format!("{tag} item {n}");
    let names: Vec<String> = (0..24).map(item).collect();
    let n: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut c = show("F2", 28.0, 48.0, top, &format!("({name})"));
    c += &list(48.0, top - 25.0, "Rank 1", &n[0..4]);
    c += &list(48.0, top - 95.0, "Rank 2", &n[4..7]);
    c += &list(179.0, top - 25.0, "Rank 3", &n[7..11]);
    c += &list(179.0, top - 95.0, "Rank 4", &n[11..14]);
    c += &list(310.0, top - 25.0, "Rank 5", &n[14..22]);
    c += &list(441.0, top - 25.0, "Rank 6", &n[22..24]);
    c
}

#[test]
fn blocks_of_list_columns_stacked_on_a_page_are_read_block_by_block() {
    let mut c = show("F2", 28.0, 218.0, 740.0, "(Lantern Lists)");
    c += &show(
        "F2",
        9.0,
        178.0,
        715.0,
        "(The lists below name the charms each order may learn.)",
    );
    c += &list_block(680.0, "Tinkers", "tinker");
    c += &list_block(430.0, "Weavers", "weaver");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing: {t}"));
    let order = [
        "Lantern Lists",
        "Tinkers",
        "tinker item 0",
        "tinker item 6",
        "tinker item 7",
        "tinker item 14",
        "tinker item 23",
        "Weavers",
        "weaver item 0",
        "weaver item 23",
    ];
    assert!(order.windows(2).all(|w| at(w[0]) < at(w[1])), "{t}");
    assert!(!t.lines().any(|l| l.contains(" | ")), "{t}");
    assert!(t.lines().any(|l| l.ends_with("Rank 4")), "{t}");
}

#[test]
fn a_big_heading_in_one_column_of_prose_does_not_cut_the_page() {
    // Left column: prose, a big heading, prose. Right column: prose all the way down, and a
    // second page where the right column ends above the left column's big heading.
    let mut c = prose(72.0, 740.0, 6, "the hollow lantern road winds on");
    c += &show("F2", 24.0, 72.0, 640.0, "(Moth Gate)");
    c += &prose(72.0, 615.0, 6, "past the moth gate the road climbs");
    c += &prose(320.0, 740.0, 14, "a quiet bell rings over the moor");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    let at = |s: &str| t.find(s).unwrap();
    assert!(at("Moth Gate") < at("road climbs 5"), "{t}");
    assert!(at("road climbs 5") < at("over the moor 0"), "{t}");

    let mut c = prose(72.0, 740.0, 6, "the hollow lantern road winds on");
    c += &show("F2", 24.0, 72.0, 640.0, "(Moth Gate)");
    c += &prose(72.0, 615.0, 6, "past the moth gate the road climbs");
    c += &prose(320.0, 740.0, 6, "a quiet bell rings over the moor");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    let at = |s: &str| t.find(s).unwrap();
    assert!(at("road climbs 5") < at("over the moor 0"), "{t}");
}

#[test]
fn lists_side_by_side_on_shared_baselines_are_not_a_table() {
    let names: Vec<String> = (0..30).map(|n| format!("charm {n}")).collect();
    let n: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut c = show("F2", 28.0, 48.0, 700.0, "(Wardens)");
    c += &list(48.0, 680.0, "Rank 1", &n[0..6]);
    c += &list(48.0, 589.0, "Rank 2", &n[6..12]);
    c += &list(179.0, 680.0, "Rank 3", &n[12..19]);
    c += &list(179.0, 576.0, "Rank 4", &n[19..24]);
    c += &list(310.0, 680.0, "Rank 5", &n[24..29]);
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    assert!(!t.lines().any(|l| l.contains(" | ")), "{t}");
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing: {t}"));
    let order = ["charm 0", "charm 11", "charm 12", "charm 23", "charm 24"];
    assert!(order.windows(2).all(|w| at(w[0]) < at(w[1])), "{t}");
}

#[test]
fn a_two_column_table_in_a_column_reads_by_rows_and_the_heading_after_it_keeps_its_text() {
    let mut c = prose(72.0, 740.0, 30, "the hollow lantern road winds on");
    c += &prose(320.0, 740.0, 8, "a quiet bell rings over the moor");
    c += &show("F2", 12.0, 320.0, 610.0, "(Ember Calling)");
    let rows = [
        ("Level", "Charms"),
        ("1st", "spark ward, glow"),
        ("3rd", "cinder step, flare"),
        ("5th", "ash cloak, kindle"),
        ("7th", "smoke veil, char"),
    ];
    for (k, (a, b)) in rows.iter().enumerate() {
        let y = 594.0 - 12.0 * k as f32;
        c += &show("F2", 9.0, 350.0, y, &format!("({a})"));
        c += &show("F2", 9.0, 410.0, y, &format!("({b})"));
    }
    c += &show("F2", 12.0, 320.0, 520.0, "(Gifts of Ember)");
    c += &prose(320.0, 505.0, 4, "when you choose this calling you gain");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    for (a, b) in &rows {
        assert!(t.lines().any(|l| l == format!("{a} | {b}")), "{t}");
    }
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing: {t}"));
    assert!(at("smoke veil") < at("Gifts of Ember"), "{t}");
    let after = &t[at("Gifts of Ember")..];
    let next = after
        .lines()
        .skip(1)
        .find(|l| !l.trim().is_empty())
        .unwrap();
    assert!(next.starts_with("when you choose"), "{t}");
}

#[test]
fn a_row_label_set_tight_against_the_next_cell_is_split_from_it() {
    let mut c = prose(72.0, 740.0, 4, "the hollow lantern road winds on");
    let rows = [
        ("d100", "Metal", "Reach", "Rarity"),
        ("01-20", "Tin", "5 feet", "Common"),
        ("21-40", "Iron", "5 feet", "Common"),
        ("41-60", "Lead", "5 feet", "Common"),
        ("61-90", "Brass", "10 feet", "Rare"),
    ];
    for (k, (a, b, d, e)) in rows.iter().enumerate() {
        let y = 660.0 - 14.0 * k as f32;
        for (cell, x) in [(a, 72.0), (b, 110.0), (d, 170.0), (e, 230.0)] {
            c += &show("F2", 10.0, x, y, &format!("({cell})"));
        }
    }
    c += &show("F2", 10.0, 70.0, 590.0, "(91-100 Pewter)");
    c += &show("F2", 10.0, 170.0, 590.0, "(15 feet)");
    c += &show("F2", 10.0, 230.0, 590.0, "(Legendary)");
    let t = extract_pdf_text(&pdf("a", &c)).unwrap().text;
    assert!(
        t.lines().any(|l| l == "61-90 | Brass | 10 feet | Rare"),
        "{t}"
    );
    assert!(
        t.lines()
            .any(|l| l == "91-100 | Pewter | 15 feet | Legendary"),
        "{t}"
    );
}
