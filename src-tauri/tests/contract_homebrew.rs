//! Homebrew creator contract (WB-00): the Rust wire types in `homebrew_dto.rs` serialise to the shapes the UI declares in
//! `ui/src/api/types/homebrew.ts`, and the request types read what the UI sends.

mod contract_support;
use borealis_authoring::form::Memo;
use borealis_lib::content_dto::{ContentPack, PackKind};
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::*;
use contract_support::Contract;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

fn check<T: Serialize>(c: &Contract, def: &str, value: &T) {
    let v = serde_json::to_value(value).expect("serialises");
    let errs = c.def(def, &v);
    assert!(errs.is_empty(), "{def}:\n{}", errs.join("\n"));
}

fn field() -> HomebrewFieldSpec {
    HomebrewFieldSpec {
        key: "level".into(),
        label: "Level".into(),
        tab: "general".into(),
        section: Some("Casting".into()),
        control: "number".into(),
        required: true,
        editions: Some(vec![Ruleset::Dnd2024]),
        choices: Some(vec![HomebrewChoice {
            value: "1".into(),
            label: "1st".into(),
        }]),
        allow_custom: Some(false),
        min: Some(0.0),
        max: Some(9.0),
        picker: Some(HomebrewPickerTarget {
            types: vec!["Spell".into()],
            tags: Some(vec!["Wizard".into()]),
            multiple: Some(true),
        }),
        bindings: vec!["setter:level".into(), "data.level".into()],
        help: Some("Cantrip is 0.".into()),
        expert: Some(false),
    }
}

fn pack() -> ContentPack {
    ContentPack {
        id: "homebrew-gl".into(),
        name: "Gloom".into(),
        version: "1.0.1".into(),
        kind: PackKind::Homebrew,
        rulesets: vec![Ruleset::Dnd2024],
        element_count: 1,
        enabled: true,
        license: "CC-BY-4.0".into(),
        attribution: "Me".into(),
        source_ids: vec!["GL".into()],
        redistributable: Some(true),
    }
}

fn problem() -> HomebrewProblem {
    HomebrewProblem {
        severity: "error".into(),
        message: "A spell needs a school.".into(),
        path: Some("fields.school".into()),
        element_id: Some("ID_GL_SPELL_EMBER".into()),
        code: "missing_required_setter".into(),
    }
}

fn form() -> HomebrewForm {
    let mut fields = BTreeMap::new();
    fields.insert("name".to_string(), json!("Ember"));
    fields.insert("level".to_string(), json!(1));
    HomebrewForm {
        key: "e".into(),
        kind: "spell".into(),
        fields: fields.clone(),
        blocks: vec![HomebrewBlock {
            key: "b1".into(),
            kind: "grant".into(),
            sentence: "Gives proficiency: Stealth".into(),
            fields: BTreeMap::new(),
            preserved: vec![HomebrewPreservedPart {
                key: "setters".into(),
                label: "Values set under this rule".into(),
                kept: None,
            }],
            memo: None,
        }],
        children: vec![HomebrewForm {
            key: "e/0".into(),
            kind: "classFeature".into(),
            fields,
            blocks: vec![],
            children: vec![],
            preserved: vec![],
            memo: Memo::default(),
        }],
        preserved: vec![HomebrewPreservedPart {
            key: "extra.foo".into(),
            label: "An unknown attribute".into(),
            kept: None,
        }],
        memo: Memo::default(),
    }
}

#[test]
fn catalog_types_match_the_ts_contract() {
    let c = Contract::load();
    // the real kinds (WB-04 descriptors) as `homebrew_kinds` answers them (WB-06)
    let kinds = borealis_lib::commands_homebrew::homebrew_kinds().expect("kinds");
    assert!(kinds.len() > 10);
    for k in &kinds {
        check(&c, "HomebrewKindDescriptor", k);
    }
    check(
        &c,
        "HomebrewKindDescriptor",
        &HomebrewKindDescriptor {
            id: "spell".into(),
            label: "Spell".into(),
            plural_label: "Spells".into(),
            description: "A spell.".into(),
            group: "main".into(),
            element_types: vec!["Spell".into()],
            parent_kinds: vec![],
            editions: vec![Ruleset::Dnd2014, Ruleset::Dnd2024],
            tabs: vec![HomebrewTab {
                id: "general".into(),
                label: "GENERAL".into(),
            }],
            fields: vec![field()],
        },
    );
    let block_field = |base: HomebrewFieldSpec| HomebrewBlockField {
        base,
        dialect: Some("saveDc".into()),
        shown_as_note: Some(true),
        scope: Some("resource".into()),
        multiple: Some(true),
        fields: Some(vec![]),
        variants: Some(vec![HomebrewBlockVariant {
            value: "fixed".into(),
            label: "A fixed number".into(),
            fields: vec![],
        }]),
        kinds: Some(vec!["stat".into()]),
    };
    check(
        &c,
        "HomebrewBlockSpec",
        &HomebrewBlockSpec {
            kind: "grant_spell".into(),
            label: "Gives a spell".into(),
            group: "spells".into(),
            group_label: "Spells".into(),
            common: true,
            core_only: false,
            effect_only: false,
            level_gated: true,
            only_if: true,
            fields: vec![block_field(field())],
        },
    );
    check(
        &c,
        "HomebrewStatGroup",
        &HomebrewStatGroup {
            id: "senses".into(),
            label: "Senses".into(),
            stats: vec![HomebrewStatEntry {
                name: "darkvision:range".into(),
                label: "Darkvision range".into(),
                class_scoped: Some(false),
            }],
        },
    );
}

