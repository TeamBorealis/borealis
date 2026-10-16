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

#[test]
fn choice_wordings_without_score_or_increase_become_ability_increases() {
    for text in [
        "<p>While you wear it, you gain +1 to your Str or Dex score.</p>",
        "<p>Increase your Strength or Dexterity by 1.</p>",
        "<p>You gain a +1 bonus to Strength, Dexterity, or Constitution.</p>",
    ] {
        let mut el = json!({"description": text,
            "rules": [{"kind": "stat", "name": "dexterity", "value": "1"}]});
        let notes = super::super::rules::normalise_abilities(&mut el);
        assert_eq!(notes.len(), 1, "{text}: {notes:?}");
        assert_eq!(rules(&el)[0]["kind"], "ability_increase", "{text}");
        assert_eq!(rules(&el)[0]["options"][0]["pick"], 1, "{text}");
    }
    // A bonus to rolls of the abilities is no score choice.
    let mut saves = json!({"description":
        "<p>You gain a +1 bonus to Strength or Dexterity saving throws.</p>",
        "rules": [{"kind": "stat", "name": "strength", "value": "1"}]});
    assert!(super::super::rules::normalise_abilities(&mut saves).is_empty());
}

/// A wondrous item worn by a fighter, its rules normalised as finalize does.
fn worn_item(tag: &str, text: &str, rules: Value) -> Value {
    let mut el = json!({"id": "ID_T_MAGIC_ITEM_OAK_GIRDLE", "name": "Oak Girdle",
        "type": "Magic Item", "source": "Test", "ruleset": tag,
        "description": text, "rules": rules, "setters": [
        {"name": "category", "value": "Wondrous Items"},
        {"name": "type", "value": "Wondrous item"}, {"name": "rarity", "value": "Rare"}]});
    super::super::rules::normalise_abilities(&mut el);
    el
}

#[test]
fn a_fixed_increase_with_a_stated_maximum_passes_twenty_up_to_it_in_both_editions() {
    use borealis_content::Ruleset;
    let strength = |el: &Value, ruleset: Ruleset| {
        let snap = super::super::items::tests::fighter_sheet(
            ruleset,
            std::slice::from_ref(el),
            &["ID_T_MAGIC_ITEM_OAK_GIRDLE"],
            [18, 10, 14, 10, 14, 8],
        );
        snap.abilities[0].score
    };
    let fixed = json!([{"kind": "stat", "name": "strength", "value": "4"}]);
    for (ruleset, tag) in [(Ruleset::Dnd2014, "dnd2014"), (Ruleset::Dnd2024, "dnd2024")] {
        let capped = worn_item(
            tag,
            "<p>While you wear this girdle, your Strength score increases by 4, to a maximum of 21.</p>",
            fixed.clone(),
        );
        assert_eq!(
            rules(&capped).as_slice(),
            [
                json!({"kind": "stat", "name": "strength:boon", "value": "4"}),
                json!({"kind": "stat", "name": "strength:boon:cap", "value": "21", "bonus": "base"})
            ],
            "{tag}"
        );
        assert_eq!(
            strength(&capped, ruleset),
            21,
            "{tag}: 18 + 4, stopped at 21"
        );
        let raised = worn_item(
            tag,
            "<p>Your Strength score increases by 4, as does your maximum for that score.</p>",
            fixed.clone(),
        );
        assert_eq!(rules(&raised)[1]["name"], "strength:max", "{tag}");
        assert_eq!(
            strength(&raised, ruleset),
            22,
            "{tag}: the maximum rose to 24"
        );
        // The ordinary maximum stays the plain stat (and the ordinary cap of 20).
        let plain = worn_item(
            tag,
            "<p>Your Strength score increases by 4, to a maximum of 20.</p>",
            fixed.clone(),
        );
        assert_eq!(
            rules(&plain).as_slice(),
            fixed.as_array().unwrap().as_slice()
        );
        assert_eq!(strength(&plain, ruleset), 20, "{tag}");
    }
    // Idempotent, and a stated choice is left to the choice reading.
    let mut again = worn_item(
        "dnd2024",
        "<p>Your Strength score increases by 2, to a maximum of 22.</p>",
        json!([{"kind": "stat", "name": "strength", "value": "2"}]),
    );
    assert!(super::super::rules::normalise_abilities(&mut again).is_empty());
    let choice = worn_item(
        "dnd2024",
        "<p>Your Strength or Constitution score increases by 2, to a maximum of 22.</p>",
        json!([{"kind": "stat", "name": "strength", "value": "2"}]),
    );
    assert_eq!(rules(&choice)[0]["kind"], "ability_increase");
    assert_eq!(rules(&choice)[0]["cap"], 22);
}
