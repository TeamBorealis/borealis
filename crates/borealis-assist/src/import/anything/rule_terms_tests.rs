//! Spelling fixes of model-written rules (synthetic elements).

use super::*;
use serde_json::json;

fn rules(el: &Value) -> &Vec<Value> {
    el["rules"].as_array().expect("rules")
}

#[test]
fn roll_advantage_terms_are_the_readers_words() {
    let mut el = json!({"rules": [
        {"kind": "roll_advantage", "roll": "skill_check", "skill": "ID_TEST_SKILL_SLEIGHT_OF_HAND"},
        {"kind": "roll_advantage", "roll": "saves", "ability": "con"},
        {"kind": "roll_advantage", "roll": "initiative"}
    ]});
    let notes = super::super::rules::normalise_resources(&mut el);
    assert_eq!(notes.len(), 4, "{notes:?}");
    assert_eq!(
        rules(&el)[0],
        json!({"kind": "roll_advantage", "roll": "ability_check", "skill": "sleight of hand"})
    );
    assert_eq!(
        rules(&el)[1],
        json!({"kind": "roll_advantage", "roll": "saving_throw", "ability": "constitution"})
    );
    assert!(
        super::super::rules::normalise_resources(&mut el).is_empty(),
        "idempotent"
    );
}

#[test]
fn conditions_named_by_id_become_condition_names() {
    let mut el = json!({"rules": [
        {"kind": "condition_immunity", "conditions": ["ID_TEST_CONDITION_FRIGHTENED", "charmed"]}
    ]});
    let notes = super::super::rules::normalise_abilities(&mut el);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        rules(&el)[0]["conditions"],
        json!(["frightened", "charmed"])
    );
}

#[test]
fn a_stated_choice_written_as_a_fixed_stat_becomes_an_ability_increase() {
    let mut el = json!({
        "description": "<p>While you wear this band, your Strength or Dexterity score increases by 1.</p>",
        "rules": [{"kind": "stat", "name": "strength", "value": "1"},
                  {"kind": "stat", "name": "speed", "value": "5"}]
    });
    let notes = super::super::rules::normalise_resources(&mut el);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        rules(&el)[0],
        json!({"kind": "ability_increase", "name": "Ability Score Increase",
            "options": [{"abilities": ["strength", "dexterity"], "pick": 1, "amount": 1}],
            "cap": 20})
    );
    assert_eq!(rules(&el)[1]["name"], "speed", "other rules stay");
    assert!(
        super::super::rules::normalise_resources(&mut el).is_empty(),
        "idempotent"
    );
}

#[test]
fn any_ability_choice_keeps_the_stated_maximum_and_the_fixed_part() {
    let mut el = json!({
        "description": "Your Wisdom score increases by 2. Increase one ability score of your choice by 1, to a maximum of 22.",
        "rules": [{"kind": "stat", "name": "wisdom", "value": "2"},
                  {"kind": "stat", "name": "charisma", "value": 1}]
    });
    assert_eq!(ability_choice(&mut el).len(), 1);
    assert_eq!(
        rules(&el)[0],
        json!({"kind": "stat", "name": "wisdom", "value": "2"})
    );
    assert_eq!(
        rules(&el)[1],
        json!({"kind": "ability_increase", "name": "Ability Score Increase",
            "options": [{"abilities": 1, "amount": 1}], "cap": 22})
    );
}

#[test]
fn fixed_increases_are_left_alone() {
    let mut el = json!({
        "description": "Your Strength score increases by 2, and your Constitution score increases by 1.",
        "rules": [{"kind": "stat", "name": "strength", "value": "2"},
                  {"kind": "stat", "name": "constitution", "value": "1"}]
    });
    let before = el.clone();
    assert!(ability_choice(&mut el).is_empty());
    assert_eq!(el, before);
}

fn shell(equipped: &str) -> Value {
    json!({"id": "ID_T_SHELL", "name": "Test Shell", "type": "Magic Item",
        "setters": [{"name": "type", "value": "Armor", "attributes": {"addition": "hide"}},
                    {"name": "attunement", "value": "false"}],
        "rules": [{"kind": "stat", "name": "ac", "value": "2", "equipped": equipped},
                  {"kind": "roll_advantage", "roll": "ability_check", "skill": "stealth",
                   "requirements": "[attuned]"},
                  {"kind": "stat", "name": "speed", "value": "5", "equipped": "[armor:none]"}]})
}

#[test]
fn an_items_self_conditions_are_removed_and_real_ones_kept() {
    for own in ["true", "equipped", "Test Shell", "[item:ID_T_SHELL]"] {
        let mut el = shell(own);
        assert_eq!(item_self_conditions(&mut el).len(), 2, "{own}");
        assert!(rules(&el)[0].get("equipped").is_none(), "{own}");
        assert!(rules(&el)[1].get("requirements").is_none(), "{own}");
        assert_eq!(rules(&el)[2]["equipped"], "[armor:none]");
    }
    let mut feature = shell("true");
    feature["type"] = json!("Class Feature");
    assert!(item_self_conditions(&mut feature).is_empty(), "items only");
}

#[test]
fn magic_armor_plus_two_ac_with_a_loose_condition_becomes_its_enhancement() {
    // finalize order: the item fixes (attunement_by), then the armor enhancement rewrite
    let mut el = shell("true");
    super::super::mechanics::attunement_by(&mut el);
    assert!(super::super::items::armor_enhancement(&mut el).is_some());
    assert_eq!(
        rules(&el)[0],
        json!({"kind": "stat", "name": "ac:armored:enhancement", "value": "2", "bonus": "enhancement"})
    );
}

#[test]
fn a_choice_of_several_abilities_counts_them() {
    let mut el = json!({
        "description": "Your Charisma score increases by 2, and two other ability scores of your choice increase by 1.",
        "rules": [{"kind": "stat", "name": "charisma", "value": "2"},
                  {"kind": "stat", "name": "dexterity", "value": "1"},
                  {"kind": "stat", "name": "constitution", "value": "1"}]
    });
    assert_eq!(ability_choice(&mut el).len(), 1);
    assert_eq!(rules(&el).len(), 2);
    assert_eq!(
        rules(&el)[1]["options"],
        json!([{"abilities": 2, "amount": 1}])
    );
}
