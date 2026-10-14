//! UI <-> Rust contract: command registration, argument names, and the JSON shape of real
//! responses and events against `ui/src/api/generated/contract.json` (generated from
//! `ui/src/api/contract.ts` by `npm run gen:contract`).

mod contract_support;
use contract_support::{rust_commands, Contract};
use std::collections::BTreeSet;

#[test]
fn every_ui_command_is_registered_and_vice_versa() {
    let c = Contract::load();
    let ui: BTreeSet<String> = c.command_names().into_iter().collect();
    let rust: BTreeSet<String> = rust_commands().into_iter().collect();
    let ui_only: Vec<_> = ui.difference(&rust).collect();
    let rust_only: Vec<_> = rust.difference(&ui).collect();
    assert!(
        ui_only.is_empty() && rust_only.is_empty(),
        "in COMMANDS but not in the invoke handler: {ui_only:?}\nin the invoke handler but not in COMMANDS: {rust_only:?}"
    );
}

#[test]
fn ui_argument_keys_match_the_rust_command_parameters() {
    let c = Contract::load();
    let params = contract_support::rust_params();
    let mut errs = Vec::new();
    for name in c.command_names() {
        let Some(rust) = params.get(&name) else {
            errs.push(format!("{name}: no Rust signature found"));
            continue;
        };
        let ui = c.args(&name);
        for (key, optional) in &ui {
            if !rust.iter().any(|(r, _)| r == key) {
                errs.push(format!(
                    "{name}: UI sends `{key}` but Rust has no such parameter"
                ));
            } else if !optional && rust.iter().any(|(r, opt)| r == key && *opt) {
                // Required in TS while optional in Rust is fine.
            }
        }
        for (r, opt) in rust {
            if !opt && !ui.iter().any(|(k, _)| k == r) {
                errs.push(format!(
                    "{name}: Rust requires `{r}` but the UI does not send it"
                ));
            }
        }
    }
    assert!(errs.is_empty(), "{}", errs.join("\n"));
}

#[test]
fn ts_declares_the_equipment_and_magic_column_keys() {
    let c = Contract::load();
    let expect: &[(&str, &[&str])] = &[
        (
            "ArmorFacts",
            &[
                "group",
                "acText",
                "acBase",
                "addsDex",
                "dexCap",
                "strength",
                "stealthDisadvantage",
            ],
        ),
        (
            "WeaponFacts",
            &[
                "damage",
                "damageType",
                "properties",
                "mastery",
                "range",
                "proficiency",
                "kind",
            ],
        ),
        (
            "SpellFacts",
            &[
                "castingTime",
                "range",
                "duration",
                "verbal",
                "somatic",
                "material",
                "materialText",
                "concentration",
                "ritual",
            ],
        ),
        ("ElementSummary", &["spell", "armor", "weapon"]),
        ("SpellEntry", &["casting", "sourceName"]),
        ("InventoryItem", &["armor", "weapon", "showCard"]),
        ("CharacterDetails", &["treasure", "questItems"]),
        ("SelectionSlot", &["classSlotId", "groupId", "elementType"]),
        (
            "ImportReportPreview",
            &[
                "previewId",
                "payload",
                "bytes",
                "selfChecksFailed",
                "sourceAvailable",
                "sourceBytes",
                "sourceLimit",
                "endpoint",
            ],
        ),
        ("ImportReportSent", &["id"]),
    ];
    let mut missing = Vec::new();
    for (def, keys) in expect {
        let have = c.def_keys(def);
        for k in *keys {
            if !have.iter().any(|h| h == k) {
                missing.push(format!("{def}.{k}"));
            }
        }
    }
    assert!(missing.is_empty(), "TS contract lacks: {missing:?}");
}
