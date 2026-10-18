//! IM8: a 2014 class select offers 2024 optional features typed `Option` (and a 2024 `Option`
//! select offers 2014 `Class Feature` invocations) exactly when the character allows content
//! from the other edition.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

fn engine_with(extra: Vec<Value>) -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn content() -> Vec<Value> {
    vec![
        json!({"id":"ID_T_LOCK14","name":"Old Lock","type":"Class","source":"Test","ruleset":"dnd2014",
            "setters":[{"name":"hd","value":"d8"}],
            "rules":[{"kind":"select","type":"Class Feature","name":"Eldritch Invocation","supports":"Eldritch Invocation","level":1}]}),
        json!({"id":"ID_T_LOCK24","name":"New Lock","type":"Class","source":"Test","ruleset":"dnd2024",
            "setters":[{"name":"hd","value":"d8"}],
            "rules":[{"kind":"select","type":"Option","name":"Eldritch Invocation","supports":"Eldritch Invocation","level":1}]}),
        json!({"id":"ID_T_INV14","name":"Old Sight","type":"Class Feature","source":"Test","ruleset":"dnd2014",
            "supports":["Eldritch Invocation"]}),
        json!({"id":"ID_T_INV24","name":"New Sight","type":"Option","source":"Test","ruleset":"dnd2024",
            "supports":["Eldritch Invocation"]}),
        // a same-edition element of the crossing type is never offered
        json!({"id":"ID_T_OPT14","name":"Old Option","type":"Option","source":"Test","ruleset":"dnd2014",
            "supports":["Eldritch Invocation"]}),
    ]
}

fn with_class(e: &Engine, ruleset: Ruleset, class: &str, allow: Option<bool>) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    s.allow_cross_edition = allow;
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    commands::select(e, &s, &key, Choice::Element(class.into()))
        .expect("class pick")
        .state
}

fn invocation_options(e: &Engine, s: &CharacterState, class: &str) -> Vec<String> {
    e.derive(s)
        .slot(class, "Eldritch Invocation")
        .expect("invocation slot")
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

#[test]
fn a_2014_class_offers_2024_option_invocations_only_with_the_setting() {
    let e = engine_with(content());
    let strict = with_class(&e, Ruleset::Dnd2014, "ID_T_LOCK14", None);
    let ids = invocation_options(&e, &strict, "ID_T_LOCK14");
    assert_eq!(
        ids,
        vec!["ID_T_INV14".to_string()],
        "default 2014: own edition only"
    );

    let open = with_class(&e, Ruleset::Dnd2014, "ID_T_LOCK14", Some(true));
    let ids = invocation_options(&e, &open, "ID_T_LOCK14");
    assert!(ids.contains(&"ID_T_INV14".to_string()), "{ids:?}");
    assert!(ids.contains(&"ID_T_INV24".to_string()), "{ids:?}");
    assert!(!ids.contains(&"ID_T_OPT14".to_string()), "{ids:?}");

    // and the pick holds
    let snap = e.derive(&open);
    let key = slot_key(&snap, "ID_T_LOCK14", "Eldritch Invocation", 1).expect("slot");
    let picked = commands::select(&e, &open, &key, Choice::Element("ID_T_INV24".into()))
        .expect("2024 invocation accepted")
        .state;
    assert!(e.derive(&picked).has_element("ID_T_INV24"));

    // strict refuses the same pick
    let snap = e.derive(&strict);
    let key = slot_key(&snap, "ID_T_LOCK14", "Eldritch Invocation", 1).expect("slot");
    assert!(commands::select(&e, &strict, &key, Choice::Element("ID_T_INV24".into())).is_err());
}

#[test]
fn a_2024_option_select_offers_2014_class_feature_invocations_only_with_the_setting() {
    let e = engine_with(content());
    let open = with_class(&e, Ruleset::Dnd2024, "ID_T_LOCK24", None);
    let ids = invocation_options(&e, &open, "ID_T_LOCK24");
    assert!(ids.contains(&"ID_T_INV24".to_string()), "{ids:?}");
    assert!(
        ids.contains(&"ID_T_INV14".to_string()),
        "default 2024 is open: {ids:?}"
    );

    let strict = with_class(&e, Ruleset::Dnd2024, "ID_T_LOCK24", Some(false));
    let ids = invocation_options(&e, &strict, "ID_T_LOCK24");
    assert_eq!(ids, vec!["ID_T_INV24".to_string()]);
}
