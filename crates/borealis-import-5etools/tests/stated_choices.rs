//! Choices a class feature states only in its prose, applied by the engine on a built
//! character: a creature chosen through a bestiary filter (filled from the bundled SRD 5.1
//! stat blocks) and a skill choice whose chosen skills get a doubled proficiency bonus.
//! Invented data only (source `ZZZ`, nothing from a book).

mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, MonsterData, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::{json, Value};
use std::path::Path;
use support::core_pack;

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

fn invented(dir: &Path) {
    let feature = |name: &str, text: &str| {
        json!({"name": name, "source": "ZZZ", "className": "Tamer", "classSource": "ZZZ",
               "level": 1, "entries": [text]})
    };
    write(
        dir,
        "class/class-tamer.json",
        &json!({
            "class": [{"name": "Tamer", "source": "ZZZ", "hd": {"number": 1, "faces": 8},
                       "proficiency": ["wis", "cha"],
                       "classFeatures": ["Beast Bond|Tamer|ZZZ|1", "Wild Lore|Tamer|ZZZ|1"]}],
            "classFeature": [
                feature("Beast Bond", "A beast joins you. Choose a {@filter small beast|bestiary|\
                    challenge rating=[&0;&1/4]|type=beast|size=t;s;m|miscellaneous=!swarm}."),
                feature("Wild Lore", "You become proficient in your choice of two of the \
                    following skills: {@skill Arcana}, {@skill Insight}, or {@skill Medicine}. \
                    Your proficiency bonus is doubled for any ability check you make that uses \
                    either of those skills."),
            ]
        }),
    );
}

fn content() -> std::path::PathBuf {
    core_pack().join("..")
}

/// Invented creatures in `bestiary/`: two fit the Beast Bond filter (one through a `_copy`
/// with a template and a `_mod`), one is too large, one cannot be read (its hit points are
/// not a number).
fn invented_creatures(dir: &Path) {
    let beast = |name: &str, size: &str, cr: &str| {
        json!({"name": name, "source": "ZZZ", "size": [size], "type": "beast",
               "alignment": ["U"], "ac": [12], "hp": {"average": 9, "formula": "2d8"},
               "speed": {"walk": 40}, "str": 12, "dex": 14, "con": 11, "int": 3, "wis": 12,
               "cha": 6, "passive": 11, "cr": cr,
               "action": [{"name": "Bite", "entries": ["{@atk mw} {@hit 3} to hit, reach 5 ft., \
                   one target. {@h}4 ({@damage 1d6 + 1}) piercing damage."]}]})
    };
    let mut odd = beast("Probe Shade", "M", "1/4");
    odd["hp"] = json!({"special": "half the tamer's"});
    write(
        dir,
        "bestiary/bestiary-zzz.json",
        &json!({"monster": [
            beast("Probe Hound", "M", "1/8"),
            beast("Probe Bear", "L", "1/4"),
            odd,
            {"name": "Probe Hound Elder", "source": "ZZZ", "cr": "1/4",
             "_copy": {"name": "Probe Hound", "source": "ZZZ",
                       "_templates": [{"name": "Keen", "source": "ZZZ"}],
                       "_mod": {"_": {"mode": "addSkills", "skills": {"perception": 1}}}}},
        ]}),
    );
    write(
        dir,
        "bestiary/template.json",
        &json!({"monsterTemplate": [{"name": "Keen", "source": "ZZZ", "apply": {
            "_root": {"size": ["S"]},
            "_mod": {"_": {"mode": "addSenses", "senses": [{"type": "darkvision", "range": 60}]}}}}]}),
    );
}

fn store(tmp: &Path) -> ContentStore {
    store_with(tmp, &["core", "srd-5.1", "srd-5.1-monsters"]).0
}

