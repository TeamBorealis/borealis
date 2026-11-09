//! #38: spells a feat picks outside any class reach the character's spellcasting blocks, so the
//! sheet, the spell cards and Play list them. The bundled SRD 5.2.1 Magic Initiate tags its spell
//! selects with a spellcasting name (`Magic Initiate`); Aurora-format XML feats pick their spells
//! in selects that carry no spellcasting name at all (`Cantrip (<Feat>)`), which no block listed.
//!
//! U19: such a pick keeps a block of its own only when its spellcasting ability can be named
//! (a real save DC and attack bonus). Otherwise its spells print in Aurora's "Other Spells",
//! named with their source as Aurora names them.

use borealis_content::Rule;
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::other_spells::source_label;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use std::path::Path;

/// An Aurora XML origin feat in the shape of Aurora's feats: the feat chooses a spell list option
/// (a `Feat Feature`), whose selects pick the spells without a `spellcasting` attribute.
fn aurora_xml() -> ContentFile {
    aurora_xml_with(None)
}

/// [`aurora_xml`], the spell list option carrying `rule` too (a rule that names an ability).
fn aurora_xml_with(rule: Option<Rule>) -> ContentFile {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Spark Initiate" type="Feat" source="Test" id="ID_T_I38_FEAT_SPARK_INITIATE">
    <supports>Origin Feat</supports>
    <description><p>Synthetic.</p></description>
    <setters><set name="category">Origin</set></setters>
    <rules>
      <select type="Feat Feature" name="Spark Initiate" supports="Spark Initiate List" />
    </rules>
  </element>
  <element name="Wizard" type="Feat Feature" source="Test" id="ID_T_I38_SPARK_INITIATE_WIZARD">
    <supports>Spark Initiate List</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <select type="Spell" name="Cantrip (Spark Initiate)" supports="0,Wizard" number="2" />
      <select type="Spell" name="Level 1 Spell (Spark Initiate)" supports="1,Wizard" />
      <grant type="Spell" id="ID_2024_SPELL_LIGHT" />
    </rules>
  </element>
</elements>"#;
    let conv = convert_xml(xml, "spark.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2024);
    if let Some(rule) = rule {
        let wizard = file
            .elements
            .iter_mut()
            .find(|el| el.id == "ID_T_I38_SPARK_INITIATE_WIZARD")
            .expect("spell list option");
        wizard.rules.push(rule);
    }
    file
}

fn engine(with_xml: bool) -> Option<Engine> {
    engine_with(with_xml.then(aurora_xml))
}

fn engine_with(xml: Option<ContentFile>) -> Option<Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let srd = root.join("srd-5.2.1");
    if !srd.join("pack.json").is_file() {
        eprintln!("SKIPPED: content/srd-5.2.1 not present");
        return None;
    }
    let mut b = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&srd))
        .expect("packs load");
    if let Some(file) = xml {
        b = b.add_file("spark.json", PackOrigin::Custom, file);
    }
    Some(Engine::new(&b.build()))
}

/// Fill the first open slot named `slot` with the option (or list entry) named `pick`.
fn choose(e: &Engine, s: &CharacterState, slot: &str, pick: &str) -> CharacterState {
    let snap = e.derive(s);
    let sl = snap
        .slots
        .iter()
        .find(|k| {
            k.name == slot
                && k.chosen.is_none()
                && k.list_choice.is_none()
                && (k.options.iter().any(|o| o.name == pick)
                    || k.list_items.iter().any(|(_, t)| t == pick))
        })
        .unwrap_or_else(|| {
            let open: Vec<&str> = snap
                .slots
                .iter()
                .filter(|k| k.chosen.is_none() && k.list_choice.is_none())
                .map(|k| k.name.as_str())
                .collect();
            panic!("no open slot {slot} offering {pick}; open: {open:?}")
        });
    let choice = sl.options.iter().find(|o| o.name == pick).map_or_else(
        || {
            let id = sl.list_items.iter().find(|(_, t)| t == pick).unwrap().0;
            Choice::ListItem(id)
        },
        |o| Choice::Element(o.id.clone()),
    );
    commands::select(e, s, &sl.key, choice)
        .unwrap_or_else(|err| panic!("{slot}/{pick}: {err}"))
        .state
}

fn human_bard(e: &Engine, picks: &[(&str, &str)]) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    for (slot, pick) in [("Race", "Human"), ("Class", "Bard")].iter().chain(picks) {
        s = choose(e, &s, slot, pick);
    }
    s
}

fn names(snap: &CharacterSnapshot, block: &str) -> Vec<String> {
    snap.caster(block)
        .unwrap_or_else(|| {
            panic!(
                "no {block} block: {:?}",
                snap.spellcasting
                    .iter()
                    .map(|b| &b.name)
                    .collect::<Vec<_>>()
            )
        })
        .spells
        .iter()
        .map(|x| x.name.clone())
        .collect()
}

