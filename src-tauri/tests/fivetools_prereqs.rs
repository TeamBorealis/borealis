//! Local-only check that imported 5etools prerequisites gate options in the engine, as
//! Aurora does for its own feats and invocations. Enabled by `BOREALIS_5ETOOLS_DIR` (the
//! `data/` directory of a 5etools checkout); skipped otherwise. Only element ids appear here,
//! never content text.
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis \
//!     --test fivetools_prereqs -- --nocapture
//! ```
//!
//! Each case builds two characters that differ only in what the prerequisite looks at and
//! checks the option is hidden on one and offered on the other: an ability-gated feat, a
//! spellcasting-gated feat (strict: a half caster qualifies at level 2, feat-only casting never), a race-gated feat, a feat that requires another feat, a
//! class-level-gated invocation and a level + pact-gated invocation.

use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const LOW_CHA: [i32; 6] = [15, 14, 14, 12, 12, 10];
const HIGH_CHA: [i32; 6] = [15, 14, 14, 10, 10, 14];
const FILL_PASSES: usize = 80;
/// Slot types the fill leaves open (the cases pick them).
const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Species",
    "Background",
    "Archetype",
    "Feat",
    "Multiclass",
];

const FIGHTER: &str = "ID_5ET_PHB_CLASS_FIGHTER";
const WIZARD: &str = "ID_5ET_PHB_CLASS_WIZARD";
const PALADIN: &str = "ID_5ET_PHB_CLASS_PALADIN";
const WARLOCK: &str = "ID_5ET_PHB_CLASS_WARLOCK";
const HUMAN: &str = "ID_5ET_PHB_RACE_HUMAN";
const VARIANT_HUMAN: &str = "ID_5ET_PHB_SUBRACE_HUMAN_VARIANT";
const DWARF: &str = "ID_5ET_PHB_RACE_DWARF";

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

struct World<'a> {
    e: &'a Engine,
}

impl World<'_> {
    fn character(&self, class: &str, scores: [i32; 6]) -> CharacterState {
        let e = self.e;
        let s = commands::new_character(e, Ruleset::Dnd2014);
        let s = commands::set_base_scores(e, &s, scores)
            .expect("scores")
            .state;
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
        commands::select(e, &s, &key, Choice::Element(class.into()))
            .expect("class")
            .state
    }

    /// Pick `id` in the first open slot that offers it.
    fn pick(&self, s: &CharacterState, id: &str) -> CharacterState {
        let snap = self.e.derive(s);
        let slot = snap
            .slots
            .iter()
            .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
            .unwrap_or_else(|| panic!("{id} is not offered"));
        commands::select(self.e, s, &slot.key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id} refused: {err}"))
            .state
    }

    /// Fill open slots with `prefer` when offered, else their first option.
    fn fill(&self, mut s: CharacterState, prefer: &[&str]) -> CharacterState {
        let mut refused = HashSet::new();
        for _ in 0..FILL_PASSES {
            let snap = self.e.derive(&s);
            let Some(slot) = snap.slots.iter().find(|x| {
                x.chosen.is_none()
                    && !x.options.is_empty()
                    && !NOT_FILLED.contains(&x.ty.as_str())
                    // the 2014 improvement: the cases choose the "Feat" option themselves
                    && !x.options.iter().any(|o| o.name.starts_with("Feat"))
                    && !refused.contains(&x.key)
            }) else {
                break;
            };
            let id = slot
                .options
                .iter()
                .find(|o| prefer.contains(&o.id.as_str()))
                .unwrap_or(&slot.options[0])
                .id
                .clone();
            match commands::select(self.e, &s, &slot.key, Choice::Element(id)) {
                Ok(o) => s = o.state,
                Err(_) => {
                    refused.insert(slot.key.clone());
                }
            }
        }
        s
    }

    fn to_level(&self, mut s: CharacterState, level: u32, prefer: &[&str]) -> CharacterState {
        s = self.fill(s, prefer);
        while self.e.derive(&s).level < level {
            s = commands::level_up_main(self.e, &s, None)
                .expect("level up")
                .state;
            s = self.fill(s, prefer);
        }
        s
    }

    /// 2014 improvement: take the "Feat" option so a feat select opens.
    fn take_feat_option(&self, s: CharacterState) -> CharacterState {
        let snap = self.e.derive(&s);
        let Some((key, id)) = snap.slots.iter().find_map(|x| {
            let o = x.options.iter().find(|o| o.name.starts_with("Feat"))?;
            x.chosen.is_none().then(|| (x.key.clone(), o.id.clone()))
        }) else {
            panic!("no improvement slot with a Feat option");
        };
        commands::select(self.e, &s, &key, Choice::Element(id))
            .expect("feat option")
            .state
    }

    fn offered(&self, s: &CharacterState, id: &str) -> bool {
        self.e
            .derive(s)
            .slots
            .iter()
            .any(|x| x.options.iter().any(|o| o.id == id))
    }

    /// A level-1 variant human (open feat slot) of `class`.
    fn variant_human(&self, class: &str, scores: [i32; 6]) -> CharacterState {
        let s = self.pick(&self.character(class, scores), HUMAN);
        self.fill(self.pick(&s, VARIANT_HUMAN), &[])
    }

    /// A level-4 fighter of `race` with the feat select open.
    fn fighter_4(&self, race: &str) -> CharacterState {
        let s = self.pick(&self.character(FIGHTER, LOW_CHA), race);
        self.take_feat_option(self.to_level(s, 4, &[]))
    }
}

