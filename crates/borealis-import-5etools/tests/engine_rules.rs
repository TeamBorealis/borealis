//! Converted rules applied by the engine on a built character: starting equipment packages,
//! item spell attack / save DC and proficiency bonus bonuses, condition immunities, languages
//! and proficiencies an item's text grants, chosen ability bonuses, uncapped medium armor.
//! Invented data only (source `ZZZ`, nothing from a book).

mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use serde_json::{json, Value};
use std::path::Path;
use support::core_pack;

const SCORES: [i32; 6] = [10, 16, 14, 15, 12, 8];

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

fn invented(dir: &Path) {
    write(
        dir,
        "items.json",
        &json!({"item": [
            {"name": "Spark Wand", "source": "ZZZ", "type": "WD", "rarity": "uncommon",
             "bonusSpellAttack": "+2", "bonusSpellSaveDc": "+1", "entries": ["It crackles."]},
            {"name": "Calm Charm", "source": "ZZZ", "wondrous": true, "rarity": "rare",
             "conditionImmune": ["frightened"], "grantsLanguage": true, "grantsProficiency": true,
             "bonusProficiencyBonus": "+1",
             "entries": ["You can speak, read, and write Dwarvish.",
                         "You gain proficiency in the {@skill History} skill."]},
            {"name": "Odd Tome", "source": "ZZZ", "wondrous": true, "rarity": "rare",
             "ability": {"choose": [{"from": ["str", "dex"], "count": 1, "amount": 2}]},
             "entries": ["Study it."]}
        ]}),
    );
    write(
        dir,
        "items-base.json",
        &json!({"baseitem": [
            {"name": "Scale Shirt", "source": "ZZZ", "type": "MA", "ac": 14, "weight": 20,
             "dexterityMax": null}
        ]}),
    );
    write(
        dir,
        "backgrounds.json",
        &json!({"background": [
            {"name": "Traveller", "source": "ZZZ", "skillProficiencies": [{"insight": true}],
             "weaponProficiencies": [{"choose": {"fromFilter": "type=martial weapon|property=finesse", "count": 1}}],
             "startingEquipment": [
                {"_": [{"item": "spark wand|zzz"}, {"value": 1000}]},
                {"a": [{"special": "walking stick"}], "b": [{"value": 500}]}
             ],
             "entries": ["You have been places."]}
        ]}),
    );
}

fn store(tmp: &Path) -> ContentStore {
    let content = core_pack().join("..");
    let data = tmp.join("data");
    invented(&data);
    let options = ImportOptions {
        srd_dirs: vec![content.join("srd-5.1"), content.join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let result = import_folder(&data, &options).expect("invented data imports");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        builder = builder
            .add_pack_dir(&content.join(pack))
            .expect("bundled pack");
    }
    for pack in &result.packs {
        let dir = pack.write_to(&tmp.join("packs")).expect("write pack");
        builder = builder.add_pack_dir(&dir).expect("imported pack loads");
    }
    builder.build()
}

fn wizard(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let mut s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    for (slot, id) in [
        ("Race", "ID_RACE_HUMAN"),
        ("Background", "ID_5ET_ZZZ_BACKGROUND_TRAVELLER"),
        ("Class", "ID_WOTC_PHB_CLASS_WIZARD"),
    ] {
        let key =
            slot_key(&e.derive(&s), "ID_LEVEL_1", slot, 1).unwrap_or_else(|| panic!("{slot} slot"));
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id}: {err}"))
            .state;
    }
    s
}

/// Add and equip an item; the snapshot before and after.
fn equipped(e: &Engine, s: &CharacterState, id: &str) -> (CharacterState, CharacterSnapshot) {
    let s = commands::add_item(e, s, id, 1).expect("add").state;
    let uid = s.inventory.items.last().unwrap().uid.clone();
    let s = commands::equip(e, &s, &uid, None).expect("equip").state;
    let snap = e.derive(&s);
    (s, snap)
}

fn has_element(snap: &CharacterSnapshot, id: &str) -> bool {
    snap.elements.iter().any(|r| r.element.id == id)
}

#[test]
fn converted_rules_apply_on_a_built_character() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store(tmp.path());
    let e = Engine::new(&store);
    let s = wizard(&e);
    let base = e.derive(&s);

    // the background's starting equipment: package A = the fixed items + choice (a)
    let pkg = base
        .equipment_packages
        .iter()
        .find(|p| p.owner == "Traveller")
        .expect("background package offered");
    assert_eq!(pkg.options.len(), 2);
    let second = base
        .equipment_packages
        .iter()
        .filter(|p| p.owner == "Traveller")
        .count();
    assert_eq!(second, 1, "one choice group, one package");
    let taken = commands::choose_equipment_package(&e, &s, &pkg.package, "B")
        .expect("option B")
        .state;
    let snap = e.derive(&taken);
    assert_eq!(
        snap.inventory.coins.gp, 15,
        "10 gp fixed + 5 gp of option B"
    );
    assert!(snap.inventory.items.iter().any(|l| l.name == "Spark Wand"));

    // a weapon proficiency chosen by item filter offers the matching core proficiencies
    let filtered = base
        .slots
        .iter()
        .find(|x| x.ty == "Proficiency" && x.name.starts_with("Weapon Proficiency"))
        .expect("weapon proficiency choice");
    let offered: Vec<&str> = filtered.options.iter().map(|o| o.id.as_str()).collect();
    assert!(
        offered.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_RAPIER"),
        "{offered:?}"
    );
    assert!(!offered.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_LONGSWORD"));
    assert!(!offered.contains(&"ID_PROFICIENCY_WEAPON_PROFICIENCY_DAGGER"));

    // spell attack +2 and save DC +1 on the wizard's block while the wand is equipped
    let (_, wand) = equipped(&e, &s, "ID_5ET_ZZZ_ITEM_SPARK_WAND");
    let (b, w) = (&base.spellcasting[0], &wand.spellcasting[0]);
    assert_eq!(w.attack, b.attack + 2);
    assert_eq!(w.dc, b.dc + 1);

    // condition immunity, Dwarvish, History and +1 proficiency bonus from the charm
    let (_, charm) = equipped(&e, &s, "ID_5ET_ZZZ_ITEM_CALM_CHARM");
    assert!(charm
        .defenses
        .condition_immunities
        .iter()
        .any(|d| d.name == "frightened" && d.active));
    assert_eq!(charm.proficiency_bonus, base.proficiency_bonus + 1);
    assert!(!has_element(&base, "ID_LANGUAGE_DWARVISH"));
    assert!(has_element(&charm, "ID_LANGUAGE_DWARVISH"));
    assert!(has_element(&charm, "ID_PROFICIENCY_SKILL_HISTORY"));

    // the tome opens a +2 ability choice owned by the item
    let (_, tome) = equipped(&e, &s, "ID_5ET_ZZZ_ITEM_ODD_TOME");
    assert!(tome
        .slots
        .iter()
        .any(|x| x.owner.id == "ID_5ET_ZZZ_ITEM_ODD_TOME"));

    // medium armor without a Dex cap: 14 + Dex 3
    let (_, shirt) = equipped(&e, &s, "ID_5ET_ZZZ_ITEM_SCALE_SHIRT");
    assert_eq!(shirt.armor_class, 17);
}
