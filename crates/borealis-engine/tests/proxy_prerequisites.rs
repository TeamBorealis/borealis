//! Discord #3 (GitHub issue 3): an epic boon added as an "Additional Feat" item and equipped did
//! not show until level 19. That is Aurora's rule: a grant whose target's own requirements fail is
//! not applied (`ProgressionManager`: "not granting ... due to not meeting element requirements"),
//! and AuroraLegacy's 2024 boons read `[character:19]||ID_WOTC_PHB24_GRANTS_FEAT_UNLOCKER`, so
//! the book's "Feat Unlocker" item is the way to ignore the prerequisite. Borealis keeps that rule
//! and now says so when the item is equipped: the warning names the feat, its prerequisite and
//! the item that lifts it, when the content has one. 2014 and 2024 characters, synthetic content
//! in the shape of AuroraLegacy's boons and the bundled SRD 5.2.1 boon.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::state::CharacterState;
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;

const BOOK: &str = "Test Book";
const BOON: &str = "ID_TB_FEAT_BOON_OF_TESTING";
const BOON_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_BOON_OF_TESTING";
const UNLOCKER_ITEM: &str = "ID_TB_ITEM_FEAT_UNLOCKER";
const UNLOCKER: &str = "ID_TB_GRANTS_FEAT_UNLOCKER";
const PLAIN: &str = "ID_TB_FEAT_PLAIN";
const PLAIN_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_PLAIN";
const SRD_BOON: &str = "ID_2024_FEAT_BOON_OF_FATE";
const SRD_BOON_PROXY: &str = "ID_SRD_INTERNAL_ITEM_FEAT_PROXY_BOON_OF_FATE";

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let elements = vec![
            json!({"id": "ID_TB_SOURCE", "name": BOOK, "type": "Source", "source": "Core",
                   "setters": [{"name": "abbreviation", "value": "TB"}]}),
            json!({"id": BOON, "name": "Boon of Testing", "type": "Feat", "source": BOOK,
                   "supports": ["Epic Boon"], "prerequisite": "Level 19+",
                   "requirements": format!("[character:19]||{UNLOCKER}")}),
            json!({"id": PLAIN, "name": "Plain Feat", "type": "Feat", "source": BOOK}),
            json!({"id": UNLOCKER_ITEM, "name": "Feat Unlocker", "type": "Item", "source": BOOK,
                   "setters": [{"name": "category", "value": "Additional Feature"},
                               {"name": "slot", "value": "misc"},
                               {"name": "inventory-hidden", "value": "true"}],
                   "rules": [{"kind": "grant", "type": "Grants", "id": UNLOCKER}]}),
            json!({"id": UNLOCKER, "name": "Feat Unlocker", "type": "Grants", "source": BOOK}),
        ];
        let file: ContentFile =
            serde_json::from_value(json!({"format": "borealis-content/1", "elements": elements}))
                .expect("synthetic content");
        let store = ["core", "srd-5.1", "srd-5.2.1"]
            .iter()
            .fold(ContentStoreBuilder::new(), |b, p| {
                b.add_pack_dir(&root.join(p)).expect("bundled pack")
            })
            .add_file("boons.json", PackOrigin::Custom, file)
            .build();
        Engine::new(&store)
    })
}

/// Add `item` and equip it: the new state and the equip warnings.
fn equip(state: &CharacterState, item: &str) -> (CharacterState, Vec<String>) {
    let e = engine();
    let added = commands::add_item(e, state, item, 1).expect("add").state;
    let uid = added.inventory.items.last().expect("line").uid.clone();
    let out = commands::equip(e, &added, &uid, None).expect("equip");
    (out.state, out.warnings)
}

fn has(state: &CharacterState, id: &str) -> bool {
    engine()
        .derive(state)
        .elements
        .iter()
        .any(|e| e.element.id == id)
}

#[test]
fn an_equipped_boon_below_its_level_is_withheld_with_a_warning_naming_the_unlocker() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let hero = commands::new_character(engine(), ruleset);
        let (state, warnings) = equip(&hero, BOON_PROXY);
        assert!(
            !has(&state, BOON),
            "{ruleset:?}: Aurora withholds it below level 19"
        );
        assert_eq!(
            warnings,
            vec![
                "Boon of Testing is not applied yet: its prerequisite (Level 19+) is not met. \
                  Equipping Feat Unlocker lifts it."
                    .to_string()
            ],
            "{ruleset:?}"
        );
        // with the book's unlocker equipped the boon applies, and nothing is said
        let (unlocked, _) = equip(&hero, UNLOCKER_ITEM);
        let (state, warnings) = equip(&unlocked, BOON_PROXY);
        assert!(
            has(&state, BOON),
            "{ruleset:?}: the unlocker lifts the prerequisite"
        );
        assert!(warnings.is_empty(), "{ruleset:?}: {warnings:?}");
    }
}

#[test]
fn the_proxy_detail_names_the_prerequisite() {
    let uni = engine().universe(Ruleset::Dnd2024);
    let boon = &uni.get(BOON_PROXY).expect("boon proxy").el.description;
    assert!(
        boon.starts_with(
            "<p>Equip this item to gain Boon of Testing. It applies once the character meets its \
             prerequisite (Level 19+)."
        ),
        "{boon}"
    );
    let plain = &uni.get(PLAIN_PROXY).expect("plain proxy").el.description;
    assert!(!plain.contains("prerequisite"), "{plain}");
}

#[test]
fn a_feat_without_prerequisite_equips_silently() {
    let (state, warnings) = equip(
        &commands::new_character(engine(), Ruleset::Dnd2024),
        PLAIN_PROXY,
    );
    assert!(has(&state, PLAIN));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn the_srd_boon_names_its_prerequisite_without_an_unlocker() {
    let hero = commands::new_character(engine(), Ruleset::Dnd2024);
    let (state, warnings) = equip(&hero, SRD_BOON_PROXY);
    assert!(!has(&state, SRD_BOON));
    assert_eq!(
        warnings,
        vec![
            "Boon of Fate is not applied yet: its prerequisite (Level 19+) is not met.".to_string()
        ]
    );
}
