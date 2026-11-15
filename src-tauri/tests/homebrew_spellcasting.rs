//! WB-17 (homebrew creator spellcasting section, spec 4.6) through the real `AppState` on a temporary data folder with
//! the bundled content: a new 2024 half caster (round up) that prepares Wisdom spells from the Ranger's list, made on
//! its class's Spellcasting feature (a child made with `homebrew_new_draft {parentId}`), checks clean (live and the
//! 1-20 level sweep), saves in the SRD's shape, and a level-5 character of it has the right slots and prepares from that
//! list; multiclassed with a Wizard, its slots pool with the Wizard's.

use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{HomebrewDraftView, HomebrewPackRequest};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;

const CLASS_ID: &str = "ID_EI_CLASS_WARDEN";
const WIZARD_MC: &str = "ID_2024_MULTICLASS_WIZARD";
const PREPARED: [u32; 20] = [
    2, 3, 4, 5, 6, 6, 7, 7, 9, 9, 10, 10, 11, 11, 12, 12, 14, 14, 15, 15,
];

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

fn app() -> (tempfile::TempDir, AppState, String) {
    let data = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    })
    .unwrap();
    let pack = state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: "Ember Isles".into(),
                code: "EI".into(),
                edition: Ruleset::Dnd2024,
                author: None,
                description: None,
                license: None,
            },
            &Quiet,
        )
        .unwrap()
        .id;
    (data, state, pack)
}

fn errors(v: &HomebrewDraftView) -> Vec<(String, String)> {
    v.problems
        .iter()
        .filter(|p| p.severity == "error")
        .map(|p| (p.path.clone().unwrap_or_default(), p.message.clone()))
        .collect()
}

fn saved(data: &tempfile::TempDir, pack: &str, id: &str) -> Value {
    let path = data.path().join("library").join(pack).join("content.json");
    let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    content["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("{id} saved"))
}

/// A 2024 character of `class` at level 1 with these scores.
fn first_level(e: &Engine, class: &str, scores: [i32; 6]) -> CharacterState {
    let mut s = commands::new_character(e, ContentRuleset::Dnd2024);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(class.into()))
        .unwrap()
        .state;
    commands::set_base_scores(e, &s, scores).unwrap().state
}

