//! Aurora's proxy items in the equipment screen (`specs/proxy-items.md` P3, P4): the generated
//! "Additional ..." categories, their rows, and adding / equipping one.

use super::*;
use crate::dto::Page;
use crate::dto_ext::ItemSummary;
use borealis_engine::proxy::is_proxy_id;
use borealis_engine::snapshot::CharacterSnapshot as EngineSnapshot;
use borealis_engine::state::InventoryItem;

const WIZARD: &str = "ID_2024_CLASS_WIZARD";
const FEAT_ID: &str = "ID_UICAT_FEAT_WATCHFUL";
/// The target types of the generated categories a wizard sees, in Aurora's order.
const GENERATED: [&str; 6] = [
    "Feat",
    "Language",
    "Proficiency",
    "Ability Score Improvement",
    "Spell",
    "Wizard Spell",
];

fn pack() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id": "ID_UICAT_COMPANION", "name": "Companion Feature", "type": "Item",
            "source": PACK_SOURCE, "ruleset": "dnd2024",
            "setters": [{"name": "category", "value": "Additional Feature"}]}),
        serde_json::json!({"id": FEAT_ID, "name": "Watchful", "type": "Feat",
            "source": PACK_SOURCE, "ruleset": "dnd2024",
            "description": "<p>Watchful test feat.</p>"}),
    ]
}

/// A 2024 wizard with the test pack installed.
fn wizard() -> (tempfile::TempDir, AppState, String) {
    let (dir, app, id) = app_with(pack());
    let snap = app.get_snapshot(&id).unwrap();
    app.apply_selection(&id, &slot_id(&snap, "Class"), WIZARD)
        .expect("class");
    (dir, app, id)
}

fn search(
    app: &AppState,
    id: &str,
    category: Option<&str>,
    query: Option<&str>,
) -> Page<ItemSummary> {
    app.search_items(&ItemFilter {
        category: category.map(str::to_string),
        query: query.map(str::to_string),
        limit: Some(1000),
        character_id: Some(id.to_string()),
        ..ItemFilter::default()
    })
    .unwrap()
}

fn count(list: &[ItemCategory], name: &str) -> usize {
    list.iter().find(|c| c.name == name).map_or(0, |c| c.count)
}

#[test]
fn a_wizard_sees_the_generated_categories_after_the_content_ones() {
    let (_dir, app, id) = wizard();
    let list = app.list_item_categories(&id).unwrap();
    let shown = names(&list);
    let feature = shown
        .iter()
        .position(|n| *n == "Additional Feature")
        .unwrap_or_else(|| panic!("{shown:?}"));
    assert_eq!(
        shown[feature + 1..],
        GENERATED.map(|t| format!("Additional {t}")),
        "{shown:?}"
    );
    assert!(!shown.contains(&"Additional Cleric Spell"), "{shown:?}");
    for c in &list {
        let page = search(&app, &id, Some(&c.name), None);
        assert_eq!(page.total, c.count, "{}", c.name);
    }
    // the rows of a generated category are its proxies, by name
    let feats = search(&app, &id, Some("Additional Feat"), None);
    assert!(feats.items.iter().all(|i| is_proxy_id(&i.summary.id)));
    assert!(feats
        .items
        .windows(2)
        .all(|w| w[0].summary.name <= w[1].summary.name));
    assert!(feats
        .items
        .iter()
        .any(|i| i.summary.name == "Additional Feat, Watchful"));
}

#[test]
fn restricting_a_source_lowers_the_counts() {
    let (_dir, app, id) = wizard();
    let before = count(&app.list_item_categories(&id).unwrap(), "Additional Feat");
    let enabled: Vec<String> = app
        .list_sources()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .filter(|s| s != PACK_SOURCE)
        .collect();
    app.set_source_restrictions(&id, Some(&enabled)).unwrap();
    let list = app.list_item_categories(&id).unwrap();
    assert_eq!(count(&list, "Additional Feat"), before - 1);
    assert_eq!(
        search(&app, &id, Some("Additional Feat"), None).total,
        before - 1
    );
}

#[test]
fn a_search_without_a_category_lists_no_proxies() {
    let (_dir, app, id) = wizard();
    let page = search(&app, &id, None, Some("additional"));
    assert!(page.items.iter().all(|i| !is_proxy_id(&i.summary.id)));
    let page = search(&app, &id, None, Some("watchful"));
    assert_eq!(page.total, 0);
}

#[test]
fn a_search_inside_a_category_matches_every_word_in_the_name_or_a_keyword() {
    let (_dir, app, id) = wizard();
    let cat = Some("Additional Feat");
    let names_of = |q: &str| -> Vec<String> {
        search(&app, &id, cat, Some(q))
            .items
            .into_iter()
            .map(|i| i.summary.name)
            .collect()
    };
    assert_eq!(names_of("watch feat"), ["Additional Feat, Watchful"]);
    assert_eq!(names_of("WATCHFUL"), ["Additional Feat, Watchful"]);
    assert!(names_of("watchful nothing").is_empty());
    let either = names_of("watchful+no such feat at all");
    assert_eq!(either, ["Additional Feat, Watchful"]);
}

