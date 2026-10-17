//! Item shapes of `specs/handoffs/det-engine-1.md` applied by the engine on a built character:
//! a `+1` ammunition bundle adds to the bow that fires it, a weapon's own critical threshold,
//! ability-check and concentration-save bonuses, a toggled speed multiplier and an attunement
//! restriction read from `reqAttuneTags`. Plus IM8: an unflagged book published since the 2024
//! Player's Handbook imports as 2024 content. Invented data only (sources `ZZZ`, `NEWB`, `OLDB`).

mod support;

use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions, ImportResult};
use serde_json::{json, Value};
use std::path::Path;
use support::{core_pack, find};

const SCORES: [i32; 6] = [15, 14, 13, 12, 10, 8];

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

fn invented(dir: &Path) {
    write(
        dir,
        "items-base.json",
        &json!({"baseitem": [
            {"name": "Zip Bow", "source": "ZZZ", "type": "R", "weaponCategory": "martial",
             "property": ["A"], "dmg1": "1d6", "dmgType": "P", "range": "80/320",
             "ammoType": "zip arrow|zzz", "weight": 2},
            {"name": "Zip Arrow", "source": "ZZZ", "type": "A", "arrow": true, "weight": 0.05},
            {"name": "Zip Arrows (20)", "source": "ZZZ", "type": "A", "arrow": true, "weight": 1,
             "packContents": [{"item": "zip arrow|zzz", "quantity": 20}]}
        ]}),
    );
    write(
        dir,
        "magicvariants.json",
        &json!({"magicvariant": [
            {"name": "+1 Zip Ammunition", "type": "GV|ZZZ", "requires": [{"type": "A"}],
             "ammo": true, "inherits": {"namePrefix": "+1 ", "source": "ZZZ",
             "bonusWeapon": "+1", "rarity": "uncommon", "entries": ["Magic."]}}
        ]}),
    );
    write(
        dir,
        "items.json",
        &json!({"item": [
            {"name": "Keen Blade", "source": "ZZZ", "type": "M", "weaponCategory": "martial",
             "dmg1": "1d8", "dmgType": "S", "rarity": "rare", "critThreshold": 19,
             "entries": ["It is sharp."]},
            {"name": "Luck Pebble", "source": "ZZZ", "wondrous": true, "rarity": "uncommon",
             "bonusAbilityCheck": "+1", "bonusSavingThrowConcentration": "+2",
             "entries": ["Lucky."]},
            {"name": "Swift Shoes", "source": "ZZZ", "wondrous": true, "rarity": "rare",
             "modifySpeed": {"multiply": {"walk": 2}}, "entries": ["Click the heels."]},
            {"name": "Tower Rod", "source": "ZZZ", "type": "RD", "rarity": "rare",
             "reqAttune": "by the heir of the tower, who must be a wizard",
             "reqAttuneTags": [{"class": "wizard"}], "entries": ["Old."]}
        ]}),
    );
    write(
        dir,
        "books.json",
        &json!({"book": [
            {"name": "Player's Handbook (2024)", "id": "XPHB", "source": "XPHB",
             "published": "2024-09-17"},
            {"name": "New Book", "id": "NEWB", "source": "NEWB", "published": "2024-09-17"},
            {"name": "Old Book", "id": "OLDB", "source": "OLDB", "published": "2024-09-16"}
        ]}),
    );
    write(
        dir,
        "feats.json",
        &json!({"feat": [
            {"name": "New Knack", "source": "NEWB", "entries": ["New."]},
            {"name": "Flagged Knack", "source": "NEWB", "edition": "classic", "entries": ["Old."]},
            {"name": "Old Knack", "source": "OLDB", "entries": ["Old."]}
        ]}),
    );
}

fn import(tmp: &Path) -> (ImportResult, ContentStore) {
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
    (result, builder.build())
}

fn fighter(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let mut s = commands::set_base_scores(e, &s, SCORES)
        .expect("scores")
        .state;
    for (slot, id) in [
        ("Race", "ID_RACE_HUMAN"),
        ("Class", "ID_WOTC_PHB_CLASS_FIGHTER"),
    ] {
        let key =
            slot_key(&e.derive(&s), "ID_LEVEL_1", slot, 1).unwrap_or_else(|| panic!("{slot} slot"));
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{id}: {err}"))
            .state;
    }
    s
}

/// Add an item and, unless `carry_only`, equip it.
fn with_item(e: &Engine, s: &CharacterState, id: &str, carry_only: bool) -> CharacterState {
    let s = commands::add_item(e, s, id, 1)
        .unwrap_or_else(|err| panic!("add {id}: {err}"))
        .state;
    if carry_only {
        return s;
    }
    let uid = s.inventory.items.last().unwrap().uid.clone();
    commands::equip(e, &s, &uid, None)
        .unwrap_or_else(|err| panic!("equip {id}: {err}"))
        .state
}

fn attack<'a>(
    snap: &'a CharacterSnapshot,
    name: &str,
) -> &'a borealis_engine::snapshot::AttackSnapshot {
    snap.attacks
        .iter()
        .find(|a| a.name.contains(name))
        .unwrap_or_else(|| panic!("no {name} attack in {:?}", snap.attacks))
}

