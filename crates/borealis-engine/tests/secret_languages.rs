//! HC13 (2), item H_2Gc: language choices never offer a language tagged Secret. Secret languages are taught, not
//! chosen: Druidic (Core: Exotic and Secret) and Thieves' Cant arrive only where a class grants them. A choice that
//! names Secret (or the language's id) itself still offers it: the author asked for it.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use serde_json::json;
use std::path::Path;

const DRUIDIC: &str = "ID_LANGUAGE_DRUIDIC";
const CANT: &str = "ID_WOTC_PHB_LANGUAGE_THIEVES_CANT";
const DRACONIC: &str = "ID_LANGUAGE_DRACONIC";
const SECRET_RACE: &str = "ID_T_RACE_SECRET_KEEPER";

fn engine(pack: &str, ruleset: &str) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    // a species whose own choice asks for a secret language by name
    let race = json!({"id": SECRET_RACE, "name": "Secret Keeper", "type": "Race", "source": "Test",
        "rules": [{"kind": "select", "type": "Language", "name": "Language (Secret Keeper)", "supports": "Secret"}]});
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": [race]}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join(pack)))
        .expect("bundled packs")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn options(snap: &CharacterSnapshot, name: &str) -> Vec<String> {
    snap.slots
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn has(snap: &CharacterSnapshot, id: &str) -> bool {
    snap.elements.iter().any(|r| r.element.id == id)
}

#[test]
fn an_any_language_choice_offers_no_secret_language() {
    let e = engine("srd-5.1", "dnd2014");
    let s = pick(
        &e,
        &commands::new_character(&e, Ruleset::Dnd2014),
        "Race",
        "ID_RACE_HUMAN",
    );
    let offered = options(&e.derive(&s), "Language (Human)");
    assert!(
        offered.iter().any(|id| id == DRACONIC),
        "exotic languages stay: {offered:?}"
    );
    assert!(!offered.iter().any(|id| id == DRUIDIC), "{offered:?}");
    assert!(!offered.iter().any(|id| id == CANT), "{offered:?}");
}

#[test]
fn a_choice_that_names_secret_still_offers_secret_languages() {
    let e = engine("srd-5.1", "dnd2014");
    let s = pick(
        &e,
        &commands::new_character(&e, Ruleset::Dnd2014),
        "Race",
        SECRET_RACE,
    );
    let offered = options(&e.derive(&s), "Language (Secret Keeper)");
    assert!(offered.iter().any(|id| id == DRUIDIC), "{offered:?}");
    assert!(offered.iter().any(|id| id == CANT), "{offered:?}");
}

#[test]
fn druidic_and_thieves_cant_arrive_where_a_class_grants_them() {
    for (pack, ruleset, edition, druid, rogue) in [
        (
            "srd-5.1",
            "dnd2014",
            Ruleset::Dnd2014,
            "ID_WOTC_PHB_CLASS_DRUID",
            "ID_WOTC_PHB_CLASS_ROGUE",
        ),
        (
            "srd-5.2.1",
            "dnd2024",
            Ruleset::Dnd2024,
            "ID_2024_CLASS_DRUID",
            "ID_2024_CLASS_ROGUE",
        ),
    ] {
        let e = engine(pack, ruleset);
        let s = pick(&e, &commands::new_character(&e, edition), "Class", druid);
        assert!(
            has(&e.derive(&s), DRUIDIC),
            "{pack}: the druid knows Druidic"
        );
        let s = pick(&e, &commands::new_character(&e, edition), "Class", rogue);
        assert!(
            has(&e.derive(&s), CANT),
            "{pack}: the rogue knows Thieves' Cant"
        );
    }
}