#[test]
fn pack_element_and_draft_types_match_the_ts_contract() {
    let c = Contract::load();
    check(
        &c,
        "HomebrewElementSummary",
        &HomebrewElementSummary {
            id: "ID_GL_SPELL_EMBER".into(),
            name: "Ember".into(),
            kind: "spell".into(),
            element_type: "Spell".into(),
            parent_id: None,
            edition: Some(Ruleset::Dnd2024),
            problems: HomebrewProblemCounts {
                errors: 1,
                warnings: 0,
            },
            has_draft: true,
            tags: vec!["Wizard".into()],
            text: "A lance of fire.".into(),
            copied_text: false,
        },
    );
    check(
        &c,
        "HomebrewDraftView",
        &HomebrewDraftView {
            draft_id: "d1".into(),
            pack_id: "homebrew-gl".into(),
            element_id: Some("ID_GL_SPELL_EMBER".into()),
            form: form(),
            preview: None,
            problems: vec![problem()],
            generated: HomebrewGeneratedView {
                json: "{}".into(),
                tree: vec![HomebrewDataNode {
                    key: "Ember".into(),
                    kind: borealis_authoring::form::DataKind::Object,
                    text: None,
                    sentence: None,
                    children: vec![HomebrewDataNode {
                        key: "requirements".into(),
                        kind: borealis_authoring::form::DataKind::Expr,
                        text: Some("[str:13]".into()),
                        sentence: Some("Strength 13".into()),
                        children: vec![],
                    }],
                }],
            },
            updated_at: "2026-11-10T10:00:00Z".into(),
            base_changed: false,
            saved_with: vec!["Evoker".into()],
        },
    );
    check(
        &c,
        "HomebrewDraftSummary",
        &HomebrewDraftSummary {
            draft_id: "d1".into(),
            pack_id: "homebrew-gl".into(),
            element_id: None,
            kind: "spell".into(),
            name: "Ember".into(),
            edition: Some(borealis_lib::dto::Ruleset::Dnd2024),
            updated_at: "2026-11-10T10:00:00Z".into(),
        },
    );
    check(
        &c,
        "HomebrewSaveResult",
        &HomebrewSaveResult {
            pack: pack(),
            element_ids: vec!["ID_GL_SPELL_EMBER".into()],
        },
    );
    check(
        &c,
        "HomebrewDeleteResult",
        &HomebrewDeleteResult {
            deleted: false,
            used_by: vec![HomebrewCharacterRef {
                character_id: "c1".into(),
                name: "Mira".into(),
            }],
            pack: Some(pack()),
            undo_id: Some("undo-1".into()),
        },
    );
    check(
        &c,
        "HomebrewBulkResult",
        &HomebrewBulkResult {
            pack: pack(),
            element_ids: vec!["ID_GL_SPELL_EMBER".into()],
            undo_id: None,
        },
    );
}

