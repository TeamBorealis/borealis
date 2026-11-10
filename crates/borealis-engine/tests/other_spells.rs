//! Spells Aurora-format content grants with no spellcasting name (an invocation's at-will spell,
//! a subclass feature's rituals, a racial trait's innate spells) belong to no class block. Aurora
//! lists every such spell in one "Other Spells" section after the class pages (no ability, attack
//! or save; each spell named with where it came from), so Borealis gathers them in one last
//! `Other Spells` block: never one block per granting element.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::other_spells::{source_label, OTHER_SPELLS};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use std::path::Path;

/// Synthetic Aurora XML in the three shapes (plus a Sacred Oath whose spells name the Paladin's
/// spellcasting in another letter case): a class feature of a spellcasting class (a Warlock
/// invocation), a racial trait of a subrace, and a subclass feature of a class without
/// spellcasting (a Barbarian path). None of the grants names a spellcasting block.
fn aurora_xml() -> ContentFile {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Veil of Dusk" type="Class Feature" source="Test" id="ID_T_OS_INVOCATION_VEIL_OF_DUSK">
    <supports>Eldritch Invocation</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Spell" id="ID_PHB_SPELL_MAGE_ARMOR" />
    </rules>
  </element>
  <element name="Gloom Elf" type="Sub Race" source="Test" id="ID_T_OS_SUB_RACE_GLOOM_ELF">
    <supports>Elf</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Racial Trait" id="ID_T_OS_RACIAL_TRAIT_GLOOM_MAGIC" />
    </rules>
  </element>
  <element name="Gloom Magic" type="Racial Trait" source="Test" id="ID_T_OS_RACIAL_TRAIT_GLOOM_MAGIC">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Spell" id="ID_PHB_SPELL_DANCING_LIGHTS" level="1" />
      <grant type="Spell" id="ID_PHB_SPELL_FAERIE_FIRE" level="3" />
    </rules>
  </element>
  <element name="Path of the Test Beast" type="Archetype" source="Test" id="ID_T_OS_ARCHETYPE_TEST_BEAST">
    <supports>Primal Path</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Archetype Feature" id="ID_T_OS_ARCHETYPE_FEATURE_BEAST_SEEKER" level="3" />
    </rules>
  </element>
  <element name="Beast Seeker" type="Archetype Feature" source="Test" id="ID_T_OS_ARCHETYPE_FEATURE_BEAST_SEEKER">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Spell" id="ID_PHB_SPELL_SPEAK_WITH_ANIMALS" />
    </rules>
  </element>
  <element name="Oath of the Test Ward" type="Archetype" source="Test" id="ID_T_OS_ARCHETYPE_OATH_TEST_WARD">
    <supports>Sacred Oath</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Archetype Feature" id="ID_T_OS_ARCHETYPE_FEATURE_WARD_SPELLS" level="3" />
    </rules>
  </element>
  <element name="Ward Spells" type="Archetype Feature" source="Test" id="ID_T_OS_ARCHETYPE_FEATURE_WARD_SPELLS">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Spell" id="ID_PHB_SPELL_SANCTUARY" spellcasting="paladin" prepared="true" />
      <select type="Spell" name="Ward Pick" supports="Cleric,1" spellcasting="PALADIN" />
    </rules>
  </element>
</elements>"#;
    let conv = convert_xml(xml, "other-spells.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2014);
    file
}

fn engine() -> Option<Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let srd = root.join("srd-5.1");
    if !srd.join("pack.json").is_file() {
        eprintln!("SKIPPED: content/srd-5.1 not present");
        return None;
    }
    let b = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&srd))
        .expect("packs load")
        .add_file("other-spells.json", PackOrigin::Custom, aurora_xml());
    Some(Engine::new(&b.build()))
}

/// Fill the first open slot named `slot` with the option named `pick`.
fn choose(e: &Engine, s: &CharacterState, slot: &str, pick: &str) -> CharacterState {
    let snap = e.derive(s);
    let sl = snap
        .slots
        .iter()
        .find(|k| k.name == slot && k.chosen.is_none() && k.options.iter().any(|o| o.name == pick))
        .unwrap_or_else(|| {
            let open: Vec<&str> = snap
                .slots
                .iter()
                .filter(|k| k.chosen.is_none())
                .map(|k| k.name.as_str())
                .collect();
            panic!("no open slot {slot} offering {pick}; open: {open:?}")
        });
    let id = sl
        .options
        .iter()
        .find(|o| o.name == pick)
        .unwrap()
        .id
        .clone();
    commands::select(e, s, &sl.key, Choice::Element(id))
        .unwrap_or_else(|err| panic!("{slot}/{pick}: {err}"))
        .state
}

fn character(e: &Engine, race: &[(&str, &str)], class: &str, level: u32) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    for (slot, pick) in race {
        s = choose(e, &s, slot, pick);
    }
    s = choose(e, &s, "Class", class);
    for _ in 1..level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn blocks(snap: &CharacterSnapshot) -> Vec<&str> {
    snap.spellcasting.iter().map(|b| b.name.as_str()).collect()
}

