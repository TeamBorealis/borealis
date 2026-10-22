//! Seeded property tests over characters built from the bundled packs with random valid
//! choices (engine-test-audit-1, property plan). No proptest dependency: a tiny xorshift
//! generator with fixed seeds drives the cases, so every run is identical.
//!
//! - save/load: `load(save(c))` derives the identical snapshot and re-saves to the identical file.
//! - level up then down returns to the same snapshot; levelling 1 to 20 one level at a time
//!   never panics and holds the settle invariants (`borealis_engine::invariants`).

use borealis_character::Character;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

const FILL_PASSES: usize = 60;
const ROUNDTRIP_CASES: u64 = 12;
const UPDOWN_CASES: u64 = 10;
const MAX_LEVEL: u32 = 20;
/// The one level where open selects are filled on the way up (filling at 20 is slow in debug).
const FILL_LEVEL: u32 = 10;

/// Minimal deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct World {
    store: ContentStore,
    engine: Engine,
}

fn world() -> &'static World {
    static W: OnceLock<World> = OnceLock::new();
    W.get_or_init(|| {
        let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&content.join(pack)).expect("bundled pack");
        }
        let store = b.build();
        let engine = Engine::new(&store);
        World { store, engine }
    })
}

/// Ids of one element type usable in a ruleset, sorted for determinism.
fn ids_of(w: &World, ty: &str, rs: Ruleset) -> Vec<String> {
    let mut ids: Vec<String> = w
        .store
        .by_type(ty)
        .filter(|c| c.element.ruleset.unwrap_or(Ruleset::Dnd2014) == rs)
        .map(|c| c.element.id.clone())
        .collect();
    ids.sort();
    ids
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> Option<CharacterState> {
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1)?;
    commands::select(e, s, &key, Choice::Element(id.into()))
        .ok()
        .map(|o| o.state)
}

/// Fill open selects with a random option each (refused picks are skipped).
fn random_fill(e: &Engine, mut s: CharacterState, rng: &mut Rng) -> CharacterState {
    let mut refused = Vec::new();
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let open: Vec<_> = snap
            .slots
            .iter()
            .filter(|x| {
                x.chosen.is_none()
                    && x.list_choice.is_none()
                    && !refused.contains(&x.key)
                    && (!x.options.is_empty() || !x.list_items.is_empty())
            })
            .collect();
        if open.is_empty() {
            break;
        }
        let slot = open[rng.below(open.len())];
        let choice = if slot.options.is_empty() {
            Choice::ListItem(slot.list_items[rng.below(slot.list_items.len())].0)
        } else {
            Choice::Element(slot.options[rng.below(slot.options.len())].id.clone())
        };
        match commands::select(e, &s, &slot.key, choice) {
            Ok(o) => s = o.state,
            Err(_) => refused.push(slot.key.clone()),
        }
    }
    s
}

/// A random level-1 character (class, race, background) with random choices filled.
fn random_character(w: &World, seed: u64) -> (CharacterState, String) {
    let mut rng = Rng::new(seed);
    let rs = if rng.below(2) == 0 {
        Ruleset::Dnd2014
    } else {
        Ruleset::Dnd2024
    };
    let e = &w.engine;
    let mut s = commands::new_character(e, rs);
    let class = {
        let ids = ids_of(w, "Class", rs);
        ids[rng.below(ids.len())].clone()
    };
    s = pick(e, &s, "Class", &class).expect("class offered");
    for ty in ["Race", "Background"] {
        let ids = ids_of(w, ty, rs);
        if !ids.is_empty() {
            let id = &ids[rng.below(ids.len())];
            s = pick(e, &s, ty, id).unwrap_or(s);
        }
    }
    (random_fill(e, s, &mut rng), class)
}

/// The JSON paths where two values differ (first few), for readable failures.
fn diff_paths(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    use serde_json::Value::{Array, Object};
    if out.len() >= 6 {
        return;
    }
    match (a, b) {
        (Object(x), Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                let (l, r) = (x.get(k), y.get(k));
                diff_paths(
                    l.unwrap_or(&serde_json::Value::Null),
                    r.unwrap_or(&serde_json::Value::Null),
                    &format!("{path}.{k}"),
                    out,
                );
            }
        }
        (Array(x), Array(y)) if x.len() == y.len() => {
            for (i, (l, r)) in x.iter().zip(y).enumerate() {
                diff_paths(l, r, &format!("{path}[{i}]"), out);
            }
        }
        _ if a != b => out.push(format!("{path}: {a} vs {b}")),
        _ => {}
    }
}

