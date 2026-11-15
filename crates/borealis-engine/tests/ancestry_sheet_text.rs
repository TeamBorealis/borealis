//! GitHub #52 and #53 (Aurora-format Dragonborn content): a draconic ancestry option sets its
//! damage type as an inline statistic (`<stat name="... ancestry:damage type" value="Fire"
//! inline="true"/>`) and describes itself by injecting the ancestry trait's description
//! (`<div element="ID_..."/>`).
//!
//! - #52: Aurora fills a `{{name}}` placeholder in sheet text from the statistic groups, then
//!   from the inline values (StatisticsHandler2.ReplaceDoubleBrackets), so the breath weapon's
//!   sheet text names the damage type.
//! - #53: Aurora's description panel replaces a `<div element="ID"/>` with the referenced
//!   element's upper-cased name and its description (ReplaceInjectedElementDescription), so the
//!   option has text to show.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::{CharacterSnapshot, FeatureDetail};
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use std::path::Path;

/// Synthetic Aurora XML in the shape of a Dragonborn file: a race, the ancestry trait whose
/// options set the damage type inline, and a breath weapon whose sheet text reads it.
fn aurora_xml() -> ContentFile {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Test Wyrmkin" type="Race" source="Test" id="ID_T_AST_RACE_WYRMKIN">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Racial Trait" id="ID_T_AST_RACIAL_TRAIT_ANCESTRY" />
      <grant type="Racial Trait" id="ID_T_AST_RACIAL_TRAIT_BREATH_WEAPON" />
    </rules>
  </element>
  <element name="Test Ancestry" type="Racial Trait" source="Test" id="ID_T_AST_RACIAL_TRAIT_ANCESTRY">
    <description><p>You descend from a test dragon. Gold dragons breathe fire.</p></description>
    <sheet display="false" />
    <rules>
      <select type="Racial Trait" name="Test Ancestry" supports="Test Ancestry" />
    </rules>
  </element>
  <element name="Gold" type="Racial Trait" source="Test" id="ID_T_AST_RACIAL_TRAIT_ANCESTRY_GOLD">
    <supports>Test Ancestry</supports>
    <compendium display="false" />
    <description>
      <div element="ID_T_AST_RACIAL_TRAIT_ANCESTRY" />
    </description>
    <sheet alt="Test Ancestry">
      <description>Your ancestor was a gold dragon.</description>
    </sheet>
    <rules>
      <stat name="test ancestry:damage type" value="Fire" inline="true" />
    </rules>
  </element>
  <element name="Breath Weapon" type="Racial Trait" source="Test" id="ID_T_AST_RACIAL_TRAIT_BREATH_WEAPON">
    <description><p>Synthetic.</p></description>
    <sheet action="Attack" usage="{{proficiency}}/Long Rest">
      <description>Each creature in the area must make a Dexterity saving throw against a DC{{breath weapon:dc}}. A creature takes {{breath weapon:dice count}}d10 {{test ancestry:damage type}} damage on a failed save.</description>
    </sheet>
    <rules>
      <stat name="breath weapon:dc" value="8" />
      <stat name="breath weapon:dc" value="constitution:modifier" />
      <stat name="breath weapon:dc" value="proficiency" />
      <stat name="breath weapon:dice count" value="1" level="1" bonus="base" />
    </rules>
  </element>
</elements>"#;
    let conv = convert_xml(xml, "ancestry-sheet-text.xml");
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
        .add_file("ancestry-sheet-text.json", PackOrigin::Custom, aurora_xml());
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
        .expect("option")
        .id
        .clone();
    commands::select(e, s, &sl.key, Choice::Element(id))
        .unwrap_or_else(|err| panic!("{slot}/{pick}: {err}"))
        .state
}

fn gold_wyrmkin(e: &Engine) -> CharacterSnapshot {
    borealis_engine::invariants::enable_for_thread();
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = choose(e, &s, "Race", "Test Wyrmkin");
    let s = choose(e, &s, "Class", "Fighter");
    let s = choose(e, &s, "Test Ancestry", "Gold");
    e.derive(&s)
}

fn feature<'a>(snap: &'a CharacterSnapshot, name: &str) -> &'a FeatureDetail {
    snap.features
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = snap.features.iter().map(|f| f.name.as_str()).collect();
            panic!("no feature {name}; features: {names:?}")
        })
}

#[test]
fn breath_weapon_sheet_text_names_the_ancestry_damage_type() {
    let Some(e) = engine() else { return };
    let snap = gold_wyrmkin(&e);
    let text = feature(&snap, "Breath Weapon")
        .sheet_summary
        .clone()
        .expect("sheet text");
    assert!(
        text.contains("takes 1d10 Fire damage on a failed save"),
        "the inline statistic fills the placeholder: {text}"
    );
    assert!(!text.contains("{{"), "no placeholder left: {text}");
}

#[test]
fn ancestry_option_description_injects_the_referenced_trait() {
    let Some(e) = engine() else { return };
    let snap = gold_wyrmkin(&e);
    let gold = feature(&snap, "Gold");
    assert!(
        gold.description.contains("<h5>TEST ANCESTRY</h5>"),
        "the injected element's upper-cased name heads its text: {}",
        gold.description
    );
    assert!(
        gold.description
            .contains("You descend from a test dragon. Gold dragons breathe fire."),
        "the injected element's description follows: {}",
        gold.description
    );
}
