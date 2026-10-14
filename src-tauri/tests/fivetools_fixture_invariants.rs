//! Public CI: characters built from the importer's output on the SRD-only 5etools fixtures
//! (`crates/borealis-import-5etools/tests/fixtures`) hold the settle invariants
//! (`borealis_engine::invariants`, engine-test-audit-1 WP2) at every derive. Each imported class
//! is levelled 1 to 20 with every imported background of its edition, open selects filled with
//! their first option (the subclass included) at levels 1, 3, 5, 11, 17 and 20.

use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SCORES: [i32; 6] = [12, 14, 14, 16, 14, 16];
const FILL_PASSES: usize = 80;
const MAX_LEVEL: u32 = 20;
/// Levels whose open selects are filled (every derive on the way is checked).
const FILL_LEVELS: [u32; 5] = [3, 5, 11, 17, 20];

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data")
}

struct World {
    engine: Engine,
    imported: Vec<Element>,
}

fn world(tmp: &Path) -> World {
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(&fixtures(), &options).expect("fixtures import");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content_dir().join(pack))
            .expect("bundled pack");
    }
    let mut imported = Vec::new();
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp).expect("write pack"))
            .expect("imported pack");
        for e in pack.elements() {
            let mut e = e.clone();
            e.ruleset = e.ruleset.or(pack.ruleset);
            imported.push(e);
        }
    }
    World {
        engine: Engine::new(&builder.build()),
        imported,
    }
}

fn select(e: &Engine, s: &CharacterState, name: &str, id: &str) -> Option<CharacterState> {
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1)?;
    commands::select(e, s, &key, Choice::Element(id.into()))
        .ok()
        .map(|o| o.state)
}

/// Fill every open select with its first option (list selects with their first entry).
fn fill(e: &Engine, mut s: CharacterState) -> CharacterState {
    let mut refused = HashSet::new();
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let Some(slot) = snap.slots.iter().find(|x| {
            x.chosen.is_none()
                && x.list_choice.is_none()
                && !refused.contains(&x.key)
                && (!x.options.is_empty() || !x.list_items.is_empty())
        }) else {
            break;
        };
        let choice = match (slot.options.first(), slot.list_items.first()) {
            (Some(o), _) => Choice::Element(o.id.clone()),
            (None, Some((i, _))) => Choice::ListItem(*i),
            (None, None) => break,
        };
        match commands::select(e, &s, &slot.key, choice) {
            Ok(o) => s = o.state,
            Err(_) => {
                refused.insert(slot.key.clone());
            }
        }
    }
    s
}

#[test]
fn imported_fixture_characters_hold_the_settle_invariants() {
    borealis_engine::invariants::enable_for_thread();
    // the fixtures hold one fighting style (Archery), so the Champion's second style select
    // has nothing left to offer
    borealis_engine::invariants::allow_for_thread(
        borealis_engine::invariants::kind::EMPTY_SELECT,
        "ID_5ET_PHB_SUBCLASS_FIGHTER_CHAMPION",
    );
    let tmp = tempfile::tempdir().expect("temp dir");
    let w = world(tmp.path());
    let e = &w.engine;
    let of = |ty: &str| -> Vec<&Element> { w.imported.iter().filter(|x| x.ty == ty).collect() };
    let classes = of("Class");
    let backgrounds = of("Background");
    assert!(classes.len() >= 2, "the fixtures import Fighter and Wizard");
    assert!(!backgrounds.is_empty(), "the fixtures import a background");
    let mut built = 0;
    for class in &classes {
        let rs = class.ruleset.unwrap_or(Ruleset::Dnd2014);
        for bg in backgrounds
            .iter()
            .filter(|b| b.ruleset.unwrap_or(Ruleset::Dnd2014) == rs)
        {
            let s = commands::new_character(e, rs);
            let s = commands::set_base_scores(e, &s, SCORES)
                .expect("scores")
                .state;
            let s = select(e, &s, "Class", &class.id).expect("imported class offered");
            let mut s = fill(e, select(e, &s, "Background", &bg.id).unwrap_or(s));
            while s.level() < MAX_LEVEL {
                s = commands::level_up_main(e, &s, None)
                    .expect("level up")
                    .state;
                if FILL_LEVELS.contains(&s.level()) {
                    s = fill(e, s);
                }
            }
            assert_eq!(e.derive(&s).level, MAX_LEVEL, "{} {}", class.id, bg.id);
            built += 1;
        }
    }
    assert!(built >= 2, "at least one class per edition was built");
}