/// The proxy row of `category` named `name`.
fn row(app: &AppState, id: &str, category: &str, name: &str) -> String {
    let page = search(app, id, Some(category), None);
    page.items
        .into_iter()
        .find(|i| i.summary.name == name)
        .unwrap_or_else(|| panic!("no row {name} in {category}"))
        .summary
        .id
}

fn lines(app: &AppState, id: &str) -> Vec<InventoryItem> {
    app.read(id, |_, s| Ok(s.character.build.inventory.items.clone()))
        .unwrap()
}

fn derived(app: &AppState, id: &str) -> EngineSnapshot {
    app.read(id, |ctx, s| Ok(ctx.hub.engine.derive(&s.character.build)))
        .unwrap()
}

fn has(snap: &EngineSnapshot, element: &str) -> bool {
    snap.elements.iter().any(|r| r.element.id == element)
}

/// A spell of the wizard's own list, as named in the catalogue.
fn wizard_spell(app: &AppState, id: &str) -> (String, String) {
    let page = search(app, id, Some("Additional Wizard Spell"), None);
    let first = page.items.first().expect("a wizard spell proxy");
    let name = first
        .summary
        .name
        .replacen("Additional Wizard Spell, ", "", 1);
    let spell = app
        .read(id, |ctx, s| {
            Ok(ctx
                .hub
                .engine
                .universe(s.character.ruleset())
                .get(&first.summary.id)
                .map(|p| match &p.el.rules[0] {
                    borealis_content::Rule::Grant(g) => g.id.clone(),
                    _ => String::new(),
                }))
        })
        .unwrap()
        .expect("proxy resolves");
    (name, spell)
}

#[test]
fn an_added_feat_proxy_grants_the_feat_only_while_equipped() {
    let (_dir, app, id) = wizard();
    let proxy = row(&app, &id, "Additional Feat", "Additional Feat, Watchful");
    let before = lines(&app, &id).len();
    app.add_item(&id, &proxy, 1, true, None)
        .expect("buy costs 0");
    app.add_item(&id, &proxy, 1, false, None)
        .expect("add again");
    let added = lines(&app, &id);
    assert_eq!(added.len(), before + 2, "a proxy never stacks");
    let line = added.last().unwrap();
    assert!(line.hidden && !line.card, "hidden-item defaults");
    assert!(!has(&derived(&app, &id), FEAT_ID), "nothing until equipped");
    app.equip_item(&id, &line.uid, crate::dto_ext::EquipRequest::Auto)
        .expect("equip");
    assert!(has(&derived(&app, &id), FEAT_ID));
    app.unequip_item(&id, &line.uid).expect("unequip");
    assert!(!has(&derived(&app, &id), FEAT_ID));
}

#[test]
fn spell_proxies_join_their_class_block_or_none() {
    let (_dir, app, id) = wizard();
    let (name, spell) = wizard_spell(&app, &id);
    for category in ["Additional Wizard Spell", "Additional Spell"] {
        let proxy = row(&app, &id, category, &format!("{category}, {name}"));
        app.add_item(&id, &proxy, 1, false, None).unwrap();
        let uid = lines(&app, &id).last().unwrap().uid.clone();
        app.equip_item(&id, &uid, crate::dto_ext::EquipRequest::Auto)
            .unwrap();
    }
    let snap = derived(&app, &id);
    assert!(has(&snap, &spell));
    let in_block: Vec<(&str, bool)> = snap
        .spellcasting
        .iter()
        .flat_map(|b| {
            b.spells
                .iter()
                .filter(|s| s.id == spell)
                .map(move |s| (b.name.as_str(), s.always_prepared))
        })
        .collect();
    assert_eq!(
        in_block,
        [("Wizard", false)],
        "only the class proxy adds it to a block"
    );

    app.save_character(&id).unwrap();
    app.close_character(&id, false).unwrap();
    app.load_character(&id).unwrap();
    let again = derived(&app, &id);
    let ids = |s: &EngineSnapshot| {
        let mut v: Vec<String> = s.elements.iter().map(|r| r.element.id.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(
        ids(&again),
        ids(&snap),
        "reload keeps what the proxies grant"
    );
}

#[test]
fn the_detail_of_a_proxy_shows_its_targets_description() {
    let (_dir, app, id) = wizard();
    let proxy = row(&app, &id, "Additional Feat", "Additional Feat, Watchful");
    let detail = app.get_element(&proxy).unwrap();
    assert!(
        detail.description_html.contains("Watchful test feat."),
        "{}",
        detail.description_html
    );
}
