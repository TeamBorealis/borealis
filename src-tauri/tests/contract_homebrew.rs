//! Homebrew creator contract (WB-00): the Rust wire types in `homebrew_dto.rs` serialise to the shapes the UI declares in
//! `ui/src/api/types/homebrew.ts`, the request types read what the UI sends, and every creator command answers the
//! documented "not implemented" error until its work item lands.

mod contract_support;
use borealis_lib::content_dto::{ContentPack, PackKind};
use borealis_lib::dto::Ruleset;
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::*;
use contract_support::Contract;
use serde::Serialize;
use serde_json::{json, Value};
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
        kind: "spell".into(),
        fields: fields.clone(),
        blocks: vec![HomebrewBlock {
            key: "b1".into(),
            kind: "grant".into(),
            sentence: "Gives proficiency: Stealth".into(),
            fields: BTreeMap::new(),
        }],
        children: vec![HomebrewForm {
            kind: "classFeature".into(),
            fields,
            blocks: vec![],
            children: vec![],
            preserved: vec![],
        }],
        preserved: vec![HomebrewPreservedPart {
            key: "extra.foo".into(),
            label: "An unknown attribute".into(),
        }],
    }
}

#[test]
fn catalog_types_match_the_ts_contract() {
    let c = Contract::load();
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
    check(
        &c,
        "HomebrewBlockSpec",
        &HomebrewBlockSpec {
            kind: "grant_spell".into(),
            label: "Gives a spell".into(),
            group: "Spells".into(),
            common: true,
            core_only: false,
            level_gated: true,
            fields: vec![field()],
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
                tree: vec![json!({"key": "name", "value": "Ember"})],
            },
            updated_at: "2026-11-10T10:00:00Z".into(),
            base_changed: false,
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
        },
    );
    check(
        &c,
        "HomebrewSpellListView",
        &HomebrewSpellListView {
            list_tag: "Wizard".into(),
            own: true,
            count: 1,
            rows: vec![HomebrewSpellListRow {
                id: "ID_X".into(),
                name: "Ember".into(),
                level: 1,
                school: "Evocation".into(),
                verbal: true,
                somatic: true,
                material: false,
                source_name: "Gloom".into(),
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
fn creator_commands_answer_not_implemented_until_their_item_lands() {
    use borealis_lib::commands_homebrew as hb;
    let errs = [
        hb::homebrew_kinds().unwrap_err(),
        hb::homebrew_list_drafts().unwrap_err(),
        hb::homebrew_save_draft("d1".into()).unwrap_err(),
        hb::homebrew_export_pack("p".into(), true).unwrap_err(),
    ];
    for e in errs {
        assert_eq!(e.code, ErrorCode::RequirementFailed);
        let details: &Value = e.details.as_ref().expect("details");
        assert!(details["notImplemented"]
            .as_str()
            .is_some_and(|c| c.starts_with("homebrew_")));
    }
}