#[test]
fn imported_item_shapes_apply_on_a_built_character() {
    let tmp = tempfile::tempdir().unwrap();
    let (result, store) = import(tmp.path());
    // a toggle's own id is not a reference to another element
    assert!(
        result.report.external_references.is_empty(),
        "{:?}",
        result.report.external_references
    );
    let e = Engine::new(&store);
    let base = fighter(&e);

    // a +1 bundle filed under Equipment Packs is ammunition of the bow's kind
    let bundle = find(&result, "ID_5ET_ZZZ_ITEM_PLUS_1_ZIP_ARROWS_20").expect("bundle");
    assert_eq!(bundle.setter_value("category"), Some("Equipment Packs"));
    assert_eq!(bundle.setter_value("ammunition"), Some("arrow"));
    let bow = find(&result, "ID_5ET_ZZZ_ITEM_ZIP_BOW").expect("bow");
    assert_eq!(bow.setter_value("ammunition"), Some("arrow"));
    let armed = with_item(&e, &base, "ID_5ET_ZZZ_ITEM_ZIP_BOW", false);
    let before = attack(&e.derive(&armed), "Zip Bow").attack;
    let loaded = with_item(&e, &armed, &bundle.id, true);
    let snap = e.derive(&loaded);
    let shot = attack(&snap, "Zip Bow");
    assert_eq!(shot.attack, before + 1, "{shot:?}");
    assert!(shot.ammunition.is_some(), "{shot:?}");

    // the blade's critical range is its own: 19 on the blade, 20 on the bow
    let both = with_item(&e, &loaded, "ID_5ET_ZZZ_ITEM_KEEN_BLADE", false);
    let snap = e.derive(&both);
    assert_eq!(attack(&snap, "Keen Blade").crit_range, 19);
    assert_eq!(attack(&snap, "Zip Bow").crit_range, 20);

    // ability checks (initiative) +1, concentration saves +2
    let plain = e.derive(&base);
    let lucky = e.derive(&with_item(&e, &base, "ID_5ET_ZZZ_ITEM_LUCK_PEBBLE", false));
    assert_eq!(lucky.initiative, plain.initiative + 1);
    assert_eq!(lucky.concentration_save, plain.concentration_save + 2);

    // the shoes' toggle doubles the walking speed while on
    let shod = with_item(&e, &base, "ID_5ET_ZZZ_ITEM_SWIFT_SHOES", false);
    let snap = e.derive(&shod);
    let toggle = snap
        .toggles
        .iter()
        .find(|t| t.id == "ID_5ET_ZZZ_ITEM_SWIFT_SHOES_SPEED")
        .unwrap_or_else(|| panic!("no toggle in {:?}", snap.toggles));
    let on = commands::set_toggle(&e, &shod, &toggle.id, true).expect("toggle on");
    assert_eq!(e.derive(&on.state).speed.walk, 2 * snap.speed.walk);

    // "by the heir of the tower, who must be a wizard": the tag bars the fighter
    let rod = find(&result, "ID_5ET_ZZZ_ITEM_TOWER_ROD").expect("rod");
    let setter = rod.setter("attunement").expect("attunement setter");
    assert_eq!(setter.attribute("requirements"), Some("[wizard:1]"));
    let snap = e.derive(&with_item(&e, &base, &rod.id, false));
    let line = snap
        .inventory
        .items
        .iter()
        .find(|l| l.name == "Tower Rod")
        .expect("rod line");
    assert!(line.attunement_barred && !line.attuned, "{line:?}");
}

#[test]
fn unflagged_books_published_since_the_2024_handbook_are_2024_content() {
    let tmp = tempfile::tempdir().unwrap();
    let (result, _) = import(tmp.path());
    // an element's own ruleset, else its pack's (a single-edition pack states it once)
    let ruleset = |id: &str| {
        result
            .packs
            .iter()
            .find_map(|p| {
                let e = p.elements().find(|e| e.id == id)?;
                Some(e.ruleset.or(p.ruleset))
            })
            .unwrap_or_else(|| panic!("{id}"))
    };
    // published the day of the 2024 Player's Handbook, no edition flag: 2024
    assert_eq!(
        ruleset("ID_5ET_NEWB_FEAT_NEW_KNACK"),
        Some(Ruleset::Dnd2024)
    );
    // an explicit flag wins
    assert_eq!(
        ruleset("ID_5ET_NEWB_FEAT_FLAGGED_KNACK"),
        Some(Ruleset::Dnd2014)
    );
    // published the day before: 2014
    assert_eq!(
        ruleset("ID_5ET_OLDB_FEAT_OLD_KNACK"),
        Some(Ruleset::Dnd2014)
    );
    // the items of a book without a date keep their 2014 default
    assert_eq!(
        ruleset("ID_5ET_ZZZ_ITEM_KEEN_BLADE"),
        Some(Ruleset::Dnd2014)
    );
}
