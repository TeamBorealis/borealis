mod common;

use borealis_assist::advisor::{level_up_advisor, AdvisorOptions};
use borealis_assist::options::{apply_option, legal_options, OptionAction};
use borealis_assist::summary::effects;
use borealis_assist::ProseSource;
use borealis_engine::commands::{self, Improvement};
use common::{archetype_id, engine, fighter, FakeRunner, GRAPPLER};
use serde_json::json;

const ASI_STR: &str = "main:asi:plus2:strength";

#[test]
fn legal_options_come_from_the_engine() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let opts = legal_options(engine(), &state).unwrap();
    let ids: Vec<&str> = opts.iter().map(|o| o.id.as_str()).collect();
    assert!(ids.contains(&"main:level"), "{ids:?}");
    assert!(ids.contains(&ASI_STR), "{ids:?}");
    assert!(ids.contains(&"main:asi:plus1:strength,dexterity"));
    // a feat whose prerequisite is met is offered, ids are unique
    assert!(ids.contains(&format!("main:feat:{GRAPPLER}").as_str()));
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

#[test]
fn feat_with_unmet_prerequisite_is_not_offered() {
    let state = fighter(3, [10, 12, 14, 10, 10, 10]);
    let ids: Vec<String> = legal_options(engine(), &state)
        .unwrap()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(!ids.iter().any(|i| i.contains("GRAPPLER")), "{ids:?}");
}

#[test]
fn ability_at_its_maximum_is_not_offered() {
    let state = fighter(3, [20, 12, 14, 10, 10, 10]);
    let ids: Vec<String> = legal_options(engine(), &state)
        .unwrap()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(!ids.contains(&ASI_STR.to_string()), "{ids:?}");
    assert!(ids.contains(&"main:asi:plus2:dexterity".to_string()));
}

#[test]
fn second_level_offers_the_archetype_choice() {
    let state = fighter(2, [15, 12, 14, 10, 10, 10]);
    let id = archetype_id();
    let ids: Vec<String> = legal_options(engine(), &state)
        .unwrap()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(ids.iter().any(|i| i.ends_with(&id)), "{ids:?}");
}

#[tokio::test]
async fn nonexistent_option_ids_are_dropped_and_deltas_match_the_engine() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let first = json!({
        "picks": [
            {"option_id": "main:feat:ID_MADE_UP", "rationale": "Great for you."},
            {"option_id": ASI_STR, "rationale": "More strength makes your attacks hit harder."},
        ],
        "summary": "Strength first."
    })
    .to_string();
    // still contains the made-up id after the single repair round
    let runner = FakeRunner::new(&[&first, &first]);

    let advice = level_up_advisor(
        &runner,
        engine(),
        &state,
        "a strong melee fighter",
        &AdvisorOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(runner.calls(), 2, "exactly one repair round");
    assert!(runner.prompt(1).contains("is not in <options>"));
    assert_eq!(advice.picks.len(), 1);
    assert_eq!(advice.dropped.len(), 1);
    assert_eq!(advice.dropped[0].option_id, "main:feat:ID_MADE_UP");
    let pick = &advice.picks[0];
    assert_eq!(pick.option.id, ASI_STR);
    assert_eq!(pick.rationale_source, ProseSource::Model);

    // independent computation with the engine commands
    let before = engine().derive(&state);
    let after = commands::choose_improvement(
        engine(),
        &commands::level_up_main(engine(), &state, None)
            .unwrap()
            .state,
        "Fighter",
        4,
        &Improvement::PlusTwo(0),
    )
    .unwrap()
    .state;
    let expected = effects(&before, &engine().derive(&after));
    assert_eq!(pick.effects, expected);
    let str_delta = pick
        .effects
        .stats
        .iter()
        .find(|d| d.stat.ends_with("score") && d.stat.to_lowercase().contains("strength"))
        .unwrap();
    assert_eq!(
        (str_delta.before, str_delta.after, str_delta.change),
        (15, 17, 2)
    );
    let lvl = pick
        .effects
        .stats
        .iter()
        .find(|d| d.stat == "character level")
        .unwrap();
    assert_eq!((lvl.before, lvl.after), (3, 4));
}

