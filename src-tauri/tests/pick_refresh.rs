//! GitHub #7: a pick can change what other open lists of the same tab offer. These tests pin the
//! data the UI re-reads after a pick (`get_slot_options` of every other slot), for 2014 and 2024
//! characters, with a synthetic imported pack:
//! - a homebrew "skill tree": a node whose requirement is another node of the same select type
//!   appears in the sibling slot once that node is picked, and a node that grants another select
//!   of the same type adds a new slot;
//! - skills: an expertise list offers a skill once a skill pick grants proficiency in it, and an
//!   option that grants an extra skill choice adds a skill slot.

use borealis_lib::dto::AbilityMethodDto;
use borealis_lib::dto::{CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const FOLK_2024: &str = "ID_T7_SPECIES_TREE_FOLK";
const FOLK_2014: &str = "ID_T7_RACE_TREE_FOLK";
const ROOT: &str = "ID_T7_NODE_ROOT";
const BRANCH: &str = "ID_T7_NODE_BRANCH";
const GRANTER: &str = "ID_T7_NODE_GRANTER";
const LEAF: &str = "ID_T7_NODE_LEAF";
const SCHOLAR: &str = "ID_T7_NODE_SCHOLAR";
const ATHLETICS: &str = "ID_PROFICIENCY_SKILL_ATHLETICS";
const ATHLETICS_EXPERTISE: &str = "ID_EXPERTISE_SKILL_ATHLETICS";

const TREE: &str = "Tree Node";
const BONUS_TREE: &str = "Tree Node (Granted)";
const SKILL: &str = "Tree Skill";
const EXTRA_SKILL: &str = "Tree Skill (Scholar)";
const EXPERTISE: &str = "Tree Expertise";

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(path, serde_json::to_string_pretty(value).expect("json")).expect("write");
}

/// The folk's selects: two tree nodes, one skill, one expertise.
fn folk_rules() -> Value {
    json!([
        {"kind": "select", "type": "Racial Trait", "name": TREE, "number": 2, "supports": "Test Tree"},
        {"kind": "select", "type": "Proficiency", "name": SKILL, "supports": "Skill"},
        {"kind": "select", "type": "Class Feature", "name": EXPERTISE, "supports": "Expertise"},
    ])
}

fn node(id: &str, name: &str, requirements: Option<&str>, rules: Value) -> Value {
    let mut el = json!({"id": id, "name": name, "type": "Racial Trait", "source": "Test Imports",
        "supports": ["Test Tree"], "rules": rules});
    if let Some(r) = requirements {
        el["requirements"] = json!(r);
    }
    el
}

fn install(data: &Path) {
    let dir = data.join("library/t7-tree");
    let file = |ruleset: &str, elements: Value| json!({"format": "borealis-content/1", "ruleset": ruleset, "elements": elements});
    write_json(
        &dir.join("modern.json"),
        &file(
            "dnd2024",
            json!([{"id": FOLK_2024, "name": "Tree Folk", "type": "Species",
            "source": "Test Imports", "rules": folk_rules()}]),
        ),
    );
    write_json(
        &dir.join("legacy.json"),
        &file(
            "dnd2014",
            json!([{"id": FOLK_2014, "name": "Tree Folk", "type": "Race",
            "source": "Test Imports", "rules": folk_rules()}]),
        ),
    );
    write_json(
        &dir.join("tree.json"),
        &file(
            "any",
            json!([
                node(ROOT, "Root", None, json!([])),
                node(BRANCH, "Branch", Some(ROOT), json!([])),
                node(
                    GRANTER,
                    "Granter",
                    Some(ROOT),
                    json!([{"kind": "select", "type": "Racial Trait",
                    "name": BONUS_TREE, "supports": "Test Tree"}])
                ),
                node(LEAF, "Leaf", Some(GRANTER), json!([])),
                node(
                    SCHOLAR,
                    "Scholar",
                    None,
                    json!([{"kind": "select", "type": "Proficiency",
                    "name": EXTRA_SKILL, "supports": "Skill"}])
                ),
            ]),
        ),
    );
    write_json(
        &dir.join("pack.json"),
        &json!({"format": "borealis-pack/1", "id": "t7-tree", "name": "#7 skill tree",
            "version": "1.0.0", "origin": "custom", "rulesets": ["dnd2014", "dnd2024"],
            "files": [{"path": "modern.json", "kind": "content"},
                      {"path": "legacy.json", "kind": "content"},
                      {"path": "tree.json", "kind": "content"}]}),
    );
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    install(dir.path());
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let state = AppState::new(paths).expect("state");
    (dir, state)
}

/// A level 1 character of `ruleset` with Tree Folk picked.
fn folk(app: &AppState, ruleset: Ruleset) -> CharacterSnapshot {
    let request = NewCharacterRequest {
        name: "Ash".into(),
        ruleset,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch::default(),
        portrait: None,
        group: None,
    };
    let snap = app.create_character(ruleset, &request).expect("create");
    let species = snap
        .slots
        .iter()
        .find(|s| matches!(s.element_type.as_str(), "race" | "species"))
        .expect("species slot")
        .id
        .clone();
    let id = if ruleset == Ruleset::Dnd2024 {
        FOLK_2024
    } else {
        FOLK_2014
    };
    app.apply_selection(&snap.id, &species, id)
        .expect("tree folk")
}

