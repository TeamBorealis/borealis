//! Local-only: every caster class and subclass of a real 5etools `data/` import can learn and
//! prepare spells like a bundled SRD caster. Enabled by `BOREALIS_5ETOOLS_DIR`; skipped
//! otherwise. Refers to entities by id and numbers only.
//!
//! For each caster in `class/class-*.json` (both editions) a character is levelled 1 to 20; at
//! levels 1, 3, 5, 11 and 20 the spell choice slots owned by the class (or subclass) must equal
//! what its own progression arrays say (cantrips + spells known + spellbook pages + fixed-level
//! spells), every one of them must be fillable, and the prepared spell limit must follow
//! `preparedSpellsProgression`. Expected numbers are computed from the corpus, never hardcoded.
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis \
//!     --test fivetools_casters -- --nocapture
//! ```

use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SCORES: [i32; 6] = [12, 14, 14, 16, 14, 16];
const CHECK_LEVELS: [u32; 5] = [1, 3, 5, 11, 20];
const FILL_PASSES: usize = 80;

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

/// One caster of the corpus with its progression arrays.
struct Caster {
    /// Spellcasting name (class or subclass name).
    name: String,
    /// Imported class id.
    class_id: String,
    /// Imported subclass id for subclass casters.
    sub_id: Option<String>,
    ruleset: Ruleset,
    raw: Value,
}

impl Caster {
    fn series(&self, key: &str, level: u32) -> i64 {
        self.raw
            .get(key)
            .and_then(Value::as_array)
            .and_then(|a| a.get(level as usize - 1))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    /// Spell choice slots the class table promises at a class level.
    fn expected_slots(&self, level: u32) -> i64 {
        let book: i64 = (1..=level)
            .map(|l| self.series("spellsKnownProgressionFixed", l))
            .sum();
        let fixed: i64 = self
            .raw
            .get("spellsKnownProgressionFixedByLevel")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.parse::<u32>().is_ok_and(|k| k <= level))
            .flat_map(|(_, v)| v.as_object().into_iter().flatten())
            .filter_map(|(_, n)| n.as_i64())
            .sum();
        self.series("cantripProgression", level)
            + self.series("spellsKnownProgression", level)
            + book
            + fixed
    }
}

fn fragment(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn ruleset_of(v: &Value) -> Ruleset {
    if str_of(v, "edition") == "one" {
        Ruleset::Dnd2024
    } else {
        Ruleset::Dnd2014
    }
}

/// Casters of the corpus that were imported (by their deterministic ids).
fn corpus_casters(data: &Path, imported: &BTreeMap<String, Element>) -> Vec<Caster> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(data.join("class"))
        .expect("class dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("class-") && n.ends_with(".json"))
        })
        .collect();
    files.sort();
    for file in files {
        let json: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        for c in json
            .get("class")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if c.get("casterProgression").is_none() || c.get("_copy").is_some() {
                continue;
            }
            let id = format!(
                "ID_5ET_{}_CLASS_{}",
                fragment(str_of(c, "source")),
                fragment(str_of(c, "name"))
            );
            if imported.contains_key(&id) {
                out.push(Caster {
                    name: str_of(c, "name").to_string(),
                    class_id: id,
                    sub_id: None,
                    ruleset: ruleset_of(c),
                    raw: c.clone(),
                });
            }
        }
        for sc in json
            .get("subclass")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if sc.get("casterProgression").is_none() || sc.get("_copy").is_some() {
                continue;
            }
            let class_id = format!(
                "ID_5ET_{}_CLASS_{}",
                fragment(str_of(sc, "classSource")),
                fragment(str_of(sc, "className"))
            );
            let sub_id = format!(
                "ID_5ET_{}_SUBCLASS_{}_{}",
                fragment(str_of(sc, "source")),
                fragment(str_of(sc, "className")),
                fragment(
                    sc.get("shortName")
                        .and_then(Value::as_str)
                        .unwrap_or(str_of(sc, "name"))
                )
            );
            let Some(class) = imported.get(&class_id) else {
                continue;
            };
            if imported.contains_key(&sub_id) {
                out.push(Caster {
                    name: str_of(sc, "name").to_string(),
                    class_id,
                    sub_id: Some(sub_id),
                    ruleset: class.ruleset.unwrap_or_else(|| ruleset_of(sc)),
                    raw: sc.clone(),
                });
            }
        }
    }
    out
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

