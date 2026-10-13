//! Wizard spellbook selection slots in both editions (SRD 5.1 and SRD 5.2.1): six level-1 spells
//! at Wizard level 1, two more per Wizard level after that, cantrips by the class table, and the
//! picks fill the spellbook the Wizard prepares from.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::{CharacterSnapshot, SlotSnapshot};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

const WIZARD_2014: &str = "ID_WOTC_PHB_CLASS_WIZARD";
const WIZARD_2024: &str = "ID_2024_CLASS_WIZARD";

struct Edition {
    pack: &'static str,
    ruleset: Ruleset,
    class: &'static str,
    book_prefix: &'static str,
    cantrip_prefix: &'static str,
}

const ED_2014: Edition = Edition {
    pack: "srd-5.1",
    ruleset: Ruleset::Dnd2014,
    class: WIZARD_2014,
    book_prefix: "Spellbook (Wizard",
    cantrip_prefix: "Cantrip (Wizard",
};

const ED_2024: Edition = Edition {
    pack: "srd-5.2.1",
    ruleset: Ruleset::Dnd2024,
    class: WIZARD_2024,
    book_prefix: "Spellbook (Wizard",
    cantrip_prefix: "Cantrip (Wizard",
};

fn engine(pack: &str) -> &'static Engine {
    static E14: OnceLock<Engine> = OnceLock::new();
    static E24: OnceLock<Engine> = OnceLock::new();
    let cell = if pack == "srd-5.1" { &E14 } else { &E24 };
    cell.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join(pack)))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

fn wizard(ed: &Edition, level: u32) -> CharacterState {
    let e = engine(ed.pack);
    let mut s = commands::new_character(e, ed.ruleset);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(ed.class.into()))
        .expect("class")
        .state;
    level_to(ed, s, level)
}

fn level_to(ed: &Edition, mut s: CharacterState, level: u32) -> CharacterState {
    let e = engine(ed.pack);
    while e.derive(&s).level < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn slots_named<'a>(snap: &'a CharacterSnapshot, prefix: &str) -> Vec<&'a SlotSnapshot> {
    snap.slots
        .iter()
        .filter(|s| s.name.starts_with(prefix) && s.spellcasting.as_deref() == Some("Wizard"))
        .collect()
}

fn option_levels(slot: &SlotSnapshot) -> BTreeSet<u32> {
    slot.options.iter().filter_map(|o| o.level).collect()
}

/// Fill every open spellbook slot with a distinct spell.
fn fill_book(ed: &Edition, mut s: CharacterState) -> CharacterState {
    let e = engine(ed.pack);
    loop {
        let snap = e.derive(&s);
        let taken: BTreeSet<String> = snap
            .slots
            .iter()
            .filter_map(|x| x.chosen.as_ref().map(|c| c.id.clone()))
            .collect();
        let open = slots_named(&snap, ed.book_prefix)
            .into_iter()
            .find(|x| x.chosen.is_none());
        let Some(slot) = open else { return s };
        let pick = slot
            .options
            .iter()
            .find(|o| !taken.contains(&o.id))
            .expect("a free spellbook option")
            .id
            .clone();
        s = commands::select(e, &s, &slot.key.clone(), Choice::Element(pick))
            .expect("spellbook pick")
            .state;
    }
}

fn assert_slot_counts(ed: &Edition, level: u32, book: usize, cantrips: usize) {
    let snap = engine(ed.pack).derive(&wizard(ed, level));
    let book_slots = slots_named(&snap, ed.book_prefix);
    assert_eq!(
        book_slots.len(),
        book,
        "{} level {level} spellbook",
        ed.pack
    );
    assert_eq!(
        slots_named(&snap, ed.cantrip_prefix).len(),
        cantrips,
        "{} level {level} cantrips",
        ed.pack
    );
    // the level-1 picks are level-1 spells; later picks follow the slot levels
    let first: Vec<_> = book_slots.iter().filter(|s| s.level == 1).collect();
    assert_eq!(first.len(), 6);
    assert!(first
        .iter()
        .all(|s| option_levels(s) == BTreeSet::from([1]) && !s.replaceable));
    let top = match level {
        1..=2 => 1,
        3..=4 => 2,
        _ => 3,
    };
    let highest = book_slots.iter().max_by_key(|s| s.level).expect("a slot");
    assert_eq!(
        option_levels(highest),
        (1..=top).collect::<BTreeSet<u32>>(),
        "{} level {level} highest spellbook slot",
        ed.pack
    );
}