#[test]
fn picker_list_and_trial_types_match_the_ts_contract() {
    let c = Contract::load();
    check(
        &c,
        "HomebrewPickerRow",
        &HomebrewPickerRow {
            id: "ID_SRD_SPELL_FIREBALL".into(),
            name: "Fireball".into(),
            ty: "Spell".into(),
            summary: "3rd-level evocation".into(),
            prerequisites: None,
            source_name: "SRD 5.2.1".into(),
            edition: Some(Ruleset::Dnd2024),
            draft: false,
            level: Some(3),
            school: Some("Evocation".into()),
            spell: None,
        },
    );
    check(
        &c,
        "HomebrewElementRef",
        &HomebrewElementRef {
            id: "ID_X".into(),
            name: "Wizard".into(),
            ty: "Class".into(),
            pack_id: None,
            relation: "lists".into(),
            draft_id: None,
        },
    );
    check(
        &c,
        "HomebrewElementRef",
        &HomebrewElementRef {
            id: "ID_EI_BACKGROUND_EMBER_WARDEN".into(),
            name: "Ember Warden".into(),
            ty: "Background".into(),
            pack_id: Some("ember-isles".into()),
            relation: "grants".into(),
            draft_id: Some("hb-0123".into()),
        },
    );
    check(
        &c,
        "HomebrewSpellListView",
        &HomebrewSpellListView {
            list_tag: "Wizard".into(),
            expanded_of: Some("ID_Y".into()),
            own: true,
            count: 1,
            rows: vec![HomebrewSpellListRow {
                row: HomebrewPickerRow {
                    id: "ID_X".into(),
                    name: "Ember".into(),
                    ty: "Spell".into(),
                    summary: String::new(),
                    prerequisites: None,
                    source_name: "Gloom".into(),
                    edition: None,
                    draft: false,
                    level: Some(1),
                    school: Some("Evocation".into()),
                    spell: None,
                },
                on_list: true,
                removable: true,
            }],
        },
    );
    check(
        &c,
        "HomebrewLadderRow",
        &HomebrewLadderRow {
            level: 3,
            features: vec!["Arcane Ward".into()],
            choices: vec!["Subclass".into()],
            slots: vec![4, 2, 0, 0, 0, 0, 0, 0, 0],
            problems: vec![problem()],
        },
    );
    // A trial view needs a real snapshot (WB-08); its keys are checked here.
    let mut keys = c.def_keys("HomebrewTrialView");
    keys.sort();
    assert_eq!(
        keys,
        ["changed", "ladder", "problems", "snapshot", "trialId"]
    );
}

#[test]
fn request_types_read_what_the_ui_sends() {
    let req: HomebrewPackRequest = serde_json::from_value(
        json!({"name": "Gloom", "code": "GL", "edition": "any", "author": "Me", "license": "CC0"}),
    )
    .expect("pack request");
    assert_eq!(req.edition, Ruleset::Any);
    let patch: HomebrewPackPatch =
        serde_json::from_value(json!({"name": "Gloom 2"})).expect("patch");
    assert_eq!(patch.name.as_deref(), Some("Gloom 2"));
    let bulk: HomebrewBulkPatch =
        serde_json::from_value(json!({"edition": null, "addTags": ["Fire"]})).expect("bulk");
    assert_eq!(bulk.edition, Some(None));
    let dup: HomebrewBulkPatch = serde_json::from_value(json!({"duplicate": true})).expect("bulk");
    assert_eq!(dup.duplicate, Some(true));
    let keep: HomebrewBulkPatch = serde_json::from_value(json!({"addTags": []})).expect("bulk");
    assert_eq!(keep.edition, None, "absent keeps, null clears");
    let filters: HomebrewPickerFilters =
        serde_json::from_value(json!({"spellLevels": [1, 2], "ritual": true})).expect("filters");
    assert_eq!(filters.spell_levels, Some(vec![1, 2]));
    let page: HomebrewPageRequest =
        serde_json::from_value(json!({"offset": 0, "limit": 50})).expect("page");
    assert_eq!(page.limit, 50);
    let setup: HomebrewTrialSetup =
        serde_json::from_value(json!({"level": 5, "classId": "ID_X"})).expect("setup");
    assert_eq!(setup.level, Some(5));
    let change: HomebrewTrialChange =
        serde_json::from_value(json!({"slotId": "s1", "elementId": null})).expect("change");
    assert_eq!(change.element_id, Some(None));
    let round: HomebrewForm =
        serde_json::from_value(serde_json::to_value(form()).unwrap()).expect("form");
    assert_eq!(round, form());
}

#[test]
fn pack_file_types_match_the_ts_contract() {
    let c = Contract::load();
    check(
        &c,
        "HomebrewExportResult",
        &HomebrewExportResult {
            file_name: "Gloom Hollow.borealis-pack.zip".into(),
            path: Some("/Users/me/Gloom Hollow.borealis-pack.zip".into()),
            shared: false,
            data_base64: None,
            left_out: 2,
        },
    );
    check(
        &c,
        "HomebrewImportInfo",
        &borealis_lib::homebrew_transfer::HomebrewImportInfo {
            code: "GH".into(),
            code_taken: false,
            copy_code: "GH2".into(),
            copy_name: "Gloom Hollow (copy)".into(),
            replace_used_by: vec![HomebrewCharacterRef {
                character_id: "c1".into(),
                name: "Mira".into(),
            }],
            replace_drops: vec!["Ember Bolt".into()],
        },
    );
}

