//! Lair actions and regional effects of a typed stat block (`lair_actions`, `regional_effects`):
//! the sections printed with a creature's lair (5etools keeps them in legendary groups). Each is
//! an introduction, the options in printed order (a name when the option has one) and closing
//! text. Stored stat blocks without them read and write back byte for byte (R4). SYNTHETIC data
//! only.

use borealis_content::monster::{LairOption, LairSection};
use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentFile, ContentStoreBuilder, IssueCode, MonsterData, PackOrigin, Severity,
};
use serde_json::{json, Value};
use std::path::Path;

const ID: &str = "ID_TEST_MONSTER_MOSS_WYRM";

fn content() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn goblin() -> MonsterData {
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&content().join("srd-5.1-monsters"))
        .unwrap()
        .build();
    let e = store
        .by_type("Monster")
        .find(|s| s.element.name == "Goblin")
        .expect("goblin")
        .element
        .clone();
    MonsterData::from_element(&e).unwrap()
}

fn option(name: Option<&str>, text: &str) -> LairOption {
    LairOption {
        name: name.map(str::to_string),
        text: text.to_string(),
    }
}

/// A synthetic creature with a lair.
fn wyrm() -> MonsterData {
    let mut m = goblin();
    m.lair_actions = Some(LairSection {
        preamble: "On initiative count 20 the wyrm picks one option.".into(),
        options: vec![
            option(None, "Moss grows over the floor."),
            option(Some("Spore Cloud"), "Spores fill a 10-foot cube."),
        ],
        closing: String::new(),
    });
    m.regional_effects = Some(LairSection {
        preamble: "The land around the lair changes.".into(),
        options: vec![option(None, "Paths overgrow within a day.")],
        closing: "The effects fade over 1d10 days if the wyrm dies.".into(),
    });
    m
}

fn file_of(m: &MonsterData) -> ContentFile {
    ContentFile {
        elements: vec![m.to_element(ID, "Moss Wyrm", "Test Homebrew")],
        ..ContentFile::default()
    }
}

fn errors(m: &MonsterData) -> Vec<String> {
    ContentStoreBuilder::new()
        .add_file("homebrew.json", PackOrigin::Custom, file_of(m))
        .build()
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error && i.code == IssueCode::InvalidMonsterData)
        .map(|i| i.message)
        .collect()
}

fn schema_violations(m: &MonsterData) -> usize {
    let doc = serde_json::to_value(file_of(m)).unwrap();
    validate_document(DocumentKind::Content, &doc)
        .unwrap()
        .len()
}

#[test]
fn lair_actions_and_regional_effects_are_valid_and_round_trip() {
    let m = wyrm();
    assert!(errors(&m).is_empty(), "{:?}", errors(&m));
    assert_eq!(schema_violations(&m), 0);
    let el = m.to_element(ID, "Moss Wyrm", "Test Homebrew");
    assert_eq!(
        el.data["lair_actions"]["options"][1],
        json!({"name": "Spore Cloud", "text": "Spores fill a 10-foot cube."})
    );
    // an option without a name and an empty closing are left out of the stored data
    assert_eq!(
        el.data["lair_actions"]["options"][0],
        json!({"text": "Moss grows over the floor."})
    );
    assert!(el.data["lair_actions"].get("closing").is_none());
    assert_eq!(
        el.data["regional_effects"]["closing"],
        json!("The effects fade over 1d10 days if the wyrm dies.")
    );
    assert_eq!(MonsterData::from_element(&el).unwrap(), m, "round trip");
}

#[test]
fn an_empty_section_or_option_is_not_valid() {
    let mut empty = wyrm();
    empty.lair_actions = Some(LairSection {
        preamble: " ".into(),
        options: Vec::new(),
        closing: String::new(),
    });
    let e = errors(&empty);
    assert!(e.iter().any(|x| x == "the lair actions are empty"), "{e:?}");

    let mut blank = wyrm();
    blank.regional_effects.as_mut().unwrap().options[0].text = String::new();
    let e = errors(&blank);
    assert!(
        e.iter()
            .any(|x| x.starts_with("option 0 of the regional effects")),
        "{e:?}"
    );
    assert!(schema_violations(&blank) > 0);

    let mut nameless = wyrm();
    nameless.lair_actions.as_mut().unwrap().options[1].name = Some(String::new());
    assert!(!errors(&nameless).is_empty());
    assert!(schema_violations(&nameless) > 0);
}

/// R4: no bundled stat block has a lair section, and each writes back to the same bytes.
#[test]
fn stored_stat_blocks_have_no_lair_sections_and_write_back_unchanged() {
    let mut seen = 0;
    for pack in ["srd-5.1-monsters", "srd-5.2.1-monsters"] {
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&content().join(pack))
            .unwrap()
            .build();
        for s in store.by_type("Monster") {
            let e = &s.element;
            let m = MonsterData::from_element(e).unwrap();
            assert!(
                m.lair_actions.is_none() && m.regional_effects.is_none(),
                "{}",
                e.id
            );
            let back = m.to_element(e.id.clone(), e.name.clone(), e.source.clone());
            let rewritten: Value = serde_json::to_value(&back.data).unwrap();
            assert_eq!(
                serde_json::to_string(&rewritten).unwrap(),
                serde_json::to_string(&e.data).unwrap(),
                "{}",
                e.id
            );
            seen += 1;
        }
    }
    assert!(seen > 600, "{seen} stat blocks");
}
