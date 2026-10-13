//! Service tests: spell and weapon DTO mapping against the real SRD 5.1 pack, equipment
//! rules, spells, sheet export and the compendium.

use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::dto_ext::{
    CompendiumQuery, EquipRequest, ItemFilter, ItemPatch, SheetOptionsDto,
};
use borealis_lib::error::{ApiError, ErrorCode};
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const WIZARD: &str = "ID_WOTC_PHB_CLASS_WIZARD";
const CLERIC: &str = "ID_WOTC_PHB_CLASS_CLERIC";
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const SORCERER: &str = "ID_WOTC_PHB_CLASS_SORCERER";
const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";
const RAPIER: &str = "ID_WOTC_PHB_WEAPON_RAPIER";
const SHIELD: &str = "ID_WOTC_GEAR_SHIELD";
const ATTUNABLE: [&str; 4] = [
    "ID_WOTC_DMG_MAGIC_ITEM_AMULET_OF_HEALTH",
    "ID_WOTC_DMG_MAGIC_ITEM_BELT_OF_HILL_GIANT_STRENGTH",
    "ID_WOTC_DMG_MAGIC_ITEM_CLOAK_OF_PROTECTION",
    "ID_WOTC_DMG_MAGIC_ITEM_RING_OF_PROTECTION",
];

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

fn code<T: std::fmt::Debug>(r: Result<T, ApiError>) -> ErrorCode {
    r.expect_err("expected an error").code
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

/// A level-`level` human of `class` with default point-buy scores.
fn hero(app: &AppState, class: &str, level: u32) -> (String, CharacterSnapshot) {
    let request = NewCharacterRequest {
        name: "Test Hero".into(),
        ruleset: Ruleset::Dnd2014,
        starting_level: 1,
        gender: "f".into(),
        ability_method: AbilityMethodDto::PointBuy,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(Ruleset::Dnd2014, &request)
        .expect("create");
    let id = snap.id.clone();
    let s = app
        .apply_selection(&id, &slot_id(&snap, "Race"), "ID_RACE_HUMAN")
        .expect("race");
    let mut s = app
        .apply_selection(&id, &slot_id(&s, "Class"), class)
        .expect("class");
    for _ in 1..level {
        s = app.level_up(&id, "main").expect("level up");
    }
    (id, s)
}

/// Choose the first offered option in every empty cantrip / spell slot.
fn fill_spell_slots(app: &AppState, id: &str) -> CharacterSnapshot {
    let mut snap = app.get_snapshot(id).expect("snapshot");
    for _ in 0..60 {
        let open = snap.slots.iter().find(|s| {
            s.chosen.is_none()
                && (s.title.starts_with("Cantrip")
                    || s.title.starts_with("Spellbook")
                    || s.title.starts_with("Spell"))
        });
        let Some(slot) = open else { break };
        let options = app
            .get_slot_options(id, &slot.id, None, Some(1), None)
            .expect("options");
        let Some(first) = options.items.first() else {
            break;
        };
        snap = app
            .apply_selection(id, &slot.id, &first.summary.id)
            .expect("pick spell");
    }
    snap
}

fn add_and_find(app: &AppState, id: &str, item: &str) -> (CharacterSnapshot, String) {
    let snap = app.add_item(id, item, 1, false, None).expect("add item");
    let line = snap
        .inventory
        .items
        .iter()
        .find(|i| i.item_id == item)
        .unwrap_or_else(|| panic!("{item} missing"))
        .id
        .clone();
    (snap, line)
}

#[test]
fn level_5_wizard_spells_map_with_real_fields() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, WIZARD, 5);
    let snap = fill_spell_slots(&app, &id);
    let class = &snap.spellcasting.classes[0];
    assert_eq!(class.class_name, "Wizard");
    assert_eq!(class.model, "spellbook", "wizards prepare from a spellbook");
    assert_eq!(class.ability, "int");
    assert!(
        class.cantrips_max >= 4,
        "cantrips max {}",
        class.cantrips_max
    );
    assert_eq!(class.cantrips_known, class.cantrips_max);
    assert_eq!(class.max_spell_level, 3);
    assert!(class.prepared_max >= 1);
    assert!(class.spells.len() >= 14, "{} spells", class.spells.len());
    assert!(
        class.spells.iter().all(|s| !s.school.is_empty()),
        "school from the setter"
    );
    assert!(
        class.spells.iter().all(|s| s.slot_id.is_some()),
        "learned spells carry their slot"
    );
    let any_concentration = class.spells.iter().any(|s| s.concentration);
    let any_ritual = class.spells.iter().any(|s| s.ritual);
    assert!(
        any_concentration || any_ritual,
        "concentration / ritual flags are read"
    );
    assert!(class.save_dc >= 8 && class.attack_bonus >= 2);
    assert_eq!(
        snap.spellcasting.slots.iter().map(|s| s.level).max(),
        Some(3)
    );
}