/// The ids of the `n`th (0-based) slot titled `title`.
fn slot(snap: &CharacterSnapshot, title: &str, n: usize) -> String {
    snap.slots
        .iter()
        .filter(|s| s.title.starts_with(title) && !s.title.starts_with(&format!("{title} (")))
        .nth(n)
        .unwrap_or_else(|| {
            let titles: Vec<_> = snap.slots.iter().map(|s| s.title.as_str()).collect();
            panic!("no slot {title} #{n} in {titles:?}")
        })
        .id
        .clone()
}

/// Every offered (enabled) element id of a slot, as the UI's picker reads it.
fn offered(app: &AppState, character: &str, slot: &str) -> Vec<String> {
    let page = app
        .get_slot_options(character, slot, None, None, None)
        .expect("options");
    let page = serde_json::to_value(page).expect("json");
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|r| r.get("disabledReason").is_none())
        .map(|r| r["id"].as_str().expect("id").to_string())
        .collect()
}

fn has(list: &[String], id: &str) -> bool {
    list.iter().any(|x| x == id)
}

#[test]
fn a_node_picked_in_one_slot_unlocks_its_child_in_the_sibling_slot() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let (_d, app) = app();
        let snap = folk(&app, ruleset);
        let (first, second) = (slot(&snap, TREE, 0), slot(&snap, TREE, 1));
        let before = offered(&app, &snap.id, &second);
        assert!(has(&before, ROOT), "{ruleset:?}: {before:?}");
        assert!(
            !has(&before, BRANCH),
            "{ruleset:?}: Branch needs Root: {before:?}"
        );

        let snap = app.apply_selection(&snap.id, &first, ROOT).expect("root");
        let after = offered(&app, &snap.id, &second);
        assert!(
            has(&after, BRANCH),
            "{ruleset:?}: Branch after Root: {after:?}"
        );
        assert!(has(&after, GRANTER), "{ruleset:?}: {after:?}");
        assert!(
            !has(&after, ROOT),
            "{ruleset:?}: Root is held by the first slot: {after:?}"
        );
    }
}

#[test]
fn a_node_that_grants_a_select_of_the_same_type_adds_a_slot_with_the_new_options() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let (_d, app) = app();
        let snap = folk(&app, ruleset);
        let snap = app
            .apply_selection(&snap.id, &slot(&snap, TREE, 0), ROOT)
            .expect("root");
        assert!(!snap.slots.iter().any(|s| s.title.starts_with(BONUS_TREE)));
        let snap = app
            .apply_selection(&snap.id, &slot(&snap, TREE, 1), GRANTER)
            .expect("granter");
        let bonus = snap
            .slots
            .iter()
            .find(|s| s.title.starts_with(BONUS_TREE))
            .unwrap_or_else(|| panic!("{ruleset:?}: no granted slot"));
        assert!(bonus.chosen.is_none());
        let options = offered(&app, &snap.id, &bonus.id);
        assert!(
            has(&options, LEAF),
            "{ruleset:?}: Leaf needs Granter: {options:?}"
        );
        assert!(has(&options, BRANCH), "{ruleset:?}: {options:?}");
    }
}

#[test]
fn a_skill_pick_adds_its_expertise_and_an_option_can_add_a_skill_slot() {
    for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let (_d, app) = app();
        let snap = folk(&app, ruleset);
        let expertise = slot(&snap, EXPERTISE, 0);
        assert!(!has(
            &offered(&app, &snap.id, &expertise),
            ATHLETICS_EXPERTISE
        ));

        let snap = app
            .apply_selection(&snap.id, &slot(&snap, SKILL, 0), ATHLETICS)
            .expect("athletics");
        let options = offered(&app, &snap.id, &expertise);
        assert!(
            has(&options, ATHLETICS_EXPERTISE),
            "{ruleset:?}: {options:?}"
        );

        // the existing removal behaviour: clearing the skill drops the expertise built on it
        let snap = app
            .apply_selection(&snap.id, &expertise, ATHLETICS_EXPERTISE)
            .expect("expertise");
        let snap = app
            .clear_selection(&snap.id, &slot(&snap, SKILL, 0))
            .expect("clear");
        let held = snap.slots.iter().find(|s| s.id == expertise).expect("slot");
        assert!(
            held.chosen.is_none(),
            "{ruleset:?}: expertise kept without its skill"
        );

        // an option that grants an extra skill choice
        let snap = app
            .apply_selection(&snap.id, &slot(&snap, TREE, 0), SCHOLAR)
            .expect("scholar");
        let extra = snap
            .slots
            .iter()
            .find(|s| s.title.starts_with(EXTRA_SKILL))
            .unwrap_or_else(|| panic!("{ruleset:?}: no extra skill slot"));
        assert!(has(&offered(&app, &snap.id, &extra.id), ATHLETICS));
    }
}
