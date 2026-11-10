//! The builder states travel to the UI inside `HomebrewForm.fields` and block fields. This test writes one sample of
//! every variant as JSON to `ui/src/api/__fixtures__/homebrew-expr-builders.json`, and the UI test
//! `homebrew-expr-builders.test.ts` checks the same samples, written as literals of the TS types in
//! `ui/src/api/types/homebrew.ts`, equal that file: the Rust serialization and the TS types agree.
//!
//! `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites the file.

use borealis_authoring::expr_build::condition::{
    ConditionContext, ConditionLanguage, ConditionRow,
};
use borealis_authoring::expr_build::equipped::EquippedLanguage;
use borealis_authoring::expr_build::formula::{FormulaDialect, FormulaLanguage};
use borealis_authoring::expr_build::tag_filter::{TagFilter, TagFilterLanguage};
use borealis_authoring::expr_build::{ExprLanguage, ExprValue, GroupMode, Node};
use serde_json::{json, Value};
use std::path::Path;

fn samples() -> Value {
    let cx = ConditionContext::new(["Warlock"]);
    let condition = ExprValue::read::<ConditionLanguage>(
        "ID_AA_BB&&!ID_CC_DD&&[str:13]&&[character:4]&&[level:3]&&[warlock:2]&&[feature:Rage]&&[spellcasting:1]\
         &&[type:Spell]&&[innate speed:fly:1]&&([ranger:archetype:hunter]||!([wis:13]||[int:13]))",
        &cx,
    );
    let bad = Node::group(
        GroupMode::All,
        vec![Node::row(ConditionRow::Feature {
            name: String::new(),
        })],
    );
    json!({
        "condition": condition,
        "conditionKept": ExprValue::read::<ConditionLanguage>("((", &cx),
        "tagFilter": ExprValue::read::<TagFilterLanguage>(
            "$(spellcasting:list),(1||2)||$(spellcasting:slots),$(param:Spell List)||Exotic||ID_AA_BB||$(custom)",
            &(),
        ),
        "tagFilterSpecific": ExprValue::new(TagFilter::elements(&["ID_AA_BB", "ID_CC_DD"])),
        "equipped": ExprValue::read::<EquippedLanguage>(
            "[armor:light]&&[armor:medium]&&[armor:heavy]&&[armor:none]&&[shield:any]&&[main:weapon]\
             &&[main:versatile]&&[offhand:id_x_dagger]&&[attunement:id_x_ring]&&[item:id_x_cloak]&&[pet:any]",
            &(),
        ),
        "formula": ExprValue::read::<FormulaLanguage>(
            "max(1, wisdom:modifier) + ceil(cleric level / 2) - floor(proficiency) * min(3, 4) / 2",
            &FormulaDialect::Formula,
        ),
        "saveDc": ExprValue::read::<FormulaLanguage>("spell save DC (Cleric)", &FormulaDialect::SaveDc),
        "value": ExprValue::read::<FormulaLanguage>("proficiency:half", &FormulaDialect::Value),
        "problems": ConditionLanguage::check(&bad, &cx),
        "dialects": FormulaDialect::ALL,
    })
}

#[test]
fn ui_fixture_holds_every_builder_shape() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/api/__fixtures__/homebrew-expr-builders.json");
    let text = serde_json::to_string_pretty(&samples()).expect("json") + "\n";
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "the UI fixture is stale: run with BOREALIS_UPDATE_SNAPSHOTS=1 and update the UI test's literals"
    );
}