#[test]
fn level_5_fighter_weapon_fields_map_with_real_fields() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 5);
    let (_, sword) = add_and_find(&app, &id, LONGSWORD);
    let (snap, rapier) = add_and_find(&app, &id, RAPIER);
    let sword_row = snap
        .inventory
        .items
        .iter()
        .find(|i| i.id == sword)
        .expect("sword");
    let w = sword_row.weapon.as_ref().expect("weapon info");
    assert_eq!(w.damage, "1d8");
    assert_eq!(w.damage_type, "slashing");
    assert!(
        w.properties.contains("Versatile (1d10)"),
        "properties: {}",
        w.properties
    );
    assert_eq!(w.ability, "str");
    assert_eq!(sword_row.category, "Weapons");
    assert!(sword_row.is_weapon && !sword_row.is_pack);
    assert!((sword_row.weight - 3.0).abs() < f64::EPSILON);
    assert_eq!(sword_row.value_cp, 1500);
    let rapier_row = snap
        .inventory
        .items
        .iter()
        .find(|i| i.id == rapier)
        .expect("rapier");
    let rw = rapier_row.weapon.as_ref().expect("rapier info");
    assert_eq!(
        (rw.damage.as_str(), rw.damage_type.as_str()),
        ("1d8", "piercing")
    );
    assert!(
        rw.properties.contains("Finesse"),
        "properties: {}",
        rw.properties
    );

    let equipped = app
        .equip_item(&id, &sword, EquipRequest::Primary)
        .expect("equip");
    let row = equipped
        .inventory
        .items
        .iter()
        .find(|i| i.id == sword)
        .expect("row");
    assert_eq!(row.equipped.as_deref(), Some("primary"));
    assert!(
        equipped
            .attacks
            .iter()
            .any(|a| a.auto && a.damage.contains("1d8")),
        "auto attack"
    );
    let versatile = app
        .equip_item(&id, &sword, EquipRequest::Versatile)
        .expect("versatile");
    let row = versatile
        .inventory
        .items
        .iter()
        .find(|i| i.id == sword)
        .expect("row");
    assert_eq!(row.equipped.as_deref(), Some("two-handed"));
    assert_eq!(
        code(app.equip_item(&id, &rapier, EquipRequest::Versatile)),
        ErrorCode::RequirementFailed,
        "a rapier is not versatile"
    );
}

