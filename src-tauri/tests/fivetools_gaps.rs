//! Local-only check of the 5etools import gaps closed in `5etools-gaps-1`: ability increases
//! (2024 background +2/+1, +2 to one ability, Epic Boon maximum 30), ritual spell filters,
//! subrace speed replacement, wizard ritual casting and readable subclass ids whose old
//! hashed ids still resolve for saved characters. Enabled by `BOREALIS_5ETOOLS_DIR`; skipped
//! otherwise. Only element ids appear here, never content text.
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis \
//!     --test fivetools_gaps -- --nocapture
//! ```

use borealis_content::{ContentStore, ContentStoreBuilder, Rule, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::path::{Path, PathBuf};

const SCORES: [i32; 6] = [15, 14, 12, 13, 10, 8];
const FILL_PASSES: usize = 120;

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

struct World<'a> {
    e: &'a Engine,
}

impl World<'_> {
    fn character(&self, rs: Ruleset, class: &str) -> CharacterState {
        borealis_engine::invariants::enable_for_thread();
        let e = self.e;
        let s = commands::new_character(e, rs);
        let s = commands::set_base_scores(e, &s, SCORES)
            .expect("scores")
            .state;
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
        commands::select(e, &s, &key, Choice::Element(class.into()))
            .expect("class")
            .state
    }

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

    /// Choose the list entry whose text is `text` in an open list slot of `owner`.
    fn pick_list(&self, s: &CharacterState, owner: &str, text: &str) -> CharacterState {
        let snap = self.e.derive(s);
        let (key, id) = snap
            .slots
            .iter()
            .filter(|x| x.owner.id == owner && x.list_choice.is_none())
            .find_map(|x| {
                let (id, _) = x.list_items.iter().find(|(_, t)| t == text)?;
                Some((x.key.clone(), *id))
            })
            .unwrap_or_else(|| panic!("{owner}: no open list entry {text:?}"));
        commands::select(self.e, s, &key, Choice::ListItem(id))
            .expect("list pick")
            .state
    }

    fn score(&self, s: &CharacterState, ability: &str) -> (i32, i32) {
        let snap = self.e.derive(s);
        let a = snap
            .abilities
            .iter()
            .find(|a| a.name == ability)
            .expect("ability");
        (a.score, a.maximum)
    }

    fn options_of(&self, s: &CharacterState, owner: &str) -> Vec<String> {
        self.e
            .derive(s)
            .slots
            .iter()
            .filter(|x| x.owner.id == owner)
            .flat_map(|x| x.options.iter().map(|o| o.id.clone()))
            .collect()
    }

    /// Fill open element slots with their first option (feat slots: `feat` when offered).
    fn fill(&self, mut s: CharacterState, feat: &str) -> CharacterState {
        let mut refused = std::collections::HashSet::new();
        for _ in 0..FILL_PASSES {
            let snap = self.e.derive(&s);
            let Some(slot) = snap.slots.iter().find(|x| {
                x.chosen.is_none()
                    && x.list_choice.is_none()
                    && !x.options.is_empty()
                    && !refused.contains(&x.key)
            }) else {
                break;
            };
            let id = slot
                .options
                .iter()
                .find(|o| o.id == feat)
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
}

#[test]
fn five_etools_gaps_are_closed() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_gaps: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn");
    handle.join().expect("gaps thread");
}

fn load(data: &Path, tmp: &Path) -> ContentStore {
    let options = ImportOptions {
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("import");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder.add_pack_dir(&content_dir().join(pack)).unwrap();
    }
    for pack in &result.packs {
        builder = builder.add_pack_dir(&pack.write_to(tmp).unwrap()).unwrap();
    }
    builder.build()
}

fn run(data: &Path) {
    let tmp = tempfile::tempdir().unwrap();
    let store = load(data, tmp.path());
    let engine = Engine::new(&store);
    let w = World { e: &engine };
    abilities(&w);
    ritual_spells(&w);
    wizard_rituals(&w);
    subrace_speed(&store);
    renamed_subclass(&w, &store);
}

