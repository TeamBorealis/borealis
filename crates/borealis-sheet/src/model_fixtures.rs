//! Test characters built from the bundled SRD packs (2014 wizard, 2024 wizard, fighter/wizard).

use borealis_character::Character;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use std::path::Path;

pub(crate) fn store(pack: &str) -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join(pack)))
        .expect("bundled packs load")
        .build()
}

fn pick(e: &Engine, s: &CharacterState, ty: &str, id: &str) -> CharacterState {
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

fn level1(e: &Engine, s: CharacterState, race: &str, class: &str, bg: &str) -> CharacterState {
    let snap = e.derive(&s);
    let key = |s: &CharacterSnapshot, n: &str| slot_key(s, "ID_LEVEL_1", n, 1).expect(n);
    let names: [(&str, &str); 3] = [("Race", race), ("Class", class), ("Background", bg)];
    let mut s = s;
    let mut snap = snap;
    for (slot, id) in names {
        let k = key(&snap, slot);
        s = commands::select(e, &s, &k, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{slot} {id}: {err}"))
            .state;
        snap = e.derive(&s);
    }
    s
}

/// Fill every open spell slot with its first offered option (cantrips, known and spellbook spells).
fn fill_spells(e: &Engine, mut s: CharacterState) -> CharacterState {
    for _ in 0..80 {
        let snap = e.derive(&s);
        let Some(slot) = snap
            .slots
            .iter()
            .find(|x| x.ty == "Spell" && x.chosen.is_none() && !x.options.is_empty())
        else {
            break;
        };
        let (key, id) = (slot.key.clone(), slot.options[0].id.clone());
        match commands::select(e, &s, &key, Choice::Element(id)) {
            Ok(r) => s = r.state,
            Err(_) => break,
        }
    }
    s
}

fn wrap(name: &str, s: CharacterState, e: &Engine) -> (Character, CharacterSnapshot) {
    let s = fill_spells(e, s);
    let snap = e.derive(&s);
    let mut c = Character::new(name);
    c.build = s;
    (c, snap)
}

pub(crate) fn wizard_2014() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = [8, 14, 13, 15, 12, 10];
    s = level1(
        &e,
        s,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_WIZARD",
        "ID_BACKGROUND_ACOLYTE",
    );
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    wrap("Mira Vale", s, &e)
}

pub(crate) fn wizard_2024() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.2.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s.abilities.base = [8, 14, 13, 15, 12, 10];
    s = level1(
        &e,
        s,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_WIZARD",
        "ID_2024_BACKGROUND_SAGE",
    );
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    wrap("Mira Vale", s, &e)
}

fn multiclass_2014(
    first: &str,
    levels: u32,
    second: &str,
    more: u32,
    base: [i32; 6],
    name: &str,
) -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = base;
    s = level1(
        &e,
        s,
        "ID_RACE_HUMAN",
        &format!("ID_WOTC_PHB_CLASS_{first}"),
        "ID_BACKGROUND_ACOLYTE",
    );
    for _ in 1..levels {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = commands::level_up_new_multiclass(&e, &s, None)
        .unwrap()
        .state;
    let start = s.level();
    s = pick(
        &e,
        &s,
        "Multiclass",
        &format!("ID_WOTC_PHB_MULTICLASS_{second}"),
    );
    for _ in 1..more {
        s = commands::level_up_multiclass(&e, &s, start, None)
            .unwrap()
            .state;
    }
    wrap(name, s, &e)
}

/// Fighter 4 / Wizard 3 (one caster class, so no pooled slot table).
pub(crate) fn fighter_wizard_2014() -> (Character, CharacterSnapshot) {
    multiclass_2014(
        "FIGHTER",
        4,
        "WIZARD",
        3,
        [15, 12, 14, 13, 10, 8],
        "Brann Ironquill",
    )
}

/// Cleric 3 / Wizard 2 (two caster classes, pooled slots).
pub(crate) fn cleric_wizard_2014() -> (Character, CharacterSnapshot) {
    multiclass_2014(
        "CLERIC",
        3,
        "WIZARD",
        2,
        [10, 12, 14, 13, 15, 8],
        "Sela Moonhand",
    )
}

/// Level-5 Hill Dwarf Life Cleric with a shield, scale mail and mace.
pub(crate) fn cleric_2014() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = [13, 10, 14, 8, 15, 12];
    s = level1(
        &e,
        s,
        "ID_SRD_RACE_DWARF",
        "ID_WOTC_PHB_CLASS_CLERIC",
        "ID_BACKGROUND_ACOLYTE",
    );
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
    wrap("Brunhild Stonehand", s, &e)
}
