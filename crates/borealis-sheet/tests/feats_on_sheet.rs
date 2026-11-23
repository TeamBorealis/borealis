//! GitHub #62: every feat a character has is on the sheet, whichever way it came: a background's
//! feat, a feat a species trait picks, a class's Ability Score Improvement feat, a feat added in
//! the builder (Added feats) and a feat from an equipped "Additional Feat" item.
//!
//! Imported Aurora-format content writes many feats in two shapes the sheet used to drop:
//! - the feat hides itself (`<sheet display="false"/>`) and has nothing else on the sheet, such as
//!   a feat whose benefit is a number already counted (a higher hit point maximum);
//! - the feat hides itself and its sheet text lives in the option it lets the player choose (a
//!   `Feat Feature`, such as the ability score it raises), named after the feat with
//!   `<sheet alt="...">`.
//!
//! The content below is made up, in Aurora's XML shape, converted as an import is.

use borealis_character::Character;
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::added_picks::AddedKind;
use borealis_engine::commands;
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use borealis_sheet::model_types::FeatureGroup;
use borealis_sheet::{render_html, render_pdf, sheet_data, SheetOptions};
use std::path::Path;

/// One option of a feat's ability choice, carrying the feat's sheet text under the feat's name.
fn ability_option(feat_id: &str, feat: &str, ability: &str) -> String {
    let up = ability.to_uppercase();
    format!(
        r#"<element name="{ability}" type="Feat Feature" source="Test Book" id="{feat_id}_{up}">
    <supports>{feat_id}</supports>
    <description><p>Your {ability} score increases by 1.</p></description>
    <sheet alt="{feat}"><description>{feat} sheet text.</description></sheet>
    <rules><stat name="{ability}" value="1" /></rules>
  </element>"#
    )
}

/// A general feat that hides itself and lets the player raise Intelligence or Wisdom.
fn split_feat(id: &str, name: &str) -> String {
    format!(
        r#"<element name="{name}" type="Feat" source="Test Book" id="{id}">
    <supports>General</supports>
    <description><p>{name} description.</p></description>
    <sheet display="false" />
    <rules>
      <select type="Feat Feature" name="Ability Score Increase ({name})" supports="{id}_INTELLIGENCE|{id}_WISDOM" />
    </rules>
  </element>
  {}
  {}"#,
        ability_option(id, name, "Intelligence"),
        ability_option(id, name, "Wisdom"),
    )
}

fn content() -> ContentFile {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Test Book" type="Source" source="Core" id="ID_SOURCE_TEST_BOOK">
    <setters><set name="abbreviation">TB</set></setters>
  </element>
  <element name="Testfolk" type="Race" source="Test Book" id="ID_TB_RACE_TESTFOLK">
    <description><p>Synthetic.</p></description>
    <sheet display="false" />
    <rules><grant type="Racial Trait" id="ID_TB_RACIAL_TRAIT_ADAPTABLE" /></rules>
  </element>
  <element name="Adaptable" type="Racial Trait" source="Test Book" id="ID_TB_RACIAL_TRAIT_ADAPTABLE">
    <description><p>You gain an Origin feat of your choice.</p></description>
    <sheet display="false" />
    <rules><select type="Feat" name="Feat (Adaptable)" supports="Origin" /></rules>
  </element>
  <element name="Deckhand" type="Background" source="Test Book" id="ID_TB_BACKGROUND_DECKHAND">
    <description><p>Synthetic.</p></description>
    <rules><grant type="Feat" id="ID_TB_FEAT_BRAWLER" /></rules>
  </element>
  <element name="Brawler" type="Feat" source="Test Book" id="ID_TB_FEAT_BRAWLER">
    <supports>Origin</supports>
    <description><p>Brawler description.</p></description>
    <sheet><description>Brawler sheet text.</description></sheet>
  </element>
  <element name="Hardy" type="Feat" source="Test Book" id="ID_TB_FEAT_HARDY">
    <supports>Origin</supports>
    <description><p>Your hit point maximum increases by twice your level.</p></description>
    <sheet display="false" />
    <rules><stat name="hardy:hp" value="level" /><stat name="hardy:hp" value="level" /><stat name="hp" value="hardy:hp" /></rules>
  </element>
  {}
  {}
</elements>"#,
        split_feat("ID_TB_FEAT_MINDMOVER", "Mindmover"),
        split_feat("ID_TB_FEAT_MINDSPEAKER", "Mindspeaker"),
    );
    let conv = convert_xml(&xml, "feats-62.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2024);
    file
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs load")
        .add_file("feats-62.json", PackOrigin::Custom, content())
        .build();
    Engine::new(&store)
}