#[test]
fn a_new_half_caster_prepares_from_its_list_and_pools_slots_when_multiclassed() {
    let (data, state, pack) = app();
    let v = state
        .homebrew_new_draft(&pack, "class", None, None, None)
        .unwrap();
    // the Spellcasting feature, made inside the class (the SPELLCASTING tab's "Add spellcasting")
    let v = state
        .homebrew_new_draft(&pack, "classFeature", None, Some(&v.draft_id), None)
        .unwrap();
    let key = v.form.children.last().expect("the new feature").key.clone();
    let mut form = v.form.clone();
    form.fields.insert("name".into(), json!("Warden"));
    form.fields.insert("hitDie".into(), json!("d10"));
    form.fields.insert(
        "skills".into(),
        json!({"count": 1, "skills": [{"id": "ID_PROFICIENCY_SKILL_STEALTH", "name": "", "type": "Proficiency"}]}),
    );
    let sc = form.children.iter_mut().find(|c| c.key == key).unwrap();
    assert_eq!(
        sc.fields["casts"],
        json!(false),
        "a new feature gives no spellcasting yet"
    );
    for (k, x) in [
        ("name", json!("Spellcasting")),
        ("casts", json!(true)),
        ("casterType", json!("halfUp")),
        ("ability", json!("Wisdom")),
        ("preparation", json!("prepared")),
        ("spellList", json!("Ranger")),
        ("swap", json!(true)),
        ("prepared", json!(PREPARED)),
        ("preparedChange", json!("longRestOne")),
    ] {
        sc.fields.insert(k.into(), x);
    }
    let v = state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    assert_eq!(errors(&v), Vec::<(String, String)>::new());
    let deep: Vec<(String, String)> = state
        .homebrew_check_draft(&v.draft_id)
        .unwrap()
        .into_iter()
        .filter(|p| p.severity != "info")
        .map(|p| (p.code, p.message))
        .collect();
    assert_eq!(
        deep,
        Vec::<(String, String)>::new(),
        "the level sweep stays quiet"
    );
    let saved_ids = state
        .homebrew_save_draft(&v.draft_id, &Quiet)
        .unwrap()
        .element_ids;
    assert_eq!(saved_ids[0], CLASS_ID);

    // the SRD's shape: the node, the slot stats, the multiclass grant, the class table's prepared column
    let feature = saved(&data, &pack, "ID_EI_CLASS_FEATURE_WARDEN_SPELLCASTING");
    assert_eq!(
        feature["spellcasting"],
        json!({"name": "Warden", "ability": "Wisdom", "prepare": true, "allow_replace": true,
            "list": {"supports": "Ranger", "known": true}})
    );
    let rules = feature["rules"].as_array().unwrap();
    assert!(rules
        .contains(&json!({"kind": "stat", "name": "warden:spellcasting:slots:1", "value": "2"})));
    assert!(rules.contains(
        &json!({"kind": "grant", "id": "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP",
        "type": "Grants", "requirements": "ID_INTERNAL_GRANT_MULTICLASS"})
    ));
    let class = saved(&data, &pack, CLASS_ID);
    assert_eq!(class["data"]["progression"]["prepared"], json!(PREPARED));
    assert_eq!(class["data"]["progression"]["slot_1"][0], 2);

    // a level-5 Warden: half caster rounded up has 4 first- and 2 second-level slots, prepares 6 from the Ranger list
    let hub = state.hub().unwrap();
    let e = &hub.engine;
    let mut s = first_level(e, CLASS_ID, [10, 12, 14, 14, 15, 8]);
    for _ in 1..5 {
        s = commands::level_up_main(e, &s, None).unwrap().state;
    }
    let snap = e.derive(&s);
    let caster = snap.caster("Warden").expect("the Warden casts");
    assert_eq!(caster.slots, [4, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(caster.prepare_limit, Some(6));
    assert_eq!(caster.ability.to_lowercase(), "wisdom");
    assert!(
        caster
            .spells
            .iter()
            .any(|x| x.id == "ID_2024_SPELL_GOODBERRY"),
        "a Ranger spell is on its list"
    );
    assert!(
        !caster
            .spells
            .iter()
            .any(|x| x.id == "ID_2024_SPELL_MAGIC_MISSILE"),
        "a Wizard-only spell is not"
    );
    s = commands::set_prepared(e, &s, "Warden", "ID_2024_SPELL_GOODBERRY", true)
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let caster = snap.caster("Warden").unwrap();
    assert!(caster
        .spells
        .iter()
        .any(|x| x.id == "ID_2024_SPELL_GOODBERRY" && x.prepared));

    // Warden 5 / Wizard 1: caster level 3 (5 halved, rounded up) + 1 = 4 -> 4 first- and 3 second-level slots
    let opened = commands::level_up_new_multiclass(e, &s, None)
        .unwrap()
        .state;
    let slot = e
        .derive(&opened)
        .slots
        .into_iter()
        .find(|x| x.name.starts_with("Multiclass"))
        .expect("multiclass slot");
    assert!(
        slot.options.iter().any(|o| o.id == WIZARD_MC),
        "Intelligence 14 qualifies for the Wizard"
    );
    let s = commands::select(e, &opened, &slot.key, Choice::Element(WIZARD_MC.into()))
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let pooled = snap.multiclass_slots.expect("two slot sources pool");
    assert_eq!(pooled.caster_level, 4);
    assert_eq!(pooled.slots, [4, 3, 0, 0, 0, 0, 0, 0, 0]);
}
