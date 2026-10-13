//! Card pages and the basic spellcasting page of the new Borealis sheet.

mod common;

use borealis_engine::CharacterSnapshot;
use borealis_sheet::{
    render_html_with, render_pdf_with, CardBreaks, CardKinds, CardText, PageSelection,
    SheetContent, SheetOptions, SheetTheme,
};
use lopdf::Document;

/// Sheet pages off: only cards and the basic spell page can appear.
fn bare() -> SheetOptions {
    SheetOptions {
        pages: PageSelection {
            core: false,
            spellcasting: false,
            inventory: false,
            story: false,
        },
        form_fillable: false,
        ..SheetOptions::default()
    }
}

fn doc(pdf: &[u8]) -> Document {
    Document::load_mem(pdf).expect("lopdf parses the PDF")
}

fn pages(pdf: &[u8]) -> usize {
    doc(pdf).get_pages().len()
}

fn text(pdf: &[u8]) -> String {
    let d = doc(pdf);
    let all: Vec<u32> = d.get_pages().keys().copied().collect();
    d.extract_text(&all).unwrap_or_default()
}

fn page_content(d: &Document, page: u32) -> String {
    String::from_utf8_lossy(&d.get_page_content(d.get_pages()[&page])).into_owned()
}

/// Spell ids that get a card (known or prepared, once each) and item ids with a card.
fn card_ids(snap: &CharacterSnapshot) -> (Vec<String>, Vec<String>) {
    let mut spells: Vec<String> = Vec::new();
    for c in &snap.spellcasting {
        for s in c.spells.iter().filter(|s| s.known || s.prepared) {
            if !spells.contains(&s.id) {
                spells.push(s.id.clone());
            }
        }
    }
    let mut items: Vec<String> = Vec::new();
    for i in snap.inventory.items.iter().filter(|i| i.show_card) {
        if !items.contains(&i.item.id) {
            items.push(i.item.id.clone());
        }
    }
    (spells, items)
}

/// Content whose card bodies are unique markers `SPELLBODY<id>` / `ITEMBODY<id>`.
fn marked_content(snap: &CharacterSnapshot) -> SheetContent {
    let (spells, items) = card_ids(snap);
    let mut content = SheetContent::default();
    for id in spells {
        content.spells.insert(
            id.clone(),
            CardText {
                subtitle: "School".into(),
                body: format!("SPELLBODY{id}"),
                source: "Test Book".into(),
            },
        );
    }
    for id in items {
        content.items.insert(
            id.clone(),
            CardText {
                subtitle: "Gear".into(),
                body: format!("ITEMBODY{id}"),
                source: "Test Book".into(),
            },
        );
    }
    content
}

fn render(o: &SheetOptions, snap: &CharacterSnapshot, content: &SheetContent) -> Vec<u8> {
    let (c, _) = common::cleric_2014();
    render_pdf_with(&c, snap, o, content).unwrap()
}

fn per_page(n: usize) -> usize {
    n.div_ceil(9)
}

#[test]
fn card_groups_add_pages_and_breaks_change_the_count() {
    let (_, snap) = common::cleric_2014();
    let (spells, items) = card_ids(&snap);
    assert!(spells.len() > 1 && !items.is_empty(), "fixture has cards");
    let content = marked_content(&snap);
    let cards = |spell, item, feature| CardKinds {
        spell,
        item,
        attack: false,
        feature,
    };
    let with = |kinds, new_page_item| SheetOptions {
        cards: kinds,
        cards_new_page: CardBreaks {
            item: new_page_item,
            attack: false,
            feature: false,
        },
        ..bare()
    };
    let n = |o: &SheetOptions| pages(&render(o, &snap, &content));
    assert!(render_pdf_with(&common::cleric_2014().0, &snap, &bare(), &content).is_err());
    assert_eq!(
        n(&with(cards(true, false, false), true)),
        per_page(spells.len())
    );
    assert_eq!(
        n(&with(cards(false, true, false), true)),
        per_page(items.len())
    );
    let flowing = n(&with(cards(true, true, false), false));
    let broken = n(&with(cards(true, true, false), true));
    assert_eq!(flowing, per_page(spells.len() + items.len()));
    assert_eq!(broken, per_page(spells.len()) + per_page(items.len()));
}