#[test]
fn wizard_2014_level_1_and_5_spellbook_and_cantrip_slots() {
    assert_slot_counts(&ED_2014, 1, 6, 3);
    assert_slot_counts(&ED_2014, 5, 14, 4);
}

#[test]
fn wizard_2024_level_1_and_5_spellbook_and_cantrip_slots() {
    assert_slot_counts(&ED_2024, 1, 6, 3);
    assert_slot_counts(&ED_2024, 5, 14, 4);
}

#[test]
fn level_up_adds_two_spellbook_slots_in_both_editions() {
    for ed in [&ED_2014, &ED_2024] {
        let e = engine(ed.pack);
        for level in 1..6 {
            let before = slots_named(&e.derive(&wizard(ed, level)), ed.book_prefix).len();
            let after = slots_named(&e.derive(&wizard(ed, level + 1)), ed.book_prefix).len();
            assert_eq!(after, before + 2, "{} {level} -> {}", ed.pack, level + 1);
        }
    }
}

#[test]
fn wizard_2024_picks_fill_the_spellbook_and_can_be_prepared() {
    let e = engine(ED_2024.pack);
    let s = fill_book(&ED_2024, wizard(&ED_2024, 1));
    let snap = e.derive(&s);
    let c = snap.caster("Wizard").expect("wizard caster");
    assert_eq!(c.prepare_source, "spellbook");
    assert_eq!(c.prepare_limit, Some(4));
    let book = c.spellbook.as_ref().expect("spellbook");
    assert_eq!((book.capacity, book.spells.len()), (6, 6));
    assert!(book.spells.iter().all(|s| s.level == 1 && s.known));
    assert!(book
        .candidates
        .iter()
        .all(|cand| !book.spells.iter().any(|s| s.id == cand.id)));
    // the book is full: copying a seventh spell is refused
    let extra = &book.candidates[0].id;
    assert!(commands::set_spellbook(e, &s, "Wizard", extra, true).is_err());
    // four of the six book spells can be prepared, the fifth is refused
    let ids: Vec<String> = book.spells.iter().map(|s| s.id.clone()).collect();
    let mut s = s;
    for id in &ids[..4] {
        s = commands::set_prepared(e, &s, "Wizard", id, true)
            .expect("prepare from book")
            .state;
    }
    assert!(commands::set_prepared(e, &s, "Wizard", &ids[4], true).is_err());
    assert_eq!(e.derive(&s).caster("Wizard").unwrap().prepared_count, 4);

    // level 5: 14 spells in the book after the level-up picks
    let s = fill_book(&ED_2024, level_to(&ED_2024, s, 5));
    let snap = e.derive(&s);
    let book = snap.caster("Wizard").unwrap().spellbook.as_ref().unwrap();
    assert_eq!((book.capacity, book.spells.len()), (14, 14));
    assert!(book.spells.iter().all(|s| (1..=3).contains(&s.level)));
}

#[test]
fn wizard_2014_picks_are_known_and_preparable() {
    let e = engine(ED_2014.pack);
    let s = fill_book(&ED_2014, wizard(&ED_2014, 1));
    let snap = e.derive(&s);
    let c = snap.caster("Wizard").expect("wizard caster");
    let book: Vec<String> = c
        .spells
        .iter()
        .filter(|s| s.level == 1 && s.known)
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(book.len(), 6);
    let s = commands::set_prepared(e, &s, "Wizard", &book[0], true)
        .expect("prepare a book spell")
        .state;
    assert_eq!(e.derive(&s).caster("Wizard").unwrap().prepared_count, 1);
}
