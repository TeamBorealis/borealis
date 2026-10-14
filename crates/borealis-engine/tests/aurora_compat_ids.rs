//! Newer Aurora internal ids defined in the core pack (race tags, background ASI marker,
//! custom ability score options, background combinations, sizes) and Aurora PHB 2024 ids
//! resolved to their SRD 5.2.1 equivalents, on small synthetic community-style content.

use borealis_content::{
    ContentFile, ContentStore, ContentStoreBuilder, IssueCode, PackOrigin, Ruleset,
};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const COMBINATION: &str = "ID_INTERNAL_ABILITY_SCORE_IMPROVEMENT_COMBINATION_STR_DEX_CHA";

fn content() -> Vec<Value> {
    vec![
        json!({"id": "ID_T_RACE_SHADE", "name": "Tshade", "type": "Race", "source": "Test",
               "setters": [{"name": "names", "value": "A", "attributes": {"type": "male"}}], "rules": [
            {"kind": "grant", "type": "Grants", "id": "ID_INTERNAL_GRANT_RACE_ELF"},
            {"kind": "grant", "type": "Spell", "id": "ID_WOTC_PHB24_SPELL_ALARM"},
            {"kind": "select", "type": "Ability Score Improvement", "name": "Shade ASI",
             "supports": "Custom Ability Score Improvement Option",
             "requirements": "!ID_INTERNAL_GRANTS_BACKGROUND_ASI"},
            {"kind": "select", "type": "Racial Trait", "name": "Shade Size",
             "supports": "ID_INTERNAL_RACIAL_TRAIT_SMALL|ID_INTERNAL_RACIAL_TRAIT_MEDIUM"}]}),
        json!({"id": "ID_T_BACKGROUND_ELVEN", "name": "Elven Ward", "type": "Background",
               "source": "Test", "requirements": "ID_INTERNAL_GRANT_RACE_ELF", "rules": [
            {"kind": "grant", "type": "Grants", "id": "ID_INTERNAL_GRANTS_BACKGROUND_ASI"},
            {"kind": "grant", "type": "Ability Score Improvement", "id": COMBINATION}]}),
        json!({"id": "ID_T_FEAT_WARDED", "name": "Warded", "type": "Feat", "source": "Test",
               "requirements": "ID_WOTC_PHB24_SPELL_ALARM", "rules": [
            {"kind": "select", "type": "Ability Score Improvement", "name": "Warded ASI",
             "supports": "ID_INTERNAL_ABILITY_SCORE_IMPROVEMENT_FEAT_WISDOM|ID_INTERNAL_ABILITY_SCORE_IMPROVEMENT_FEAT_CHARISMA"}]}),
    ]
}

fn store() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": content()}),
    )
    .expect("synthetic content");
    let mut b = ContentStoreBuilder::new();
    for p in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(p)).expect("bundled pack");
    }
    b.add_file("community.json", PackOrigin::Custom, file)
        .build()
}

fn options(snap: &CharacterSnapshot, name: &str) -> Vec<String> {
    snap.slots
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no slot {name}"))
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let snap = e.derive(s);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.name == name || x.ty == name)
        .unwrap_or_else(|| panic!("no slot {name}"));
    commands::select(e, s, &slot.key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn shade_fighter(e: &Engine) -> CharacterState {
    let s = commands::new_character(e, Ruleset::Dnd2014);
    let s = commands::set_base_scores(e, &s, [12, 12, 12, 12, 12, 12]).map_or(s, |o| o.state);
    let key = slot_key(&e.derive(&s), "ID_LEVEL_1", "Class", 1).expect("class slot");
    let s = commands::select(e, &s, &key, Choice::Element(FIGHTER.into()))
        .expect("class")
        .state;
    pick(e, &s, "Race", "ID_T_RACE_SHADE")
}

#[test]
fn community_references_to_newer_aurora_ids_resolve() {
    let store = store();
    let dangling: Vec<String> = store
        .validate()
        .into_iter()
        .filter(|i| i.code == IssueCode::DanglingReference)
        .filter(|i| i.element.as_deref().is_some_and(|e| e.starts_with("ID_T_")))
        .map(|i| i.message)
        .collect();
    assert!(dangling.is_empty(), "{dangling:?}");
    let grant = store
        .element("ID_T_RACE_SHADE")
        .unwrap()
        .rules
        .iter()
        .find_map(|r| match r {
            borealis_content::Rule::Grant(g) if g.ty.as_deref() == Some("Spell") => {
                Some(g.id.clone())
            }
            _ => None,
        });
    assert_eq!(grant.as_deref(), Some("ID_2024_SPELL_ALARM"));
}

#[test]
fn race_asi_options_sizes_and_aliased_spell_work() {
    let store = store();
    let e = Engine::new(&store);
    let s = shade_fighter(&e);
    let snap = e.derive(&s);
    assert_eq!(
        options(&snap, "Shade ASI"),
        [
            "ID_INTERNAL_CUSTOM_ASI_OPTION_PLUS_1_PLUS_1_PLUS_1",
            "ID_INTERNAL_CUSTOM_ASI_OPTION_PLUS_2_PLUS_1"
        ]
    );
    assert_eq!(options(&snap, "Shade Size").len(), 2);
    assert!(snap
        .elements
        .iter()
        .any(|r| r.element.id == "ID_2024_SPELL_ALARM"));

    let s = pick(
        &e,
        &s,
        "Shade ASI",
        "ID_INTERNAL_CUSTOM_ASI_OPTION_PLUS_2_PLUS_1",
    );
    let snap = e.derive(&s);
    assert_eq!(options(&snap, "Ability Score Increase +2").len(), 6);
    let s = pick(
        &e,
        &s,
        "Ability Score Increase +2",
        "ID_WOTC_TCOE_OPTION_CUSTOMIZED_ASI_WISDOM_INCREASE_2",
    );
    let wis = e
        .derive(&s)
        .abilities
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("wisdom"))
        .map(|a| a.score);
    assert_eq!(wis, Some(14));

    // a requirement written with the PHB 2024 id reads the aliased SRD id
    let req = &store.element("ID_T_FEAT_WARDED").unwrap().requirements;
    assert_eq!(req, "ID_2024_SPELL_ALARM");
}

#[test]
fn race_tag_gates_background_and_background_asi_replaces_race_asi() {
    let store = store();
    let e = Engine::new(&store);
    let s = shade_fighter(&e);
    let s = pick(&e, &s, "Background", "ID_T_BACKGROUND_ELVEN");
    let snap = e.derive(&s);
    assert!(
        !snap.slots.iter().any(|x| x.name == "Shade ASI"),
        "the race ASI select is off once the background provides the increases"
    );
    assert_eq!(
        options(&snap, "Background Ability Scores"),
        [format!("{COMBINATION}_1"), format!("{COMBINATION}_2")]
    );
    let s = pick(
        &e,
        &s,
        "Background Ability Scores",
        &format!("{COMBINATION}_2"),
    );
    let snap = e.derive(&s);
    let score = |n: &str| {
        snap.abilities
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(n))
            .map(|a| a.score)
    };
    assert_eq!(
        (
            score("strength"),
            score("dexterity"),
            score("charisma"),
            score("wisdom")
        ),
        (Some(13), Some(13), Some(13), Some(12))
    );
}
