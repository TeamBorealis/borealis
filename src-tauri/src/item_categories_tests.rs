use super::*;
use crate::content_dto::ImportPackRequest;
use crate::content_packs::NullSink;
use crate::dto::{AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch};
use crate::dto_ext::ItemFilter;
use crate::state::AppPaths;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_content::Setter;
use std::path::Path;

fn item(id: &str, ty: &str, category: Option<&str>, source: &str) -> Element {
    let mut el = Element::new(id, id, ty, source);
    if let Some(c) = category {
        el.setters = vec![Setter::new("category", c)];
    }
    el
}

/// The add-item rows of a listing: one per type family and normalized name (B-55).
pub(super) fn row_count(items: &[crate::dto_ext::ItemSummary]) -> usize {
    items
        .iter()
        .map(|i| group_key(&i.summary.ty, &i.summary.name))
        .collect::<HashSet<_>>()
        .len()
}

fn names(list: &[ItemCategory]) -> Vec<&str> {
    list.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn fixed_names_first_then_content_order_then_additional_ones() {
    let els = [
        item("a", "Item", Some("Additional Feat"), "S"),
        item("b", "Item", Some("Vehicles"), "S"),
        item("c", "Item", Some("Tools"), "S"),
        item("d", "Magic Item", Some("Artifacts"), "S"),
        item("e", "Item", Some("Additional Gizmo"), "S"),
        item("f", "Item", Some("Vehicles"), "S"),
    ];
    let order = ordered_names(els.iter());
    assert_eq!(&order[..22], FIXED_CATEGORIES.as_slice());
    assert_eq!(
        &order[22..],
        [
            "Vehicles",
            "Artifacts",
            "Additional Feat",
            "Additional Gizmo"
        ]
    );
}

#[test]
fn weapons_and_armor_sit_in_their_fixed_category_and_blank_items_fall_back() {
    assert_eq!(
        list_category(&item("w", "Weapon", Some("Firearms"), "S")),
        "Weapons"
    );
    assert_eq!(list_category(&item("r", "Armor", None, "S")), "Armor");
    assert_eq!(
        list_category(&item("g", "Item", None, "S")),
        "Adventuring Gear"
    );
    assert_eq!(
        list_category(&item("m", "Magic Item", Some("  "), "S")),
        "Wondrous Items"
    );
    assert_eq!(
        list_category(&item("p", "Magic Item", Some("Potions"), "S")),
        "Potions"
    );
}

#[test]
fn empty_and_fully_restricted_categories_are_hidden_and_counts_skip_restricted_items() {
    let els = [
        item("rope", "Item", Some("Adventuring Gear"), "Open"),
        item("gun", "Weapon", Some("Firearms"), "Open"),
        item("boat", "Item", Some("Vehicles"), "Locked"),
        item("cart", "Item", Some("Wagons"), "Locked"),
        item("cart2", "Item", Some("Wagons"), "Open"),
    ];
    let refs: Vec<&Element> = els.iter().collect();
    let scope = ItemScope {
        ruleset: None,
        restricted: vec!["Locked".into()],
        ..ItemScope::default()
    };
    let list = category_list(&refs, &scope, &[]);
    // Firearms holds no item (the gun sits in Weapons), Vehicles only restricted ones.
    assert_eq!(names(&list), ["Adventuring Gear", "Weapons", "Wagons"]);
    assert_eq!(list[2].count, 1);
    let open = category_list(&refs, &ItemScope::default(), &[]);
    assert_eq!(
        names(&open),
        ["Adventuring Gear", "Weapons", "Vehicles", "Wagons"]
    );
}

#[test]
fn ruleset_scope_hides_other_edition_items() {
    let mut new = item("n", "Item", Some("Modern Gear"), "S");
    new.ruleset = Some(Ruleset::Dnd2024);
    let refs = [&new];
    let scope = ItemScope {
        ruleset: Some(Ruleset::Dnd2014),
        restricted: Vec::new(),
        ..ItemScope::default()
    };
    assert!(category_list(&refs, &scope, &[]).is_empty());
}

#[test]
fn an_offered_other_edition_item_is_admitted_unless_its_source_is_restricted() {
    let mut new = item("n", "Item", Some("Modern Gear"), "S");
    new.ruleset = Some(Ruleset::Dnd2024);
    let offered = ItemScope::new(Ruleset::Dnd2014, Vec::new(), ["n".to_string()].into());
    assert!(offered.admits(&new));
    let restricted = ItemScope::new(Ruleset::Dnd2014, vec!["S".into()], ["n".to_string()].into());
    assert!(!restricted.admits(&new));
}

#[test]
fn same_name_items_count_as_one_row() {
    let mut a = item("a", "Item", Some("Lights"), "S");
    a.name = "Hooded Lantern".into();
    let mut b = item("b", "Item", Some("Lights"), "T");
    b.name = "hooded  lantern".into();
    let refs = [&a, &b];
    let list = category_list(&refs, &ItemScope::default(), &[]);
    assert_eq!(
        list.iter().find(|c| c.name == "Lights").map(|c| c.count),
        Some(1)
    );
}

#[test]
fn additional_class_spell_categories_need_that_spellcasting() {
    let wizard = vec!["Wizard".to_string()];
    assert!(spell_category_allowed("Additional Spell", &[]));
    assert!(spell_category_allowed("Additional Feat", &[]));
    assert!(!spell_category_allowed("Additional Wizard Spell", &[]));
    assert!(spell_category_allowed("Additional Wizard Spell", &wizard));
    assert!(!spell_category_allowed("Additional Cleric Spell", &wizard));
}

const PACK_SOURCE: &str = "UiCat Test";

fn app_with_pack() -> (tempfile::TempDir, AppState, String) {
    let el = |id: &str, name: &str, category: &str| {
        serde_json::json!({"id": id, "name": name, "type": "Item", "source": PACK_SOURCE, "ruleset": "dnd2024",
            "setters": [{"name": "category", "value": category}]})
    };
    app_with(vec![
        el(
            "ID_UICAT_ADD_WIZ",
            "Additional Wizard Spell, Test",
            "Additional Wizard Spell",
        ),
        el(
            "ID_UICAT_ADD_SPELL",
            "Additional Spell, Test",
            "Additional Spell",
        ),
        el("ID_UICAT_GIFT", "Test Gift", "Supernatural Gifts"),
        el("ID_UICAT_ODD", "Odd Thing", "Curiosities"),
    ])
}

/// An app with the bundled content plus `elements`, and a new 2024 character.
fn app_with(elements: Vec<serde_json::Value>) -> (tempfile::TempDir, AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(AppPaths {
        content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    })
    .unwrap();
    let pack = serde_json::json!({"format": "borealis-content/1", "elements": elements});
    let imported = app
        .import_pack(
            &ImportPackRequest {
                file_name: "uicat.json".into(),
                content_base64: STANDARD.encode(pack.to_string()),
                format: Some("json".into()),
            },
            &NullSink,
        )
        .unwrap();
    assert!(imported.pack.is_some(), "{:?}", imported.report);
    let request = NewCharacterRequest {
        name: "Lister".into(),
        ruleset: crate::dto::Ruleset::Dnd2024,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let id = app
        .create_character(crate::dto::Ruleset::Dnd2024, &request)
        .unwrap()
        .id;
    (dir, app, id)
}

fn slot_id(snap: &CharacterSnapshot, title: &str) -> String {
    snap.slots
        .iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("no slot {title}"))
        .id
        .clone()
}

#[test]
fn the_command_lists_imported_categories_and_gates_class_spell_ones() {
    let (_dir, app, id) = app_with_pack();
    let list = app.list_item_categories(&id).unwrap();
    let shown = names(&list);
    let fixed_end = shown
        .iter()
        .rposition(|n| FIXED_CATEGORIES.contains(n))
        .expect("a fixed category");
    let curio = shown
        .iter()
        .position(|n| *n == "Curiosities")
        .unwrap_or_else(|| panic!("imported category: {shown:?}"));
    assert!(shown.contains(&"Supernatural Gifts"), "{shown:?}");
    assert!(curio > fixed_end, "{shown:?}");
    // the content's own "Additional ..." categories come before the generated proxy ones
    let at = |n: &str| shown.iter().position(|s| *s == n);
    assert!(at("Additional Spell") < at("Additional Feat"), "{shown:?}");
    assert!(!shown.contains(&"Additional Wizard Spell"), "{shown:?}");
    // Every count is the number of rows the item search gives for that category: one per name (B-55).
    for c in &list {
        let page = app
            .search_items(&ItemFilter {
                category: Some(c.name.clone()),
                limit: Some(1000),
                character_id: Some(id.clone()),
                ..ItemFilter::default()
            })
            .unwrap();
        assert_eq!(page.items.len(), page.total, "{}", c.name);
        assert_eq!(row_count(&page.items), c.count, "{}", c.name);
    }

    let snap = app.get_snapshot(&id).unwrap();
    app.apply_selection(&id, &slot_id(&snap, "Class"), "ID_2024_CLASS_WIZARD")
        .expect("class");
    let caster = app.list_item_categories(&id).unwrap();
    let shown = names(&caster);
    let at = |n: &str| shown.iter().position(|s| *s == n);
    assert_eq!(
        at("Additional Wizard Spell").map(|i| i + 1),
        at("Additional Spell"),
        "{shown:?}"
    );
}

#[path = "proxy_items_tests.rs"]
mod proxy_items;