/// The real catalogs (WB-05) match the TS contract, and the web mock serves exactly them:
/// `ui/src/api/mock/homebrew-catalog.ts` is written from them (`BOREALIS_UPDATE_SNAPSHOTS=1` rewrites it), and the UI
/// typecheck holds that file against the TS types.
#[test]
fn block_and_stat_catalogs_match_the_ts_contract_and_the_mock() {
    use borealis_lib::homebrew_creator::{block_catalog, stat_catalog};
    let c = Contract::load();
    let blocks = block_catalog();
    let stats = stat_catalog(None);
    assert!(blocks.len() > 70, "core blocks, 69 kinds, effects");
    for b in &blocks {
        check(&c, "HomebrewBlockSpec", b);
    }
    for g in &stats {
        check(&c, "HomebrewStatGroup", g);
    }
    assert_eq!(
        borealis_lib::commands_homebrew::homebrew_block_catalog().expect("catalog"),
        blocks
    );
    let text = format!(
        "// Generated by src-tauri/tests/contract_homebrew.rs from borealis-authoring's catalogs (WB-05). Do not edit:\n\
         // run that test with BOREALIS_UPDATE_SNAPSHOTS=1.\n\
         import type {{ HomebrewBlockSpec, HomebrewStatGroup }} from \"../types/homebrew\";\n\n\
         export const MOCK_BLOCK_CATALOG: HomebrewBlockSpec[] = {};\n\n\
         export const MOCK_STAT_CATALOG: HomebrewStatGroup[] = {};\n",
        serde_json::to_string_pretty(&blocks).expect("json"),
        serde_json::to_string_pretty(&stats).expect("json")
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ui/src/api/mock/homebrew-catalog.ts");
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == text,
        "ui/src/api/mock/homebrew-catalog.ts is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}

/// The real form (WB-04 form model, blocks read by the WB-05 catalog) as the UI receives it matches the TS contract,
/// and reads as the stub DTO `HomebrewForm` the commands declare.
#[test]
fn the_creator_form_with_catalog_blocks_matches_the_ts_contract() {
    use borealis_authoring::expr_build::condition::ConditionContext;
    use borealis_authoring::form::{
        element_to_form, to_ui, CatalogBlockCodec, FormContext, NoLookup,
    };
    let element: borealis_content::Element = serde_json::from_value(json!({
        "id": "ID_GL_FEAT_HOWL", "name": "Howl", "type": "Feat", "source": "Gloom",
        "requirements": "[level:4]",
        "rules": [
            {"kind": "grant", "id": "ID_GL_TRAIT_PACK_TACTICS", "type": "Racial Trait", "level": 3},
            {"kind": "stat", "name": "speed", "value": "10", "equipped": "[armor:none]", "merge": true},
            {"kind": "resource", "id": "howl", "max": {"type": "formula", "expr": "wisdom:modifier"}},
            {"kind": "toggle", "id": "frenzy", "effects": [{"kind": "stat", "name": "speed", "value": "5"}]},
            {"kind": "unknown", "tag": "howl"}
        ]
    }))
    .expect("element");
    let codec = CatalogBlockCodec;
    let cx = FormContext {
        conditions: ConditionContext::new(Vec::<String>::new()),
        lookup: &NoLookup,
        blocks: &codec,
    };
    let form = serde_json::to_value(to_ui(&element_to_form(&element, &cx))).expect("json");
    let c = Contract::load();
    let errs = c.def("HomebrewForm", &form);
    assert!(errs.is_empty(), "HomebrewForm:\n{}", errs.join("\n"));
    let stub: HomebrewForm = serde_json::from_value(form.clone()).expect("reads as the stub DTO");
    assert_eq!(stub.blocks.len(), 5);
    assert_eq!(serde_json::to_value(&stub).expect("json"), form);
}

/// The web mock serves the real forms of the kinds that have their own form (WB-11 spell first):
/// `ui/src/api/mock/homebrew-kinds.ts` is written from `homebrew_kinds` (`BOREALIS_UPDATE_SNAPSHOTS=1` rewrites it), and
/// the UI typecheck holds that file against the TS types.
#[test]
fn kind_descriptors_match_the_mock() {
    let kinds = borealis_lib::commands_homebrew::homebrew_kinds().expect("kinds");
    assert!(kinds.iter().any(|k| k.id == "spell"));
    let text = format!(
        "// Generated by src-tauri/tests/contract_homebrew.rs from borealis-authoring's forms (WB-11). Do not edit:\n\
         // run that test with BOREALIS_UPDATE_SNAPSHOTS=1.\n\
         import type {{ HomebrewKindDescriptor }} from \"../types/homebrew\";\n\n\
         export const MOCK_KIND_DESCRIPTORS: HomebrewKindDescriptor[] = {};\n",
        serde_json::to_string_pretty(&kinds).expect("json"),
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ui/src/api/mock/homebrew-kinds.ts");
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == text,
        "ui/src/api/mock/homebrew-kinds.ts is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
    );
}
