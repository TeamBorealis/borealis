//! Hm2aU: the feat form's display sentence, as the backend writes it, for the web mock to match. The web mock words a
//! feat draft's prerequisite in pickers and its preview without the backend (`ui/src/api/mock/homebrew-feat.ts`); so
//! that its wording cannot drift, this test writes new feat forms (as the UI sends them) with the sentence the real
//! codec writes for each to `ui/src/api/__fixtures__/homebrew-feat-prerequisites.json`, and the UI test
//! `homebrew-feat-prerequisite.test.ts` runs the mock over the same forms.
//!
//! `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites the file.

use borealis_authoring::expr_build::condition::{Condition, ConditionContext, ConditionLanguage};
use borealis_authoring::expr_build::{Built, ExprValue};
use borealis_authoring::form::{
    form_to_element, to_ui, CatalogBlockCodec, FormContext, FormState, RefInfo,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// The elements a condition may name, as the creator's lookup knows them (`Has` / `Lacks` rows say their names).
const NAMES: [(&str, &str, &str); 2] = [
    ("ID_X_FEAT_TOUGH", "Tough", "Feat"),
    ("ID_X_RACE_ELF", "Elf", "Race"),
];

/// `(case, requirements as the builder holds them, the text typed in "Prerequisite as shown")`.
const CASES: [(&str, &str, &str); 20] = [
    (
        "level and one of two abilities",
        "[character:4]&&([str:13]||[dex:13])",
        "",
    ),
    ("class level written as level", "[level:3]", ""),
    ("one ability", "[wis:13]", ""),
    (
        "abilities with different minimums",
        "[str:13]||[cha:15]",
        "",
    ),
    ("three abilities", "[int:13]||[wis:13]||[cha:13]", ""),
    ("a class level", "[wizard:5]", ""),
    ("a feature", "[feature:Fighting Style]", ""),
    ("spellcasting", "[spellcasting:1]", ""),
    ("spell slots", "[spellcasting:3]", ""),
    ("an element by id", "ID_X_FEAT_TOUGH", ""),
    ("without an element", "!ID_X_FEAT_TOUGH", ""),
    ("none of", "!([str:13]||[dex:13])", ""),
    (
        "all of with a group of other rows",
        "[character:4]&&([feature:Rage]||[wizard:2])",
        "",
    ),
    ("a statistic", "[innate speed:fly:1]", ""),
    ("a speed of at least", "[innate speed:climb:30]", ""),
    ("a statistic the picker names", "[darkvision:range:60]", ""),
    ("a statistic of the author's", "[my homebrew:counter:2]", ""),
    ("a test words can't say", "[type:Spell]", ""),
    ("an element and a level", "ID_X_RACE_ELF&&[character:8]", ""),
    (
        "the author's own words",
        "[character:4]",
        "4th level, sworn to the Order",
    ),
];

/// A new 2024 feat form with `requirements` as the condition builder holds it (a model, no stored text).
fn feat(requirements: &str, shown: &str, cx: &ConditionContext) -> FormState {
    let read = ExprValue::read::<ConditionLanguage>(requirements, cx);
    let Built::Model { model } = read.value else {
        panic!("{requirements} reads as a model");
    };
    let mut f = FormState::new("e", "feat");
    f.memo.id = Some("ID_X_FEAT_SAMPLE".into());
    for (k, v) in [
        ("name", json!("Sample")),
        ("category", json!("General")),
        ("requirements", json!(ExprValue::<Condition>::new(model))),
        ("prerequisiteText", json!(shown)),
    ] {
        f.fields.insert(k.into(), v);
    }
    f
}

fn cases() -> Value {
    let codec = CatalogBlockCodec;
    let lookup: HashMap<String, RefInfo> = NAMES
        .iter()
        .map(|(id, name, ty)| {
            (
                (*id).to_string(),
                RefInfo {
                    name: (*name).into(),
                    ty: (*ty).into(),
                },
            )
        })
        .collect();
    let cx = FormContext {
        conditions: ConditionContext::new(["Wizard"]),
        lookup: &lookup,
        blocks: &codec,
    };
    let rows: Vec<Value> = CASES
        .iter()
        .map(|(case, requirements, shown)| {
            let form = feat(requirements, shown, &cx.conditions);
            let element = form_to_element(&form, &cx).unwrap_or_else(|p| panic!("{case}: {p:?}"));
            assert_eq!(element.requirements, *requirements, "{case}: the condition");
            json!({
                "case": case,
                "form": to_ui(&form),
                "prerequisite": element.prerequisite,
            })
        })
        .collect();
    let names: BTreeMap<&str, &str> = NAMES.iter().map(|(id, name, _)| (*id, *name)).collect();
    json!({ "names": names, "cases": rows })
}

#[test]
fn the_mock_fixture_holds_the_backend_s_prerequisite_sentences() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/api/__fixtures__/homebrew-feat-prerequisites.json");
    let text = serde_json::to_string_pretty(&cases()).expect("json") + "\n";
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "ui/src/api/__fixtures__/homebrew-feat-prerequisites.json is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}
