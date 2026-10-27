//! The gear list leaves out inventory lines with "include in inventory" off, as Aurora's
//! equipment page does (`specs/proxy-items.md` 1.5, IM13 D3, P7): an equipped proxy item
//! ("Additional Feat, X") grants its feat but prints no gear row.

mod common;

use borealis_engine::snapshot::{ElementRef, ItemSnapshot};
use borealis_engine::state::InventoryItem;
use borealis_sheet::{render_html_with, SheetContent, SheetOptions};

const HIDDEN: &str = "Additional Feat, Test Hidden Line";
const SHOWN: &str = "Test Visible Trinket";

#[test]
fn hidden_lines_are_left_out_of_the_gear_list() {
    let (mut c, mut snap) = common::wizard_2014();
    for (uid, name, hidden) in [("t-hidden", HIDDEN, true), ("t-shown", SHOWN, false)] {
        snap.inventory.items.push(ItemSnapshot {
            uid: uid.into(),
            item: ElementRef {
                id: "ID_TEST_ITEM".into(),
                name: name.into(),
                ty: "Item".into(),
                source: "Test".into(),
            },
            name: name.into(),
            amount: 1,
            equipped: None,
            attuned: false,
            requires_attunement: false,
            attunement_restriction: None,
            attunement_barred: false,
            proficient_by_item: false,
            equippable: false,
            weight: 0.0,
            cost_gp: 0.0,
            storage: None,
            show_card: false,
        });
        let mut item = InventoryItem::new(uid, "ID_TEST_ITEM");
        item.hidden = hidden;
        c.build.inventory.items.push(item);
    }
    let html = render_html_with(
        &c,
        &snap,
        &SheetOptions::default(),
        &SheetContent::default(),
    )
    .expect("renders");
    assert!(html.contains(SHOWN), "a visible line is listed");
    assert!(!html.contains(HIDDEN), "a hidden line is not");
}
