//! Decision IM12a: a stat block whose armor class or hit points are a formula (kept as text) has
//! no fixed numbers, so the encounter builder leaves it out, as it does a creature without a
//! challenge rating. SYNTHETIC creatures built from the bundled Goblin Warrior only.

use std::path::Path;

use borealis_assist::monster_index;
use borealis_content::{ContentFile, ContentStoreBuilder, MonsterData, PackOrigin, Ruleset};

const PLAIN: &str = "ID_TEST_MONSTER_PLAIN_BRAMBLEKIN";
const AC_FORMULA: &str = "ID_TEST_MONSTER_AC_FORMULA_BRAMBLEKIN";
const HP_FORMULA: &str = "ID_TEST_MONSTER_HP_FORMULA_BRAMBLEKIN";

#[test]
fn a_stat_block_with_a_formula_armor_class_or_hit_points_is_not_an_encounter_monster() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/srd-5.2.1-monsters");
    let base = ContentStoreBuilder::new()
        .add_pack_dir(&dir)
        .expect("pack loads");
    let warrior = base
        .clone()
        .build()
        .by_type("Monster")
        .find(|s| s.element.name == "Goblin Warrior")
        .expect("goblin warrior")
        .element
        .clone();
    let plain = MonsterData::from_element(&warrior).unwrap();
    let mut ac_formula = plain.clone();
    ac_formula.ac.clear();
    ac_formula.ac_note = Some("10 + the bramble's age".into());
    let mut hp_formula = plain.clone();
    hp_formula.hp = None;
    hp_formula.hp_note = Some("4 per bramble".into());
    let file = ContentFile {
        elements: vec![
            plain.to_element(PLAIN, "Plain Bramblekin", "Test Homebrew"),
            ac_formula.to_element(AC_FORMULA, "Walled Bramblekin", "Test Homebrew"),
            hp_formula.to_element(HP_FORMULA, "Rooted Bramblekin", "Test Homebrew"),
        ],
        ..ContentFile::default()
    };
    let store = base
        .add_file("homebrew.json", PackOrigin::Custom, file)
        .build();
    let ids: Vec<String> = monster_index(&store, Ruleset::Dnd2024)
        .into_iter()
        .map(|m| m.id)
        .filter(|id| id.starts_with("ID_TEST_"))
        .collect();
    assert_eq!(ids, [PLAIN]);
}
