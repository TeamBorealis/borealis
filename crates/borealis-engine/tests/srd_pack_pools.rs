//! det-src-6: the last st3-golden findings, on the bundled SRD packs. SRD 5.1 usage pools
//! (Relentless Endurance, Breath Weapon, the Paladin's Channel Divinity) and the 2014 Tiefling's
//! Infernal Legacy free casts.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use std::path::Path;

fn engine(pack: &str) -> Option<Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let dir = root.join(pack);
    if !dir.join("pack.json").is_file() {
        eprintln!("SKIPPED: content/{pack} not present");
        return None;
    }
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&dir))
        .expect("packs load")
        .build();
    Some(Engine::new(&store))
}

/// Fill the first open slot named `slot` with the option named `pick`.
fn choose(e: &Engine, s: &CharacterState, slot: &str, pick: &str) -> CharacterState {
    let snap = e.derive(s);
    let sl = snap
        .slots
        .iter()
        .find(|k| {
            k.name == slot
                && k.chosen.is_none()
                && k.rejected.is_none()
                && k.options.iter().any(|o| o.name == pick)
        })
        .unwrap_or_else(|| panic!("no open slot {slot} offering {pick}"));
    let id = sl
        .options
        .iter()
        .find(|o| o.name == pick)
        .unwrap()
        .id
        .clone();
    commands::select(e, s, &sl.key, Choice::Element(id))
        .unwrap_or_else(|err| panic!("{slot}/{pick}: {err}"))
        .state
}

fn level_to(e: &Engine, mut s: CharacterState, from: u32, to: u32) -> CharacterState {
    for _ in from..to {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    s
}

fn hero(e: &Engine, ruleset: Ruleset, picks: &[(&str, &str)], levels: u32) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    for (slot, pick) in picks {
        s = choose(e, &s, slot, pick);
    }
    level_to(e, s, 1, levels)
}

fn pool(snap: &CharacterSnapshot, name: &str) -> Option<(i32, Vec<String>)> {
    snap.resources
        .iter()
        .find(|r| r.name == name)
        .map(|r| (r.max, r.recharge.clone()))
}

fn short_rest() -> Vec<String> {
    vec!["short_rest".into(), "long_rest".into()]
}

#[test]
fn relentless_endurance_is_once_per_long_rest() {
    let Some(e) = engine("srd-5.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2014,
        &[("Race", "Half-Orc"), ("Class", "Barbarian")],
        1,
    );
    let snap = e.derive(&s);
    assert_eq!(
        pool(&snap, "Relentless Endurance"),
        Some((1, vec!["long_rest".into()]))
    );
}

#[test]
fn breath_weapon_is_once_per_short_rest() {
    let Some(e) = engine("srd-5.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2014,
        &[
            ("Race", "Dragonborn"),
            ("Draconic Ancestry", "Draconic Ancestry (Red)"),
            ("Class", "Fighter"),
        ],
        1,
    );
    let snap = e.derive(&s);
    assert_eq!(pool(&snap, "Breath Weapon"), Some((1, short_rest())));
}

#[test]
fn a_devotion_paladin_has_one_channel_divinity_per_short_rest() {
    let Some(e) = engine("srd-5.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2014,
        &[("Race", "Human"), ("Class", "Paladin")],
        2,
    );
    assert_eq!(pool(&e.derive(&s), "Channel Divinity"), None, "no oath yet");
    let s = level_to(&e, s, 2, 3);
    let s = choose(&e, &s, "Sacred Oath", "Oath of Devotion");
    let snap = e.derive(&s);
    assert_eq!(pool(&snap, "Channel Divinity"), Some((1, short_rest())));
    let s = level_to(&e, s, 3, 20);
    assert_eq!(
        pool(&e.derive(&s), "Channel Divinity"),
        Some((1, short_rest())),
        "SRD 5.1 Paladin: once per short or long rest at every level"
    );
}

#[test]
fn infernal_legacy_casts_hellish_rebuke_at_3_and_darkness_at_5() {
    let Some(e) = engine("srd-5.1") else { return };
    let s = hero(
        &e,
        Ruleset::Dnd2014,
        &[("Race", "Tiefling"), ("Class", "Fighter")],
        2,
    );
    assert!(
        e.derive(&s).free_casts.is_empty(),
        "nothing before 3rd level"
    );
    let s = level_to(&e, s, 2, 3);
    let snap = e.derive(&s);
    let names: Vec<&str> = snap.free_casts.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Hellish Rebuke"]);
    let s = level_to(&e, s, 3, 5);
    let snap = e.derive(&s);
    let rebuke = snap
        .free_casts
        .iter()
        .find(|f| f.name == "Hellish Rebuke")
        .expect("Hellish Rebuke");
    assert_eq!(rebuke.uses, Some(1));
    assert_eq!(rebuke.recharge, "long_rest");
    assert_eq!(rebuke.cast_level, Some(2), "cast as a 2nd-level spell");
    let darkness = snap
        .free_casts
        .iter()
        .find(|f| f.name == "Darkness")
        .expect("Darkness");
    assert_eq!(darkness.uses, Some(1));
    assert_eq!(darkness.recharge, "long_rest");
    let tiefling = snap
        .spellcasting
        .iter()
        .find(|c| c.name == "Tiefling")
        .expect("Tiefling block");
    for spell in ["Thaumaturgy", "Hellish Rebuke", "Darkness"] {
        assert!(
            tiefling.spells.iter().any(|x| x.name == spell),
            "{spell} stays in the Tiefling block"
        );
    }
}