#[tokio::test]
async fn repair_round_can_fix_the_ids() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let bad = json!({"picks": [{"option_id": "nope", "rationale": "x"}]}).to_string();
    let good = json!({"picks": [{"option_id": "main:asi:plus2:dexterity", "rationale": "Better defense."}]}).to_string();
    let runner = FakeRunner::new(&[&bad, &good]);
    let advice = level_up_advisor(
        &runner,
        engine(),
        &state,
        "tanky",
        &AdvisorOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(advice.attempts, 2);
    assert!(advice.dropped.is_empty());
    assert_eq!(advice.picks[0].option.id, "main:asi:plus2:dexterity");
}

#[tokio::test]
async fn invented_numbers_in_a_rationale_are_replaced_by_engine_effects() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let reply = json!({
        "picks": [{"option_id": ASI_STR, "rationale": "This gives +9 to hit and 42 extra hit points."}],
        "summary": "Take it for 31 extra damage."
    })
    .to_string();
    let runner = FakeRunner::new(&[&reply]);
    let advice = level_up_advisor(
        &runner,
        engine(),
        &state,
        "damage",
        &AdvisorOptions::default(),
    )
    .await
    .unwrap();
    let pick = &advice.picks[0];
    assert_eq!(pick.rationale_source, ProseSource::Template);
    assert!(!pick.rationale.contains("42") && !pick.rationale.contains("+9"));
    assert!(
        pick.rationale.contains("strength score 15 -> 17"),
        "{}",
        pick.rationale
    );
    assert!(advice.summary.is_none());
}

#[tokio::test]
async fn top_n_limits_picks_and_duplicates_collapse() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let reply = json!({"picks": [
        {"option_id": ASI_STR, "rationale": "a"},
        {"option_id": ASI_STR, "rationale": "again"},
        {"option_id": "main:asi:plus2:dexterity", "rationale": "b"},
        {"option_id": "main:asi:plus2:constitution", "rationale": "c"},
    ]})
    .to_string();
    let runner = FakeRunner::new(&[&reply]);
    let opts = AdvisorOptions {
        top_n: 2,
        ..AdvisorOptions::default()
    };
    let advice = level_up_advisor(&runner, engine(), &state, "x", &opts)
        .await
        .unwrap();
    let ids: Vec<&str> = advice.picks.iter().map(|p| p.option.id.as_str()).collect();
    assert_eq!(ids, [ASI_STR, "main:asi:plus2:dexterity"]);
    assert_eq!(advice.picks[1].rank, 2);
}

#[test]
fn applying_an_option_twice_gives_the_same_state() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let opt = legal_options(engine(), &state)
        .unwrap()
        .into_iter()
        .find(|o| o.id == ASI_STR)
        .unwrap();
    assert!(matches!(opt.action, OptionAction::Improve { .. }));
    let a = apply_option(engine(), &state, &opt).unwrap();
    let b = apply_option(engine(), &state, &opt).unwrap();
    assert_eq!(a.state, b.state);
    assert_eq!(state.level(), 3, "the original is untouched");
}

#[tokio::test]
async fn guessed_multiclass_element_id_is_dropped_and_prompt_lists_valid_ids() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let reply = json!({
        "picks": [
            {"option_id": "ID_ZZ_MULTICLASS_NOPE", "rationale": "Rage."},
            {"option_id": "new:ID_ZZ_MULTICLASS_NOPE:level", "rationale": "Rage."},
            {"option_id": ASI_STR, "rationale": "More strength helps your attacks."},
        ]
    })
    .to_string();
    let runner = FakeRunner::new(&[&reply, &reply]);
    let advice = level_up_advisor(
        &runner,
        engine(),
        &state,
        "multiclass into a barbarian",
        &AdvisorOptions::default(),
    )
    .await
    .unwrap();
    let legal = legal_options(engine(), &state).unwrap();
    assert!(runner
        .prompt(0)
        .contains(&format!("exactly {} valid ids", legal.len())));
    assert!(runner.prompt(0).contains("main:level"));
    assert_eq!(
        advice.picks.len(),
        1,
        "{:?}",
        advice
            .picks
            .iter()
            .map(|p| &p.option.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(advice.picks[0].option.id, ASI_STR);
    let dropped: Vec<&str> = advice
        .dropped
        .iter()
        .map(|d| d.option_id.as_str())
        .collect();
    assert_eq!(dropped.len(), 2, "{dropped:?}");
    assert!(dropped.iter().all(|d| d.contains("MULTICLASS_NOPE")));
}
