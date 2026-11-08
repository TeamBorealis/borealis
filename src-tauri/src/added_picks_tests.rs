//! B3 (GitHub #35): the builder's Add actions through the real backend: `list_added_pick_options`,
//! `add_pick`, `remove_pick`, the snapshot's `addedPicks` (and their lines kept out of the
//! inventory), the Added mark on a spell, the HR3 house rule, and save / close / load.

use super::*;
use crate::added_picks::AddPickOption;
use crate::error::ErrorCode;
use borealis_engine::added_picks::AddedKind;

const FEAT: &str = "ID_UICAT_FEAT_WATCHFUL";
const STRONG: &str = "ID_UICAT_FEAT_STRONG";
const STEALTH: &str = "ID_PROFICIENCY_SKILL_STEALTH";
const MAGIC_MISSILE: &str = "ID_2024_SPELL_MAGIC_MISSILE";

/// A 2024 wizard (Strength 8) with a plain feat and one that needs Strength 13 installed.
fn wizard() -> (tempfile::TempDir, AppState, String) {
    let (dir, app, id) = app_with(vec![
        serde_json::json!({"id": FEAT, "name": "Watchful", "type": "Feat", "source": PACK_SOURCE,
            "ruleset": "dnd2024"}),
        serde_json::json!({"id": STRONG, "name": "Strong", "type": "Feat", "source": PACK_SOURCE,
            "ruleset": "dnd2024", "prerequisite": "Strength 13", "requirements": "[str:13]"}),
    ]);
    let snap = app.get_snapshot(&id).unwrap();
    app.apply_selection(&id, &slot_id(&snap, "Class"), "ID_2024_CLASS_WIZARD")
        .expect("class");
    app.set_base_score(&id, "str", 8).expect("strength");
    (dir, app, id)
}

fn options(app: &AppState, id: &str, kind: AddedKind, cast: Option<&str>) -> Vec<AddPickOption> {
    app.list_added_pick_options(id, kind, cast)
        .expect("options")
}

fn offered(app: &AppState, id: &str, kind: AddedKind, cast: Option<&str>, el: &str) -> bool {
    options(app, id, kind, cast).iter().any(|o| o.id == el)
}

#[test]
fn each_kind_is_added_listed_kept_out_of_the_inventory_and_removed() {
    let (_dir, app, id) = wizard();
    let before = app.get_snapshot(&id).unwrap();
    let language = options(&app, &id, AddedKind::Language, None)
        .first()
        .expect("a language to add")
        .id
        .clone();
    app.add_pick(&id, AddedKind::Feat, FEAT, None)
        .expect("feat");
    app.add_pick(&id, AddedKind::Language, &language, None)
        .expect("language");
    app.add_pick(&id, AddedKind::Proficiency, STEALTH, None)
        .expect("skill");
    let snap = app
        .add_pick(&id, AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"))
        .expect("spell");
    let picks: Vec<(AddedKind, &str, Option<&str>)> = snap
        .added_picks
        .iter()
        .map(|p| (p.kind, p.element_id.as_str(), p.spellcasting.as_deref()))
        .collect();
    assert_eq!(
        picks,
        [
            (AddedKind::Feat, FEAT, None),
            (AddedKind::Language, language.as_str(), None),
            (AddedKind::Proficiency, STEALTH, None),
            (AddedKind::Spell, MAGIC_MISSILE, Some("Wizard")),
        ]
    );
    assert!(snap
        .added_picks
        .iter()
        .all(|p| p.applied && !p.prerequisites_unmet));
    assert_eq!(
        snap.inventory.items, before.inventory.items,
        "the picks' lines are not inventory lines"
    );
    let wizard = snap
        .spellcasting
        .classes
        .iter()
        .find(|c| c.class_name == "Wizard")
        .expect("wizard");
    let spell = wizard
        .spells
        .iter()
        .find(|s| s.id == MAGIC_MISSILE)
        .expect("the added spell");
    assert!(spell.added);
    assert!(wizard
        .spells
        .iter()
        .filter(|s| s.id != MAGIC_MISSILE)
        .all(|s| !s.added));
    assert!(!offered(&app, &id, AddedKind::Feat, None, FEAT));
    // removing each takes it away again
    for p in &snap.added_picks {
        app.remove_pick(&id, &p.inventory_id).expect("remove");
    }
    let after = app.get_snapshot(&id).unwrap();
    assert!(after.added_picks.is_empty());
    assert!(offered(&app, &id, AddedKind::Feat, None, FEAT));
}

#[test]
fn what_is_not_offered_cannot_be_added_and_equipment_lines_cannot_be_removed() {
    let (_dir, app, id) = wizard();
    assert!(!offered(&app, &id, AddedKind::Feat, None, STRONG));
    let refused = app
        .add_pick(&id, AddedKind::Feat, STRONG, None)
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::RequirementFailed);
    assert_eq!(
        refused.message,
        "That feat cannot be added to this character."
    );
    let missing = app
        .add_pick(&id, AddedKind::Feat, "ID_NO_SUCH_FEAT", None)
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::NotFound);
    let classless = app
        .add_pick(&id, AddedKind::Spell, MAGIC_MISSILE, None)
        .unwrap_err();
    assert_eq!(classless.code, ErrorCode::InvalidArgument);
    assert!(app
        .list_added_pick_options(&id, AddedKind::Spell, None)
        .is_err());
    let snap = app
        .add_item(&id, "ID_2024_ITEM_ROPE", 1, false, None)
        .expect("rope");
    let uid = snap.inventory.items.last().expect("rope").id.clone();
    assert!(app.remove_pick(&id, &uid).is_err());
}

