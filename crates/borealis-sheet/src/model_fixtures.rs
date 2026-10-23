//! Test characters built from the bundled SRD packs (2014 wizard, 2024 wizard, fighter/wizard).

use crate::model_types::{FeatureData, FeatureGroup, GearItem, SheetData};
use borealis_character::Character;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::CharacterState;
use borealis_engine::{CharacterSnapshot, Choice, Engine};
use std::path::Path;

pub(crate) fn store(pack: &str) -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join(pack)))
        .expect("bundled packs load")
        .build()
}

fn pick(e: &Engine, s: &CharacterState, ty: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let key = snap
        .slots
        .iter()
        .find(|x| x.ty == ty && x.chosen.is_none() && x.options.iter().any(|o| o.id == id))
        .map(|x| x.key.clone())
        .unwrap_or_else(|| panic!("no open {ty} slot offering {id}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn level1(e: &Engine, s: CharacterState, race: &str, class: &str, bg: &str) -> CharacterState {
    let snap = e.derive(&s);
    let key = |s: &CharacterSnapshot, n: &str| slot_key(s, "ID_LEVEL_1", n, 1).expect(n);
    let names: [(&str, &str); 3] = [("Race", race), ("Class", class), ("Background", bg)];
    let mut s = s;
    let mut snap = snap;
    for (slot, id) in names {
        let k = key(&snap, slot);
        s = commands::select(e, &s, &k, Choice::Element(id.into()))
            .unwrap_or_else(|err| panic!("{slot} {id}: {err}"))
            .state;
        snap = e.derive(&s);
    }
    s
}

/// Fill every open spell slot with its first offered option (cantrips, known and spellbook spells).
fn fill_spells(e: &Engine, mut s: CharacterState) -> CharacterState {
    for _ in 0..80 {
        let snap = e.derive(&s);
        let Some(slot) = snap
            .slots
            .iter()
            .find(|x| x.ty == "Spell" && x.chosen.is_none() && !x.options.is_empty())
        else {
            break;
        };
        let (key, id) = (slot.key.clone(), slot.options[0].id.clone());
        match commands::select(e, &s, &key, Choice::Element(id)) {
            Ok(r) => s = r.state,
            Err(_) => break,
        }
    }
    s
}

fn wrap(name: &str, s: CharacterState, e: &Engine) -> (Character, CharacterSnapshot) {
    let s = fill_spells(e, s);
    let snap = e.derive(&s);
    let mut c = Character::new(name);
    c.build = s;
    (c, snap)
}

pub(crate) fn wizard_2014() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = [8, 14, 13, 15, 12, 10];
    s = level1(
        &e,
        s,
        "ID_RACE_HUMAN",
        "ID_WOTC_PHB_CLASS_WIZARD",
        "ID_BACKGROUND_ACOLYTE",
    );
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    wrap("Mira Vale", s, &e)
}

pub(crate) fn wizard_2024() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.2.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s.abilities.base = [8, 14, 13, 15, 12, 10];
    s = level1(
        &e,
        s,
        "ID_2024_SPECIES_HUMAN",
        "ID_2024_CLASS_WIZARD",
        "ID_2024_BACKGROUND_SAGE",
    );
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    wrap("Mira Vale", s, &e)
}