#[test]
fn srd_magic_initiate_spells_of_a_bard_form_their_own_block() {
    let Some(e) = engine(false) else { return };
    let s = human_bard(
        &e,
        &[
            ("Versatile", "Magic Initiate"),
            ("Spell List", "Wizard"),
            ("Spellcasting Ability", "Intelligence"),
            ("Magic Initiate Cantrips", "True Strike"),
            ("Magic Initiate Cantrips", "Prestidigitation"),
            ("Magic Initiate Level 1 Spell", "Find Familiar"),
        ],
    );
    let snap = e.derive(&s);
    assert_eq!(
        names(&snap, "Magic Initiate (Wizard)"),
        vec!["Prestidigitation", "True Strike", "Find Familiar"]
    );
    assert!(!names(&snap, "Bard").contains(&"Find Familiar".to_string()));
}

const SPARK_PICKS: [(&str, &str); 5] = [
    ("Versatile", "Spark Initiate"),
    ("Spark Initiate", "Wizard"),
    ("Cantrip (Spark Initiate)", "True Strike"),
    ("Cantrip (Spark Initiate)", "Prestidigitation"),
    ("Level 1 Spell (Spark Initiate)", "Find Familiar"),
];

/// U19: with no spellcasting ability to be found, the feat's picks make no block of blank DC
/// boxes. They print in Aurora's "Other Spells", each named after the element holding its
/// select (Aurora's `SelectRule.ElementHeader`): here the feat's spell list option, `Wizard`.
#[test]
fn aurora_feat_spells_picked_without_an_ability_print_under_other_spells() {
    let Some(e) = engine(true) else { return };
    let snap = e.derive(&human_bard(&e, &SPARK_PICKS));
    let blocks: Vec<&str> = snap.spellcasting.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(blocks, vec!["Bard", "Other Spells"]);
    assert_eq!(
        names(&snap, "Other Spells"),
        vec!["Light", "Prestidigitation", "True Strike", "Find Familiar"]
    );
    let other = snap.caster("Other Spells").unwrap();
    assert!(other.other && other.ability.is_empty());
    let labels: Vec<(String, Option<String>)> = other
        .spells
        .iter()
        .map(|x| (x.name.clone(), source_label(x)))
        .collect();
    assert!(
        labels.iter().all(|(_, l)| l.as_deref() == Some("Wizard")),
        "{labels:?}"
    );
    let bard = names(&snap, "Bard");
    assert!(
        !bard
            .iter()
            .any(|n| n == "Find Familiar" || n == "True Strike"),
        "{bard:?}"
    );
}

/// U19: a pick whose owner names its ability keeps its own block (1.4.0 behaviour), with that
/// ability's save DC and attack bonus. Only the spell granted with no name goes to Other Spells.
#[test]
fn aurora_feat_spells_picked_with_an_ability_keep_a_block_named_after_the_feat() {
    let rule: Rule = serde_json::from_value(serde_json::json!({
        "kind": "spell_free_cast",
        "select": "Level 1 Spell (Spark Initiate)",
        "uses": 1,
        "recharge": "long_rest",
        "ability": "intelligence"
    }))
    .expect("rule");
    let Some(e) = engine_with(Some(aurora_xml_with(Some(rule)))) else {
        return;
    };
    let snap = e.derive(&human_bard(&e, &SPARK_PICKS));
    let blocks: Vec<&str> = snap.spellcasting.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(blocks, vec!["Bard", "Spark Initiate", "Other Spells"]);
    assert_eq!(
        names(&snap, "Spark Initiate"),
        vec!["Prestidigitation", "True Strike", "Find Familiar"]
    );
    let block = snap.caster("Spark Initiate").unwrap();
    assert_eq!(block.ability, "Intelligence");
    assert!(block.dc > 0 && !block.other);
    assert!(block.spells.iter().all(|x| x.known));
    assert_eq!(names(&snap, "Other Spells"), vec!["Light"]);
}

/// Only picks get a block of their own: a spell an element grants with no spellcasting name
/// (Aurora's invocations, disciplines and totem rituals) adds no block per element. It joins the
/// one "Other Spells" block, where Aurora's sheet lists every spell no class page lists
/// (`tests/other_spells.rs`).
#[test]
fn aurora_spells_granted_without_a_spellcasting_name_add_no_block_of_their_own() {
    let Some(e) = engine(true) else { return };
    let s = human_bard(
        &e,
        &[
            ("Versatile", "Spark Initiate"),
            ("Spark Initiate", "Wizard"),
        ],
    );
    let snap = e.derive(&s);
    let blocks: Vec<&str> = snap.spellcasting.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(blocks, vec!["Bard", "Other Spells"]);
    assert_eq!(names(&snap, "Other Spells"), vec!["Light"]);
}
