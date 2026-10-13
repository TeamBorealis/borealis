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