fn level_up(e: &Engine, s: &CharacterState) -> CharacterState {
    commands::level_up_main(e, s, None).expect("level up").state
}

#[test]
fn save_then_load_keeps_the_snapshot_and_the_file() {
    let w = world();
    let dir = tempfile::tempdir().expect("temp dir");
    for seed in 1..=ROUNDTRIP_CASES {
        let mut rng = Rng::new(seed + 1000);
        let (mut s, class) = random_character(w, seed);
        for _ in 0..rng.below(8) {
            s = level_up(&w.engine, &s);
        }
        let s = random_fill(&w.engine, s, &mut rng);
        let mut c = Character::with_ruleset(format!("Case {seed}"), s.ruleset);
        c.build = s;
        let (first, second) = (dir.path().join("a.json"), dir.path().join("b.json"));
        c.save(&first).expect("save");
        let loaded = Character::load(&first).expect("load");
        assert_eq!(loaded.build, c.build, "seed {seed} {class}: state");
        assert_eq!(
            w.engine.derive(&loaded.build),
            w.engine.derive(&c.build),
            "seed {seed} {class}: snapshot"
        );
        loaded.save(&second).expect("re-save");
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            std::fs::read_to_string(&second).unwrap(),
            "seed {seed} {class}: file"
        );
    }
}

#[test]
fn level_up_then_down_returns_to_the_same_snapshot() {
    let w = world();
    let e = &w.engine;
    for seed in 1..=UPDOWN_CASES {
        let mut rng = Rng::new(seed + 2000);
        let (mut base, class) = random_character(w, seed + 50);
        for _ in 0..rng.below(4) {
            base = level_up(e, &base);
        }
        // every pick that exists at the base level is made, so only picks above it are new
        base = random_fill(e, base, &mut rng);
        let before = e.derive(&base);
        let steps = 1 + rng.below(5) as u32;
        let mut s = base.clone();
        for _ in 0..steps {
            s = level_up(e, &s);
            s = random_fill(e, s, &mut rng);
        }
        assert_eq!(s.level(), base.level() + steps, "seed {seed} {class}");
        for _ in 0..steps {
            s = commands::level_down(e, &s).expect("level down").state;
        }
        assert_eq!(s.level(), base.level(), "seed {seed} {class}");
        let after = e.derive(&s);
        assert_eq!(after.level, before.level, "seed {seed} {class}");
        let mut diffs = Vec::new();
        let (l, r) = (
            serde_json::to_value(&after).unwrap(),
            serde_json::to_value(&before).unwrap(),
        );
        diff_paths(&l, &r, "", &mut diffs);
        assert!(diffs.is_empty(), "seed {seed} {class}: {diffs:?}");
        assert_eq!(s.selections, base.selections, "seed {seed} {class}: leak");
    }
}

#[test]
fn every_2014_class_levels_one_to_twenty_holding_the_settle_invariants() {
    levels_one_to_twenty(Ruleset::Dnd2014);
}

#[test]
fn every_2024_class_levels_one_to_twenty_holding_the_settle_invariants() {
    levels_one_to_twenty(Ruleset::Dnd2024);
}

fn levels_one_to_twenty(rs: Ruleset) {
    use borealis_engine::invariants::{self, kind};
    invariants::enable_for_thread();
    invariants::record_for_thread();
    let w = world();
    let e = &w.engine;
    {
        for class in ids_of(w, "Class", rs) {
            let mut rng = Rng::new(7);
            let s = commands::new_character(e, rs);
            let mut s = pick(e, &s, "Class", &class).expect("class offered");
            for lvl in 2..=MAX_LEVEL {
                s = level_up(e, &s);
                assert_eq!(e.derive(&s).level, lvl, "{class}");
                if lvl == FILL_LEVEL {
                    s = random_fill(e, s, &mut rng);
                }
            }
        }
    }
    // The SRD 5.1 pack has a single feat (with a prerequisite), so the generated "feat instead
    // of Ability Score Improvement" select can offer nothing while the ASI alternative stays
    // open. That is content, not an engine fault; anything else is a violation.
    let rest: Vec<String> = invariants::take_recorded()
        .into_iter()
        .filter(|v| {
            !(v.kind == kind::EMPTY_SELECT
                && v.element
                    .as_deref()
                    .is_some_and(|id| id.starts_with("ID_INTERNAL_CLASS_FEATURE_FEAT_")))
        })
        .map(|v| {
            format!(
                "{} [{}] {}",
                v.kind,
                v.element.unwrap_or_default(),
                v.message
            )
        })
        .collect();
    assert!(rest.is_empty(), "settle invariants failed: {rest:#?}");
}