#[test]
fn cleric_prepares_spells_up_to_the_limit() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, CLERIC, 5);
    let snap = app.get_snapshot(&id).expect("snapshot");
    let class = &snap.spellcasting.classes[0];
    assert_eq!(
        class.model, "prepared",
        "clerics prepare from the whole list"
    );
    let limit = class.prepared_max as usize;
    assert!(limit >= 1);
    let always: Vec<_> = class.spells.iter().filter(|s| s.always_prepared).collect();
    assert!(!always.is_empty(), "domain spells are always prepared");
    assert_eq!(
        code(app.set_spell_prepared(&id, "main", &always[0].id, false)),
        ErrorCode::RequirementFailed
    );
    let candidates: Vec<String> = class
        .spells
        .iter()
        .filter(|s| s.level > 0 && !s.always_prepared)
        .map(|s| s.id.clone())
        .collect();
    assert!(candidates.len() > limit);
    let mut last = snap;
    for spell in candidates.iter().take(limit) {
        last = app
            .set_spell_prepared(&id, "main", spell, true)
            .expect("prepare");
    }
    assert_eq!(last.spellcasting.classes[0].prepared_count as usize, limit);
    assert_eq!(
        code(app.set_spell_prepared(&id, "main", &candidates[limit], true)),
        ErrorCode::RequirementFailed,
        "the limit is enforced"
    );
    let freed = app
        .set_spell_prepared(&id, "main", &candidates[0], false)
        .expect("unprepare");
    assert_eq!(
        freed.spellcasting.classes[0].prepared_count as usize,
        limit - 1
    );
    let flagged = freed.spellcasting.classes[0]
        .spells
        .iter()
        .find(|s| s.id == candidates[0])
        .expect("spell");
    assert!(!flagged.prepared);
    assert_eq!(
        code(app.set_spell_prepared(&id, "nope", &candidates[0], true)),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn replace_spell_frees_the_slot_only_for_classes_that_swap() {
    let (_dir, app) = app();
    let (wizard, _) = hero(&app, WIZARD, 2);
    let snap = fill_spell_slots(&app, &wizard);
    let slot = snap.spellcasting.classes[0].spells[0]
        .slot_id
        .clone()
        .expect("slot");
    assert_eq!(
        code(app.replace_spell(&wizard, &slot)),
        ErrorCode::RequirementFailed
    );

    let (sorc, _) = hero(&app, SORCERER, 3);
    let snap = fill_spell_slots(&app, &sorc);
    let spell = snap.spellcasting.classes[0]
        .spells
        .iter()
        .find(|s| s.level > 0 && s.can_replace)
        .expect("a swappable spell")
        .clone();
    let after = app
        .replace_spell(&sorc, spell.slot_id.as_deref().expect("slot id"))
        .expect("replace");
    assert!(after.spellcasting.classes[0]
        .spells
        .iter()
        .all(|s| s.id != spell.id));
    let reopened = after
        .slots
        .iter()
        .find(|s| Some(&s.id) == spell.slot_id.as_ref())
        .expect("slot stays");
    assert!(reopened.chosen.is_none());
    assert!(
        !reopened.allow_replace,
        "an empty slot has nothing to replace"
    );
    assert_eq!(
        reopened.gained_on_level, 3,
        "reopens at the current class level"
    );
    assert_eq!(reopened.replaced.len(), 1);
    assert_eq!(reopened.replaced[0].id, spell.id);
    assert_eq!(reopened.replaced[0].name, spell.name);
}

#[test]
fn attunement_limit_is_enforced_e10() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 1);
    let mut lines = Vec::new();
    for item in ATTUNABLE {
        lines.push(add_and_find(&app, &id, item).1);
    }
    let mut snap = app.get_snapshot(&id).expect("snapshot");
    for line in &lines[..3] {
        snap = app
            .equip_item(&id, line, EquipRequest::Worn)
            .expect("equip");
        assert!(snap.inventory.warnings.is_empty());
    }
    assert_eq!(
        (
            snap.inventory.attunement.used,
            snap.inventory.attunement.max
        ),
        (3, 3)
    );
    let fourth = app
        .equip_item(&id, &lines[3], EquipRequest::Worn)
        .expect("equip fourth");
    let row = fourth
        .inventory
        .items
        .iter()
        .find(|i| i.id == lines[3])
        .expect("row");
    assert_eq!(row.equipped.as_deref(), Some("worn"));
    assert!(!row.attuned, "equipped but not attuned");
    assert_eq!(fourth.inventory.attunement.used, 3);
    assert!(
        fourth
            .inventory
            .warnings
            .iter()
            .any(|w| w.contains("attunement limit")),
        "warnings: {:?}",
        fourth.inventory.warnings
    );
    assert_eq!(
        code(app.set_attuned(&id, &lines[3], true)),
        ErrorCode::RequirementFailed
    );
    let freed = app
        .set_attuned(&id, &lines[0], false)
        .expect("end attunement");
    assert_eq!(freed.inventory.attunement.used, 2);
    let now = app.set_attuned(&id, &lines[3], true).expect("attune");
    assert_eq!(now.inventory.attunement.used, 3);
    let unequipped = app.unequip_item(&id, &lines[3]).expect("unequip");
    assert_eq!(unequipped.inventory.attunement.used, 2);
    assert!(
        !unequipped
            .inventory
            .items
            .iter()
            .find(|i| i.id == lines[3])
            .expect("row")
            .attuned
    );
}

