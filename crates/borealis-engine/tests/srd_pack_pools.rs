//! det-src-6: the last st3-golden findings, on the bundled SRD packs. SRD 5.1 usage pools
//! (Relentless Endurance, Breath Weapon, the Paladin's Channel Divinity), the 2014 Tiefling's
//! Infernal Legacy free casts, and Elemental Affinity (`damage_bonus_spell`) on cantrip rows in
//! both editions.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use std::path::Path;

const CHARISMA: usize = 5;

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

/// Charisma 16 (+3), everything else 10.
fn charismatic(e: &Engine, s: &CharacterState) -> CharacterState {
    commands::set_base_scores(e, s, [10, 10, 10, 10, 10, 16])
        .expect("scores")
        .state
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

fn damage(snap: &CharacterSnapshot, name: &str) -> String {
    snap.attacks
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no {name} row"))
        .damage
        .clone()
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

#[test]
fn elemental_affinity_adds_charisma_to_cantrips_of_the_ancestry_type_2014() {
    let Some(e) = engine("srd-5.1") else { return };
    let mut s = hero(
        &e,
        Ruleset::Dnd2014,
        &[("Race", "Human"), ("Class", "Sorcerer")],
        1,
    );
    s = charismatic(&e, &s);
    s = choose(&e, &s, "Sorcerous Origin", "Draconic Bloodline");
    s = choose(&e, &s, "Draconic Ancestor", "Dragon Ancestor (Red)");
    s = choose(&e, &s, "Cantrip (Sorcerer)", "Fire Bolt");
    s = choose(&e, &s, "Cantrip (Sorcerer)", "Ray of Frost");
    let s5 = level_to(&e, s, 1, 5);
    let snap = e.derive(&s5);
    assert_eq!(
        damage(&snap, "Fire Bolt"),
        "2d10 fire",
        "not before 6th level"
    );
    let s6 = level_to(&e, s5, 5, 6);
    let snap = e.derive(&s6);
    let cha = snap.abilities[CHARISMA].modifier;
    assert!(cha > 0, "Charisma 16 plus the race's increases");
    assert_eq!(damage(&snap, "Fire Bolt"), format!("2d10+{cha} fire"));
    assert_eq!(
        damage(&snap, "Ray of Frost"),
        "2d8 cold",
        "only the ancestry's type"
    );
}

#[test]
fn elemental_affinity_adds_charisma_to_cantrips_of_the_chosen_type_2024() {
    let Some(e) = engine("srd-5.2.1") else { return };
    let mut s = hero(
        &e,
        Ruleset::Dnd2024,
        &[("Race", "Human"), ("Class", "Sorcerer")],
        1,
    );
    s = charismatic(&e, &s);
    s = choose(&e, &s, "Cantrip (Sorcerer 1)", "Fire Bolt");
    s = choose(&e, &s, "Cantrip (Sorcerer 1)", "Ray of Frost");
    let s = level_to(&e, s, 1, 3);
    let s = choose(&e, &s, "Sorcerous Origin", "Draconic Sorcery");
    let s = level_to(&e, s, 3, 6);
    let s = choose(&e, &s, "Elemental Affinity Damage Type", "Fire");
    let snap = e.derive(&s);
    let cha = snap.abilities[CHARISMA].modifier;
    assert_eq!(damage(&snap, "Fire Bolt"), format!("2d10+{cha} fire"));
    assert_eq!(damage(&snap, "Ray of Frost"), "2d8 cold");
}

/// SRD 5.1 damage cantrips: base die and damage type, one more die at 5th, 11th and 17th level,
/// read from the pack's own spell text (the sheet no longer knows them by name).
const SRD51_CANTRIPS: &[(&str, u32, &str)] = &[
    ("Acid Splash", 6, "acid"),
    ("Chill Touch", 8, "necrotic"),
    ("Fire Bolt", 10, "fire"),
    ("Poison Spray", 12, "poison"),
    ("Produce Flame", 8, "fire"),
    ("Ray of Frost", 8, "cold"),
    ("Sacred Flame", 8, "radiant"),
    ("Shocking Grasp", 8, "lightning"),
    ("Vicious Mockery", 4, "psychic"),
];

#[test]
fn srd51_damage_cantrips_scale_at_5_11_and_17_from_their_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    if !root.join("srd-5.1/pack.json").is_file() {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return;
    }
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("srd-5.1"))
        .expect("pack loads")
        .build();
    let text = |name: &str| {
        store
            .by_type("Spell")
            .find(|s| s.element.name == name)
            .unwrap_or_else(|| panic!("{name} in SRD 5.1"))
            .element
            .description
            .clone()
    };
    let tiers = [
        (1, 1),
        (4, 1),
        (5, 2),
        (10, 2),
        (11, 3),
        (16, 3),
        (17, 4),
        (20, 4),
    ];
    for (name, die, ty) in SRD51_CANTRIPS {
        let html = text(name);
        for (level, n) in tiers {
            assert_eq!(
                borealis_engine::cantrips::scaled_damage(&html, level),
                Some(format!("{n}d{die} {ty}")),
                "{name} at level {level}"
            );
        }
    }
    // Eldritch Blast adds beams: no dice tier, one beam's dice on the attack row.
    let eb = text("Eldritch Blast");
    assert_eq!(borealis_engine::cantrips::scaled_damage(&eb, 5), None);
    assert_eq!(
        borealis_engine::cantrips::attack_damage(&eb, 5).as_deref(),
        Some("1d10 force")
    );
}
