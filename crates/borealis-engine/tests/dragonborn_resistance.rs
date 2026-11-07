//! Issue #26: a Dragonborn's defences list only the resistance of the chosen Draconic Ancestry,
//! never every ancestry's resistance, in both editions. The 2014 trait grants each resistance
//! under a requirement on the ancestry; the 2024 ancestry options grant it themselves.
mod common;

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::{Choice, Engine};
use common::{class, each_variant, Hero, LEVEL1};
use std::path::Path;
use std::sync::OnceLock;

fn resistances(s: &CharacterSnapshot) -> Vec<(String, String)> {
    s.defenses
        .resistances
        .iter()
        .map(|r| (r.name.clone(), r.source.clone()))
        .collect()
}

#[test]
fn dragonborn_2014_resists_only_the_chosen_ancestry() {
    each_variant("dragonborn_2014_resistance", |e, _| {
        let mut h = Hero::new(e, true);
        h.race_class("ID_RACE_DRAGONBORN", &class("Fighter"));
        assert_eq!(
            resistances(&h.snap()),
            Vec::<(String, String)>::new(),
            "no ancestry chosen yet: no resistance"
        );
        h.pick_any("Racial Trait", "ID_RACIAL_TRAIT_DRACONIC_ANCESTRY_RED");
        assert_eq!(
            resistances(&h.snap()),
            [("fire".to_string(), "Damage Resistance".to_string())]
        );
    });
}

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

#[test]
fn dragonborn_2024_resists_only_the_chosen_ancestry() {
    borealis_engine::invariants::enable_for_thread();
    let e = engine_2024();
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    let key = slot_key(&e.derive(&s), LEVEL1, "Race", 1).expect("species slot");
    s = commands::select(
        e,
        &s,
        &key,
        Choice::Element("ID_2024_SPECIES_DRAGONBORN".into()),
    )
    .expect("species")
    .state;
    assert!(resistances(&e.derive(&s)).is_empty());
    let snap = e.derive(&s);
    let key = snap
        .slots
        .iter()
        .find(|sl| sl.ty == "Species Trait Option" && sl.chosen.is_none())
        .expect("ancestry slot")
        .key
        .clone();
    s = commands::select(
        e,
        &s,
        &key,
        Choice::Element("ID_2024_OPTION_DRACONIC_ANCESTRY_SILVER".into()),
    )
    .expect("ancestry")
    .state;
    let got = resistances(&e.derive(&s));
    assert_eq!(got.len(), 1, "one resistance, got {got:?}");
    assert_eq!(got[0].0, "cold");
}
