use crate::content_dto::ImportPackRequest;
use crate::content_packs::NullSink;
use crate::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, RolledCharges, Ruleset};
use crate::dto_ext::EquipRequest;
use crate::state::{AppPaths, AppState};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::path::Path;

const WAND: &str = "ID_RCT_WAND_LIVES";
const CHARGES: &str = "ID_RCT_WAND_LIVES_CHARGES";

fn app_with_wand() -> (tempfile::TempDir, AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    })
    .unwrap();
    let pack = serde_json::json!({"format": "borealis-content/1", "elements": [{
        "id": WAND, "name": "Wand of Lives", "type": "Magic Item", "source": "Test",
        "setters": [{"name": "category", "value": "Wondrous Items"}, {"name": "slot", "value": "wondrous"}],
        "rules": [{"kind": "resource", "id": CHARGES, "name": "Wand of Lives charges",
                   "max": {"type": "rolled", "dice": "1d6 + 1"}, "recharge": ["none"]}]}]});
    let imported = app
        .import_pack(
            &ImportPackRequest {
                file_name: "wand.json".into(),
                content_base64: STANDARD.encode(pack.to_string()),
                format: Some("json".into()),
            },
            &NullSink,
        )
        .unwrap();
    assert!(imported.pack.is_some(), "{:?}", imported.report);
    let request = NewCharacterRequest {
        name: "Holder".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let id = app.create_character(Ruleset::Dnd2014, &request).unwrap().id;
    (dir, app, id)
}

fn charges(snap: &crate::dto::CharacterSnapshot) -> (String, RolledCharges) {
    let item = snap
        .inventory
        .items
        .iter()
        .find(|i| i.item_id == WAND)
        .expect("wand in inventory");
    let [c] = item.rolled_charges.as_slice() else {
        panic!("one rolled resource: {:?}", item.rolled_charges);
    };
    (item.id.clone(), c.clone())
}

#[test]
fn rolled_charges_are_shown_and_can_be_entered_or_rolled_again() {
    let (_dir, app, id) = app_with_wand();
    let added = app.add_item(&id, WAND, 1, false, None).unwrap();
    let uid = added.inventory.items.last().unwrap().id.clone();
    // an item's charges exist while it is worn
    let snap = app.equip_item(&id, &uid, EquipRequest::Auto).unwrap();
    let (uid, c) = charges(&snap);
    assert_eq!(
        (c.resource_id.as_str(), c.dice.as_str()),
        (CHARGES, "1d6 + 1")
    );
    assert_eq!((c.min, c.max), (2, 7));
    assert!(c.rolled && (2..=7).contains(&c.value), "{c:?}");
    let entered = app.set_rolled_max(&id, &uid, CHARGES, 6).unwrap();
    assert_eq!(charges(&entered).1.value, 6);
    assert!(
        app.set_rolled_max(&id, &uid, CHARGES, 8).is_err(),
        "outside 2..7"
    );
    assert_eq!(charges(&app.get_snapshot(&id).unwrap()).1.value, 6);
    let again = app.reroll_max(&id, &uid, CHARGES).unwrap();
    assert!((2..=7).contains(&charges(&again).1.value));
    assert!(app.reroll_max(&id, &uid, "ID_RCT_NOT_A_RESOURCE").is_err());
}
