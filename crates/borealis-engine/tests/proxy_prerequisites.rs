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
const BLOCKER_ITEM: &str = "ID_TB_ITEM_BLOCKING_CHARM";
const BLOCKER: &str = "ID_TB_GRANTS_BLOCKER";
const GUARDED: &str = "ID_TB_FEAT_GUARDED_BOON";
const GUARDED_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_GUARDED_BOON";
const RING: &str = "ID_TB_MAGIC_ITEM_RING_OF_TESTING";
/// What the boon grants in turn: one element without requirements, one with its own.
const BOON_PART: &str = "ID_TB_FEAT_BOON_PART";
const BOON_LATE: &str = "ID_TB_FEAT_BOON_LATE";
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
                   "requirements": format!("[character:19]||{UNLOCKER}"),
                   "rules": [{"kind": "grant", "type": "Feat", "id": BOON_PART},
                             {"kind": "grant", "type": "Feat", "id": BOON_LATE}]}),
            json!({"id": BOON_PART, "name": "Testing Gift", "type": "Feat", "source": BOOK}),
            json!({"id": BOON_LATE, "name": "Late Gift", "type": "Feat", "source": BOOK,
                   "requirements": "[character:20]"}),
            json!({"id": PLAIN, "name": "Plain Feat", "type": "Feat", "source": BOOK}),
            json!({"id": UNLOCKER_ITEM, "name": "Feat Unlocker", "type": "Item", "source": BOOK,
                   "setters": [{"name": "category", "value": "Additional Feature"},
                               {"name": "slot", "value": "misc"},
                               {"name": "inventory-hidden", "value": "true"}],
                   "rules": [{"kind": "grant", "type": "Grants", "id": UNLOCKER}]}),
            json!({"id": UNLOCKER, "name": "Feat Unlocker", "type": "Grants", "source": BOOK}),
            // review L2: an item granting an id the boon tests the *absence* of cannot unlock it
            json!({"id": BLOCKER_ITEM, "name": "Blocking Charm", "type": "Item", "source": BOOK,
                   "setters": [{"name": "slot", "value": "misc"}],
                   "rules": [{"kind": "grant", "type": "Grants", "id": BLOCKER}]}),
            json!({"id": BLOCKER, "name": "Blocker", "type": "Grants", "source": BOOK}),
            json!({"id": GUARDED, "name": "Guarded Boon", "type": "Feat", "source": BOOK,
                   "prerequisite": "Level 19+",
                   "requirements": format!("[character:19],!{BLOCKER}")}),
            // review L3: an attunement item that grants the boon
            json!({"id": RING, "name": "Ring of Testing", "type": "Magic Item", "source": BOOK,
                   "setters": [{"name": "slot", "value": "ring"}, {"name": "attunement", "value": "true"}],
                   "rules": [{"kind": "grant", "type": "Feat", "id": BOON}]}),
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
                  Equip Feat Unlocker, or turn on Override requirements for this item."
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
            "Boon of Fate is not applied yet: its prerequisite (Level 19+) is not met. \
             Turn on Override requirements for this item to apply it anyway."
                .to_string()
        ]
    );
}

// U12 (GitHub #3): "Override requirements" on an equipped item whose grant is withheld. Per item
// line, saved with the character, reversible. Only the requirements of what the item itself
// grants are bypassed; what those elements grant in turn follows the normal rules.

fn line<'s>(state: &'s CharacterState, item: &str) -> &'s borealis_engine::state::InventoryItem {
    state
        .inventory
        .items
        .iter()
        .find(|i| i.item == item)
        .expect("line")
}

fn snapshot_line(state: &CharacterState, uid: &str) -> borealis_engine::snapshot::ItemSnapshot {
    engine()
        .derive(state)
        .inventory
        .items
        .into_iter()
        .find(|i| i.uid == uid)
        .expect("snapshot line")
}

fn set_override(state: &CharacterState, uid: &str, on: bool) -> CharacterState {
    commands::set_requirements_override(engine(), state, uid, on)
        .expect("override")
        .state
}

/// JSON round trip of the build, as the character file stores it.
fn reload(state: &CharacterState) -> CharacterState {
    serde_json::from_str(&serde_json::to_string(state).expect("save")).expect("load")
}