/// Level-3 2024 wizard of `species` with `lineage` chosen in its lineage trait (SRD 5.2.1).
pub(crate) fn lineage_wizard_2024(species: &str, lineage: &str) -> (Character, CharacterSnapshot) {
    let st = store("srd-5.2.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s.abilities.base = [8, 14, 13, 15, 12, 10];
    s = level1(
        &e,
        s,
        species,
        "ID_2024_CLASS_WIZARD",
        "ID_2024_BACKGROUND_SAGE",
    );
    s = pick(&e, &s, "Species Trait Option", lineage);
    for _ in 0..2 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    wrap("Ilia Thorn", s, &e)
}

fn multiclass_2014(
    first: &str,
    levels: u32,
    second: &str,
    more: u32,
    base: [i32; 6],
    name: &str,
) -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = base;
    s = level1(
        &e,
        s,
        "ID_RACE_HUMAN",
        &format!("ID_WOTC_PHB_CLASS_{first}"),
        "ID_BACKGROUND_ACOLYTE",
    );
    for _ in 1..levels {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = commands::level_up_new_multiclass(&e, &s, None)
        .unwrap()
        .state;
    let start = s.level();
    s = pick(
        &e,
        &s,
        "Multiclass",
        &format!("ID_WOTC_PHB_MULTICLASS_{second}"),
    );
    for _ in 1..more {
        s = commands::level_up_multiclass(&e, &s, start, None)
            .unwrap()
            .state;
    }
    wrap(name, s, &e)
}

/// Fighter 4 / Wizard 3 (one caster class, so no pooled slot table).
pub(crate) fn fighter_wizard_2014() -> (Character, CharacterSnapshot) {
    multiclass_2014(
        "FIGHTER",
        4,
        "WIZARD",
        3,
        [15, 12, 14, 13, 10, 8],
        "Brann Ironquill",
    )
}

/// Cleric 3 / Wizard 2 (two caster classes, pooled slots).
pub(crate) fn cleric_wizard_2014() -> (Character, CharacterSnapshot) {
    multiclass_2014(
        "CLERIC",
        3,
        "WIZARD",
        2,
        [10, 12, 14, 13, 15, 8],
        "Sela Moonhand",
    )
}

/// Level-5 Hill Dwarf Life Cleric with a shield, scale mail and mace.
pub(crate) fn cleric_2014() -> (Character, CharacterSnapshot) {
    let st = store("srd-5.1");
    let e = Engine::new(&st);
    let mut s = commands::new_character(&e, Ruleset::Dnd2014);
    s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
    s.abilities.base = [13, 10, 14, 8, 15, 12];
    s = level1(
        &e,
        s,
        "ID_SRD_RACE_DWARF",
        "ID_WOTC_PHB_CLASS_CLERIC",
        "ID_BACKGROUND_ACOLYTE",
    );
    s = pick(&e, &s, "Sub Race", "ID_SUB_RACE_HILL_DWARF");
    s = pick(&e, &s, "Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
    for _ in 0..4 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    for item in [
        "ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL",
        "ID_WOTC_GEAR_SHIELD",
        "ID_WOTC_PHB_WEAPON_MACE",
    ] {
        s = commands::add_item(&e, &s, item, 1).unwrap().state;
        let uid = s.inventory.items.last().unwrap().uid.clone();
        s = commands::equip(&e, &s, &uid, None).unwrap().state;
    }
    wrap("Brunhild Stonehand", s, &e)
}

/// A synthetic feature whose description has a table of `rows` rows and `cols` columns.
pub(crate) fn table_feature(rows: usize, cols: usize, cell: &str) -> FeatureData {
    table_feature_with(rows, cols, cell, "Head")
}

/// [`table_feature`] with header cells `"{head} {c}"`.
pub(crate) fn table_feature_with(rows: usize, cols: usize, cell: &str, head: &str) -> FeatureData {
    let mut html = String::from("<p>Choose one option.</p><table><tr>");
    for c in 0..cols {
        html.push_str(&format!("<th>{head} {c}</th>"));
    }
    html.push_str("</tr>");
    for r in 0..rows {
        html.push_str(&format!("<tr><td>Option {r}</td>"));
        for c in 1..cols {
            html.push_str(&format!("<td>{cell} {r}.{c}</td>"));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table><p>Then rest.</p>");
    let text = crate::card_model::plain_paragraphs(&crate::table_text::tables_as_rows(&html));
    FeatureData {
        id: "ID_TABLE_FEATURE".into(),
        name: "Synthetic table feature".into(),
        group: FeatureGroup::Class,
        class: None,
        level: 1,
        sources: Vec::new(),
        summary: "Choose one option.".into(),
        tables: crate::table_text::locate_tables(&html, &text),
        text,
    }
}

/// Sample content for the visual checks: items, coins, personality and a feature with a table.
pub(crate) fn with_sample_extras(mut d: SheetData) -> SheetData {
    let names = [
        "Rope, hempen (50 feet)",
        "Torch",
        "Rations (1 day)",
        "Waterskin",
    ];
    for (i, name) in names.iter().enumerate() {
        d.gear.items.push(GearItem {
            name: (*name).to_string(),
            quantity: (i % 3 + 1) as u32,
            weight: 1.0,
            equipped: String::new(),
            attuned: false,
            requires_attunement: false,
        });
    }
    d.gear.coins = [12, 30, 0, 1234, 2];
    d.story.traits = "I quote sacred texts at odd moments.\nI always have a plan.".into();
    d.story.ideals = "Knowledge shared freely.".into();
    d.story.bonds = "The temple that raised me.".into();
    d.story.flaws = "I trust iron more than people.".into();
    d.features.insert(0, table_feature(5, 3, "Value"));
    d
}