#[test]
fn coins_make_change_e9() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 1);
    let s = app.adjust_coins(&id, "pp", 1).expect("deposit");
    assert_eq!(s.inventory.coins.pp, 1);
    let s = app
        .adjust_coins(&id, "gp", -3)
        .expect("withdraw with change");
    assert_eq!(s.inventory.coins.pp, 0, "the platinum piece was broken");
    assert!(
        (s.inventory.total_gp - 7.0).abs() < 1e-9,
        "total {}",
        s.inventory.total_gp
    );
    assert_eq!(s.inventory.coins.gp + s.inventory.coins.ep / 2, 7);
    let before = s.inventory.coins;
    assert_eq!(
        code(app.adjust_coins(&id, "gp", -100)),
        ErrorCode::RequirementFailed
    );
    assert_eq!(
        app.get_snapshot(&id).expect("snap").inventory.coins,
        before,
        "failure changes nothing"
    );
    assert_eq!(
        code(app.adjust_coins(&id, "zz", 1)),
        ErrorCode::InvalidArgument
    );

    app.adjust_coins(&id, "sp", 7).expect("silver");
    let s = app.convert_coins(&id, "gp").expect("convert");
    assert!((s.inventory.total_gp - 7.7).abs() < 1e-9);
    assert_eq!(
        (
            s.inventory.coins.gp,
            s.inventory.coins.sp,
            s.inventory.coins.ep
        ),
        (7, 7, 0)
    );

    let bought = app
        .add_item(&id, LONGSWORD, 1, true, None)
        .expect_err("cannot afford 15 gp");
    assert_eq!(bought.code, ErrorCode::RequirementFailed);
    let rich = app.adjust_coins(&id, "gp", 20).expect("gold");
    let before = rich.inventory.total_gp;
    let bought = app.add_item(&id, LONGSWORD, 1, true, None).expect("buy");
    assert!((before - bought.inventory.total_gp - 15.0).abs() < 1e-9);
    assert_eq!(bought.inventory.items.len(), 1);
}

#[test]
fn inventory_housekeeping_commands() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 1);
    let two = app
        .add_item(&id, LONGSWORD, 2, false, None)
        .expect("two swords");
    assert_eq!(two.inventory.items[0].quantity, 2);
    let stacked = app.add_item(&id, LONGSWORD, 1, false, None).expect("stack");
    assert_eq!(stacked.inventory.items.len(), 1);
    assert_eq!(stacked.inventory.items[0].quantity, 3);
    let line = stacked.inventory.items[0].id.clone();
    let fewer = app.remove_item(&id, &line, false).expect("remove one");
    assert_eq!(fewer.inventory.items[0].quantity, 2);

    let (_, shield) = add_and_find(&app, &id, SHIELD);
    let renamed = app
        .update_item_details(
            &id,
            &shield,
            &ItemPatch {
                display_name: Some("Family shield".into()),
                notes: Some("Dented".into()),
                list_in_inventory: Some(false),
                ..Default::default()
            },
        )
        .expect("details");
    let row = renamed
        .inventory
        .items
        .iter()
        .find(|i| i.id == shield)
        .expect("row");
    assert_eq!(
        (row.display_name.as_str(), row.notes.as_str()),
        ("Family shield", "Dented")
    );
    assert!(!row.list_in_inventory);

    let named = app
        .set_storage_names(&id, ["Backpack".into(), "Horse".into()])
        .expect("names");
    assert_eq!(named.inventory.storage_names, ["Backpack", "Horse"]);
    assert_eq!(
        code(app.set_storage_names(&id, ["Same".into(), "same".into()])),
        ErrorCode::InvalidArgument
    );
    let stored = app.store_item(&id, &shield, 2).expect("store");
    assert_eq!(
        stored
            .inventory
            .items
            .iter()
            .find(|i| i.id == shield)
            .expect("row")
            .storage,
        2
    );
    let renamed_again = app
        .set_storage_names(&id, ["Pack".into(), "Mule".into()])
        .expect("rename keeps contents");
    assert_eq!(
        renamed_again
            .inventory
            .items
            .iter()
            .find(|i| i.id == shield)
            .expect("row")
            .storage,
        2
    );
    assert_eq!(
        code(app.store_item(&id, &shield, 3)),
        ErrorCode::InvalidArgument
    );

    let first_before = renamed_again.inventory.items[0].id.clone();
    let moved = app.move_item(&id, &first_before, "down").expect("move");
    assert_eq!(moved.inventory.items[1].id, first_before);
    assert_eq!(
        code(app.move_item(&id, &first_before, "sideways")),
        ErrorCode::InvalidArgument
    );

    let removed = app.remove_item(&id, &shield, true).expect("remove");
    assert!(removed.inventory.items.iter().all(|i| i.id != shield));
    assert_eq!(
        code(app.remove_item(&id, "item-999", true)),
        ErrorCode::NotFound
    );
}