fn hidden_then_offered(
    w: &World<'_>,
    case: &str,
    id: &str,
    unmet: &CharacterState,
    met: &CharacterState,
) {
    assert!(!w.offered(unmet, id), "{case}: {id} offered although unmet");
    assert!(w.offered(met, id), "{case}: {id} hidden although met");
    println!("five_etools_prereqs: {case}: hidden when unmet, offered when met");
}

#[test]
fn five_etools_prerequisites_gate_options() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_prereqs: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn");
    handle.join().expect("prerequisite thread");
}

fn run(data: &Path) {
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("import");
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
    let engine = Engine::new(&store);
    let w = World { e: &engine };

    // ability score: Charisma 13
    let low = w.variant_human(FIGHTER, LOW_CHA);
    let high = w.variant_human(FIGHTER, HIGH_CHA);
    hidden_then_offered(
        &w,
        "ability",
        "ID_5ET_PHB_FEAT_INSPIRING_LEADER",
        &low,
        &high,
    );

    // spellcasting (strict `[spellcasting:1]`: a class spell-slot group)
    let wizard = w.variant_human(WIZARD, LOW_CHA);
    hidden_then_offered(
        &w,
        "spellcasting",
        "ID_5ET_PHB_FEAT_WAR_CASTER",
        &low,
        &wizard,
    );
    // a 2014 half caster has no slots at level 1, but does at level 2
    let paladin1 = w.variant_human(PALADIN, LOW_CHA);
    let paladin2 = w.to_level(paladin1.clone(), 2, &[]);
    hidden_then_offered(
        &w,
        "half caster level 2",
        "ID_5ET_PHB_FEAT_WAR_CASTER",
        &paladin1,
        &paladin2,
    );
    // spellcasting granted by a feat only (no class slots) does not qualify
    let feat_caster =
        w.take_feat_option(w.to_level(w.pick(&low, "ID_5ET_PHB_FEAT_MAGIC_INITIATE"), 4, &[]));
    assert!(
        !w.offered(&feat_caster, "ID_5ET_PHB_FEAT_WAR_CASTER"),
        "feat-only spellcasting must not satisfy [spellcasting:1]"
    );

    // race
    let human = w.fighter_4(HUMAN);
    let dwarf = w.fighter_4(DWARF);
    hidden_then_offered(
        &w,
        "race",
        "ID_5ET_XGE_FEAT_DWARVEN_FORTITUDE",
        &human,
        &dwarf,
    );

    // feat requires feat (and level 4)
    let chain = "ID_5ET_AU_FEAT_ELEMENTAL_FAMILIAR";
    let with = w.pick(&low, "ID_5ET_AU_FEAT_FAMILIAR_FRIEND");
    let without = w.pick(&low, "ID_5ET_PHB_FEAT_ALERT");
    assert!(
        !w.offered(&with, chain),
        "feat chain: offered below level 4"
    );
    let with = w.take_feat_option(w.to_level(with, 4, &[]));
    let without = w.take_feat_option(w.to_level(without, 4, &[]));
    hidden_then_offered(&w, "feat chain", chain, &without, &with);

    // class level: an invocation for warlock level 7
    let flesh = "ID_5ET_PHB_OPTFEATURE_SCULPTOR_OF_FLESH";
    let warlock = w.character(WARLOCK, HIGH_CHA);
    let lvl2 = w.to_level(warlock.clone(), 2, &[]);
    let lvl7 = w.to_level(lvl2.clone(), 7, &[]);
    hidden_then_offered(&w, "class level", flesh, &lvl2, &lvl7);

    // class level and pact boon
    let blade = "ID_5ET_PHB_OPTFEATURE_THIRSTING_BLADE";
    let chain_pact = w.to_level(
        warlock.clone(),
        5,
        &["ID_5ET_PHB_OPTFEATURE_PACT_OF_THE_CHAIN"],
    );
    let blade_pact = w.to_level(warlock, 5, &["ID_5ET_PHB_OPTFEATURE_PACT_OF_THE_BLADE"]);
    hidden_then_offered(&w, "pact", blade, &chain_pact, &blade_pact);
}
