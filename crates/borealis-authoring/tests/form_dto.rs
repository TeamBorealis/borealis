//! The form travels to the UI as `HomebrewForm` and the kinds as `HomebrewKindDescriptor`s. This test writes a sample
//! form (every field value shape of the shared parts, setter rows, blocks read by the block catalog, preserved parts)
//! and the descriptors of three kinds to `ui/src/api/__fixtures__/homebrew-form.json`; the UI test
//! `homebrew-form.test.ts` holds the sample form as a literal of the TS types and checks it, and the descriptors'
//! shape, against that file.
//!
//! The sample's blocks hold one value of each block-field shape the UI edits (WB-04/WB-05 join): a reference, the
//! gate fields (`level`, `requirements` as a condition), a tag filter, written-list rows, an equipped test, a variant
//! with a formula, a map, a list of formulas, a toggle with nested effect blocks, kept parts and a rule kept whole.
//!
//! `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites the file.

use borealis_authoring::expr_build::condition::ConditionContext;
use borealis_authoring::form::{
    element_to_form, kind, to_ui, CatalogBlockCodec, FormContext, RefInfo, APPEND_KIND,
};
use borealis_content::Element;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

fn sample() -> Element {
    serde_json::from_value(json!({
        "id": "ID_GL_COMPANION_WOLF", "name": "Wolf", "type": "Companion", "source": "Gloom",
        "ruleset": "dnd2024", "supports": ["Beast"], "prerequisite": "Level 3", "requirements": "[level:3]",
        "description": "<p>A <b>wolf</b>.</p>",
        "sheet": {"usage": "1/Long Rest", "descriptions": [{"text": "Bite {{level}}", "level": 2}]},
        "compendium": {"display": false},
        "setters": [{"name": "short", "value": "A wolf."}, {"name": "keywords", "value": "beast, pet"},
                    {"name": "speed", "value": "40", "attributes": {"unit": "ft"}},
                    {"name": "allow duplicate", "value": "true"}],
        "rules": [
            {"kind": "stat", "name": "speed", "value": "40", "minimum": "1", "equipped": "[armor:none]"},
            {"kind": "grant", "id": "ID_GL_TRAIT_PACK_TACTICS", "type": "Racial Trait", "level": 3,
             "requirements": "[level:3]"},
            {"kind": "select", "type": "Language", "name": "Language (Wolf)", "supports": "Standard||Exotic"},
            {"kind": "select", "type": "List", "name": "Coat (Wolf)",
             "items": [{"id": 1, "text": "Grey"}, {"id": 2, "text": "White"}]},
            {"kind": "resource", "id": "howl", "name": "Howl",
             "max": {"type": "formula", "expr": "max(1, wisdom:modifier)"}, "regain": {"short_rest": 1}},
            {"kind": "damage_reduction", "dice": "1d6", "plus": ["dexterity:modifier", "monk level"],
             "damage_types": ["bludgeoning"]},
            {"kind": "toggle", "id": "frenzy", "name": "Frenzy", "activation": "bonus_action",
             "effects": [{"kind": "roll_advantage", "roll": "ability_check", "ability": "strength"},
                         {"kind": "stat", "name": "speed", "value": "10"}]},
            {"kind": "unknown", "tag": "howl", "attributes": {"loud": "yes"}}
        ],
        "maps_to": {"dnd2014": "ID_WOLF", "kind": "approximate"},
        "data": {"pack_size": 3}
    }))
    .expect("element")
}

fn samples() -> Value {
    let codec = CatalogBlockCodec;
    let lookup: HashMap<String, RefInfo> = HashMap::from([(
        "ID_GL_TRAIT_PACK_TACTICS".to_string(),
        RefInfo {
            name: "Pack Tactics".into(),
            ty: "Racial Trait".into(),
        },
    )]);
    let cx = FormContext {
        conditions: ConditionContext::new(["Monk"]),
        lookup: &lookup,
        blocks: &codec,
    };
    let form = to_ui(&element_to_form(&sample(), &cx));
    let kinds: Vec<_> = ["language", "other", APPEND_KIND]
        .iter()
        .map(|id| kind(id).expect("kind").descriptor())
        .collect();
    json!({ "form": form, "kinds": kinds })
}

#[test]
fn ui_fixture_holds_the_form_shapes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/api/__fixtures__/homebrew-form.json");
    let text = serde_json::to_string_pretty(&samples()).expect("json") + "\n";
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "the UI fixture is stale: run with BOREALIS_UPDATE_SNAPSHOTS=1 and update the UI test's literal"
    );
}