#[test]
fn override_requirements_applies_a_withheld_boon_and_turning_it_off_withholds_it_again() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        for (proxy, boon) in [(BOON_PROXY, BOON), (SRD_BOON_PROXY, SRD_BOON)] {
            let hero = commands::new_character(engine(), ruleset);
            let (state, _) = equip(&hero, proxy);
            let uid = line(&state, proxy).uid.clone();
            let off = snapshot_line(&state, &uid);
            assert!(!off.override_requirements, "{ruleset:?} {boon}");
            assert_eq!(
                off.withheld_grants
                    .iter()
                    .map(|g| g.id.as_str())
                    .collect::<Vec<_>>(),
                vec![boon],
                "{ruleset:?}: the line offers the override"
            );
            let on = set_override(&state, &uid, true);
            assert!(
                has(&on, boon),
                "{ruleset:?} {boon}: applied with the override"
            );
            let shown = snapshot_line(&on, &uid);
            assert!(shown.override_requirements && shown.withheld_grants.is_empty());
            let back = reload(&on);
            assert!(
                line(&back, proxy).override_requirements,
                "saved with the line"
            );
            assert!(
                has(&back, boon),
                "{ruleset:?} {boon}: survives save and load"
            );
            let again = set_override(&back, &uid, false);
            assert!(
                !has(&again, boon),
                "{ruleset:?} {boon}: withheld again when off"
            );
            assert_eq!(snapshot_line(&again, &uid).withheld_grants.len(), 1);
        }
    }
}

#[test]
fn only_what_the_item_grants_is_overridden_not_what_that_grants_in_turn() {
    let hero = commands::new_character(engine(), Ruleset::Dnd2024);
    let (state, _) = equip(&hero, BOON_PROXY);
    let uid = line(&state, BOON_PROXY).uid.clone();
    let on = set_override(&state, &uid, true);
    assert!(has(&on, BOON));
    assert!(
        has(&on, BOON_PART),
        "the boon's own grant without requirements applies"
    );
    assert!(
        !has(&on, BOON_LATE),
        "the boon's grant with requirements of its own keeps them"
    );
}

#[test]
fn an_overridden_item_equips_without_the_warning_and_an_old_file_has_no_override() {
    let hero = commands::new_character(engine(), Ruleset::Dnd2024);
    let (state, _) = equip(&hero, SRD_BOON_PROXY);
    let uid = line(&state, SRD_BOON_PROXY).uid.clone();
    let on = set_override(&state, &uid, true);
    let unequipped = commands::unequip(engine(), &on, &uid)
        .expect("unequip")
        .state;
    let again = commands::equip(engine(), &unequipped, &uid, None).expect("equip");
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert!(has(&again.state, SRD_BOON));
    // a file written before the flag existed: no key, so off
    let mut json: serde_json::Value = serde_json::to_value(&state).expect("json");
    for item in json["inventory"]["items"].as_array_mut().expect("items") {
        assert!(
            item.get("override_requirements").is_none(),
            "off is not written"
        );
        item.as_object_mut()
            .unwrap()
            .remove("override_requirements");
    }
    let old: CharacterState = serde_json::from_value(json).expect("old file loads");
    assert!(!line(&old, SRD_BOON_PROXY).override_requirements);
    assert!(!has(&old, SRD_BOON));
}

#[test]
fn the_override_is_refused_for_an_unknown_line() {
    let hero = commands::new_character(engine(), Ruleset::Dnd2024);
    assert!(commands::set_requirements_override(engine(), &hero, "item-404", true).is_err());
}

#[test]
fn an_item_granting_an_id_the_requirement_negates_is_no_unlocker() {
    let hero = commands::new_character(engine(), Ruleset::Dnd2024);
    let (_, warnings) = equip(&hero, GUARDED_PROXY);
    assert_eq!(
        warnings,
        vec![
            "Guarded Boon is not applied yet: its prerequisite (Level 19+) is not met. \
              Turn on Override requirements for this item to apply it anyway."
                .to_string()
        ]
    );
}

#[test]
fn an_unattuned_attunement_item_gives_no_prerequisite_warning() {
    // no attunement slot left: equipped but not attuned, so its rules do not apply yet and only
    // the attunement warning is given (the boon's prerequisite is not the reason)
    let mut hero = commands::new_character(engine(), Ruleset::Dnd2024);
    hero.house_rules.attunement_limit = Some(0);
    let (state, warnings) = equip(&hero, RING);
    assert!(!line(&state, RING).attuned);
    assert!(
        warnings.iter().all(|w| !w.contains("prerequisite")),
        "{warnings:?}"
    );
    assert!(warnings.iter().any(|w| w.contains("attun")), "{warnings:?}");
    assert!(snapshot_line(&state, &line(&state, RING).uid)
        .withheld_grants
        .is_empty());
}