fn abilities(w: &World<'_>) {
    // 2024 background: +2/+1 distribution (was ignored by the engine before)
    let s = w.character(Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_FIGHTER");
    let s = w.pick(&s, "ID_5ET_XPHB_BACKGROUND_ACOLYTE");
    let before = w.score(&s, "intelligence").0;
    let s = w.pick_list(
        &s,
        "ID_5ET_XPHB_BACKGROUND_ACOLYTE",
        "Intelligence +2, Wisdom +1",
    );
    assert_eq!(w.score(&s, "intelligence").0, before + 2, "acolyte +2");
    println!("five_etools_gaps: 2024 background +2/+1 applies");

    // +2 to one ability (Custom Lineage), one pick instead of two +1 picks
    let s = w.character(Ruleset::Dnd2014, "ID_5ET_PHB_CLASS_FIGHTER");
    let s = w.pick(&s, "ID_5ET_TCE_RACE_CUSTOM_LINEAGE");
    let before = w.score(&s, "dexterity").0;
    let s = w.pick_list(&s, "ID_5ET_TCE_RACE_CUSTOM_LINEAGE", "Dexterity +2");
    assert_eq!(w.score(&s, "dexterity").0, before + 2, "custom lineage +2");
    println!("five_etools_gaps: +2 to one ability applies");

    // Epic Boon: +1 with a maximum of 30
    let mut s = w.character(Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_FIGHTER");
    let boon = "ID_5ET_XPHB_FEAT_BOON_OF_FORTITUDE";
    s = w.fill(s, boon);
    while w.e.derive(&s).level < 19 {
        s = commands::level_up_main(w.e, &s, None)
            .expect("level up")
            .state;
        s = w.fill(s, boon);
    }
    let s = w.pick_list(&s, boon, "Constitution +1");
    let (_, max) = w.score(&s, "constitution");
    assert_eq!(max, 30, "boon maximum");
    println!("five_etools_gaps: Epic Boon raises the maximum to 30");
}

fn ritual_spells(w: &World<'_>) {
    let opt = "ID_5ET_PHB_SPECIESOPT_RITUAL_CASTER_WIZARD_SPELLS";
    let s = w.character(Ruleset::Dnd2014, "ID_5ET_PHB_CLASS_WIZARD");
    let s = w.pick(&s, "ID_5ET_PHB_RACE_HUMAN");
    let s = w.pick(&s, "ID_5ET_PHB_SUBRACE_HUMAN_VARIANT");
    let s = w.pick(&s, "ID_5ET_PHB_FEAT_RITUAL_CASTER");
    let s = w.pick(&s, opt);
    let offered = w.options_of(&s, opt);
    assert!(
        offered
            .iter()
            .any(|i| i == "ID_5ET_PHB_SPELL_FIND_FAMILIAR"),
        "ritual offered: {offered:?}"
    );
    assert!(
        !offered
            .iter()
            .any(|i| i == "ID_5ET_PHB_SPELL_MAGIC_MISSILE"),
        "non-ritual offered"
    );
    println!(
        "five_etools_gaps: Ritual Caster offers {} ritual spells only",
        offered.len()
    );
}

fn wizard_rituals(w: &World<'_>) {
    for (rs, class) in [
        (Ruleset::Dnd2014, "ID_5ET_PHB_CLASS_WIZARD"),
        (Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_WIZARD"),
    ] {
        let s = w.fill(w.character(rs, class), "");
        let snap = w.e.derive(&s);
        assert!(
            snap.spellcasting
                .iter()
                .any(|b| b.ritual_source.as_deref() == Some("spellbook")),
            "{class}: no spellbook ritual casting"
        );
    }
    println!("five_etools_gaps: both wizards cast rituals from the spellbook");
}

fn subrace_speed(store: &ContentStore) {
    let elf = store.element("ID_5ET_PHB_RACE_ELF").expect("elf");
    let gated = elf.rules.iter().any(|r| {
        matches!(r, Rule::Stat(s) if s.name == "innate speed"
            && s.requirements.as_deref().is_some_and(|q| q.contains("ID_5ET_PHB_SUBRACE_ELF_WOOD")))
    });
    assert!(gated, "elf walking speed not replaced by the wood elf's");
    println!("five_etools_gaps: subrace speed replaces the parent's");
}

fn renamed_subclass(w: &World<'_>, store: &ContentStore) {
    let qualified: Vec<_> = store
        .iter()
        .map(|s| &s.element)
        .filter(|e| e.ty == "Archetype" && e.id.starts_with("ID_5ET_XGE_SUBCLASS_XPHB_"))
        .collect();
    assert!(qualified.len() >= 10, "readable XPHB-class XGE subclasses");
    for e in &qualified {
        let old = e.renamed_from.as_deref().expect("renamed_from");
        assert!(hashed(old), "{old}");
        assert_eq!(
            store.element(old).map(|x| x.id.as_str()),
            Some(e.id.as_str())
        );
    }
    assert!(
        !store
            .iter()
            .any(|s| s.element.ty == "Archetype" && hashed(&s.element.id)),
        "hash-suffixed subclass ids remain"
    );
    // a character saved with the old id opens with the new one
    let sub = qualified
        .iter()
        .find(|e| e.supports.iter().any(|t| t == "ID_5ET_XPHB_CLASS_FIGHTER"))
        .expect("fighter subclass");
    let old = sub.renamed_from.clone().unwrap();
    let mut s = w.fill(
        w.character(Ruleset::Dnd2024, "ID_5ET_XPHB_CLASS_FIGHTER"),
        "",
    );
    while w.e.derive(&s).level < 3 {
        s = commands::level_up_main(w.e, &s, None).expect("lvl").state;
    }
    let s = w.pick(&s, &sub.id);
    let mut saved: serde_json::Value = serde_json::to_value(&s).unwrap();
    replace_all(&mut saved, &sub.id, &old);
    let mut stale: CharacterState = serde_json::from_value(saved).unwrap();
    assert!(borealis_lib::renamed_ids::migrate(&mut stale, store) > 0);
    assert_eq!(stale, s, "migrated character equals the original");
    println!(
        "five_etools_gaps: {} readable subclass ids, old ids resolve and migrate",
        qualified.len()
    );
}

/// `<id>_<8 upper-case hex digits>`: the old collision suffix.
fn hashed(id: &str) -> bool {
    id.rsplit_once('_').is_some_and(|(_, tail)| {
        tail.len() == 8
            && tail
                .chars()
                .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
    })
}

fn replace_all(v: &mut serde_json::Value, from: &str, to: &str) {
    match v {
        serde_json::Value::String(s) if s == from => *s = to.to_string(),
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| replace_all(x, from, to)),
        serde_json::Value::Object(m) => m.values_mut().for_each(|x| replace_all(x, from, to)),
        _ => {}
    }
}
