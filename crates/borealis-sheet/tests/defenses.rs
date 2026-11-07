//! The printed sheet's Defenses list (issue #26): a 2014 Dragonborn shows only the resistance of
//! the chosen Draconic Ancestry, never every ancestry's resistance.

mod common;

use borealis_sheet::{render_html, sheet_data, SheetOptions};

#[test]
fn dragonborn_sheet_lists_only_the_chosen_ancestry_resistance() {
    let (c, snap) = common::dragonborn_2014("ID_RACIAL_TRAIT_DRACONIC_ANCESTRY_SILVER");
    let data = sheet_data(&c, &snap, &Default::default());
    assert_eq!(data.extras.resistances, ["Cold"]);
    let html = render_html(&c, &snap, &SheetOptions::default()).expect("html sheet");
    assert!(
        html.contains("Resistances: Cold") && !html.contains("Resistances: Cold,"),
        "the defenses line lists cold only"
    );
}
