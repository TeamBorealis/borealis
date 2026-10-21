//! `[feature:<name>]` and strict `[spellcasting:N]` requirement terms over the bundled SRD 5.1
//! classes, with synthetic feats and species that open a feat select.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

const FEAT_SELECT: &str = "Test Feat";
const PLAIN_RACE: &str = "ID_T_RACE_PLAIN";
const CASTING_RACE: &str = "ID_T_RACE_CASTING";

fn feat(id: &str, requirements: &str) -> Value {
    json!({"id": id, "name": id, "type": "Feat", "source": "Test", "requirements": requirements})
}

fn race(id: &str, spellcasting: Option<Value>) -> Value {
    let mut r = json!({"id": id, "name": id, "type": "Race", "source": "Test",
        "rules": [{"kind": "select", "type": "Feat", "name": FEAT_SELECT, "level": 1}]});
    if let Some(sc) = spellcasting {
        r["spellcasting"] = sc;
    }
    r
}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let elements = vec![
            race(PLAIN_RACE, None),
            race(
                CASTING_RACE,
                Some(json!({"name": "Testling", "ability": "Intelligence",
                            "list": {"supports": "Wizard"}})),
            ),
            feat("ID_T_STYLE", "[feature:Fighting Style]"),
            feat(
                "ID_T_STYLE_OR",
                "ID_T_NOPE||([feature:fighting style],[str:8])",
            ),
            feat("ID_T_NO_STYLE", "![feature:Fighting Style]"),
            feat("ID_T_STRICT", "[spellcasting:1]"),
            feat("ID_T_LENIENT", "[feature:Spellcasting]"),
            json!({"id": "ID_T_STRUCT", "name": "ID_T_STRUCT", "type": "Feat", "source": "Test",
                   "data": {"prerequisite": {"requires_feature": "Spellcasting"}}}),
            feat("ID_T_PACT_OR", "[feature:Pact Magic]||[spellcasting:3]"),
        ];
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}),
        )
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .expect("bundled packs")
            .add_file("synthetic.json", PackOrigin::Core, file)
            .build();
        Engine::new(&store)
    })
}

fn character(race: &str, class: &str, level: u32) -> CharacterState {
    let e = engine();
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = commands::set_base_scores(e, &s, [12, 12, 12, 12, 12, 12]).map_or(s, |o| o.state);
    let pick = |s: &CharacterState, name: &str, id: &str| {
        let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
        commands::select(e, s, &key, Choice::Element(id.into()))
            .expect("select")
            .state
    };
    let mut s = pick(&pick(&s, "Race", race), "Class", class);
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
    }
    s
}

fn offered(race: &str, class: &str, level: u32) -> Vec<String> {
    let snap: CharacterSnapshot = engine().derive(&character(race, class, level));
    let mut ids: Vec<String> = snap
        .slots
        .iter()
        .find(|s| s.name == FEAT_SELECT)
        .expect("feat select")
        .options
        .iter()
        .filter(|o| o.id.starts_with("ID_T_"))
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn feature_term_reads_live_feature_names_inside_or_groups() {
    let fighter = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_FIGHTER", 1);
    assert_eq!(fighter, ["ID_T_STYLE", "ID_T_STYLE_OR"], "fighter 1");
    let wizard = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WIZARD", 1);
    assert_eq!(
        wizard,
        [
            "ID_T_LENIENT",
            "ID_T_NO_STYLE",
            "ID_T_STRICT",
            "ID_T_STRUCT"
        ],
        "wizard 1"
    );
}

#[test]
fn spellcasting_term_needs_class_slots_unlike_the_lenient_feature_check() {
    // a species spellcasting block satisfies the lenient check only
    let fighter = offered(CASTING_RACE, "ID_WOTC_PHB_CLASS_FIGHTER", 1);
    assert!(fighter.contains(&"ID_T_LENIENT".into()), "{fighter:?}");
    assert!(!fighter.contains(&"ID_T_STRICT".into()), "{fighter:?}");
    // 2014 Paladin: Spellcasting (and its first slots) at level 2
    let p1 = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_PALADIN", 1);
    assert!(!p1.contains(&"ID_T_STRICT".into()), "paladin 1: {p1:?}");
    let p2 = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_PALADIN", 2);
    assert!(p2.contains(&"ID_T_STRICT".into()), "paladin 2: {p2:?}");
    // Pact Magic: pact slots count; the OR group holds by the feature name
    let w1 = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WARLOCK", 1);
    assert!(
        w1.contains(&"ID_T_STRICT".into()) && w1.contains(&"ID_T_PACT_OR".into()),
        "warlock 1: {w1:?}"
    );
    let wiz4 = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WIZARD", 4);
    assert!(!wiz4.contains(&"ID_T_PACT_OR".into()), "wizard 4: {wiz4:?}");
    let wiz5 = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WIZARD", 5);
    assert!(
        wiz5.contains(&"ID_T_PACT_OR".into()),
        "3rd-level slots: {wiz5:?}"
    );
}

#[test]
fn structured_spellcasting_prerequisite_needs_class_slots() {
    // B-23: `data.prerequisite.requires_feature: Spellcasting` (Boon of Spell Recall) is the
    // strict "Spellcasting or Pact Magic feature" rule; a species spellcasting block is not it
    let fighter = offered(CASTING_RACE, "ID_WOTC_PHB_CLASS_FIGHTER", 1);
    assert!(!fighter.contains(&"ID_T_STRUCT".into()), "{fighter:?}");
    let plain = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_FIGHTER", 1);
    assert!(!plain.contains(&"ID_T_STRUCT".into()), "{plain:?}");
    let wizard = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WIZARD", 1);
    assert!(wizard.contains(&"ID_T_STRUCT".into()), "wizard: {wizard:?}");
    let warlock = offered(PLAIN_RACE, "ID_WOTC_PHB_CLASS_WARLOCK", 1);
    assert!(warlock.contains(&"ID_T_STRUCT".into()), "pact: {warlock:?}");
}