#[test]
fn equipment_packs_open_into_their_contents() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 1);
    let (snap, pack) = add_and_find(&app, &id, "ID_WOTC_ITEM_BURGLARS_PACK");
    let row = snap
        .inventory
        .items
        .iter()
        .find(|i| i.id == pack)
        .expect("pack");
    assert!(row.is_pack);
    assert!(
        row.pack_contents.iter().any(|c| c.contains("Candle")),
        "{:?}",
        row.pack_contents
    );
    let opened = app.extract_pack(&id, &pack).expect("extract");
    assert!(opened
        .inventory
        .items
        .iter()
        .all(|i| i.item_id != "ID_WOTC_ITEM_BURGLARS_PACK"));
    assert!(opened.inventory.items.len() >= 5);
    assert_eq!(
        code(app.extract_pack(&id, &opened.inventory.items[0].id)),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn item_search_ranks_prefix_before_substring_and_filters() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, FIGHTER, 1);
    let long = app
        .search_items(&ItemFilter {
            query: Some("long".into()),
            ..ItemFilter::default()
        })
        .expect("search");
    assert!(long.total >= 2);
    let names: Vec<String> = long
        .items
        .iter()
        .map(|i| i.summary.name.to_lowercase())
        .collect();
    let first_non_prefix = names
        .iter()
        .position(|n| !n.starts_with("long"))
        .unwrap_or(names.len());
    assert!(names[..first_non_prefix]
        .iter()
        .all(|n| n.starts_with("long")));
    assert!(
        names[first_non_prefix..]
            .iter()
            .all(|n| !n.starts_with("long")),
        "prefix hits come first"
    );
    let weapons = app
        .search_items(&ItemFilter {
            category: Some("Weapons".into()),
            limit: Some(10),
            character_id: Some(id.clone()),
            ..ItemFilter::default()
        })
        .expect("weapons");
    assert!(weapons.total >= 30 && weapons.items.len() == 10);
    assert!(weapons
        .items
        .iter()
        .all(|i| i.category == "Weapons" && i.cost_cp > 0));
    let page2 = app
        .search_items(&ItemFilter {
            category: Some("Weapons".into()),
            limit: Some(10),
            offset: Some(10),
            character_id: Some(id.clone()),
            ..ItemFilter::default()
        })
        .expect("page 2");
    assert!(page2
        .items
        .iter()
        .all(|i| weapons.items.iter().all(|w| w.summary.id != i.summary.id)));
    let magic = app
        .search_items(&ItemFilter {
            query: Some("ring of protection".into()),
            ..ItemFilter::default()
        })
        .expect("magic");
    assert!(magic.items.iter().any(|i| i.requires_attunement));
    assert_eq!(
        code(app.search_items(&ItemFilter {
            character_id: Some("c_missing".into()),
            ..ItemFilter::default()
        })),
        ErrorCode::NotFound
    );
}

