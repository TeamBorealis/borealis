//! GitHub #62: the snapshot's feature list carries every feat the sheet prints.
//!
//! As Aurora's sheet does, a `Feat Feature` shown on the sheet is a feature, printed under its
//! `<sheet alt>` name; one the content hides is a recorded choice, not a feature. A feat that
//! hides itself (`<sheet display="false"/>`) is still listed on the sheet unless a part of it
//! that the sheet shows stands in for it (its chosen option, named after it).
//!
//! Made-up content in Aurora's XML shape, converted as an import is.

use borealis_content::{ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::FeatureDetail;
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use std::path::Path;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Patchwork" type="Background" source="Test" id="ID_T62_BACKGROUND_PATCHWORK">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Feat" id="ID_T62_FEAT_HARDY" />
      <grant type="Feat" id="ID_T62_FEAT_MINDMOVER" />
      <grant type="Feat" id="ID_T62_FEAT_DABBLER" />
    </rules>
  </element>
  <element name="Hardy" type="Feat" source="Test" id="ID_T62_FEAT_HARDY">
    <description><p>Your hit point maximum increases.</p></description>
    <sheet display="false" />
  </element>
  <element name="Mindmover" type="Feat" source="Test" id="ID_T62_FEAT_MINDMOVER">
    <description><p>Mindmover description.</p></description>
    <sheet display="false" />
    <rules><select type="Feat Feature" name="Ability Score Increase (Mindmover)" supports="ID_T62_FF_MINDMOVER_WISDOM" /></rules>
  </element>
  <element name="Wisdom" type="Feat Feature" source="Test" id="ID_T62_FF_MINDMOVER_WISDOM">
    <description><p>Your Wisdom score increases by 1.</p></description>
    <sheet alt="Mindmover"><description>Mindmover sheet text.</description></sheet>
  </element>
  <element name="Spell Dabbler" type="Feat" source="Test" id="ID_T62_FEAT_DABBLER">
    <description><p>Spell Dabbler description.</p></description>
    <sheet display="false" />
    <rules><select type="Feat Feature" name="Spell List (Spell Dabbler)" supports="ID_T62_FF_DABBLER_SAGE" /></rules>
  </element>
  <element name="Sage" type="Feat Feature" source="Test" id="ID_T62_FF_DABBLER_SAGE">
    <description><p>Sage list.</p></description>
    <sheet display="false" />
    <rules><select type="Feat Feature" name="Spellcasting Ability (Spell Dabbler)" supports="ID_T62_FF_DABBLER_SAGE_WISDOM" /></rules>
  </element>
  <element name="Wisdom" type="Feat Feature" source="Test" id="ID_T62_FF_DABBLER_SAGE_WISDOM">
    <description><p>Wisdom is your spellcasting ability for these spells.</p></description>
    <sheet alt="Spell Dabbler (Sage)"><description>Spell Dabbler (Sage) sheet text.</description></sheet>
  </element>
</elements>"#;

fn engine() -> Engine {
    let conv = convert_xml(XML, "feats-62.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2024);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs load")
        .add_file("feats-62.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, ty: &str, id: &str) -> CharacterState {
    let key = e
        .derive(s)
        .slots
        .iter()
        .find(|x| x.ty == ty && x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
        .map(|x| x.key.clone())
        .unwrap_or_else(|| panic!("no open {ty} slot offering {id}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn hero() -> CharacterSnapshot {
    let e = engine();
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s = pick(&e, &s, "Background", "ID_T62_BACKGROUND_PATCHWORK");
    s = pick(&e, &s, "Feat Feature", "ID_T62_FF_MINDMOVER_WISDOM");
    s = pick(&e, &s, "Feat Feature", "ID_T62_FF_DABBLER_SAGE");
    s = pick(&e, &s, "Feat Feature", "ID_T62_FF_DABBLER_SAGE_WISDOM");
    e.derive(&s)
}

fn feature<'a>(snap: &'a CharacterSnapshot, id: &str) -> Option<&'a FeatureDetail> {
    snap.features.iter().find(|f| f.id == id)
}

#[test]
fn a_feat_feature_the_sheet_shows_is_listed_under_its_sheet_name() {
    let snap = hero();
    let wisdom = feature(&snap, "ID_T62_FF_MINDMOVER_WISDOM").expect("listed");
    assert_eq!(wisdom.ty, "Feat Feature");
    assert!(wisdom.displayed);
    assert_eq!(wisdom.name, "Wisdom");
    assert_eq!(wisdom.sheet_name.as_deref(), Some("Mindmover"));
    assert_eq!(wisdom.granted_by.as_deref(), Some("Mindmover"));
    assert!(snap.sheet_summary.contains("Mindmover sheet text."));
}

#[test]
fn a_feat_feature_the_content_hides_is_not_a_feature() {
    let snap = hero();
    assert!(feature(&snap, "ID_T62_FF_DABBLER_SAGE").is_none());
    assert!(feature(&snap, "ID_T62_FF_DABBLER_SAGE_WISDOM").is_some_and(|f| f.displayed));
}

#[test]
fn a_hidden_feat_is_shown_unless_a_shown_part_of_it_stands_in_for_it() {
    let snap = hero();
    let shown = |id: &str| feature(&snap, id).map(|f| f.displayed);
    // nothing else names Hardy on the sheet: the feat itself is listed
    assert_eq!(shown("ID_T62_FEAT_HARDY"), Some(true));
    // the chosen option prints as "Mindmover", a grandchild as "Spell Dabbler (Sage)"
    assert_eq!(shown("ID_T62_FEAT_MINDMOVER"), Some(false));
    assert_eq!(shown("ID_T62_FEAT_DABBLER"), Some(false));
}

#[test]
fn a_feat_hidden_before_its_option_is_chosen_is_listed_meanwhile() {
    let e = engine();
    let s = pick(
        &e,
        &commands::new_character(&e, Ruleset::Dnd2024),
        "Background",
        "ID_T62_BACKGROUND_PATCHWORK",
    );
    let snap = e.derive(&s);
    assert!(feature(&snap, "ID_T62_FEAT_MINDMOVER").is_some_and(|f| f.displayed));
}
