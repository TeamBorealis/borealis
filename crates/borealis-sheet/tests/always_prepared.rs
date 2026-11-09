//! #39: a spell a feature grants prints the always-prepared diamond (`pip ex`) on both spell
//! tables, and a learned spell keeps the prepared dot (`pip on`).
//! Aurora-format content: a known caster whose feature grants spells without `prepared="true"`
//! (the JSON shape the XML converter writes). SRD 5.2.1: the Fiend Patron's spells.

use borealis_character::Character;
use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_sheet::{render_html, SheetOptions};
use serde_json::{json, Value};
use std::path::Path;

fn spell(id: &str, name: &str, level: u32) -> Value {
    json!({"id": id, "name": name, "type": "Spell", "source": "Test", "supports": ["Skald"],
           "setters": [{"name": "level", "value": level.to_string()},
                       {"name": "school", "value": "Enchantment"}]})
}

/// A known caster ("Skald", no `prepare`); its level 1 feature grants Glamour with no
/// `prepared` flag, and the class picks two spells.
fn aurora_shaped() -> ContentFile {
    serde_json::from_value(json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": [
        {"id": "ID_T_I39_CLASS_SKALD", "name": "Skald", "type": "Class", "source": "Test",
         "setters": [{"name": "hd", "value": "d8"}],
         "spellcasting": {"name": "Skald", "ability": "Charisma", "allow_replace": true, "list": {"supports": "Skald"}},
         "rules": [
            {"kind": "stat", "name": "skald:spellcasting:slots:1", "value": "2"},
            {"kind": "select", "type": "Spell", "name": "Spell (Skald)",
             "supports": "$(spellcasting:list), $(spellcasting:slots)", "number": 2, "spellcasting": "Skald"},
            {"kind": "grant", "type": "Class Feature", "id": "ID_T_I39_SKALD_FEATURE_CHARMS"}
         ]},
        {"id": "ID_T_I39_SKALD_FEATURE_CHARMS", "name": "Level 1: Charms", "type": "Class Feature", "source": "Test",
         "rules": [{"kind": "grant", "type": "Spell", "id": "ID_T_I39_SPELL_GLAMOUR", "spellcasting": "Skald"}]},
        spell("ID_T_I39_SPELL_GLAMOUR", "Glamour", 1),
        spell("ID_T_I39_SPELL_ALPHA", "Alpha", 1),
        spell("ID_T_I39_SPELL_BETA", "Beta", 1),
    ]}))
    .expect("synthetic content")
}

fn engine(with_synthetic: bool) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs load");
    if with_synthetic {
        b = b.add_file("skald.json", PackOrigin::Custom, aurora_shaped());
    }
    Engine::new(&b.build())
}

fn choose(e: &Engine, s: &CharacterState, slot: &str, pick: &str) -> CharacterState {
    let snap = e.derive(s);
    let sl = snap
        .slots
        .iter()
        .find(|k| k.name == slot && k.chosen.is_none() && k.options.iter().any(|o| o.name == pick))
        .unwrap_or_else(|| panic!("no open slot {slot} offering {pick}"));
    let id = sl
        .options
        .iter()
        .find(|o| o.name == pick)
        .unwrap()
        .id
        .clone();
    commands::select(e, s, &sl.key, Choice::Element(id))
        .expect("select")
        .state
}

fn character(e: &Engine, picks: &[(&str, &str)], levels: u32) -> (Character, CharacterSnapshot) {
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    s.abilities.base = [10, 10, 10, 10, 14, 14];
    let mut rest = picks;
    while let Some(((slot, pick), tail)) = rest.split_first() {
        if *slot == "level" {
            for _ in 1..levels {
                s = commands::level_up_main(e, &s, None)
                    .expect("level up")
                    .state;
            }
        } else {
            s = choose(e, &s, slot, pick);
        }
        rest = tail;
    }
    let snap = e.derive(&s);
    let mut c = Character::new("Pip Test");
    c.build = s;
    (c, snap)
}

/// The pip state printed in front of the row named `name` (the nearest pip before it).
fn pip_before(html: &str, name: &str) -> String {
    let at = html
        .find(&format!(">{name}<"))
        .unwrap_or_else(|| panic!("{name} not on the sheet"));
    let start = html[..at]
        .rfind("class=\"pip")
        .expect("a pip before the row");
    let rest = &html[start + "class=\"pip".len()..];
    rest[..rest.find('"').unwrap()].trim().to_string()
}

fn sheets(c: &Character, snap: &CharacterSnapshot) -> [String; 2] {
    [false, true].map(|legacy| {
        let o = SheetOptions {
            form_fillable: false,
            legacy_spellcasting: legacy,
            ..SheetOptions::default()
        };
        render_html(c, snap, &o).expect("sheet renders")
    })
}

#[test]
fn a_feature_granted_spell_of_a_known_caster_prints_the_always_prepared_diamond() {
    let e = engine(true);
    let (c, snap) = character(
        &e,
        &[
            ("Class", "Skald"),
            ("Spell (Skald)", "Alpha"),
            ("Spell (Skald)", "Beta"),
        ],
        1,
    );
    for html in sheets(&c, &snap) {
        assert_eq!(pip_before(&html, "Glamour"), "ex");
        assert_eq!(pip_before(&html, "Alpha"), "on");
        assert_eq!(pip_before(&html, "Beta"), "on");
    }
}

#[test]
fn srd_subclass_spells_print_the_always_prepared_diamond() {
    let e = engine(false);
    let (c, snap) = character(
        &e,
        &[
            ("Class", "Warlock"),
            ("level", ""),
            ("Otherworldly Patron", "Fiend Patron"),
        ],
        3,
    );
    for html in sheets(&c, &snap) {
        for name in ["Burning Hands", "Command", "Scorching Ray", "Suggestion"] {
            assert_eq!(pip_before(&html, name), "ex", "{name}");
        }
    }
}
