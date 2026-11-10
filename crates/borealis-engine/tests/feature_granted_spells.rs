//! #39: spells a class or subclass feature grants are always prepared, so the sheet prints them
//! with the always-prepared diamond and not the plain prepared dot.
//!
//! Aurora-format XML models several 2024 casters as known casters (no `prepare="true"`) and grants
//! their subclass spells (and capstone spells) with a plain `<grant type="Spell" spellcasting=..>`,
//! without `prepared="true"`. Such a spell is never learned in a known-spell slot: a feature gives
//! it, so it is always prepared (always known). Picks stay ordinary prepared spells.
//! The bundled SRD 5.2.1 pack says `always_prepared` on the grant itself.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::snapshot::{SpellEntry, SpellcastingSnapshot};
use borealis_engine::state::CharacterState;
use borealis_engine::{Choice, Engine};
use borealis_migrate::{convert_xml, Document};
use std::path::Path;

const SPELL: &str = r#"<description><p>Synthetic.</p></description>"#;

/// A known caster ("Skald", no `prepare`) in Aurora's XML shape. Its level 1 feature grants a
/// cantrip and two level 1 spells, one of them with `prepared="true"`; a select picks the rest.
fn aurora_xml() -> ContentFile {
    let spell = |id: &str, name: &str, level: u32| {
        format!(
            r#"<element name="{name}" type="Spell" source="Test" id="{id}">{SPELL}
    <supports>Skald</supports>
    <setters><set name="level">{level}</set><set name="school">Enchantment</set><set name="time">1 action</set></setters>
  </element>"#
        )
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Skald" type="Class" source="Test" id="ID_T_I39_CLASS_SKALD">
    <description><p>Synthetic.</p></description>
    <setters><set name="hd">d8</set></setters>
    <spellcasting name="Skald" ability="Charisma" allowReplace="true">
      <list>Skald</list>
    </spellcasting>
    <rules>
      <stat name="skald:spellcasting:slots:1" value="2" />
      <select type="Spell" name="Spell (Skald)" supports="$(spellcasting:list), $(spellcasting:slots)" number="2" spellcasting="Skald" />
      <grant type="Class Feature" id="ID_T_I39_SKALD_FEATURE_CHARMS" />
    </rules>
  </element>
  <element name="Level 1: Charms" type="Class Feature" source="Test" id="ID_T_I39_SKALD_FEATURE_CHARMS">
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Spell" id="ID_T_I39_SPELL_SPARK" spellcasting="Skald" />
      <grant type="Spell" id="ID_T_I39_SPELL_GLAMOUR" spellcasting="Skald" />
      <grant type="Spell" id="ID_T_I39_SPELL_WARD" spellcasting="Skald" prepared="true" />
    </rules>
  </element>
  {}
  {}
  {}
  {}
  {}
</elements>"#,
        spell("ID_T_I39_SPELL_SPARK", "Spark", 0),
        spell("ID_T_I39_SPELL_GLAMOUR", "Glamour", 1),
        spell("ID_T_I39_SPELL_WARD", "Ward", 1),
        spell("ID_T_I39_SPELL_ALPHA", "Alpha", 1),
        spell("ID_T_I39_SPELL_BETA", "Beta", 1),
    );
    let conv = convert_xml(&xml, "skald.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2024);
    file
}

fn engine(with_xml: bool) -> Option<Engine> {
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
    if with_xml {
        b = b.add_file("skald.json", PackOrigin::Custom, aurora_xml());
    }
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

fn build(e: &Engine, picks: &[(&str, &str)], levels: u32) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    s.abilities.base = [10, 10, 10, 10, 14, 14];
    for (slot, pick) in picks {
        s = choose(e, &s, slot, pick);
    }
    for _ in 1..levels {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn spell<'a>(c: &'a SpellcastingSnapshot, name: &str) -> &'a SpellEntry {
    c.spells
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("{name} not in {}: {:?}", c.name, c.spells))
}

#[test]
fn aurora_feature_spells_of_a_known_caster_are_always_prepared() {
    let Some(e) = engine(true) else { return };
    let s = build(
        &e,
        &[
            ("Class", "Skald"),
            ("Spell (Skald)", "Alpha"),
            ("Spell (Skald)", "Beta"),
        ],
        1,
    );
    let snap = e.derive(&s);
    let c = snap.caster("Skald").expect("Skald block");
    assert!(!c.prepare, "the synthetic class is a known caster");
    for name in ["Glamour", "Ward"] {
        let x = spell(c, name);
        assert!(
            x.known && x.prepared && x.always_prepared,
            "{name} a feature grants is always prepared: {x:?}"
        );
    }
    for name in ["Alpha", "Beta"] {
        let x = spell(c, name);
        assert!(
            x.known && !x.always_prepared,
            "{name} picked in a known-spell slot is learned, not always prepared: {x:?}"
        );
    }
    let spark = spell(c, "Spark");
    assert!(!spark.always_prepared, "cantrips keep their own mark");
}

/// The SRD 5.2.1 Fiend Patron says `always_prepared` on its grants; the Warlock's picks do not.
#[test]
fn srd_subclass_spells_are_always_prepared() {
    let Some(e) = engine(false) else { return };
    let s = build(&e, &[("Class", "Warlock")], 3);
    let s = choose(&e, &s, "Otherworldly Patron", "Fiend Patron");
    let snap = e.derive(&s);
    let c = snap.caster("Warlock").expect("Warlock block");
    for name in ["Burning Hands", "Command", "Scorching Ray", "Suggestion"] {
        let x = spell(c, name);
        assert!(x.prepared && x.always_prepared, "{name}: {x:?}");
    }
    assert!(c
        .spells
        .iter()
        .filter(|x| x.level > 0 && x.known && !x.always_prepared)
        .all(|x| x.granted_by.is_none()
            || x.granted_by
                .as_ref()
                .is_some_and(|g| g.name != "Fiend Spells")));
}

/// The SRD 5.2.1 Pact of the Chain lets the Warlock cast Find Familiar without a spell slot
/// (`grant_spell` mode `at_will_no_slot` on the Warlock list): it is ready without being
/// prepared and never counts against the prepared spells (reported on the Borealis Discord).
#[test]
fn srd_pact_of_the_chain_find_familiar_is_ready_and_uncounted() {
    let Some(e) = engine(false) else { return };
    let s = build(&e, &[("Class", "Warlock")], 1);
    let before = e.derive(&s);
    let open: Vec<&str> = before
        .slots
        .iter()
        .filter(|k| k.chosen.is_none())
        .map(|k| k.name.as_str())
        .collect();
    let slot = open
        .iter()
        .find(|n| n.starts_with("Eldritch Invocation"))
        .unwrap_or_else(|| panic!("no open invocation slot: {open:?}"));
    let s = choose(&e, &s, slot, "Pact of the Chain");
    let snap = e.derive(&s);
    let c = snap.caster("Warlock").expect("Warlock block");
    assert!(c.prepare, "the 2024 Warlock prepares its spells");
    let x = spell(c, "Find Familiar");
    assert!(x.prepared && x.always_prepared && x.no_slot, "{x:?}");
    assert_eq!(c.prepared_count, 0, "Find Familiar counts as prepared");
}
