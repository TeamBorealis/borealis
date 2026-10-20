//! A 5etools Monster Manual creature is the bundled SRD creature of the same name and edition:
//! with the SRD monster packs as `srd_dirs`, the importer gives each MM (2014) creature
//! `maps_to {dnd2014: <SRD 5.1 id>, kind: equivalent}` and each XMM (2024) creature
//! `maps_to {dnd2024: <SRD 5.2.1 id>, kind: equivalent}`. The engine's equivalence classes
//! (`borealis-engine/src/universe.rs index_equivalents`) then make the two one creature (the
//! duplicate check), and as both are of one edition neither is superseded (only a foreign
//! element a native one replaces is, `mark_superseded`). The cross-edition pair keeps the SRD
//! packs' own `approximate` link, so no edition hides the other's creature.

use borealis_content::{ContentStoreBuilder, Element, MapKind, Ruleset};
use borealis_engine::Engine;
use borealis_import_5etools::{import_folder, ImportOptions};
use std::path::{Path, PathBuf};

const SRD51_WOLF: &str = "ID_SRD51_MONSTER_WOLF";
const SRD521_WOLF: &str = "ID_SRD521_MONSTER_WOLF";

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// The fixture creatures (SRD-flagged MM / XMM entries) imported with the bundled SRD packs.
fn imported() -> Vec<Element> {
    imported_packs()
        .packs
        .iter()
        .flat_map(|p| p.elements())
        .filter(|e| e.ty == "Monster")
        .cloned()
        .collect()
}

fn imported_packs() -> borealis_import_5etools::ImportResult {
    let tmp = tempfile::tempdir().unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/creatures/bestiary");
    std::fs::create_dir_all(tmp.path().join("bestiary")).unwrap();
    for f in std::fs::read_dir(&from).unwrap() {
        let f = f.unwrap().path();
        std::fs::copy(&f, tmp.path().join("bestiary").join(f.file_name().unwrap())).unwrap();
    }
    let root = content();
    let opts = ImportOptions {
        srd_dirs: [
            "srd-5.1",
            "srd-5.2.1",
            "srd-5.1-monsters",
            "srd-5.2.1-monsters",
        ]
        .iter()
        .map(|p| root.join(p))
        .collect(),
        ..ImportOptions::default()
    };
    import_folder(tmp.path(), &opts).unwrap()
}

fn find<'a>(list: &'a [Element], name: &str, book: &str) -> &'a Element {
    list.iter()
        .find(|e| e.name == name && e.id.contains(&format!("_{book}_")))
        .unwrap_or_else(|| panic!("{name} ({book})"))
}

#[test]
fn every_monster_manual_fixture_creature_maps_to_its_srd_stat_block() {
    let monsters = imported();
    assert_eq!(monsters.len(), 6);
    let srd = ContentStoreBuilder::new()
        .add_pack_dir(&content().join("srd-5.1-monsters"))
        .and_then(|b| b.add_pack_dir(&content().join("srd-5.2.1-monsters")))
        .unwrap()
        .build();
    for m in &monsters {
        let maps = m.maps_to.as_ref().unwrap_or_else(|| panic!("{}", m.id));
        assert_eq!(maps.kind, Some(MapKind::Equivalent), "{}", m.id);
        // the pack file carries the ruleset; the stat block names its own
        let ruleset = borealis_content::MonsterData::from_element(m)
            .unwrap()
            .ruleset;
        let (target, edition) = match ruleset {
            Ruleset::Dnd2014 => (maps.dnd2014.as_deref(), Ruleset::Dnd2014),
            _ => (maps.dnd2024.as_deref(), Ruleset::Dnd2024),
        };
        let other = match edition {
            Ruleset::Dnd2014 => &maps.dnd2024,
            _ => &maps.dnd2014,
        };
        assert!(other.is_none(), "{}: no cross-edition link", m.id);
        let t = srd
            .get(target.unwrap())
            .unwrap_or_else(|| panic!("{} target", m.id));
        assert_eq!(
            (t.element.name.as_str(), t.element.ruleset),
            (m.name.as_str(), Some(edition))
        );
    }
    assert_eq!(
        find(&monsters, "Wolf", "MM")
            .maps_to
            .as_ref()
            .unwrap()
            .dnd2014
            .as_deref(),
        Some(SRD51_WOLF)
    );
    assert_eq!(
        find(&monsters, "Wolf", "XMM")
            .maps_to
            .as_ref()
            .unwrap()
            .dnd2024
            .as_deref(),
        Some(SRD521_WOLF)
    );
}

#[test]
fn the_engine_treats_the_imported_and_the_srd_creature_as_one() {
    let monsters = imported();
    let mm_wolf = find(&monsters, "Wolf", "MM").id.clone();
    let xmm_wolf = find(&monsters, "Wolf", "XMM").id.clone();
    let root = content();
    let mut builder = ContentStoreBuilder::new();
    for pack in [
        "core",
        "srd-5.1",
        "srd-5.1-monsters",
        "srd-5.2.1",
        "srd-5.2.1-monsters",
    ] {
        builder = builder.add_pack_dir(&root.join(pack)).unwrap();
    }
    // the imported packs as the app installs them
    let out = tempfile::tempdir().unwrap();
    for pack in &imported_packs().packs {
        let dir = pack.write_to(out.path()).unwrap();
        builder = builder.add_pack_dir(&dir).unwrap();
    }
    let store = builder.build();
    for (id, ruleset) in [(&mm_wolf, Ruleset::Dnd2014), (&xmm_wolf, Ruleset::Dnd2024)] {
        assert_eq!(
            store.get(id).unwrap().element.ruleset,
            Some(ruleset),
            "{id}"
        );
    }
    let engine = Engine::new(&store);
    for (ruleset, native, srd) in [
        (Ruleset::Dnd2014, &mm_wolf, SRD51_WOLF),
        (Ruleset::Dnd2024, &xmm_wolf, SRD521_WOLF),
    ] {
        let uni = engine.universe(ruleset);
        assert_eq!(uni.equivalent_ids(native), [srd], "{ruleset:?}");
        assert_eq!(uni.equivalent_ids(srd), [native.as_str()], "{ruleset:?}");
        for id in [
            &mm_wolf,
            &xmm_wolf,
            &SRD51_WOLF.to_string(),
            &SRD521_WOLF.to_string(),
        ] {
            assert!(!uni.get(id).unwrap().superseded, "{ruleset:?} {id}");
        }
    }
}
