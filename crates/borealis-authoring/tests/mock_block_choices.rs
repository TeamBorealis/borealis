//! I2iGc: the web mock checks block values against their closed choices as the backend does (item I0Yxk). The
//! backend's check is `check::fields` (`Finding::BlockChoice`, code `block.choice`; a map key outside its closed combo
//! is `block.shape`) over `blocks::fit`. This test writes block cases (closed combos and chips, open fields, skill
//! fields with and without a loaded homebrew skill, verbatim chips, an unknown variant, group parts, a map key, toggle
//! effects) and the `block.choice` / `block.shape` problems the backend's live check finds for each to
//! `ui/src/api/__fixtures__/homebrew-block-choices.json`; the UI test `homebrew-block-choices.test.ts` runs the mock's
//! check over the same forms, with the same loaded skills.
//!
//! `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites the file.

use borealis_authoring::check::{live_problems, LiveInput};
use borealis_authoring::form::{skill_keys, to_ui, CatalogBlockCodec, FormContext, FormState};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStore, ContentStoreBuilder, Element, Setter};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

const CODES: [&str; 2] = ["block.choice", "block.shape"];

fn block(kind: &str, fields: Value) -> Value {
    json!({"key": format!("b-{kind}"), "kind": kind, "sentence": "", "fields": fields})
}

fn spell() -> Value {
    json!({"id": "ID_X", "name": "X", "type": "Spell"})
}

/// `(case, the homebrew skills loaded, the block)`.
fn cases() -> Vec<(&'static str, Vec<&'static str>, Value)> {
    let rage_effect = json!({"key": "e0", "kind": "roll_advantage", "sentence": "",
        "fields": {"roll": "ability_check", "skill": "Alchemy"}});
    let rage = block(
        "toggle",
        json!({"id": "rage", "name": "Rage", "effects": [rage_effect]}),
    );
    vec![
        (
            "a closed combo value that is not a choice",
            vec![],
            block(
                "grant_spell",
                json!({"spell": spell(), "mode": "sometimes"}),
            ),
        ),
        (
            "a closed combo's choice",
            vec![],
            block("grant_spell", json!({"spell": spell(), "mode": "known"})),
        ),
        (
            "a chips value that is not a choice",
            vec![],
            block("pact_weapon", json!({"damage_types": ["fire", "plasma"]})),
        ),
        (
            "a closed action that is not a choice",
            vec![],
            block(
                "healing",
                json!({"name": "Mend", "dice": "1d8", "action": "minute"}),
            ),
        ),
        (
            "a number in a combo",
            vec![],
            block("sense", json!({"sense": 60})),
        ),
        (
            "any text in an open combo",
            vec![],
            block("sense", json!({"sense": "echolocation", "range": 30})),
        ),
        (
            "a regained-on value that is not a choice",
            vec![],
            block(
                "resource",
                json!({"id": "Embers", "max": {"type": "fixed", "value": 2}, "recharge": "long_rest"}),
            ),
        ),
        (
            "a single choice where a verbatim reader writes a list",
            vec![],
            block("pact_weapon", json!({"damage_types": "fire"})),
        ),
        (
            "a single value that is not a choice where a verbatim reader writes a list",
            vec![],
            block("pact_weapon", json!({"damage_types": "plasma"})),
        ),
        (
            "a variant of an unknown kind",
            vec![],
            block(
                "resource",
                json!({"id": "Embers", "max": {"type": "dice", "value": "1d4"}}),
            ),
        ),
        (
            "a variant of a known kind",
            vec![],
            block(
                "resource",
                json!({"id": "Embers", "max": {"type": "fixed", "value": 2}}),
            ),
        ),
        (
            "a group part that is not a choice",
            vec![],
            block(
                "grant_spell",
                json!({"spell": spell(), "free_cast": {"uses": 1, "recharge": "weekly"}}),
            ),
        ),
        (
            "a map key that is a choice",
            vec![],
            block(
                "resource",
                json!({"id": "Embers", "max": {"type": "fixed", "value": 2}, "regain": {"long_rest": 1}}),
            ),
        ),
        (
            "a map key outside its closed choices",
            vec![],
            block(
                "resource",
                json!({"id": "Embers", "max": {"type": "fixed", "value": 2}, "regain": {"dawn": 1}}),
            ),
        ),
        (
            "a toggle's effect naming a skill no content adds",
            vec![],
            rage.clone(),
        ),
        (
            "a toggle's effect naming a loaded homebrew skill",
            vec!["Alchemy"],
            rage,
        ),
        (
            "a skill list with a skill no content adds",
            vec![],
            block("skill_bonus", json!({"skills": ["Arcana", "Alchemy"]})),
        ),
        (
            "a roll's skill no content adds",
            vec![],
            block(
                "roll_advantage",
                json!({"roll": "ability_check", "skill": "Alchemy"}),
            ),
        ),
        (
            "a skill list with a loaded homebrew skill",
            vec!["Alchemy"],
            block("skill_bonus", json!({"skills": ["Arcana", "Alchemy"]})),
        ),
        (
            "a loaded homebrew skill in another case",
            vec!["Alchemy"],
            block(
                "roll_advantage",
                json!({"roll": "ability_check", "skill": "alchemy"}),
            ),
        ),
        (
            "a loaded homebrew skill with other spacing",
            vec!["Sea Lore"],
            block(
                "roll_advantage",
                json!({"roll": "ability_check", "skill": "Sea  Lore"}),
            ),
        ),
        (
            "one of the eighteen in another case",
            vec!["Alchemy"],
            block("skill_bonus", json!({"skills": ["arcana"]})),
        ),
    ]
}

