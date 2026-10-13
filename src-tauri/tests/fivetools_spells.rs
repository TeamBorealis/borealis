//! Local-only, end to end: characters built from a real 5etools `data/` import get the
//! additional spells of their race, subclass and class in the derived snapshot (the data behind
//! the Magic page and the sheet). Enabled by `BOREALIS_5ETOOLS_DIR`; skipped otherwise. Refers
//! to entities by id only.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::path::{Path, PathBuf};

const SCORES: [i32; 6] = [10, 14, 14, 12, 12, 16];
const FILL_PASSES: usize = 40;

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn character(e: &Engine, rs: Ruleset, class: &str) -> CharacterState {
    let s = commands::new_character(e, rs);
    let s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state
}

/// Pick `target` in the first open slot offering it; panics when nothing offers it.
fn pick(e: &Engine, s: &CharacterState, target: &str) -> CharacterState {
    let snap = e.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == target))
        .unwrap_or_else(|| panic!("{target} is not offered"));
    commands::select(e, s, &slot.key, Choice::Element(target.into()))
        .expect("pick")
        .state
}

/// Fill open element slots with their first option (as the engine sweep does).
fn fill(e: &Engine, mut s: CharacterState) -> CharacterState {
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let Some(slot) = snap.slots.iter().find(|x| {
            x.chosen.is_none() && !x.options.is_empty() && x.ty != "Archetype" && x.ty != "Spell"
        }) else {
            break;
        };
        let choice = Choice::Element(slot.options[0].id.clone());
        match commands::select(e, &s, &slot.key, choice) {
            Ok(o) => s = o.state,
            Err(_) => break,
        }
    }
    s
}

/// Level up to `level`, choosing the subclass `sub` as soon as it is offered.
fn level_to(e: &Engine, mut s: CharacterState, level: u32, sub: &str) -> CharacterState {
    loop {
        let offered = e
            .derive(&s)
            .slots
            .iter()
            .any(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == sub));
        if offered {
            s = pick(e, &s, sub);
        }
        if e.derive(&s).level >= level {
            return s;
        }
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
}

#[test]
fn five_etools_additional_spells_reach_the_snapshot() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_additional_spells_reach_the_snapshot: BOREALIS_5ETOOLS_DIR not set");
        return;
    };
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(Path::new(&dir), &options).expect("import");
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder.add_pack_dir(&content_dir().join(pack)).unwrap();
    }
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp.path()).unwrap())
            .unwrap();
    }
    let store = builder.build();
    let e = Engine::new(&store);

    // race with limited innate spells: its own caster tab and short-rest free casts
    let s = character(&e, Ruleset::Dnd2014, "ID_5ET_PHB_CLASS_FIGHTER");
    let s = pick(&e, &s, "ID_5ET_VGM_RACE_FIRBOLG");
    let snap = e.derive(&s);
    let firbolg = snap.caster("Firbolg").expect("Firbolg spells");
    assert_eq!(firbolg.ability, "Wisdom");
    assert!(firbolg
        .spells
        .iter()
        .any(|x| x.id == "ID_5ET_PHB_SPELL_DETECT_MAGIC" && x.known));
    let casts: Vec<_> = snap
        .free_casts
        .iter()
        .filter(|f| f.source == "ID_5ET_VGM_RACE_FIRBOLG")
        .collect();
    assert_eq!(casts.len(), 2);
    assert!(casts
        .iter()
        .all(|f| f.uses == Some(1) && f.recharge == "short_rest"));

    // 2024 patron: always-prepared patron spells and free casts counted by an ability modifier
    let archfey = "ID_5ET_XPHB_SUBCLASS_WARLOCK_ARCHFEY";
    let w = character(&e, Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_WARLOCK");
    let w3 = fill(&e, level_to(&e, w, 3, archfey));
    let snap = e.derive(&w3);
    let misty = "ID_5ET_XPHB_SPELL_MISTY_STEP";
    let warlock = snap.caster("Warlock").expect("warlock");
    assert!(warlock
        .spells
        .iter()
        .any(|x| x.id == misty && x.always_prepared));
    assert_eq!(warlock.prepared_count, 0, "patron spells do not count");
    let f = snap
        .free_casts
        .iter()
        .find(|f| f.spell == misty)
        .expect("misty step free casts");
    assert_eq!((f.uses, f.uses_rule.as_str()), (Some(3), "charisma"));

    // 2014 subclass innate spell with a daily free cast, joined to the class
    let s = character(&e, Ruleset::Dnd2014, "ID_5ET_PHB_CLASS_WARLOCK");
    let fathomless = "ID_5ET_TCE_SUBCLASS_WARLOCK_FATHOMLESS";
    let s10 = fill(&e, level_to(&e, s, 10, fathomless));
    let snap = e.derive(&s10);
    let tentacles = "ID_5ET_PHB_SPELL_EVARDS_BLACK_TENTACLES";
    assert!(snap
        .caster("Warlock")
        .unwrap()
        .spells
        .iter()
        .any(|x| x.id == tentacles && x.known));
    let f = snap
        .free_casts
        .iter()
        .find(|f| f.spell == tentacles)
        .expect("free cast");
    assert_eq!((f.uses, f.recharge.as_str()), (Some(1), "long_rest"));

    // 2024 class-level expanded list: preparable from class level 10, not before
    let bard = character(&e, Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_BARD");
    let preparable = |s: &CharacterState, spell: &str| {
        e.derive(s)
            .caster("Bard")
            .is_some_and(|c| c.spells.iter().any(|x| x.id == spell))
    };
    let bless = "ID_5ET_XPHB_SPELL_BLESS";
    let b9 = level_to(&e, bard, 9, "-");
    assert!(!preparable(&b9, bless));
    let b10 = level_to(&e, b9, 10, "-");
    assert!(preparable(&b10, bless));
}
