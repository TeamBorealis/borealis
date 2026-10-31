//! IM15 (GitHub #11, review): a supports tag written with a typographic apostrophe matches the
//! same tag written with a straight one, both ways, as book names already do (`source_key`).
//! Homebrew mixes `Spider's Cunning` in a select with `Spider’s Cunning` on its targets.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::state::Choice;
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(json!({
        "format": "borealis-content/1",
        "ruleset": "dnd2014",
        "elements": [
            {"id": "ID_T_WEAVER", "name": "Test Weaver", "type": "Class", "source": "Test",
             "setters": [{"name": "hd", "value": "d8"}], "rules": [
                {"kind": "select", "type": "Class Feature", "name": "Straight", "supports": "Spider's Cunning", "level": 1},
                {"kind": "select", "type": "Class Feature", "name": "Curly", "supports": "Silk’s Grace", "level": 1}]},
            {"id": "ID_T_WEB", "name": "Web Walker", "type": "Class Feature", "source": "Test", "supports": ["Spider’s Cunning"]},
            {"id": "ID_T_SILK", "name": "Silk Step", "type": "Class Feature", "source": "Test", "supports": ["Silk's Grace"]}
        ]}))
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("t.json", PackOrigin::Custom, file)
        .build();
    Engine::new(&store)
}

#[test]
fn straight_and_typographic_apostrophes_match_both_ways() {
    let e = engine();
    let s = commands::new_character(&e, Ruleset::Dnd2014);
    let key = commands::slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(&e, &s, &key, Choice::Element("ID_T_WEAVER".into()))
        .expect("class")
        .state;
    let snap = e.derive(&s);
    let offered = |name: &str| -> Vec<String> {
        snap.slots
            .iter()
            .find(|x| x.name == name)
            .unwrap_or_else(|| panic!("no slot {name}"))
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect()
    };
    assert_eq!(offered("Straight"), ["ID_T_WEB"]);
    assert_eq!(offered("Curly"), ["ID_T_SILK"]);
}
