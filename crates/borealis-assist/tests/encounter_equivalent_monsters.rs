//! An imported creature that declares itself the bundled SRD creature of its edition (`maps_to`
//! kind `equivalent`, as the 5etools importer writes for Monster Manual creatures) is one
//! creature: the encounter pool lists the SRD stat block once, not a duplicate. A creature
//! linked only `approximate`ly, or to the other edition, is still its own entry. SYNTHETIC
//! elements built from the bundled SRD 5.1 Wolf only.

use std::path::Path;

use borealis_assist::monster_index;
use borealis_content::{
    ContentFile, ContentStoreBuilder, MapKind, MapsTo, MonsterData, PackOrigin, Ruleset,
};

const SRD51_WOLF: &str = "ID_SRD51_MONSTER_WOLF";
const TWIN: &str = "ID_TEST_MM_MONSTER_WOLF";
const LOOKALIKE: &str = "ID_TEST_ZZZ_MONSTER_WOLF";

fn link(kind: MapKind) -> Option<MapsTo> {
    Some(MapsTo {
        dnd2014: Some(SRD51_WOLF.into()),
        kind: Some(kind),
        ..MapsTo::default()
    })
}

#[test]
fn an_equivalent_imported_creature_is_not_a_second_encounter_monster() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/srd-5.1-monsters");
    let base = ContentStoreBuilder::new()
        .add_pack_dir(&dir)
        .expect("pack loads");
    let wolf = base
        .clone()
        .build()
        .element(SRD51_WOLF)
        .expect("srd wolf")
        .clone();
    let data = MonsterData::from_element(&wolf).unwrap();
    let mut twin = data.to_element(TWIN, "Wolf", "Test Monster Manual");
    twin.maps_to = link(MapKind::Equivalent);
    let mut lookalike = data.to_element(LOOKALIKE, "Wolf", "Test Adventure");
    lookalike.maps_to = link(MapKind::Approximate);
    let file = ContentFile {
        elements: vec![twin, lookalike],
        ..ContentFile::default()
    };
    let store = base
        .add_file("imported.json", PackOrigin::Custom, file)
        .build();
    let wolves: Vec<String> = monster_index(&store, Ruleset::Dnd2014)
        .into_iter()
        .filter(|m| m.name == "Wolf")
        .map(|m| m.id)
        .collect();
    assert_eq!(wolves, [SRD51_WOLF, LOOKALIKE]);
}