/// The bundled-free store a case's check reads: only the homebrew skills it loads.
fn store_with(skills: &[&str]) -> ContentStore {
    let elements: Vec<Element> = skills
        .iter()
        .map(|name| {
            let id = format!("ID_HB_SKILL_{}", name.to_uppercase().replace(' ', "_"));
            let mut el = Element::new(id, (*name).to_string(), "Skill", "Old World");
            el.setters.push(Setter::new("ability", "Intelligence"));
            el
        })
        .collect();
    ContentStoreBuilder::new()
        .add_file(
            "old-world/content.json",
            PackOrigin::Custom,
            ContentFile::new(elements),
        )
        .build()
}

fn fixture() -> Value {
    let codec = CatalogBlockCodec;
    let hidden = HashSet::new();
    let rows: Vec<Value> = cases()
        .into_iter()
        .map(|(case, skills, b)| {
            let store = store_with(&skills);
            let cx = FormContext::from_store(&store, &codec);
            let form: FormState = serde_json::from_value(json!({
                "kind": "other", "fields": {"name": "Howl"}, "blocks": [b], "children": [], "preserved": []
            }))
            .expect("a form");
            let problems: Vec<Value> = live_problems(&LiveInput {
                form: &form,
                cx: &cx,
                store: &store,
                file: "old-world/content.json",
                hidden: &hidden,
                pack: &[],
            })
            .into_iter()
            .filter(|p| CODES.contains(&p.code.as_str()))
            .map(|p| json!({"code": p.code, "path": p.path, "message": p.message}))
            .collect();
            json!({
                "case": case,
                "skills": skill_keys(&store),
                "form": to_ui(&form),
                "problems": problems,
            })
        })
        .collect();
    json!({ "cases": rows })
}

#[test]
fn the_mock_fixture_holds_the_backend_s_block_choice_problems() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/api/__fixtures__/homebrew-block-choices.json");
    let text = serde_json::to_string_pretty(&fixture()).expect("json") + "\n";
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "ui/src/api/__fixtures__/homebrew-block-choices.json is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}

/// The fixture is no echo of an empty check: the backend finds the problems the cases are about.
#[test]
fn the_backend_finds_choice_and_shape_problems_in_the_cases() {
    let fixture = fixture();
    let rows = fixture["cases"].as_array().expect("cases");
    let with = |code: &str| {
        rows.iter()
            .filter(|r| {
                r["problems"]
                    .as_array()
                    .is_some_and(|ps| ps.iter().any(|p| p["code"] == code))
            })
            .count()
    };
    assert!(with("block.choice") >= 10, "{}", with("block.choice"));
    assert_eq!(with("block.shape"), 1);
    assert!(rows.iter().any(|r| r["problems"] == json!([])));
}
