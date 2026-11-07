//! B-104: Bardic Inspiration is used "a number of times equal to your Charisma modifier (minimum
//! of once)" in both editions, so a level-1 Bard always has a pool of at least 1 use.
//! 2024 (SRD 5.2.1): `max(1, charisma:modifier)` resource. 2014 (SRD 5.1, Aurora format): the
//! `{{bardic inspiration:count}}/Long Rest` sheet usage read as a pool (`usage.rs`).

mod common;

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::{CharacterSnapshot, ResourceSnapshot};
use borealis_engine::{Choice, Engine};
use common::{class, Hero, Variant, RACE_DWARF, SUBRACE_HILL};
use std::path::Path;
use std::sync::OnceLock;

/// (Charisma score, expected uses): modifier 0, -1 and +2.
const CASES: [(i32, i32); 3] = [(10, 1), (8, 1), (14, 2)];
const CHA: usize = 5;

fn engine_2024() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

fn charisma(snap: &CharacterSnapshot) -> i32 {
    snap.abilities
        .iter()
        .find(|a| a.name == "charisma")
        .map(|a| a.base + a.additional)
        .expect("charisma")
}

fn inspiration(snap: &CharacterSnapshot) -> Option<&ResourceSnapshot> {
    snap.resources
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case("Bardic Inspiration"))
}

/// A level-1 2024 Bard (Dwarf, no background) with every score 12 but Charisma `cha`.
fn bard_2024(cha: i32) -> CharacterSnapshot {
    borealis_engine::invariants::enable_for_thread();
    let e = engine_2024();
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    for (name, id) in [
        ("Race", "ID_2024_SPECIES_DWARF"),
        ("Class", "ID_2024_CLASS_BARD"),
    ] {
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", name, 1)
            .unwrap_or_else(|| panic!("no {name} slot"));
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("select {id}: {err}"))
            .state;
    }
    let mut scores = [12; 6];
    scores[CHA] = cha;
    s.abilities.base = scores;
    e.derive(&s)
}

/// A level-1 2014 Bard (Hill Dwarf: no Charisma bonus) with every score 12 but Charisma `cha`.
fn bard_2014(e: &Engine, cha: i32) -> CharacterSnapshot {
    let mut scores = [12; 6];
    scores[CHA] = cha;
    let mut h = Hero::new(e, true);
    h.base(scores).race_class(RACE_DWARF, &class("Bard"));
    h.pick_any("Sub Race", SUBRACE_HILL);
    h.snap()
}

#[test]
fn bardic_inspiration_2024_has_at_least_one_use() {
    for (cha, uses) in CASES {
        let snap = bard_2024(cha);
        assert_eq!(charisma(&snap), cha, "Charisma score");
        let r = inspiration(&snap)
            .unwrap_or_else(|| panic!("Cha {cha}: no Bardic Inspiration in {:?}", snap.resources));
        assert_eq!((r.max, r.current), (uses, uses), "Cha {cha}");
        assert!(r.long_rest_regain_all, "Cha {cha}");
    }
}

#[test]
fn bardic_inspiration_2014_has_at_least_one_use() {
    let Some(e) = common::engine(Variant::Bundled) else {
        assert!(!common::on_ci(), "bundled packs missing");
        return;
    };
    for (cha, uses) in CASES {
        let snap = bard_2014(e, cha);
        assert_eq!(charisma(&snap), cha, "Charisma score");
        let r = inspiration(&snap)
            .unwrap_or_else(|| panic!("Cha {cha}: no Bardic Inspiration in {:?}", snap.resources));
        assert_eq!((r.max, r.current), (uses, uses), "Cha {cha}");
        assert_eq!(r.recharge, vec!["long_rest".to_string()], "Cha {cha}");
    }
}
