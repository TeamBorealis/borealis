//! Generic magic armor is added on a base from its second list (`list_base_items`), named by
//! its `name-format` and weighing what the base weighs (B-21 M9).

use super::*;
use crate::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use crate::error::ErrorCode;
use crate::state::AppPaths;
use std::path::Path;

const ARMOR_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_ARMOR_1";
const CHAIN_MAIL: &str = "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL";
const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";

fn app() -> (tempfile::TempDir, AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    })
    .unwrap();
    let request = NewCharacterRequest {
        name: "Armorer".into(),
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

#[test]
fn generic_magic_armor_lists_armor_bases_and_needs_one() {
    let (_dir, app, id) = app();
    let bases = app.list_base_items(&id, ARMOR_1).unwrap();
    let ids: Vec<&str> = bases.iter().map(|b| b.summary.id.as_str()).collect();
    assert!(ids.contains(&CHAIN_MAIL), "{ids:?}");
    assert!(!ids.contains(&LONGSWORD));
    assert!(
        app.list_base_items(&id, CHAIN_MAIL).unwrap().is_empty(),
        "plain item"
    );
    let refused = app.add_item(&id, ARMOR_1, 1, false, None).unwrap_err();
    assert_eq!(refused.code, ErrorCode::RequirementFailed);
    let wrong = app
        .add_item(&id, ARMOR_1, 1, false, Some(LONGSWORD))
        .unwrap_err();
    assert_eq!(wrong.code, ErrorCode::RequirementFailed);
}

#[test]
fn plus_one_armor_on_chain_mail_is_named_by_its_format_and_weighs_the_base() {
    let (_dir, app, id) = app();
    let snap = app
        .add_item(&id, ARMOR_1, 1, false, Some(CHAIN_MAIL))
        .unwrap();
    let line = snap.inventory.items.last().unwrap();
    assert_eq!(line.display_name, "Chain Mail +1");
    assert!((line.weight - 55.0).abs() < 1e-9, "{}", line.weight);
}
