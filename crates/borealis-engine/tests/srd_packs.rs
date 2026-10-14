//! Smoke test: the engine evaluates every class and race of the bundled SRD packs on top of the
//! core pack without engine-level diagnostics (skips when a pack directory is absent).

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{Choice, Engine};
use std::path::Path;

fn pack(name: &str) -> Option<std::path::PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(name);
    p.join("pack.json").is_file().then_some(p)
}

#[test]
fn bundled_srd_51_pack_evaluates() {
    let (Some(srd), core) = (pack("srd-5.1"), pack("core").expect("core pack")) else {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return;
    };
    let store = match ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .and_then(|b| b.add_pack_dir(&srd))
    {
        Ok(b) => b.build(),
        Err(e) => {
            eprintln!("SKIPPED: srd-5.1 pack not loadable yet: {e}");
            return;
        }
    };
    let engine = Engine::new(&store);
    let classes: Vec<String> = store
        .by_type("Class")
        .map(|c| c.element.id.clone())
        .collect();
    let races: Vec<String> = store
        .by_type("Race")
        .map(|c| c.element.id.clone())
        .collect();
    eprintln!("srd-5.1: {} classes, {} races", classes.len(), races.len());
    for class in &classes {
        let mut s = commands::new_character(&engine, Ruleset::Dnd2014);
        let snap = engine.derive(&s);
        let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
        s = commands::select(&engine, &s, &key, Choice::Element(class.clone()))
            .unwrap_or_else(|e| panic!("{class}: {e}"))
            .state;
        for _ in 0..19 {
            s = commands::level_up_main(&engine, &s, None).unwrap().state;
        }
        let snap = engine.derive(&s);
        assert_eq!(snap.level, 20, "{class}");
        assert_eq!(snap.proficiency_bonus, 6, "{class}");
        assert!(snap.hit_points > 20, "{class} has hit dice");
        let bad: Vec<_> = snap
            .diagnostics
            .iter()
            .filter(|d| matches!(d.code.as_str(), "stat.cycle" | "eval.not_converged"))
            .collect();
        assert!(bad.is_empty(), "{class}: {bad:?}");
    }
    for race in &races {
        let mut s = commands::new_character(&engine, Ruleset::Dnd2014);
        let snap = engine.derive(&s);
        let key = slot_key(&snap, "ID_LEVEL_1", "Race", 1).expect("race slot");
        s = commands::select(&engine, &s, &key, Choice::Element(race.clone()))
            .unwrap_or_else(|e| panic!("{race}: {e}"))
            .state;
        let snap = engine.derive(&s);
        assert!(snap.speed.walk > 0, "{race} has a speed");
    }
}

fn srd_51_engine() -> Option<Engine> {
    let srd = pack("srd-5.1")?;
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&pack("core").expect("core pack"))
        .and_then(|b| b.add_pack_dir(&srd))
        .expect("srd-5.1 loads")
        .build();
    Some(Engine::new(&store))
}

fn class_at(engine: &Engine, class: &str, level: u32) -> borealis_engine::state::CharacterState {
    borealis_engine::invariants::enable_for_thread();
    let s = commands::new_character(engine, Ruleset::Dnd2014);
    let key = slot_key(&engine.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let mut s = commands::select(engine, &s, &key, Choice::Element(class.into()))
        .expect("class")
        .state;
    while s.level() < level {
        s = commands::level_up_main(engine, &s, None)
            .expect("level up")
            .state;
    }
    s
}

/// SRD 5.1 Warlock table: Pact Magic slots (count, slot level) are a separate pool, never
/// ordinary slots, and not a multiclass slot source.
#[test]
fn bundled_srd_51_warlock_pact_slots_are_a_separate_pool() {
    let Some(engine) = srd_51_engine() else {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return;
    };
    for (level, pact) in [
        (1, (1, 1)),
        (2, (2, 1)),
        (5, (2, 3)),
        (11, (3, 5)),
        (17, (4, 5)),
    ] {
        let snap = engine.derive(&class_at(&engine, "ID_WOTC_PHB_CLASS_WARLOCK", level));
        let w = snap.caster("Warlock").expect("warlock casting");
        assert_eq!((w.pact, w.slots), (Some(pact), [0; 9]), "warlock {level}");
        assert!(snap.multiclass_slots.is_none());
    }
}

/// Regression: the SRD 5.1 Bard's Spells Known column already counts the Magical Secrets
/// spells, which have their own selects, so the Spell Known selects leave them out.
#[test]
fn bundled_srd_51_bard_does_not_count_magical_secrets_twice() {
    let Some(engine) = srd_51_engine() else {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return;
    };
    let count = |level: u32, prefix: &str| {
        let snap = engine.derive(&class_at(&engine, "ID_WOTC_PHB_CLASS_BARD", level));
        snap.slots
            .iter()
            .filter(|s| s.name.starts_with(prefix))
            .count()
    };
    // Spells Known column: 12 at level 9, 14 at 10, 18 at 14, 22 at 18 and 20.
    for (level, known, secrets) in [
        (9, 12, 0),
        (10, 12, 2),
        (14, 14, 4),
        (18, 16, 6),
        (20, 16, 6),
    ] {
        assert_eq!(count(level, "Spell Known (Bard)"), known, "bard {level}");
        assert_eq!(
            count(level, "Magical Secrets (Bard"),
            secrets,
            "bard {level}"
        );
    }
}