/// Fill every open element slot with its first option (spells included), the subclass with
/// `sub` when offered.
fn fill(e: &Engine, mut s: CharacterState, sub: Option<&str>) -> CharacterState {
    let mut refused = std::collections::HashSet::new();
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let Some(slot) = snap.slots.iter().find(|x| {
            x.chosen.is_none()
                && !x.options.is_empty()
                && !refused.contains(&x.key)
                && (x.ty != "Archetype" || sub.is_some_and(|t| x.options.iter().any(|o| o.id == t)))
        }) else {
            break;
        };
        let id = match (slot.ty.as_str(), sub) {
            ("Archetype", Some(t)) => t.to_string(),
            _ => slot.options[0].id.clone(),
        };
        match commands::select(e, &s, &slot.key, Choice::Element(id)) {
            Ok(o) => s = o.state,
            Err(_) => {
                refused.insert(slot.key.clone());
            }
        }
    }
    s
}

/// Problems of one caster at one level; empty when the slots match the table.
fn check(c: &Caster, snap: &CharacterSnapshot, level: u32) -> Vec<String> {
    let owner = c.sub_id.as_deref().unwrap_or(&c.class_id);
    let spell_slots: Vec<_> = snap
        .slots
        .iter()
        .filter(|s| s.ty == "Spell" && s.owner.id == owner)
        .collect();
    let mut out = Vec::new();
    let expected = c.expected_slots(level);
    let got = i64::try_from(spell_slots.len()).unwrap_or(-1);
    if got != expected {
        out.push(format!(
            "{owner} L{level}: {got} spell choice slots, table says {expected}"
        ));
    }
    let open = spell_slots.iter().filter(|s| s.chosen.is_none()).count();
    if open > 0 {
        out.push(format!(
            "{owner} L{level}: {open} spell choice slots cannot be filled"
        ));
    }
    let block = snap.spellcasting.iter().find(|s| s.name == c.name);
    let prepared = c.series("preparedSpellsProgression", level);
    if prepared > 0 {
        let limit = block.and_then(|b| b.prepare_limit).map_or(0, i64::from);
        if limit != prepared {
            out.push(format!(
                "{owner} L{level}: prepare limit {limit}, table says {prepared}"
            ));
        }
    }
    if c.raw.get("preparedSpells").is_some()
        && level >= 5
        && block.and_then(|b| b.prepare_limit).unwrap_or(0) <= 0
    {
        out.push(format!("{owner} L{level}: prepared formula gives no limit"));
    }
    out
}

/// Can this caster acquire spells at all by level 20 (choice slots or a prepare limit)?
fn acquires_spells(c: &Caster, snap: &CharacterSnapshot) -> bool {
    let owner = c.sub_id.as_deref().unwrap_or(&c.class_id);
    snap.slots
        .iter()
        .any(|s| s.ty == "Spell" && s.owner.id == owner)
        || snap
            .spellcasting
            .iter()
            .any(|b| b.name == c.name && b.prepare_limit.unwrap_or(0) > 0)
}

#[test]
fn five_etools_casters_fill_their_class_tables() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_casters: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn");
    handle.join().expect("caster sweep thread");
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
    let mut imported: BTreeMap<String, Element> = BTreeMap::new();
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp.path()).unwrap())
            .unwrap();
        for e in pack.elements() {
            let mut e = e.clone();
            e.ruleset = e.ruleset.or(pack.ruleset);
            imported.insert(e.id.clone(), e);
        }
    }
    let store = builder.build();
    let engine = Engine::new(&store);
    let casters = corpus_casters(data, &imported);
    let editions: std::collections::BTreeSet<_> =
        casters.iter().map(|c| format!("{:?}", c.ruleset)).collect();
    println!(
        "five_etools_casters: {} casters ({} subclasses), editions {editions:?}",
        casters.len(),
        casters.iter().filter(|c| c.sub_id.is_some()).count()
    );
    assert!(casters.len() >= 20, "a full folder has at least 20 casters");
    assert_eq!(editions.len(), 2, "both editions are present");

    let mut problems = Vec::new();
    let mut without_spells = 0;
    let mut slots_checked = 0i64;
    for c in &casters {
        let mut s = character(&engine, c.ruleset, &c.class_id);
        for level in 1..=20 {
            if level > 1 {
                s = commands::level_up_main(&engine, &s, None)
                    .expect("level up")
                    .state;
            }
            s = fill(&engine, s, c.sub_id.as_deref());
            if CHECK_LEVELS.contains(&level) {
                let snap = engine.derive(&s);
                problems.extend(check(c, &snap, level));
                slots_checked += c.expected_slots(level);
                if level == 20 && !acquires_spells(c, &snap) {
                    without_spells += 1;
                    problems.push(format!("{} acquires no spells", c.class_id));
                }
            }
        }
    }
    println!(
        "five_etools_casters: {slots_checked} slots checked, {without_spells} casters without \
         any way to acquire spells, {} problems",
        problems.len()
    );
    for p in problems.iter().take(40) {
        println!("  {p}");
    }
    assert!(problems.is_empty(), "{} caster problems", problems.len());
}