fn store_with(tmp: &Path, packs: &[&str]) -> (ContentStore, borealis_import_5etools::ImportResult) {
    let data = tmp.join("data");
    invented(&data);
    invented_creatures(&data);
    let options = ImportOptions {
        srd_dirs: vec![content().join("srd-5.1"), content().join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let result = import_folder(&data, &options).expect("invented data imports");
    let mut builder = ContentStoreBuilder::new();
    for pack in packs {
        builder = builder
            .add_pack_dir(&content().join(pack))
            .expect("bundled pack");
    }
    for pack in &result.packs {
        let dir = pack.write_to(&tmp.join("packs")).expect("write pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    (builder.build(), result)
}

fn tamer(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let mut s = commands::set_base_scores(e, &s, [10, 14, 12, 15, 13, 8])
        .expect("scores")
        .state;
    for (slot, id) in [
        ("Race", "ID_RACE_HUMAN"),
        ("Class", "ID_5ET_ZZZ_CLASS_TAMER"),
    ] {
        let key =
            slot_key(&e.derive(&s), "ID_LEVEL_1", slot, 1).unwrap_or_else(|| panic!("{slot} slot"));
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id}: {err}"))
            .state;
    }
    s
}

/// The SRD 5.1 beasts no larger than Medium of challenge rating 1/4 or lower.
fn small_srd_beasts(store: &ContentStore) -> Vec<String> {
    let mut out: Vec<String> = store
        .by_type("Monster")
        .filter_map(|e| {
            let m = MonsterData::from_element(&e.element).ok()?;
            Some((e.element.name.clone(), m))
        })
        .filter(|(_, m)| {
            m.creature_type.as_str() == "beast"
                && m.size
                    .iter()
                    .all(|s| ["tiny", "small", "medium"].contains(&s.as_str()))
                && ["0", "1/8", "1/4"].contains(&m.cr.as_text().as_str())
        })
        .map(|(n, _)| n)
        .collect();
    out.sort();
    out
}

#[test]
fn a_bestiary_filter_offers_the_loaded_beasts_it_fits() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store(tmp.path());
    let e = Engine::new(&store);
    let snap = e.derive(&tamer(&e));
    let slot = snap
        .slots
        .iter()
        .find(|s| s.ty == "Companion")
        .expect("companion slot");
    let mut offered: Vec<String> = slot.options.iter().map(|o| o.name.clone()).collect();
    offered.sort();
    let wanted = small_srd_beasts(&store);
    assert!(wanted.len() > 20, "{wanted:?}");
    assert_eq!(offered, wanted);
}

fn companion_names(e: &Engine) -> Vec<String> {
    let snap = e.derive(&tamer(e));
    let slot = snap
        .slots
        .iter()
        .find(|s| s.ty == "Companion")
        .expect("companion slot");
    let mut offered: Vec<String> = slot.options.iter().map(|o| o.name.clone()).collect();
    offered.sort();
    offered
}

#[test]
fn imported_creatures_fill_the_companion_select() {
    let tmp = tempfile::tempdir().unwrap();
    // no bundled monster pack: only the imported creatures can fill it
    let (store, result) = store_with(tmp.path(), &["core", "srd-5.1"]);
    let e = Engine::new(&store);
    assert_eq!(companion_names(&e), ["Probe Hound", "Probe Hound Elder"]);
    let elder = store
        .by_type("Monster")
        .find(|x| x.element.name == "Probe Hound Elder")
        .expect("copied creature");
    let m = MonsterData::from_element(&elder.element).unwrap();
    assert_eq!(m.size[0].as_str(), "small", "template _root");
    assert_eq!(m.senses.darkvision, Some(60), "template _mod");
    assert_eq!(
        m.skills.values().copied().collect::<Vec<_>>(),
        [3],
        "the copy's _mod"
    );
    assert_eq!((m.cr.as_text(), m.xp), ("1/4".to_string(), 50));
    let skipped: Vec<&str> = result
        .report
        .skipped
        .iter()
        .filter(|s| s.kind == "monster")
        .map(|s| s.reason.as_str())
        .collect();
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert!(skipped[0].starts_with("creature stat block: hit points are not a number"));
    // with the bundled SRD 5.1 beasts loaded too, both are offered
    let (both, _) = store_with(
        &tmp.path().join("b"),
        &["core", "srd-5.1", "srd-5.1-monsters"],
    );
    let names = companion_names(&Engine::new(&both));
    assert!(names.contains(&"Probe Hound".to_string()) && names.contains(&"Wolf".to_string()));
    assert_eq!(names.len(), small_srd_beasts(&both).len());
}

#[test]
fn chosen_skills_get_proficiency_and_expertise() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store(tmp.path());
    let e = Engine::new(&store);
    let mut s = tamer(&e);
    for pick in ["Wild Lore (Arcana)", "Wild Lore (Medicine)"] {
        let snap = e.derive(&s);
        let slot = snap
            .slots
            .iter()
            .find(|x| x.ty == "Class Feature" && x.chosen.is_none() && x.name == "Wild Lore")
            .expect("open Wild Lore slot");
        assert_eq!(slot.options.iter().filter(|o| o.name == pick).count(), 1);
        let id = slot
            .options
            .iter()
            .find(|o| o.name == pick)
            .unwrap()
            .id
            .clone();
        s = commands::select(&e, &s, &slot.key, Choice::Element(id))
            .expect("pick")
            .state;
    }
    let snap = e.derive(&s);
    let skill = |name: &str| {
        snap.skills
            .iter()
            .find(|k| k.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                panic!(
                    "{name} in {:?}",
                    snap.skills.iter().map(|k| &k.name).collect::<Vec<_>>()
                )
            })
    };
    for name in ["Arcana", "Medicine"] {
        assert!(skill(name).proficient && skill(name).expertise, "{name}");
    }
    assert!(!skill("Insight").proficient);
}