fn names(snap: &CharacterSnapshot, block: &str) -> Vec<String> {
    snap.caster(block)
        .unwrap_or_else(|| panic!("no {block} block: {:?}", blocks(snap)))
        .spells
        .iter()
        .map(|x| x.name.clone())
        .collect()
}

#[test]
fn an_invocation_spell_without_a_spellcasting_name_joins_other_spells_after_the_warlock() {
    let Some(e) = engine() else { return };
    let s = character(&e, &[("Race", "Human")], "Warlock", 2);
    let s = choose(&e, &s, "Eldritch Invocation (Level 2)", "Veil of Dusk");
    let snap = e.derive(&s);
    assert_eq!(blocks(&snap), vec!["Warlock", OTHER_SPELLS]);
    assert!(!names(&snap, "Warlock").contains(&"Mage Armor".to_string()));
    assert_eq!(names(&snap, OTHER_SPELLS), vec!["Mage Armor"]);
    let other = snap.caster(OTHER_SPELLS).unwrap();
    assert!(other.other, "the block is marked as Aurora's Other Spells");
    // no casting ability: no DC, attack or slots (Aurora leaves them blank)
    assert!(other.ability.is_empty());
    assert_eq!((other.dc, other.attack, other.slots), (0, 0, [0; 9]));
    let mage_armor = &other.spells[0];
    assert!(mage_armor.known && !mage_armor.prepared && !mage_armor.always_prepared);
    let grant = mage_armor.granted_by.as_ref().unwrap();
    assert_eq!(grant.name, "Veil of Dusk");
    assert_eq!(grant.class_name.as_deref(), Some("Warlock"));
    assert_eq!(source_label(mage_armor).as_deref(), Some("Veil of Dusk"));
}

#[test]
fn racial_spells_of_a_non_caster_join_other_spells_named_after_the_subrace() {
    let Some(e) = engine() else { return };
    let race = [("Race", "Elf"), ("Elven Subrace", "Gloom Elf")];
    let s = character(&e, &race, "Fighter", 1);
    let snap = e.derive(&s);
    assert_eq!(blocks(&snap), vec![OTHER_SPELLS]);
    // the level 3 grant waits for level 3
    assert_eq!(names(&snap, OTHER_SPELLS), vec!["Dancing Lights"]);
    let lights = &snap.caster(OTHER_SPELLS).unwrap().spells[0];
    assert_eq!(lights.granted_by.as_ref().unwrap().ty, "Racial Trait");
    // Aurora names a racial trait's spell after the race that granted the trait
    assert_eq!(source_label(lights).as_deref(), Some("Gloom Elf"));

    let s = character(&e, &race, "Fighter", 3);
    let snap = e.derive(&s);
    assert_eq!(blocks(&snap), vec![OTHER_SPELLS]);
    assert_eq!(
        names(&snap, OTHER_SPELLS),
        vec!["Dancing Lights", "Faerie Fire"]
    );
}

#[test]
fn a_subclass_spell_of_a_class_without_spellcasting_joins_other_spells() {
    let Some(e) = engine() else { return };
    let s = character(&e, &[("Race", "Human")], "Barbarian", 3);
    let s = choose(&e, &s, "Primal Path", "Path of the Test Beast");
    let snap = e.derive(&s);
    assert_eq!(blocks(&snap), vec![OTHER_SPELLS]);
    assert_eq!(names(&snap, OTHER_SPELLS), vec!["Speak with Animals"]);
    let spell = &snap.caster(OTHER_SPELLS).unwrap().spells[0];
    assert_eq!(source_label(spell).as_deref(), Some("Beast Seeker"));
}

#[test]
fn a_character_without_unlisted_spells_has_no_other_spells_block() {
    let Some(e) = engine() else { return };
    let s = character(&e, &[("Race", "Human")], "Warlock", 1);
    assert_eq!(blocks(&e.derive(&s)), vec!["Warlock"]);
    let s = character(&e, &[("Race", "Human")], "Fighter", 1);
    assert!(blocks(&e.derive(&s)).is_empty());
}

/// Aurora matches a grant's or select's `spellcasting` name to a class's spellcasting ignoring
/// case (`SpellcasterSelectionControlViewModel`: `StringComparison.OrdinalIgnoreCase`), and
/// community content writes `spellcasting="paladin"`: those spells are the Paladin's, with its
/// save DC, not Other Spells.
#[test]
fn a_spellcasting_name_in_another_letter_case_joins_the_class_block() {
    let Some(e) = engine() else { return };
    let s = character(&e, &[("Race", "Human")], "Paladin", 3);
    let s = choose(&e, &s, "Sacred Oath", "Oath of the Test Ward");
    let s = choose(&e, &s, "Ward Pick", "Bless");
    let snap = e.derive(&s);
    assert_eq!(blocks(&snap), vec!["Paladin"]);
    let paladin = snap.caster("Paladin").unwrap();
    assert_eq!(paladin.ability, "Charisma");
    let sanctuary = paladin
        .spells
        .iter()
        .find(|x| x.name == "Sanctuary")
        .expect("Sanctuary");
    assert!(sanctuary.always_prepared, "granted prepared=\"true\"");
    assert!(paladin.spells.iter().any(|x| x.name == "Bless" && x.known));
}