#[test]
fn every_card_appears_exactly_once() {
    let (_, snap) = common::cleric_2014();
    let content = marked_content(&snap);
    let o = SheetOptions {
        cards: CardKinds {
            spell: true,
            item: true,
            ..CardKinds::default()
        },
        ..bare()
    };
    let t = text(&render(&o, &snap, &content));
    let (spells, items) = card_ids(&snap);
    for id in &spells {
        assert_eq!(t.matches(&format!("SPELLBODY{id}")).count(), 1, "{id}");
    }
    for id in &items {
        assert_eq!(t.matches(&format!("ITEMBODY{id}")).count(), 1, "{id}");
    }
}

#[test]
fn cards_are_numbered_with_the_rest_of_the_sheet() {
    let (c, snap) = common::cleric_2014();
    let o = SheetOptions {
        cards: CardKinds {
            spell: true,
            ..CardKinds::default()
        },
        form_fillable: false,
        ..SheetOptions::default()
    };
    let pdf = render_pdf_with(&c, &snap, &o, &marked_content(&snap)).unwrap();
    let d = doc(&pdf);
    let n = d.get_pages().len() as u32;
    assert!(d
        .extract_text(&[n])
        .unwrap()
        .contains(&format!("Page {n} of {n}")));
}

#[test]
fn fillable_cards_print_their_text_on_two_pages() {
    let (_, snap) = common::cleric_2014();
    let mut content = marked_content(&snap);
    // Enough long cards for a second card page.
    let template = snap.inventory.items[0].clone();
    let mut snap = snap;
    snap.inventory.items = (0..11).map(|_| template.clone()).collect();
    content
        .items
        .insert(template.item.id.clone(), CardText::default());
    let o = SheetOptions {
        cards: CardKinds {
            item: true,
            ..CardKinds::default()
        },
        form_fillable: true,
        ..bare()
    };
    let pdf = render(&o, &snap, &content);
    let s = String::from_utf8_lossy(&pdf).into_owned();
    assert_eq!(pages(&pdf), 2);
    // Card text is prefilled, so it is printed (PDFKit ignores a text widget's /AP) and the
    // card pages carry no text widgets; ids stay unique per page for any blank part.
    assert!(!s.contains("/T (title)") && !s.contains("/T (title@2)"));
    assert!(!s.contains("/T (title@3)"));
}

#[test]
fn ink_cards_and_basic_page_use_greys_only() {
    let (_, snap) = common::cleric_2014();
    let o = SheetOptions {
        theme: SheetTheme::Ink,
        cards: CardKinds {
            spell: true,
            item: true,
            feature: true,
            attack: true,
        },
        legacy_spellcasting: true,
        pages: PageSelection {
            core: false,
            spellcasting: true,
            inventory: false,
            story: false,
        },
        ..bare()
    };
    let d = doc(&render(&o, &snap, &marked_content(&snap)));
    for page in d.get_pages().keys() {
        let content = page_content(&d, *page);
        for line in content.lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            for (i, tok) in t.iter().enumerate() {
                if matches!(*tok, "rg" | "RG") && i >= 3 {
                    let v: Vec<f32> = t[i - 3..i].iter().filter_map(|x| x.parse().ok()).collect();
                    assert!(
                        v.len() == 3 && v[0] == v[1] && v[1] == v[2],
                        "page {page}: {line}"
                    );
                }
                assert!(!matches!(*tok, "k" | "K"), "cmyk on page {page}");
            }
        }
    }
}

/// The cleric's first caster with `n` cantrips and nothing else.
fn with_cantrips(snap: &CharacterSnapshot, n: usize) -> CharacterSnapshot {
    let mut snap = snap.clone();
    let template = snap.spellcasting[0].spells[0].clone();
    snap.spellcasting[0].spells = (0..n)
        .map(|i| borealis_engine::snapshot::SpellEntry {
            id: format!("x{i}"),
            name: format!("Cantrip {i}"),
            level: 0,
            ..template.clone()
        })
        .collect();
    snap
}

