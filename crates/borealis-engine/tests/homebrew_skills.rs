//! HC1 (WB-19 Open, item HtCY8): a `Skill` element the homebrew creator writes is a character skill like the SRD's
//! eighteen: a sheet row with its ability's modifier and passive score, proficiency from a grant or a skill choice
//! (`select type="Proficiency" supports="Skill"`, as the SRD's class and feat choices are), expertise, the bonus to
//! every ability check. Imported content never has `Skill` elements (Aurora's skills are `Proficiency` elements
//! tagged Skill), so characters without homebrew skills keep exactly the eighteen.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SkillSnapshot;
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use serde_json::{json, Value};
use std::path::Path;

const LINGUISTICS: &str = "ID_EI_SKILL_LINGUISTICS";
const SCHOLAR: &str = "ID_T_RACE_SCHOLAR";
const CHOOSER: &str = "ID_T_RACE_CHOOSER";

fn engine(extra: Vec<Value>) -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .expect("bundled packs")
        .add_file("homebrew.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

/// The creator's Skill form: name, `ability` setter, tags Skill and the ability.
fn skill(id: &str, name: &str, ability: &str) -> Value {
    json!({"id": id, "name": name, "type": "Skill", "source": "Ember Isles",
        "supports": ["Skill", ability], "setters": [{"name": "ability", "value": ability}]})
}

fn races() -> Vec<Value> {
    vec![
        json!({"id": SCHOLAR, "name": "Scholar", "type": "Race", "source": "Test",
            "rules": [{"kind": "grant", "type": "Skill", "id": LINGUISTICS}]}),
        json!({"id": CHOOSER, "name": "Chooser", "type": "Race", "source": "Test",
            "rules": [{"kind": "select", "type": "Proficiency", "name": "Skill Proficiency (Chooser)",
                "supports": "Skill"}]}),
    ]
}

fn character(e: &Engine, intelligence: i32) -> CharacterState {
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    s.abilities.base[3] = intelligence;
    s
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("no slot {name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn with_race(e: &Engine, s: &CharacterState, race: &str) -> CharacterState {
    pick(e, s, "ID_LEVEL_1", "Race", race)
}

fn linguistics(snap: &CharacterSnapshot) -> &SkillSnapshot {
    snap.skill("linguistics")
        .expect("the homebrew skill is a character skill")
}

#[test]
fn a_homebrew_skill_is_a_sheet_row_with_its_abilitys_modifier() {
    let mut extra = vec![skill(LINGUISTICS, "Linguistics", "Intelligence")];
    extra.extend(races());
    let e = engine(extra);
    let snap = e.derive(&character(&e, 14));
    assert_eq!(snap.skills.len(), 19, "the eighteen and Linguistics");
    let l = linguistics(&snap);
    assert_eq!(l.ability, "intelligence");
    assert_eq!(l.label.as_deref(), Some("Linguistics"));
    assert_eq!((l.proficiency, l.total, l.passive), (0, 2, 12));
    assert!(!l.proficient);
    // the eighteen keep their order and have no label of their own
    assert_eq!(snap.skills[0].name, "acrobatics");
    assert!(snap.skills[..18].iter().all(|s| s.label.is_none()));
}

#[test]
fn a_grant_or_a_skill_choice_makes_the_character_proficient() {
    let mut extra = vec![skill(LINGUISTICS, "Linguistics", "Intelligence")];
    extra.extend(races());
    let e = engine(extra);
    let base = character(&e, 14);
    let granted = e.derive(&with_race(&e, &base, SCHOLAR));
    let l = linguistics(&granted);
    assert!(l.proficient, "{l:?}");
    assert_eq!((l.proficiency, l.total), (2, 4));

    // an any-skill choice offers the homebrew skill next to the SRD's
    let s = with_race(&e, &base, CHOOSER);
    let snap = e.derive(&s);
    let offered: Vec<String> = snap
        .slots
        .iter()
        .find(|x| x.name == "Skill Proficiency (Chooser)")
        .expect("the choice")
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert!(offered.iter().any(|id| id == LINGUISTICS), "{offered:?}");
    assert!(offered.iter().any(|id| id == "ID_PROFICIENCY_SKILL_ARCANA"));
    let chosen = e.derive(&pick(
        &e,
        &s,
        CHOOSER,
        "Skill Proficiency (Chooser)",
        LINGUISTICS,
    ));
    assert!(linguistics(&chosen).proficient);
    assert!(!chosen.skill("arcana").expect("arcana").proficient);
}

#[test]
fn expertise_and_the_ability_check_bonus_reach_a_homebrew_skill() {
    let mut extra = vec![skill(LINGUISTICS, "Linguistics", "Intelligence")];
    extra.extend(races());
    extra.push(json!({"id": "ID_T_FEAT_POLYGLOT", "name": "Polyglot", "type": "Feat", "source": "Test",
        "rules": [{"kind": "stat", "name": "linguistics:proficiency", "value": "proficiency", "bonus": "double"},
                  {"kind": "stat", "name": "ability checks", "value": "1"}]}));
    let e = engine(extra);
    let mut s = with_race(&e, &character(&e, 14), SCHOLAR);
    s.options.push("ID_T_FEAT_POLYGLOT".into());
    let l = e.derive(&s).skill("linguistics").cloned().expect("skill");
    assert!(l.expertise, "{l:?}");
    assert_eq!((l.proficiency, l.misc, l.total), (4, 1, 7));
    assert_eq!(l.passive, 17);
}

#[test]
fn a_skill_named_like_a_statistic_or_without_an_ability_is_no_skill() {
    let e = engine(vec![
        skill("ID_EI_SKILL_STRENGTH", "Strength", "Strength"),
        skill("ID_EI_SKILL_AC", "AC", "Dexterity"),
        json!({"id": "ID_EI_SKILL_NOTHING", "name": "Nothing", "type": "Skill", "source": "Ember Isles"}),
        skill("ID_EI_SKILL_BAD", "Bad", "Luck"),
        skill("ID_EI_SKILL_COLON", "Lore: Sea", "Wisdom"),
    ]);
    let snap = e.derive(&character(&e, 10));
    assert_eq!(snap.skills.len(), 18, "{:?}", snap.skills);
}

#[test]
fn a_homebrew_skill_of_an_srd_name_is_that_skill() {
    let mut extra = vec![skill(LINGUISTICS, "Arcana", "Intelligence")];
    extra.extend(races());
    let e = engine(extra);
    let snap = e.derive(&with_race(&e, &character(&e, 14), SCHOLAR));
    assert_eq!(snap.skills.len(), 18);
    assert!(snap.skill("arcana").expect("arcana").proficient);
}

#[test]
fn characters_without_homebrew_skills_keep_the_eighteen_unchanged() {
    let e = engine(races());
    let snap = e.derive(&character(&e, 14));
    assert_eq!(snap.skills.len(), 18);
    let json = serde_json::to_value(&snap.skills).expect("json");
    assert!(
        json.as_array()
            .expect("list")
            .iter()
            .all(|s| s.get("label").is_none()),
        "no new key in the snapshot of an SRD skill"
    );
}
