//! A new inventory line of an `inventory-hidden` item starts with include-in-inventory, card and
//! sidebar off, as in Aurora (`specs/proxy-items.md` 1.4, P1).

#[path = "common/proxy_fixture.rs"]
mod proxy_fixture;

use borealis_engine::commands;
use proxy_fixture::{engine, hero};

const FEAT_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_LOOKOUT";

#[test]
fn a_new_line_of_a_hidden_item_starts_out_of_the_inventory_and_without_a_card() {
    let e = engine();
    for item in ["ID_TB_ITEM_VEIL", FEAT_PROXY] {
        let s = commands::add_item(e, &hero(), item, 1).unwrap().state;
        let line = s.inventory.items.last().unwrap();
        assert!(line.hidden, "{item}: include in inventory off");
        assert!(!line.card, "{item}: card off");
        assert!(!line.sidebar, "{item}: sidebar off");
    }
    let s = commands::add_item(e, &hero(), "ID_TB_ITEM_ROPE", 1)
        .unwrap()
        .state;
    let line = s.inventory.items.last().unwrap();
    assert!(
        !line.hidden && line.card,
        "an ordinary item keeps the defaults"
    );
}

#[test]
fn an_adorned_line_takes_the_hidden_defaults_of_its_adorner() {
    let e = engine();
    let s = commands::add_adorned_item(e, &hero(), "ID_TB_ITEM_ROPE", "ID_TB_ITEM_VEIL", 1)
        .unwrap()
        .state;
    let line = s.inventory.items.last().unwrap();
    assert_eq!(line.adorner.as_deref(), Some("ID_TB_ITEM_VEIL"));
    assert!(line.hidden && !line.card && !line.sidebar);
    let s = commands::add_adorned_item(e, &hero(), "ID_TB_ITEM_VEIL", "ID_TB_ITEM_ROPE", 1)
        .unwrap()
        .state;
    let line = s.inventory.items.last().unwrap();
    assert!(!line.hidden && line.card, "only the adorner decides");
}
