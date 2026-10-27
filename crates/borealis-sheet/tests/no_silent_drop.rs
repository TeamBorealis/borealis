//! No-silent-drop property test: characters with growing attack/feature/item/spell lists must show
//! every name somewhere in the PDF text (overflow pages included), in both editions.

mod common;

use borealis_character::{Character, ManualAttack};
use borealis_engine::snapshot::{ElementRef, FeatureDetail, ItemSnapshot, SpellEntry};
use borealis_engine::CharacterSnapshot;
use borealis_sheet::{render_pdf, SheetOptions};
use lopdf::Document;

/// (attacks, features, items, spells) per step; deterministic, growing to the stated maxima.
const STEPS: [(usize, usize, usize, usize); 5] = [
    (0, 0, 0, 0),
    (1, 3, 5, 10),
    (6, 20, 40, 50),
    (12, 50, 80, 100),
    (20, 80, 120, 150),
];

/// Letters-only tag so text extraction cannot split or merge it: `Zq` + base-26 index + kind suffix.
fn tag(kind: &str, i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap()));
        n /= 26;
        if n == 0 {
            break;
        }
    }
    format!("Zq{s}{kind}")
}

fn grow(
    base: &(Character, CharacterSnapshot),
    n: (usize, usize, usize, usize),
) -> (Character, CharacterSnapshot, Vec<String>) {
    let (mut c, mut s) = (base.0.clone(), base.1.clone());
    let mut names = Vec::new();
    for i in 0..n.0 {
        let name = tag("atk", i);
        c.details.attacks.push(ManualAttack {
            name: name.clone(),
            range: "5 ft".into(),
            attack: "+5".into(),
            damage: "1d6".into(),
            displayed: true,
            ..ManualAttack::default()
        });
        names.push(name);
    }
    let feat = s.features.first().cloned().expect("fixture has a feature");
    for i in 0..n.1 {
        let name = tag("feat", i);
        s.features.push(FeatureDetail {
            id: format!("ID_TEST_FEATURE_{i}"),
            name: name.clone(),
            displayed: true,
            sheet_summary: Some(format!("{name}: does a thing.")),
            ..feat.clone()
        });
        names.push(name);
    }
    for i in 0..n.2 {
        let name = tag("item", i);
        s.inventory.items.push(ItemSnapshot {
            uid: format!("test-item-{i}"),
            item: ElementRef {
                id: format!("ID_TEST_ITEM_{i}"),
                name: name.clone(),
                ty: "Item".into(),
                source: "Test".into(),
            },
            name: name.clone(),
            amount: 1,
            equipped: None,
            attuned: false,
            requires_attunement: false,
            attunement_restriction: None,
            attunement_barred: false,
            proficient_by_item: false,
            equippable: false,
            weight: 1.0,
            cost_gp: 1.0,
            storage: None,
            show_card: false,
        });
        names.push(name);
    }
    let spells = &mut s.spellcasting.first_mut().expect("a caster").spells;
    for i in 0..n.3 {
        let name = tag("spl", i);
        spells.push(SpellEntry {
            id: format!("ID_TEST_SPELL_{i}"),
            name: name.clone(),
            level: u32::try_from(i % 10).unwrap(),
            prepared: true,
            always_prepared: false,
            known: true,
            damage: None,
            granted_by: None,
        });
        names.push(name);
    }
    (c, s, names)
}

/// Flattened: values are page text (a fillable sheet keeps them in field values instead).
fn flat() -> SheetOptions {
    SheetOptions {
        form_fillable: false,
        ..SheetOptions::default()
    }
}

fn all_text(pdf: &[u8]) -> String {
    let doc = Document::load_mem(pdf).expect("parses");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    pages
        .iter()
        .filter_map(|p| doc.extract_text(&[*p]).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn run(base: &(Character, CharacterSnapshot), label: &str) {
    for step in STEPS {
        let (c, s, names) = grow(base, step);
        let pdf = render_pdf(&c, &s, &flat()).unwrap_or_else(|e| panic!("{label} {step:?}: {e}"));
        let text = all_text(&pdf);
        let missing: Vec<&String> = names
            .iter()
            .filter(|n| !text.contains(n.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "{label} {step:?}: {} of {} names silently dropped, e.g. {:?}",
            missing.len(),
            names.len(),
            &missing[..missing.len().min(8)]
        );
    }
}

#[test]
fn growing_lists_never_drop_a_name_2014() {
    run(&common::wizard_2014(), "2014");
}

#[test]
fn growing_lists_never_drop_a_name_2024() {
    run(&common::wizard_2024(), "2024");
}

#[test]
fn tags_are_unique_and_letters_only() {
    let all: std::collections::BTreeSet<String> = (0..200).map(|i| tag("x", i)).collect();
    assert_eq!(all.len(), 200);
    assert!(all
        .iter()
        .all(|t| t.chars().all(|c| c.is_ascii_alphabetic())));
}

/// Spell levels (one entry per added spell) for the basic page: lopsided lists that overrun the
/// spec capacity of one level while others stay empty, and one list too long for any layout.
fn legacy_lists() -> Vec<Vec<u32>> {
    let lop = |pairs: &[(u32, usize)]| {
        pairs
            .iter()
            .flat_map(|&(l, n)| std::iter::repeat_n(l, n))
            .collect::<Vec<u32>>()
    };
    vec![
        lop(&[(0, 9), (1, 14)]),
        lop(&[(0, 12), (1, 30), (2, 16)]),
        lop(&[(0, 10), (1, 70)]),
        lop(&[(0, 20), (1, 40), (2, 30), (3, 20), (4, 15), (5, 10)]),
        lop(&[(1, 300)]),
    ]
}

/// The "+N more spells not shown" count of the basic page (0 without the note).
fn legacy_note_count(text: &str) -> usize {
    text.split('+')
        .skip(1)
        .find_map(|s| {
            let (n, rest) = s.split_once(' ')?;
            rest.starts_with("more spell").then(|| n.parse().ok())?
        })
        .unwrap_or(0)
}

fn run_legacy(base: &(Character, CharacterSnapshot), label: &str) {
    let opts = SheetOptions {
        legacy_spellcasting: true,
        ..flat()
    };
    for levels in legacy_lists() {
        let (c, mut s) = base.clone();
        let spells = &mut s.spellcasting.first_mut().expect("a caster").spells;
        let names: Vec<String> = levels
            .iter()
            .enumerate()
            .map(|(i, &level)| {
                let name = tag("spl", i);
                spells.push(SpellEntry {
                    id: format!("ID_TEST_SPELL_{i}"),
                    name: name.clone(),
                    level,
                    prepared: true,
                    always_prepared: false,
                    known: true,
                    damage: None,
                    granted_by: None,
                });
                name
            })
            .collect();
        let text = all_text(&render_pdf(&c, &s, &opts).expect("renders"));
        let missing = names.iter().filter(|n| !text.contains(n.as_str())).count();
        let noted = legacy_note_count(&text);
        let at = format!("{label}, {} spells", names.len());
        if names.len() <= 100 {
            assert_eq!(missing, 0, "{at}: the basic page dropped spells");
        }
        assert!(
            missing <= noted,
            "{at}: {missing} spells missing but the note counts {noted}"
        );
    }
}

#[test]
fn basic_spell_page_redistributes_rows_instead_of_dropping_2014() {
    run_legacy(&common::wizard_2014(), "2014");
}

#[test]
fn basic_spell_page_redistributes_rows_instead_of_dropping_2024() {
    run_legacy(&common::wizard_2024(), "2024");
}
