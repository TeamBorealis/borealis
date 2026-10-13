//! Conversion of the SRD-only fixture data (`tests/fixtures/data`, see its README).

mod support;

use borealis_import_5etools::{import_folder, scan_folder, ImportOptions};
use serde_json::Value;
use std::path::Path;
use support::*;

/// Every entity in the fixtures must carry an SRD flag.
fn assert_flagged(entry: &Value, context: &str) {
    let flagged = |v: &Value| {
        v.get("srd").is_some_and(|f| f != &Value::Bool(false))
            || v.get("srd52").is_some_and(|f| f != &Value::Bool(false))
    };
    let ok = flagged(entry) || entry.get("inherits").is_some_and(flagged);
    assert!(
        ok,
        "{context}: entry {:?} is not flagged srd/srd52",
        entry.get("name")
    );
}

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, files);
        } else if p.extension().is_some_and(|x| x == "json") {
            files.push(p);
        }
    }
}

#[test]
fn fixtures_contain_only_srd_flagged_entries() {
    let mut files = Vec::new();
    walk(&fixtures(), &mut files);
    assert!(files.len() >= 12, "fixture files missing");
    let mut entries = 0;
    for path in files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "gendata-spell-source-lookup.json" {
            // class lists only: { source: { spell: { class: {...} } } }
            for spells in doc.as_object().unwrap().values() {
                for node in spells.as_object().unwrap().values() {
                    let keys: Vec<_> = node.as_object().unwrap().keys().collect();
                    assert_eq!(keys, vec!["class"], "lookup keeps class lists only");
                }
            }
            continue;
        }
        for (prop, list) in doc.as_object().unwrap() {
            for entry in list.as_array().unwrap_or(&Vec::new()) {
                assert_flagged(entry, &format!("{name}/{prop}"));
                entries += 1;
            }
        }
    }
    assert!(entries > 100, "only {entries} fixture entries");
}

#[test]
fn scan_lists_the_four_fixture_sources() {
    let sources = scan_folder(&fixtures()).unwrap();
    let abbrs: Vec<_> = sources.iter().map(|s| s.abbr.as_str()).collect();
    assert_eq!(abbrs, vec!["DMG", "PHB", "XDMG", "XPHB"]);
    let phb = &sources[1];
    assert_eq!(phb.name, "Player's Handbook (2014)");
    assert_eq!(phb.ruleset, "dnd2014");
    assert_eq!(phb.counts["race"], 2);
    assert_eq!(phb.counts["subrace"], 2);
    assert_eq!(phb.counts["spell"], 12);
    assert_eq!(phb.counts["class"], 2);
    assert_eq!(sources[3].ruleset, "dnd2024");
}

#[test]
fn produces_one_pack_per_source_with_expected_metadata() {
    let result = import_fixtures();
    let ids: Vec<_> = result
        .packs
        .iter()
        .map(|p| p.manifest.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["5etools-dmg", "5etools-phb", "5etools-xdmg", "5etools-xphb"]
    );
    for pack in &result.packs {
        assert!(!pack.redistributable);
        assert_eq!(pack.origin, "5etools-import");
        assert!(
            pack.ruleset.is_some(),
            "{} should be single-ruleset",
            pack.source
        );
    }
    let report = &result.report;
    assert_eq!(report.packs.len(), 4);
    assert_eq!(
        report.skipped.len(),
        6,
        "only the six core-pack languages are skipped"
    );
    assert!(report.skipped.iter().all(|s| s.kind == "language"));
    assert!(report.converted > 150);
}

fn kinds(id: &str) -> (Option<String>, Option<Value>) {
    let result = import_fixtures();
    let el = find(&result, id).unwrap_or_else(|| panic!("{id} not imported"));
    (Some(el.ty.clone()), serde_json::to_value(el).ok())
}

#[test]
fn races_and_species() {
    let (ty, dwarf) = kinds("ID_5ET_PHB_RACE_DWARF");
    assert_eq!(ty.as_deref(), Some("Race"));
    let dwarf = dwarf.unwrap().to_string();
    assert!(
        dwarf.contains("ID_LANGUAGE_DWARVISH"),
        "language granted by core id"
    );
    assert!(
        dwarf.contains("ID_5ET_PHB_RACIALTRAIT_DWARF"),
        "racial traits split out"
    );
    assert!(
        dwarf.contains("ID_5ET_PHB_RACE_DWARF"),
        "subrace select supports the race"
    );

    assert_eq!(
        kinds("ID_5ET_PHB_SUBRACE_ELF_HIGH").0.as_deref(),
        Some("Sub Race")
    );
    assert_eq!(kinds("ID_5ET_PHB_RACE_ELF").0.as_deref(), Some("Race"));
    assert_eq!(
        kinds("ID_5ET_XPHB_SPECIES_HUMAN").0.as_deref(),
        Some("Species")
    );
    assert_eq!(
        kinds("ID_5ET_XPHB_SPECIES_DWARF").0.as_deref(),
        Some("Species")
    );
}

