//! Sheet rendering against a real engine snapshot: level-5 Hill Dwarf Life Cleric from the
//! bundled `core` + `srd-5.1` packs.

use borealis_character::{Character, Portrait};
use borealis_content::{ContentStore, ContentStoreBuilder};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::money::Coin;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use borealis_sheet::{render_html, render_pdf, PageSelection, Paper, SheetError, SheetOptions};
use std::path::Path;

fn store() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs load")
        .build()
}

fn pick(
    e: &Engine,
    s: &borealis_engine::CharacterState,
    ty: &str,
    id: &str,
) -> borealis_engine::CharacterState {
    let snap = e.derive(s);
    let key = snap
        .slots
        .iter()
        .find(|x| x.ty == ty && x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
        .map(|x| x.key.clone())
        .unwrap_or_else(|| panic!("no open {ty} slot offering {id}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn cleric(store: &ContentStore) -> (Character, CharacterSnapshot) {
    let e = Engine::new(store);
    let mut s = commands::new_character(&e, borealis_content::Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = [13, 10, 14, 8, 15, 12];
    let snap = e.derive(&s);
    let key = |s: &CharacterSnapshot, n: &str| slot_key(s, "ID_LEVEL_1", n, 1).expect(n);
    s = commands::select(
        &e,
        &s,
        &key(&snap, "Race"),
        Choice::Element("ID_SRD_RACE_DWARF".into()),
    )
    .unwrap()
    .state;
    let snap = e.derive(&s);
    s = commands::select(
        &e,
        &s,
        &key(&snap, "Class"),
        Choice::Element("ID_WOTC_PHB_CLASS_CLERIC".into()),
    )
    .unwrap()
    .state;
    let snap = e.derive(&s);
    s = commands::select(
        &e,
        &s,
        &key(&snap, "Background"),
        Choice::Element("ID_BACKGROUND_ACOLYTE".into()),
    )
    .unwrap()
    .state;
    s = pick(&e, &s, "Sub Race", "ID_SUB_RACE_HILL_DWARF");
    s = pick(&e, &s, "Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    for item in [
        "ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL",
        "ID_WOTC_GEAR_SHIELD",
        "ID_WOTC_PHB_WEAPON_MACE",
    ] {
        s = commands::add_item(&e, &s, item, 1).unwrap().state;
        let uid = s.inventory.items.last().unwrap().uid.clone();
        s = commands::equip(&e, &s, &uid, None).unwrap().state;
    }
    s = commands::deposit(&s, Coin::Gp, 123);
    let snap = e.derive(&s);
    let mut c = Character::new("Brunhild <Stonehand> & Co");
    c.build = s;
    c.details.backstory = "Raised in the temple of Moradin.\nShe tends the forge-fires.".into();
    c.details.personality.traits = "I quote sacred texts at odd moments.".into();
    (c, snap)
}

fn fixture() -> (Character, CharacterSnapshot) {
    cleric(&store())
}

fn esc_cell(s: &str) -> String {
    s.replace('&', "&amp;")
}

fn count_pages(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    let at =
        text.find("/Type /Pages /Count ").expect("pages object") + "/Type /Pages /Count ".len();
    text[at..]
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn html_contains_the_computed_values() {
    let (c, snap) = fixture();
    assert_eq!(snap.level, 5);
    let html = render_html(&c, &snap, &flat()).unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(!html.contains("<script"));
    assert!(
        html.contains("Brunhild &lt;Stonehand&gt; &amp; Co"),
        "name is escaped"
    );
    assert!(!html.contains("<Stonehand>"));
    assert!(html.contains("Cleric 5"), "class line");
    assert!(html.contains("Hill Dwarf"));
    let field = |id: &str, v: &str| {
        let at = html
            .find(&format!("data-field=\"{id}\""))
            .unwrap_or_else(|| panic!("{id}"));
        let tail = &html[at..];
        let open = tail.find('>').unwrap() + 1;
        tail[open..].starts_with(&format!("{v}<"))
    };
    assert!(field("prof.bonus", &format!("+{}", snap.proficiency_bonus)));
    assert!(field("hp.max", &snap.hit_points.to_string()), "HP");
    let wis = snap.ability("wisdom").unwrap();
    assert!(field("abil.wis.score", &wis.score.to_string()));
    assert!(html.contains("123"), "gold");
    assert!(html.contains("Scale Mail"));
    assert!(!snap.attacks.is_empty(), "the mace yields an attack row");
    assert!(html.contains(&esc_cell(&snap.attacks[0].damage)));
    assert!(html.contains("Sleight of Hand"));
    assert!(html.contains("Raised in the temple of Moradin."));
    // 2014 sheet: character, details, spellcasting, inventory.
    assert_eq!(html.matches("<section class=\"page ").count(), 4);
    for page in ["character", "details", "spellcasting", "inventory"] {
        assert!(html.contains(&format!("data-page=\"{page}\"")), "{page}");
    }
}

#[test]
fn page_selection_compact_and_empty_selection() {
    let (c, snap) = fixture();
    let only_core = SheetOptions {
        pages: PageSelection {
            core: true,
            spellcasting: false,
            inventory: false,
            story: false,
        },
        ..flat()
    };
    // Core = page 1 and the 2014 Details pages.
    let html = render_html(&c, &snap, &only_core).unwrap();
    assert!(html.contains("data-page=\"character\"") && !html.contains("data-page=\"inventory\""));
    let none = SheetOptions {
        pages: PageSelection {
            core: false,
            spellcasting: false,
            inventory: false,
            story: false,
        },
        ..flat()
    };
    assert!(matches!(
        render_html(&c, &snap, &none),
        Err(SheetError::NoPages)
    ));
    assert!(matches!(
        render_pdf(&c, &snap, &none),
        Err(SheetError::NoPages)
    ));
    let compact = SheetOptions {
        compact: true,
        ..flat()
    };
    assert!(render_html(&c, &snap, &compact)
        .unwrap()
        .contains("<body class=\"sheet compact\""));
    let a4 = SheetOptions {
        paper: Paper::A4,
        ..flat()
    };
    assert!(render_html(&c, &snap, &a4).unwrap().contains("size:A4"));
}

#[test]
fn pdf_is_valid_and_deterministic() {
    let (c, snap) = fixture();
    let opts = flat();
    let pdf = render_pdf(&c, &snap, &opts).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    let pages = count_pages(&pdf);
    assert!(pages >= 4, "four logical pages, got {pages}");
    let text = String::from_utf8_lossy(&pdf);
    assert_eq!(text.matches("/Type /Page ").count(), pages);
    assert!(pdf_lines(&pdf)
        .iter()
        .any(|l| l == "Brunhild <Stonehand> & Co"));
    assert!(text.contains("MediaBox [0 0 612 792]"));
    // startxref points at the xref table.
    let at: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(pdf[at..].starts_with(b"xref\n"));
    assert_eq!(pdf, render_pdf(&c, &snap, &opts).unwrap(), "deterministic");
}

#[test]
fn pdf_respects_paper_pages_and_compact() {
    let (c, snap) = fixture();
    let a4 = render_pdf(
        &c,
        &snap,
        &SheetOptions {
            paper: Paper::A4,
            ..flat()
        },
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&a4).contains("MediaBox [0 0 595 842]"));
    let core = SheetOptions {
        pages: PageSelection {
            core: true,
            spellcasting: false,
            inventory: false,
            story: false,
        },
        compact: true,
        ..flat()
    };
    let one = render_pdf(&c, &snap, &core).unwrap();
    assert_eq!(
        count_pages(&one),
        2,
        "core = page 1 + the 2014 Details page"
    );
}

#[test]
fn portrait_is_embedded_as_data_uri_and_pdf_xobject() {
    let (mut c, snap) = fixture();
    // A 1x1 grayscale JPEG.
    let jpeg: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B,
        0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
        0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xFF, 0xD9,
    ];
    c.portrait = Some(Portrait {
        file_name: "p.jpg".into(),
        data_base64: encode(jpeg),
    });
    let html = render_html(&c, &snap, &flat()).unwrap();
    assert!(html.contains("src=\"data:image/jpeg;base64,/9j/"));
    let pdf = render_pdf(&c, &snap, &flat()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Subtype /Image") && text.contains("/DCTDecode"));
    let contents: String = parse_pdf(&pdf).iter().map(Obj::text_stream).collect();
    assert!(contents.contains("/Im0 Do"));
}

#[test]
fn html_portrait_injection_payloads_are_neutralised() {
    let store = store();
    let (mut c, snap) = cleric(&store);
    for payload in [
        "iVBORw0KGgo=\" onload=\"alert(1)",
        "iVBORw0KGgo=\\\"><script>alert(1)</script>",
        "iVBORw0KGgo=\"><script>alert(1)</script>",
    ] {
        c.portrait = Some(Portrait {
            file_name: "p.png".into(),
            data_base64: payload.into(),
        });
        let html = render_html(&c, &snap, &flat()).unwrap();
        assert!(!html.contains("onload"), "payload leaked: {payload}");
        assert!(!html.contains("<script"), "payload leaked: {payload}");
        assert!(
            !html.contains("Character portrait"),
            "portrait should be dropped"
        );
    }
    let html = render_html(&c, &snap, &flat()).unwrap();
    assert!(html.contains("http-equiv=\"Content-Security-Policy\""));
    assert!(html.contains("default-src 'none'; img-src data:"));
}

/// A 3x2 RGBA PNG with one transparent pixel.
fn rgba_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 3, 2);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    let px: Vec<u8> = (0..6u8)
        .flat_map(|i| [i * 40, 255 - i * 30, 90, if i == 2 { 0 } else { 255 }])
        .collect();
    w.write_image_data(&px).unwrap();
    drop(w);
    out
}

#[test]
fn png_portrait_embeds_as_flate_rgb_with_soft_mask() {
    let (mut c, snap) = fixture();
    c.portrait = Some(Portrait {
        file_name: "p.png".into(),
        data_base64: encode(&rgba_png()),
    });
    let pdf = render_pdf(&c, &snap, &flat()).unwrap();
    let objs = parse_pdf(&pdf);
    let image = objs
        .iter()
        .find(|o| o.dict.contains("/Subtype /Image") && o.dict.contains("/DeviceRGB"))
        .expect("RGB image object");
    assert!(image.dict.contains("/FlateDecode") && image.dict.contains("/SMask"));
    assert_eq!(image.data().len(), 3 * 2 * 3, "raw RGB samples");
    let mask = objs
        .iter()
        .find(|o| o.dict.contains("/DeviceGray") && o.dict.contains("/Subtype /Image"))
        .expect("soft mask");
    assert_eq!(mask.data(), vec![255, 255, 0, 255, 255, 255]);
    let contents: String = objs.iter().map(|o| o.text_stream()).collect();
    assert!(contents.contains("/Im0 Do"));
}

#[test]
fn webp_and_gif_portraits_are_embedded_in_the_pdf() {
    for (name, b64) in [
        ("p.gif", "R0lGODlhAQABAIAAAAUEBAAAACwAAAAAAQABAAACAkQBADs="),
        ("p.webp", "UklGRhoAAABXRUJQVlA4TA0AAAAvAAAAEAcQERGIiP4HAA=="),
    ] {
        let (mut c, snap) = fixture();
        c.portrait = Some(Portrait {
            file_name: name.into(),
            data_base64: b64.into(),
        });
        let pdf = render_pdf(&c, &snap, &flat()).unwrap();
        let objs = parse_pdf(&pdf);
        assert!(
            objs.iter()
                .any(|o| o.dict.contains("/Subtype /Image") && o.dict.contains("/DeviceRGB")),
            "{name} not embedded"
        );
    }
}

#[test]
fn non_latin_name_is_embedded_and_extractable() {
    let (mut c, snap) = fixture();
    c.meta.name = "אלון Ægir Šimek".into();
    let opts = flat();
    let pdf = render_pdf(&c, &snap, &opts).unwrap();
    assert_eq!(pdf, render_pdf(&c, &snap, &opts).unwrap(), "deterministic");
    let objs = parse_pdf(&pdf);
    // Every drawn glyph is a real glyph (CID 0 is .notdef) and none degraded to '?'.
    for line in content_lines(&objs) {
        assert!(!line.cids.contains(&0), "notdef glyph in {:?}", line.text);
    }
    let lines = pdf_lines(&pdf);
    // Display order: the Hebrew run is reversed, the Latin part follows it.
    assert!(
        lines.iter().any(|l| l == "ןולא Ægir Šimek"),
        "visual text via ToUnicode: {lines:?}"
    );
    // Reading order is preserved for search/copy through ActualText.
    let actual = actual_texts(&objs);
    assert!(actual.iter().any(|t| t == "אלון Ægir Šimek"), "{actual:?}");
    // The /Title is UTF-16.
    assert!(String::from_utf8_lossy(&pdf).contains("/Title <FEFF05D005DC05D505DF"));
    // Embedded subsets are valid fonts and small.
    let fonts: Vec<_> = objs
        .iter()
        .filter(|o| o.dict.contains("/Length1"))
        .collect();
    assert!(fonts.len() >= 3, "sans, bold and hebrew subsets");
    for f in fonts {
        let data = f.data();
        let face = ttf_parser::Face::parse(&data, 0).expect("subset parses");
        assert!(face.number_of_glyphs() < 200, "{}", face.number_of_glyphs());
    }
}

#[test]
fn pdf_size_is_reported_for_the_default_sheet() {
    let (c, snap) = fixture();
    let pdf = render_pdf(&c, &snap, &flat()).unwrap();
    eprintln!("default sheet PDF: {} bytes", pdf.len());
    assert!(pdf.len() < 400_000, "{}", pdf.len());
}

/// One PDF object: dictionary text and (optional) raw stream bytes.
struct Obj {
    id: usize,
    dict: String,
    stream: Option<Vec<u8>>,
}

impl Obj {
    fn data(&self) -> Vec<u8> {
        let raw = self.stream.clone().unwrap_or_default();
        if self.dict.contains("/FlateDecode") {
            miniz_oxide::inflate::decompress_to_vec_zlib(&raw).expect("valid zlib stream")
        } else {
            raw
        }
    }
    fn text_stream(&self) -> String {
        String::from_utf8_lossy(&self.data()).into_owned()
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Parse objects via the xref table (binary safe: stream length comes from `/Length`).
fn parse_pdf(pdf: &[u8]) -> Vec<Obj> {
    let text = String::from_utf8_lossy(pdf);
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let table = std::str::from_utf8(&pdf[start..]).unwrap_or_default();
    let offsets: Vec<usize> = table
        .lines()
        .skip(2)
        .take_while(|l| !l.starts_with("trailer"))
        .skip(1)
        .map(|l| l[..10].parse().unwrap())
        .collect();
    offsets
        .iter()
        .enumerate()
        .map(|(i, off)| {
            let body = find(pdf, b"obj\n", *off).unwrap() + 4;
            let end = find(pdf, b"\nendobj", body).unwrap();
            let stream_at = find(pdf, b"stream\n", body).filter(|s| *s < end);
            let (dict_end, stream) = match stream_at {
                Some(s) => {
                    let head = String::from_utf8_lossy(&pdf[body..s]).into_owned();
                    let len: usize = head
                        .split("/Length ")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    (s, Some(pdf[s + 7..s + 7 + len].to_vec()))
                }
                None => (end, None),
            };
            Obj {
                id: i + 1,
                dict: String::from_utf8_lossy(&pdf[body..dict_end]).into_owned(),
                stream,
            }
        })
        .collect()
}

struct Line {
    text: String,
    cids: Vec<u16>,
}

fn hex_pairs(hex: &str, width: usize) -> Vec<u32> {
    (0..hex.len() / width)
        .map(|i| u32::from_str_radix(&hex[i * width..(i + 1) * width], 16).unwrap())
        .collect()
}

/// Parse a ToUnicode CMap's `bfchar` entries (CID -> text).
fn parse_cmap(cmap: &str) -> std::collections::HashMap<u16, String> {
    let mut map = std::collections::HashMap::new();
    let mut in_block = false;
    for line in cmap.lines() {
        match line {
            l if l.ends_with("beginbfchar") => in_block = true,
            "endbfchar" => in_block = false,
            l if in_block => {
                let mut parts = l.split(['<', '>']).filter(|p| !p.trim().is_empty());
                let cid = u16::from_str_radix(parts.next().unwrap(), 16).unwrap();
                let units: Vec<u16> = hex_pairs(parts.next().unwrap(), 4)
                    .into_iter()
                    .map(|u| u as u16)
                    .collect();
                map.insert(cid, String::from_utf16(&units).unwrap());
            }
            _ => {}
        }
    }
    map
}

/// Decoded text of every `BT ... ET` line, using each font's own ToUnicode map.
fn content_lines(objs: &[Obj]) -> Vec<Line> {
    let by_id = |id: usize| objs.iter().find(|o| o.id == id).unwrap();
    let ref_after = |dict: &str, key: &str| -> usize {
        dict.split(key)
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let page = objs
        .iter()
        .find(|o| o.dict.contains("/Type /Page "))
        .unwrap();
    let res = page
        .dict
        .split("/Font << ")
        .nth(1)
        .unwrap()
        .split(">>")
        .next()
        .unwrap();
    let mut cmaps = std::collections::HashMap::new();
    for entry in res.split("0 R").filter(|e| e.contains('/')) {
        let mut it = entry.split_whitespace();
        let name = it.next().unwrap().trim_start_matches('/').to_string();
        let type0 = by_id(it.next().unwrap().parse().unwrap());
        let uni = ref_after(&type0.dict, "/ToUnicode ");
        cmaps.insert(name, parse_cmap(&by_id(uni).text_stream()));
    }
    let mut out = Vec::new();
    for o in objs.iter().filter(|o| {
        o.stream.is_some() && !o.dict.contains("/Subtype") && !o.dict.contains("/Length1")
    }) {
        for l in o.text_stream().lines().filter(|l| l.starts_with("BT ")) {
            let (mut text, mut cids) = (String::new(), Vec::new());
            let mut font = String::new();
            let toks: Vec<&str> = l.split_whitespace().collect();
            for (i, t) in toks.iter().enumerate() {
                if t.starts_with('/') && toks.get(i + 2) == Some(&"Tf") {
                    font = t[1..].to_string();
                }
                if t.starts_with('<') && toks.get(i + 1) == Some(&"Tj") {
                    for cid in hex_pairs(&t[1..t.len() - 1], 4) {
                        let cid = cid as u16;
                        cids.push(cid);
                        text.push_str(cmaps[&font].get(&cid).map_or("\u{FFFD}", String::as_str));
                    }
                }
            }
            out.push(Line { text, cids });
        }
    }
    out
}

/// Visual text of every drawn string (decoded through the PDF's own ToUnicode maps).
fn pdf_lines(pdf: &[u8]) -> Vec<String> {
    content_lines(&parse_pdf(pdf))
        .into_iter()
        .map(|l| l.text)
        .collect()
}

fn actual_texts(objs: &[Obj]) -> Vec<String> {
    let mut out = Vec::new();
    for o in objs.iter().filter(|o| o.stream.is_some()) {
        for l in o
            .text_stream()
            .lines()
            .filter(|l| l.contains("/ActualText <FEFF"))
        {
            let hex = l.split("<FEFF").nth(1).unwrap().split('>').next().unwrap();
            let units: Vec<u16> = hex_pairs(hex, 4).into_iter().map(|u| u as u16).collect();
            out.push(String::from_utf16(&units).unwrap());
        }
    }
    out
}

fn encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n = ch
            .iter()
            .enumerate()
            .fold(0u32, |a, (i, b)| a | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= ch.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
#[ignore = "writes sample output to BOREALIS_SHEET_OUT for manual inspection"]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_SHEET_OUT") else {
        return;
    };
    let (c, snap) = fixture();
    let o = flat();
    std::fs::write(
        Path::new(&dir).join("sheet.pdf"),
        render_pdf(&c, &snap, &o).unwrap(),
    )
    .unwrap();
    let fill = SheetOptions {
        form_fillable: true,
        cards: borealis_sheet::CardKinds {
            spell: true,
            item: true,
            attack: true,
            feature: true,
        },
        ..flat()
    };
    std::fs::write(
        Path::new(&dir).join("fillable.pdf"),
        render_pdf(&c, &snap, &fill).unwrap(),
    )
    .unwrap();
    let flat_cards = SheetOptions {
        form_fillable: false,
        ..fill
    };
    std::fs::write(
        Path::new(&dir).join("cards.pdf"),
        render_pdf(&c, &snap, &flat_cards).unwrap(),
    )
    .unwrap();
    std::fs::write(
        Path::new(&dir).join("sheet.html"),
        render_html(&c, &snap, &o).unwrap(),
    )
    .unwrap();
}

// ---- CHARACTER SHEET settings (sheet-3) ----

fn html_with(f: impl FnOnce(&mut SheetOptions)) -> String {
    let (c, snap) = fixture();
    let mut o = flat();
    f(&mut o);
    render_html(&c, &snap, &o).expect("html")
}

#[test]
fn swap_ability_positions_puts_the_modifier_in_the_big_slot() {
    let normal = html_with(|_| {});
    let swapped = html_with(|o| o.swap_ability_positions = true);
    let mod_in = |h: &str, class: &str| {
        h.contains(&format!(
            "class=\"{class}\"><span class=\"fv \" data-field=\"abil.str.mod\""
        ))
    };
    assert!(mod_in(&normal, "mod") && !mod_in(&normal, "score"));
    assert!(mod_in(&swapped, "score") && !mod_in(&swapped, "mod"));
}

#[test]
fn style_formatting_is_part_of_the_html_options() {
    let styled = html_with(|_| {});
    let plain = html_with(|o| o.style_formatting = false);
    assert!(styled.contains("Features"));
    assert!(plain.contains("Features"));
}

#[test]
fn non_prepared_spells_are_listed_only_when_enabled() {
    let (_, snap) = fixture();
    let caster = &snap.spellcasting[0];
    let hidden = caster
        .spells
        .iter()
        .find(|s| s.level > 0 && !s.prepared && !s.always_prepared)
        .expect("fixture has an unprepared spell");
    assert!(caster.prepare);
    let on = html_with(|_| {});
    let off = html_with(|o| o.include_non_prepared_spells = false);
    assert!(on.contains(&esc_cell(&hidden.name)));
    assert!(!off.contains(&esc_cell(&hidden.name)));
    if let Some(cantrip) = caster.spells.iter().find(|s| s.level == 0) {
        assert!(off.contains(&esc_cell(&cantrip.name)));
    }
}

#[test]
fn options_reach_the_pdf_too() {
    let (c, snap) = fixture();
    let base = render_pdf(&c, &snap, &flat()).unwrap();
    let o = SheetOptions {
        cards: borealis_sheet::CardKinds {
            spell: true,
            item: true,
            attack: true,
            feature: true,
        },
        legacy_spellcasting: true,
        swap_ability_positions: true,
        ..flat()
    };
    let with = render_pdf(&c, &snap, &o).unwrap();
    assert!(count_pages(&with) > count_pages(&base));
    let lines = pdf_lines(&with).join("\n");
    assert!(lines.to_lowercase().contains("(basic)"), "{lines}");
}

fn pdf_for(c: &Character, snap: &CharacterSnapshot, f: impl FnOnce(&mut SheetOptions)) -> Vec<u8> {
    let mut o = flat();
    f(&mut o);
    render_pdf(c, snap, &o).unwrap()
}

fn pages_for(c: &Character, snap: &CharacterSnapshot, f: impl FnOnce(&mut SheetOptions)) -> usize {
    count_pages(&pdf_for(c, snap, f))
}

#[test]
fn each_card_group_adds_pages_after_the_new_sheet() {
    let (c, snap) = fixture();
    let base = pages_for(&c, &snap, |_| {});
    for (name, set) in [
        (
            "spell",
            (|o: &mut SheetOptions| o.cards.spell = true) as fn(&mut SheetOptions),
        ),
        ("item", |o| o.cards.item = true),
        ("feature", |o| o.cards.feature = true),
    ] {
        assert!(pages_for(&c, &snap, set) > base, "{name} cards add a page");
    }
}

#[test]
fn spell_card_text_comes_from_supplied_content() {
    let (c, snap) = fixture();
    let sp = snap.spellcasting[0]
        .spells
        .iter()
        .find(|s| s.known || s.prepared)
        .unwrap();
    let mut content = borealis_sheet::SheetContent::default();
    content.spells.insert(
        sp.id.clone(),
        borealis_sheet::CardText {
            subtitle: "Evocation".into(),
            body: "A very specific description".into(),
            source: "Test Book".into(),
        },
    );
    let o = SheetOptions {
        cards: borealis_sheet::CardKinds {
            spell: true,
            ..Default::default()
        },
        ..flat()
    };
    let pdf = borealis_sheet::render_pdf_with(&c, &snap, &o, &content).unwrap();
    let text = pdf_lines(&pdf).join("\n");
    assert!(text.contains("A very specific description") && text.contains("Test Book"));
}

#[test]
fn item_cards_flow_nine_to_a_page() {
    let (c, mut snap) = fixture();
    let base = pages_for(&c, &snap, |_| {});
    let template = snap.inventory.items[0].clone();
    snap.inventory.items = (0..19).map(|_| template.clone()).collect();
    let cards = |snap: &CharacterSnapshot| {
        pages_for(&c, snap, |o| {
            o.cards.item = true;
            o.pages.inventory = false;
        }) - pages_for(&c, snap, |o| o.pages.inventory = false)
    };
    let _ = base;
    assert_eq!(cards(&snap), 3);
    snap.inventory.items.truncate(18);
    assert_eq!(cards(&snap), 2, "no empty page after a full grid");
    snap.inventory.items.truncate(1);
    snap.inventory.items[0].show_card = false;
    assert_eq!(cards(&snap), 0, "show_card off removes the card");
}

#[test]
fn legacy_spell_page_replaces_the_spell_pages_and_moves_rows_to_full_levels() {
    let (c, mut snap) = fixture();
    let template = snap.spellcasting[0].spells.first().unwrap().clone();
    snap.spellcasting[0].spells = (0..10)
        .map(|i| borealis_engine::snapshot::SpellEntry {
            id: format!("x{i}"),
            name: format!("Cantrip {i}"),
            level: 0,
            ..template.clone()
        })
        .collect();
    let text = |snap: &CharacterSnapshot| {
        pdf_lines(&pdf_for(&c, snap, |o| o.legacy_spellcasting = true)).join("\n")
    };
    let legacy = text(&snap);
    assert!(
        legacy.contains("Cantrip 9") && !legacy.contains("not shown"),
        "ten cantrips take blank rows from the higher levels"
    );
    snap.spellcasting[0].spells.truncate(8);
    let full = text(&snap);
    assert!(full.contains("Cantrip 7") && !full.contains("not shown"));
}

/// Structural PDF check: every xref entry points at its `N 0 obj`, trailer and startxref agree.
fn assert_pdf_structure(pdf: &[u8]) {
    let starts = |at: usize, pat: &str| pdf[at..].starts_with(pat.as_bytes());
    let tail = String::from_utf8_lossy(&pdf[pdf.len() - 40..]).to_string();
    let at: usize = tail
        .split("startxref\n")
        .nth(1)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(starts(at, "xref\n"));
    let table = String::from_utf8_lossy(&pdf[at..]).to_string();
    let mut lines = table.lines().skip(1);
    let count: usize = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    lines.next(); // free entry
    for id in 1..count {
        let off: usize = lines
            .next()
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(off > 0, "object {id} was never written");
        assert!(starts(off, &format!("{id} 0 obj")), "xref of object {id}");
    }
}

fn fillable_pdf(f: impl FnOnce(&mut SheetOptions)) -> Vec<u8> {
    let (c, snap) = fixture();
    let mut o = SheetOptions {
        form_fillable: true,
        ..flat()
    };
    f(&mut o);
    render_pdf(&c, &snap, &o).unwrap()
}

#[test]
fn form_fillable_pdf_has_named_text_widgets() {
    let (c, snap) = fixture();
    let flat = render_pdf(&c, &snap, &flat()).unwrap();
    let flat_text = String::from_utf8_lossy(&flat);
    assert!(!flat_text.contains("/AcroForm") && !flat_text.contains("/Widget"));
    assert_pdf_structure(&flat);

    let pdf = fillable_pdf(|o| {
        o.cards = borealis_sheet::CardKinds {
            item: true,
            ..Default::default()
        };
    });
    assert_pdf_structure(&pdf);
    let text = String::from_utf8_lossy(&pdf);
    // Play fields are widgets with appearance streams; prefilled values are printed (PDFKit
    // ignores /AP). /DA names standard-14 faces any viewer can type in.
    assert!(text.contains("/AcroForm") && text.contains("/AP << /N "));
    assert!(!text.contains("/NeedAppearances"));
    let widgets = text.matches("/Subtype /Widget").count();
    assert!(widgets > 20, "{widgets} widgets");
    assert!(text.matches("/FT /Tx").count() > 5, "text widgets");
    assert!(text.contains("/FT /Btn"), "check boxes");
    assert!(
        text.contains("/T (char)") || text.contains("/T (abil)"),
        "semantic names"
    );
}

#[test]
fn form_fillable_matches_the_flat_sheet_page_for_page() {
    let (c, snap) = fixture();
    let flat = render_pdf(&c, &snap, &flat()).unwrap();
    assert_eq!(count_pages(&fillable_pdf(|_| {})), count_pages(&flat));
}

/// Flat (non-fillable) options: the default is fillable.
fn flat() -> SheetOptions {
    SheetOptions {
        form_fillable: false,
        ..SheetOptions::default()
    }
}