#[test]
fn ignoring_feat_prerequisites_offers_and_marks_and_turning_it_off_withholds() {
    let (_dir, app, id) = wizard();
    let on = crate::dto::OptionsPatch {
        ignore_feat_prerequisites: Some(true),
        ..crate::dto::OptionsPatch::default()
    };
    app.set_character_options(&id, &on).expect("rule on");
    let strong = options(&app, &id, AddedKind::Feat, None)
        .into_iter()
        .find(|o| o.id == STRONG)
        .expect("offered under the rule");
    assert!(!strong.qualifies);
    assert_eq!(strong.prerequisite, "Strength 13");
    let snap = app
        .add_pick(&id, AddedKind::Feat, STRONG, None)
        .expect("add");
    let pick = &snap.added_picks[0];
    assert!(pick.applied && pick.prerequisites_unmet);
    let off = crate::dto::OptionsPatch {
        ignore_feat_prerequisites: Some(false),
        ..crate::dto::OptionsPatch::default()
    };
    let snap = app.set_character_options(&id, &off).expect("rule off");
    let pick = &snap.added_picks[0];
    assert!(!pick.applied && !pick.prerequisites_unmet, "kept, withheld");
}

#[test]
fn added_picks_survive_save_close_and_load() {
    let (_dir, app, id) = wizard();
    app.add_pick(&id, AddedKind::Feat, FEAT, None)
        .expect("feat");
    app.add_pick(&id, AddedKind::Proficiency, STEALTH, None)
        .expect("skill");
    let snap = app
        .add_pick(&id, AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"))
        .expect("spell");
    app.save_character(&id).expect("save");
    app.close_character(&id, false).expect("close");
    let loaded = app.load_character(&id).expect("load");
    assert_eq!(loaded.added_picks, snap.added_picks);
    assert_eq!(loaded.inventory.items, snap.inventory.items);
    let line = app
        .read(&id, |_, s| {
            Ok(s.character
                .build
                .inventory
                .items
                .iter()
                .filter(|i| i.added_pick)
                .count())
        })
        .unwrap();
    assert_eq!(line, 3);
}

#[test]
fn moving_an_item_steps_over_the_added_picks_lines() {
    let (_dir, app, id) = wizard();
    app.add_item(&id, "ID_2024_ITEM_ROPE", 1, false, None)
        .expect("rope");
    app.add_pick(&id, AddedKind::Feat, FEAT, None)
        .expect("feat");
    let snap = app
        .add_item(&id, "ID_2024_ITEM_TORCH", 1, false, None)
        .expect("torch");
    let n = snap.inventory.items.len();
    let last = snap.inventory.items[n - 1].id.clone();
    let moved = app.move_item(&id, &last, "up").expect("move");
    assert_eq!(
        moved.inventory.items[n - 2].id,
        last,
        "one visible step up, over the hidden line"
    );
}