fn basic(snap: &CharacterSnapshot) -> Vec<u8> {
    let o = SheetOptions {
        legacy_spellcasting: true,
        pages: PageSelection {
            spellcasting: true,
            ..bare().pages
        },
        ..bare()
    };
    render(&o, snap, &SheetContent::default())
}

#[test]
fn basic_page_grows_a_full_level_and_counts_only_what_cannot_fit() {
    let (_, snap) = common::cleric_2014();
    let full = text(&basic(&with_cantrips(&snap, 8)));
    assert!(full.contains("Cantrip 7") && !full.contains("not shown"));
    let over = text(&basic(&with_cantrips(&snap, 10)));
    assert!(
        over.contains("Cantrip 9") && !over.contains("not shown"),
        "{over}"
    );
    let huge = text(&basic(&with_cantrips(&snap, 400)));
    assert!(huge.contains("Cantrip 0") && huge.contains("more spells not shown"));
}

#[test]
fn basic_page_replaces_the_spell_pages_and_has_one_page_per_caster() {
    let (c, snap) = common::cleric_wizard_2014();
    let pages_with = |legacy| {
        let o = SheetOptions {
            legacy_spellcasting: legacy,
            form_fillable: true,
            pages: PageSelection {
                spellcasting: true,
                ..bare().pages
            },
            ..SheetOptions::default()
        };
        let pdf = render_pdf_with(&c, &snap, &o, &SheetContent::default()).unwrap();
        (String::from_utf8_lossy(&pdf).into_owned(), pages(&pdf))
    };
    let (basic_pdf, basic_n) = pages_with(true);
    let (new_pdf, _) = pages_with(false);
    assert_eq!(
        basic_n,
        snap.spellcasting.len(),
        "one basic page per caster"
    );
    assert!(basic_pdf.contains("/T (legacy)") && !basic_pdf.contains("/T (spell)"));
    assert!(new_pdf.contains("/T (spell)") && !new_pdf.contains("/T (legacy)"));
}

#[test]
fn html_preview_shows_cards_only_when_enabled() {
    let (c, snap) = common::cleric_2014();
    let content = marked_content(&snap);
    let html = |cards| {
        let o = SheetOptions {
            cards,
            ..SheetOptions::default()
        };
        render_html_with(&c, &snap, &o, &content).unwrap()
    };
    let off = html(CardKinds::default());
    assert!(!off.contains("class=\"card\""));
    let on = html(CardKinds {
        spell: true,
        item: true,
        ..CardKinds::default()
    });
    let (spells, items) = card_ids(&snap);
    assert_eq!(
        on.matches("<article class=\"card\">").count(),
        spells.len() + items.len()
    );
    assert!(on.contains("data-page=\"cards\""));
    for id in spells.iter().chain(items.iter()) {
        assert!(on.contains(id.as_str()));
    }
}

#[test]
#[ignore = "writes sample PDFs to BOREALIS_E2E_OUT for manual inspection"]
fn write_samples() {
    let Ok(dir) = std::env::var("BOREALIS_E2E_OUT") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let long = "A very long description that should wrap over several lines of the card so the fit and clip rules show. \
        It continues with a second sentence about range, duration and the exact effect on a target.\n\nSecond paragraph of text. "
        .repeat(3);
    for (name, (c, snap)) in [
        ("w2014", common::cleric_2014()),
        ("w2024", common::wizard_2024()),
    ] {
        let mut content = marked_content(&snap);
        for t in content
            .spells
            .values_mut()
            .chain(content.items.values_mut())
        {
            t.body = long.clone();
        }
        for (tag, theme) in [("colour", SheetTheme::Colour), ("ink", SheetTheme::Ink)] {
            let o = SheetOptions {
                theme,
                cards: CardKinds {
                    spell: true,
                    item: true,
                    attack: true,
                    feature: true,
                },
                legacy_spellcasting: true,
                pages: PageSelection {
                    spellcasting: true,
                    ..bare().pages
                },
                ..bare()
            };
            let pdf = render_pdf_with(&c, &snap, &o, &content).unwrap();
            std::fs::write(dir.join(format!("{name}-{tag}.pdf")), pdf).unwrap();
        }
    }
}