#[test]
fn sheet_preview_and_pdf_export_produce_real_documents() {
    let (_dir, app) = app();
    let (id, _) = hero(&app, WIZARD, 3);
    fill_spell_slots(&app, &id);
    let preview = app
        .render_sheet(&id, &SheetOptionsDto::default())
        .expect("preview");
    assert!(
        preview.html.starts_with("<!DOCTYPE html>") || preview.html.starts_with("<!doctype html>")
    );
    assert!(preview.html.contains("Test Hero"));
    assert!(preview.page_count >= 2);
    assert!(preview.generated_at.ends_with('Z'));

    let pdf = app
        .sheet_file(&id, &SheetOptionsDto::default())
        .expect("pdf");
    assert_eq!(
        (pdf.file_name.as_str(), pdf.mime),
        ("Test_Hero.pdf", "application/pdf")
    );
    assert!(pdf.bytes.starts_with(b"%PDF-"));
    assert!(pdf.bytes.len() > 5_000, "{} bytes", pdf.bytes.len());
    let tail = String::from_utf8_lossy(&pdf.bytes[pdf.bytes.len() - 16..]).to_string();
    assert!(tail.contains("%%EOF"), "tail: {tail:?}");
    let again = app
        .sheet_file(&id, &SheetOptionsDto::default())
        .expect("pdf again");
    assert_eq!(again.bytes, pdf.bytes, "rendering is deterministic");

    let encoded = pdf.clone().into_base64();
    assert!(encoded.path.is_none() && encoded.data_base64.is_some_and(|d| !d.is_empty()));
    let html = app
        .sheet_file(
            &id,
            &SheetOptionsDto {
                format: Some("html".into()),
            },
        )
        .expect("html");
    assert_eq!(html.mime, "text/html");
    assert_eq!(
        code(app.sheet_file(
            &id,
            &SheetOptionsDto {
                format: Some("docx".into())
            }
        )),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.render_sheet("c_missing", &SheetOptionsDto::default())),
        ErrorCode::NotFound
    );
}

fn query(text: &str) -> CompendiumQuery {
    CompendiumQuery {
        query: text.into(),
        types: None,
        sources: None,
        ruleset: None,
        limit: 20,
        offset: 0,
    }
}

#[test]
fn compendium_ranks_names_first_and_paginates() {
    let (_dir, app) = app();
    let all = app
        .search_compendium(&CompendiumQuery {
            limit: 500,
            ..query("fire")
        })
        .expect("search");
    assert!(all.total > 10);
    let rank = |name: &str| {
        let n = name.to_lowercase();
        if n == "fire" {
            0
        } else if n.starts_with("fire") {
            1
        } else if n
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.starts_with("fire"))
        {
            2
        } else if n.contains("fire") {
            3
        } else {
            4
        }
    };
    let ranks: Vec<u8> = all.items.iter().map(|i| rank(&i.name)).collect();
    assert!(
        ranks.windows(2).all(|w| w[0] <= w[1]),
        "ranks must not decrease: {ranks:?}"
    );
    assert!(
        ranks[0] <= 1,
        "names starting with the text lead: {ranks:?}"
    );
    assert!(
        ranks.contains(&4),
        "description-only matches are included last"
    );

    let first = app
        .search_compendium(&CompendiumQuery {
            limit: 5,
            ..query("fire")
        })
        .expect("p1");
    let second = app
        .search_compendium(&CompendiumQuery {
            limit: 5,
            offset: 5,
            ..query("fire")
        })
        .expect("p2");
    assert_eq!((first.total, second.total), (all.total, all.total));
    let ids = |p: &borealis_lib::dto::Page<borealis_lib::dto::ElementSummary>| {
        p.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>()
    };
    let joined: Vec<String> = ids(&first).into_iter().chain(ids(&second)).collect();
    assert_eq!(
        joined,
        ids(&all)[..10].to_vec(),
        "pages are slices of one stable order"
    );
    let beyond = app
        .search_compendium(&CompendiumQuery {
            offset: 100_000,
            ..query("fire")
        })
        .expect("beyond");
    assert!(beyond.items.is_empty() && beyond.total == all.total);
}