/// Fill the open slot of type `ty` that offers `id`; `name` narrows it to one slot name.
fn pick(e: &Engine, s: &CharacterState, ty: &str, name: Option<&str>, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = snap
        .slots
        .iter()
        .find(|x| {
            x.ty == ty
                && x.chosen.is_none()
                && name.is_none_or(|n| x.name == n)
                && x.options.iter().any(|o| o.id == id)
        })
        .map(|x| x.key.clone())
        .unwrap_or_else(|| panic!("no open {ty} slot offering {id}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

/// The reporter's build in made-up content: a level 7 2024 fighter whose background grants a
/// feat, whose species trait picks one, who took a feat at Fighter 4, added one under Added
/// feats and carries one as an equipped "Additional Feat" item.
fn hero() -> (Character, CharacterSnapshot) {
    let e = engine();
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s.abilities.base = [15, 14, 13, 10, 12, 8];
    s = pick(&e, &s, "Race", None, "ID_TB_RACE_TESTFOLK");
    s = pick(&e, &s, "Class", None, "ID_2024_CLASS_FIGHTER");
    s = pick(&e, &s, "Background", None, "ID_TB_BACKGROUND_DECKHAND");
    s = pick(&e, &s, "Feat", Some("Feat (Adaptable)"), "ID_TB_FEAT_HARDY");
    for _ in 1..7 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = pick(
        &e,
        &s,
        "Feat",
        Some("Ability Score Improvement (Fighter 4)"),
        "ID_2024_FEAT_GRAPPLER",
    );
    s = commands::add_pick(&e, &s, AddedKind::Feat, "ID_TB_FEAT_MINDMOVER", None)
        .expect("added feat")
        .state;
    s = pick(&e, &s, "Feat Feature", None, "ID_TB_FEAT_MINDMOVER_WISDOM");
    s = commands::add_item(&e, &s, "ID_TB_INTERNAL_ITEM_FEAT_PROXY_MINDSPEAKER", 1)
        .expect("additional feat item")
        .state;
    let uid = s.inventory.items.last().expect("item line").uid.clone();
    s = commands::equip(&e, &s, &uid, None).expect("equipped").state;
    s = pick(
        &e,
        &s,
        "Feat Feature",
        None,
        "ID_TB_FEAT_MINDSPEAKER_INTELLIGENCE",
    );
    let snap = e.derive(&s);
    let mut c = Character::new("Jory Keel");
    c.build = s;
    (c, snap)
}

const FEATS: [&str; 5] = ["Brawler", "Hardy", "Grappler", "Mindmover", "Mindspeaker"];

#[test]
fn every_feat_is_listed_under_feats_whichever_way_it_came() {
    let (c, snap) = hero();
    let d = sheet_data(&c, &snap, &Default::default());
    let feats: Vec<&str> = d
        .features
        .iter()
        .filter(|f| f.group == FeatureGroup::Feat)
        .map(|f| f.name.as_str())
        .collect();
    for name in FEATS {
        assert!(
            feats.contains(&name),
            "{name} missing from Feats: {feats:?}"
        );
    }
    // the chosen ability options print under their feat's name, not as "Wisdom"
    let all: Vec<&str> = d.features.iter().map(|f| f.name.as_str()).collect();
    for option in ["Wisdom", "Intelligence"] {
        assert!(!all.contains(&option), "{option} listed: {all:?}");
    }
    assert_eq!(feats.len(), FEATS.len(), "each feat once: {feats:?}");
}

#[test]
fn a_feat_keeps_its_sheet_text_or_falls_back_to_its_description() {
    let (c, snap) = hero();
    let d = sheet_data(&c, &snap, &Default::default());
    let text = |name: &str| {
        d.features
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.text.clone())
            .unwrap_or_else(|| panic!("{name} listed"))
    };
    assert_eq!(text("Mindmover"), "Mindmover sheet text.");
    assert_eq!(text("Mindspeaker"), "Mindspeaker sheet text.");
    assert!(text("Hardy").contains("hit point maximum increases"));
}

#[test]
fn the_preview_and_the_pdf_print_every_feat() {
    let (c, snap) = hero();
    let options = SheetOptions {
        form_fillable: false,
        ..SheetOptions::default()
    };
    let html = render_html(&c, &snap, &options).expect("preview");
    let pdf = render_pdf(&c, &snap, &options).expect("pdf");
    let doc = lopdf::Document::load_mem(&pdf).expect("parses");
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    let text = doc.extract_text(&pages).expect("pdf text");
    for name in FEATS {
        assert!(
            html.contains(&format!(">{name}<")),
            "{name} not in the preview"
        );
        assert!(text.contains(name), "{name} not in the PDF");
    }
}
