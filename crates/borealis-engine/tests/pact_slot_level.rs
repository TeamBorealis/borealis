//! Regression: 2014 Pact Magic content that writes `<class>:spellcasting:slots:count` and the
//! per-level slot groups but no `<class>:spellcasting:slot` level stat (Aurora's Blood Hunter
//! Order of the Profane Soul). The pact pool takes its level from the per-level group, so the
//! caster keeps its slots instead of reporting a level-0 pact pool.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const RACE: &str = "ID_T_RACE_SOUL";

fn stat(name: &str, value: &str, level: u32) -> Value {
    json!({"kind": "stat", "name": name, "value": value, "level": level})
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let count = "soul:spellcasting:slots:count";
    let race = json!({"id": RACE, "name": RACE, "type": "Race", "source": "Test",
    "spellcasting": {"name": "Soul", "ability": "Intelligence", "list": {"supports": "Wizard"}},
    "rules": [
        stat(count, "1", 1),
        stat(count, "1", 3),
        stat("soul:spellcasting:slots:1", count, 1),
        stat("soul:spellcasting:slots:1", &format!("-{count}"), 3),
        stat("soul:spellcasting:slots:2", count, 3),
        // a way to learn a levelled spell (settle invariant: casters can acquire spells)
        {"kind": "select", "type": "Spell", "name": "Spell Known (Soul)",
         "supports": "$(spellcasting:list), $(spellcasting:slots)", "spellcasting": "Soul"},
    ]});
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": [race]}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn fighter(e: &Engine, level: u32) -> CharacterState {
    borealis_engine::invariants::enable_for_thread();
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let pick = |s: &CharacterState, name: &str, id: &str| {
        let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
        commands::select(e, s, &key, Choice::Element(id.into()))
            .expect("select")
            .state
    };
    let mut s = pick(
        &pick(&s, "Race", RACE),
        "Class",
        "ID_WOTC_PHB_CLASS_FIGHTER",
    );
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

#[test]
fn pact_level_comes_from_the_slot_group_without_a_slot_level_stat() {
    let e = engine();
    for (level, pact) in [(1, (1, 1)), (2, (1, 1)), (3, (2, 2)), (5, (2, 2))] {
        let snap = e.derive(&fighter(&e, level));
        let c = snap.caster("Soul").expect("soul casting");
        assert_eq!((c.pact, c.slots), (Some(pact), [0; 9]), "level {level}");
    }
}