#[test]
fn compendium_filters_by_type_source_and_ruleset() {
    let (_dir, app) = app();
    let spells = app
        .search_compendium(&CompendiumQuery {
            types: Some(vec!["spell".into()]),
            limit: 500,
            ..query("")
        })
        .expect("spells");
    assert!(spells.total >= 300 && spells.items.iter().all(|i| i.ty == "spell"));
    assert!(spells
        .items
        .iter()
        .all(|i| i.level.is_some() && i.school.is_some()));
    let y2024 = app
        .search_compendium(&CompendiumQuery {
            types: Some(vec!["spell".into()]),
            ruleset: Some("dnd2024".into()),
            limit: 500,
            ..query("")
        })
        .expect("2024");
    assert!(y2024
        .items
        .iter()
        .all(|i| i.rulesets.contains(&Ruleset::Dnd2024)));
    assert!(y2024.total < spells.total);
    let srd = app
        .search_compendium(&CompendiumQuery {
            sources: Some(vec!["SRD 5.1".into()]),
            limit: 500,
            ..query("")
        })
        .expect("source");
    assert!(srd.items.iter().all(|i| i.source == "SRD 5.1"));
    let plain = app.search_compendium(&query("")).expect("everything");
    assert!(plain
        .items
        .iter()
        .all(|i| !["multiclass", "asi", "list", "source"].contains(&i.ty.as_str())));
    assert_eq!(
        code(app.search_compendium(&CompendiumQuery {
            ruleset: Some("5e".into()),
            ..query("")
        })),
        ErrorCode::InvalidArgument
    );
    let facets = app.compendium_facets().expect("facets");
    let spell_facet = facets
        .types
        .iter()
        .find(|f| f.id == "spell")
        .expect("spell facet");
    assert_eq!(
        (spell_facet.label.as_str(), spell_facet.count),
        ("Spells", spells.total)
    );
    assert!(facets
        .sources
        .iter()
        .any(|f| f.id == "SRD 5.1" && f.count > 0));
}

#[test]
fn element_detail_is_sanitised_and_carries_facts_and_grants() {
    let (_dir, app) = app();
    let fireball = app.get_element("ID_PHB_SPELL_FIREBALL").expect("fireball");
    assert_eq!(fireball.summary.name, "Fireball");
    assert!(fireball.description_html.contains("<p>"));
    assert!(!fireball.description_html.contains("<script"));
    let fact = |label: &str| {
        fireball
            .facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| f.value.clone())
    };
    assert_eq!(fact("Level").as_deref(), Some("3"));
    assert_eq!(fact("School").as_deref(), Some("Evocation"));
    assert!(fact("Components").is_some_and(|c| c.starts_with("V, S, M")));
    assert!(fact("Range").is_some());
    let cleric = app.get_element(CLERIC).expect("cleric");
    assert!(!cleric.grants.is_empty(), "class grants are listed");
    let sword = app.get_element(LONGSWORD).expect("longsword");
    assert!(sword
        .facts
        .iter()
        .any(|f| f.label == "Damage" && f.value.contains("slashing")));
    assert_eq!(code(app.get_element("ID_NOPE")), ErrorCode::NotFound);
}

#[test]
fn explain_stat_returns_the_engine_trace() {
    let (_dir, app) = app();
    let (id, snap) = hero(&app, FIGHTER, 3);
    let trace = app.explain_stat(&id, "ac").expect("trace");
    assert_eq!(trace.stat, "ac");
    assert_eq!(trace.value, snap.stats.armor_class);
    assert!(!trace.steps.is_empty());
    let json = serde_json::to_value(&trace).expect("json");
    assert!(
        json["steps"][0].get("ruleKind").is_some(),
        "camelCase fields"
    );
    assert_eq!(
        code(app.explain_stat(&id, "  ")),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(app.explain_stat("c_missing", "ac")),
        ErrorCode::NotFound
    );
}