#[test]
fn classes_subclasses_and_features() {
    let result = import_fixtures();
    for (id, ty) in [
        ("ID_5ET_PHB_CLASS_FIGHTER", "Class"),
        ("ID_5ET_PHB_CLASS_WIZARD", "Class"),
        ("ID_5ET_XPHB_CLASS_FIGHTER", "Class"),
        ("ID_5ET_PHB_SUBCLASS_FIGHTER_CHAMPION", "Archetype"),
        ("ID_5ET_PHB_SUBCLASS_WIZARD_EVOCATION", "Archetype"),
        ("ID_5ET_XPHB_SUBCLASS_WIZARD_EVOKER", "Archetype"),
    ] {
        assert_eq!(find(&result, id).unwrap_or_else(|| panic!("{id}")).ty, ty);
    }
    let fighter = find(&result, "ID_5ET_PHB_CLASS_FIGHTER").unwrap();
    let json = serde_json::to_string(fighter).unwrap();
    assert!(json.contains("SECOND_WIND"), "class grants its features");
    assert!(fighter.multiclass.is_some());
    let features = result
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .filter(|e| e.ty == "Class Feature" && e.id.contains("_CLASSFEATURE_"))
        .count();
    assert!(features > 40, "only {features} class features");
    let wizard = serde_json::to_string(find(&result, "ID_5ET_XPHB_CLASS_WIZARD").unwrap()).unwrap();
    assert!(wizard.contains("spellcasting"), "wizard is a caster");
}

#[test]
fn spells_items_backgrounds_feats() {
    let result = import_fixtures();
    let fireball = find(&result, "ID_5ET_PHB_SPELL_FIREBALL").unwrap();
    assert_eq!(fireball.setter_value("level"), Some("3"));
    assert_eq!(fireball.setter_value("school"), Some("Evocation"));
    assert_eq!(
        fireball.supports,
        vec!["Sorcerer", "Wizard"],
        "class list from the lookup"
    );
    assert!(find(&result, "ID_5ET_XPHB_SPELL_MAGIC_MISSILE").is_some());

    assert_eq!(
        find(&result, "ID_5ET_PHB_ITEM_LONGSWORD").unwrap().ty,
        "Weapon"
    );
    assert_eq!(
        find(&result, "ID_5ET_PHB_ITEM_CHAIN_MAIL").unwrap().ty,
        "Armor"
    );
    assert_eq!(
        find(&result, "ID_5ET_DMG_ITEM_PLUS_1_LONGSWORD")
            .unwrap()
            .ty,
        "Weapon"
    );
    assert_eq!(
        find(&result, "ID_5ET_DMG_ITEM_BAG_OF_HOLDING").unwrap().ty,
        "Magic Item"
    );
    assert!(find(&result, "ID_5ET_XPHB_ITEMPROP_MASTERY_CLEAVE").is_some());
    assert!(find(&result, "ID_5ET_XPHB_ITEM_ARCANE_FOCUS").is_some());

    assert_eq!(
        find(&result, "ID_5ET_PHB_BACKGROUND_ACOLYTE").unwrap().ty,
        "Background"
    );
    assert!(find(
        &result,
        "ID_5ET_PHB_BGFEATURE_ACOLYTE_SHELTER_OF_THE_FAITHFUL"
    )
    .is_some());
    let acolyte24 =
        serde_json::to_string(find(&result, "ID_5ET_XPHB_BACKGROUND_ACOLYTE").unwrap()).unwrap();
    assert!(acolyte24.contains("select.ability_distribution"));
    assert!(
        acolyte24.contains("ID_5ET_XPHB_FEAT_MAGIC_INITIATE"),
        "origin feat granted"
    );

    assert_eq!(find(&result, "ID_5ET_XPHB_FEAT_ALERT").unwrap().ty, "Feat");
    assert!(find(&result, "ID_5ET_XPHB_FEAT_MAGIC_INITIATE").is_some());
    assert!(find(&result, "ID_5ET_PHB_SOURCE").is_some());
}

#[test]
fn variants_do_not_cross_editions() {
    let result = import_fixtures();
    // 2014 "+1 Weapon" applies to PHB bases only, 2024 variants to XPHB bases only: the
    // generated names are unique per source and no duplicate was dropped.
    assert!(result.report.skipped.iter().all(|s| s.kind != "item"));
    assert!(find(&result, "ID_5ET_XDMG_ITEM_PLUS_1_LONGSWORD").is_some());
    assert!(find(&result, "ID_5ET_DMG_ITEM_PLUS_1_LONGSWORD").is_some());
}

#[test]
fn only_srd_filter_keeps_all_fixture_entries() {
    let opts = ImportOptions {
        only_srd: true,
        ..ImportOptions::default()
    };
    let strict = import_folder(&fixtures(), &opts).unwrap();
    let all = import_fixtures();
    assert_eq!(strict.report, all.report);
}

#[test]
fn ruleset_and_source_filters() {
    let opts = ImportOptions {
        ruleset: Some(borealis_content::Ruleset::Dnd2024),
        ..ImportOptions::default()
    }
    .with_sources(["xphb"]);
    let r = import_folder(&fixtures(), &opts).unwrap();
    assert_eq!(r.packs.len(), 1);
    assert_eq!(r.packs[0].manifest.id, "5etools-xphb");
    assert!(find(&r, "ID_5ET_PHB_RACE_DWARF").is_none());
}

#[test]
fn srd_dirs_add_maps_to_links() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let opts = ImportOptions {
        srd_dirs: vec![root.join("srd-5.1"), root.join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let r = import_folder(&fixtures(), &opts).unwrap();
    let with_maps = r
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .filter(|e| e.maps_to.is_some())
        .count();
    assert!(
        with_maps > 20,
        "only {with_maps} elements map to SRD content"
    );
    let fireball = find(&r, "ID_5ET_PHB_SPELL_FIREBALL").unwrap();
    let target = fireball
        .maps_to
        .as_ref()
        .and_then(|m| m.dnd2014.clone())
        .unwrap();
    let store = store_with(&[]);
    let srd = borealis_content::ContentStoreBuilder::new()
        .add_pack_dir(&root.join("srd-5.1"))
        .unwrap()
        .build();
    assert!(
        srd.contains_id(&target) || store.contains_id(&target),
        "{target}"
    );
}
