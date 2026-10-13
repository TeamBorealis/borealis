mod common;

use borealis_assist::plan::{apply_plan, replay_step, Plan, PlanChoice, PlanStep};
use borealis_assist::planner::{build_planner, PlannerOptions};
use borealis_assist::{AssistError, LevelPath};
use common::{archetype_id, engine, fighter, FakeRunner, GRAPPLER};
use serde_json::json;

fn main_step(level: u32) -> serde_json::Value {
    json!({"level": level, "path": {"kind": "main"}})
}

fn plan_reply(grappler_at_4: bool) -> String {
    let arch = archetype_id();
    let mut l2 = main_step(2);
    l2["rationale"] = json!("Action Surge arrives and you gain +55 to hit.");
    let mut l3 = main_step(3);
    l3["choices"] = json!([{"slot": "Martial Archetype", "element": arch}]);
    l3["rationale"] = json!("Pick the subclass.");
    let mut l4 = main_step(4);
    l4["improvement"] = if grappler_at_4 {
        json!({"kind": "feat", "feat": GRAPPLER})
    } else {
        json!({"kind": "plus_two", "ability": "strength"})
    };
    json!({"steps": [l2, l3, l4], "notes": "Straightforward melee build."}).to_string()
}

fn fixed_l4_reply() -> String {
    let mut l4 = main_step(4);
    l4["improvement"] = json!({"kind": "plus_two", "ability": "strength"});
    json!({"steps": [l4]}).to_string()
}

#[tokio::test]
async fn illegal_feat_step_is_repaired_with_engine_feedback() {
    // STR 12: the Grappler prerequisite (STR 13) is not met at level 4.
    let state = fighter(1, [12, 12, 14, 10, 10, 10]);
    let runner = FakeRunner::new(&[&plan_reply(true), &fixed_l4_reply()]);

    let plan = build_planner(
        &runner,
        engine(),
        &state,
        4,
        "a sturdy melee fighter",
        &PlannerOptions::default(),
    )
    .await
    .unwrap();

    assert!(plan.complete, "{:?}", plan.unresolved);
    assert_eq!(plan.repair_rounds, 1);
    assert_eq!(runner.calls(), 2);
    let repair_prompt = runner.prompt(1);
    assert!(
        repair_prompt.contains("not an allowed choice"),
        "{repair_prompt}"
    );
    assert!(repair_prompt.contains("previous_plan"));
    assert!(repair_prompt.contains("Plan levels 4 to 4"));
    assert_eq!(plan.levels.len(), 3);
    assert!(plan.unresolved.is_empty());

    // numbers in the model's rationale that the engine did not compute are removed
    assert!(plan.levels[0].rationale_redacted);
    assert!(plan.levels[0].step.rationale.is_empty());
    assert_eq!(plan.levels[1].step.rationale, "Pick the subclass.");

    // the per-level digest comes from the engine
    let last = &plan.levels[2].snapshot;
    assert_eq!(last.level, 4);
    assert_eq!(last.abilities[0], ("strength".to_string(), 14));
    assert_eq!(last.proficiency_bonus, 2);
    assert!(plan.levels[1].snapshot.classes[0].contains("Fighter 3"));

    // plans are data: apply up to level N
    let data: Plan = serde_json::from_str(&serde_json::to_string(&plan.plan()).unwrap()).unwrap();
    let at3 = apply_plan(engine(), &state, &data, 3).unwrap();
    assert_eq!(at3.level(), 3);
    let at4 = apply_plan(engine(), &state, &data, 4).unwrap();
    assert_eq!(engine().derive(&at4).abilities[0].score, 14);
    assert_eq!(state.level(), 1);
}

#[tokio::test]
async fn legal_feat_is_accepted_without_repair() {
    let state = fighter(1, [13, 12, 14, 10, 10, 10]);
    let runner = FakeRunner::new(&[&plan_reply(true)]);
    let plan = build_planner(
        &runner,
        engine(),
        &state,
        4,
        "grappler",
        &PlannerOptions::default(),
    )
    .await
    .unwrap();
    assert!(plan.complete);
    assert_eq!(plan.repair_rounds, 0);
    assert_eq!(runner.calls(), 1);
    let applied = apply_plan(engine(), &state, &plan.plan(), 4).unwrap();
    assert!(engine().derive(&applied).has_element(GRAPPLER));
}

#[tokio::test]
async fn unrepairable_steps_stay_unresolved_after_two_rounds() {
    let state = fighter(1, [12, 12, 14, 10, 10, 10]);
    let bad = plan_reply(true);
    let still_bad = {
        let mut l4 = main_step(4);
        l4["improvement"] = json!({"kind": "feat", "feat": GRAPPLER});
        json!({"steps": [l4]}).to_string()
    };
    let runner = FakeRunner::new(&[&bad, &still_bad, &still_bad]);

    let plan = build_planner(
        &runner,
        engine(),
        &state,
        6,
        "x",
        &PlannerOptions::default(),
    )
    .await
    .unwrap();

    assert_eq!(runner.calls(), 3, "one proposal plus two repair rounds");
    assert!(!plan.complete);
    assert_eq!(plan.repair_rounds, 2);
    assert_eq!(plan.validated_through(), 3);
    assert_eq!(plan.plan().steps.len(), 2);
    let levels: Vec<u32> = plan.unresolved.iter().map(|u| u.level).collect();
    assert_eq!(levels, [4, 5, 6]);
    assert!(plan.unresolved[0].reasons[0].contains("not an allowed choice"));
    assert!(plan.unresolved[0].proposed.is_some());
    assert_eq!(plan.unresolved[1].reasons, ["blocked by level 4"]);
    // what is validated can still be applied
    assert_eq!(
        apply_plan(engine(), &state, &plan.plan(), 20)
            .unwrap()
            .level(),
        3
    );
}

#[tokio::test]
async fn skipped_required_choices_are_reported_not_invented() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let reply = json!({"steps": [main_step(2), main_step(3)]}).to_string();
    let runner = FakeRunner::new(&[&reply]);
    let plan = build_planner(
        &runner,
        engine(),
        &state,
        3,
        "x",
        &PlannerOptions::default(),
    )
    .await
    .unwrap();
    assert!(plan.complete);
    let open: Vec<&str> = plan.levels[1]
        .open_choices
        .iter()
        .map(|o| o.slot.as_str())
        .collect();
    assert_eq!(open, ["Martial Archetype"]);
}

#[tokio::test]
async fn a_plan_that_stops_early_is_sent_back() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let short = json!({"steps": [main_step(2)]}).to_string();
    let rest = json!({"steps": [main_step(3)]}).to_string();
    let runner = FakeRunner::new(&[&short, &rest]);
    let plan = build_planner(
        &runner,
        engine(),
        &state,
        3,
        "x",
        &PlannerOptions::default(),
    )
    .await
    .unwrap();
    assert!(plan.complete);
    assert!(runner.prompt(1).contains("the plan stops at level 2"));
    assert!(runner.prompt(1).contains("Plan levels 3 to 3"));
}

#[tokio::test]
async fn target_must_be_above_the_current_level() {
    let state = fighter(3, [15, 12, 14, 10, 10, 10]);
    let runner = FakeRunner::new(&[]);
    for target in [3, 2, 21] {
        let err = build_planner(
            &runner,
            engine(),
            &state,
            target,
            "x",
            &PlannerOptions::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AssistError::Invalid(_)));
    }
    assert_eq!(runner.calls(), 0);
}

#[test]
fn replay_names_the_open_slots_for_an_unknown_slot() {
    let state = fighter(1, [15, 12, 14, 10, 10, 10]);
    let step = PlanStep {
        level: 2,
        path: LevelPath::Main,
        choices: vec![PlanChoice {
            slot: "Imaginary Slot".into(),
            ordinal: None,
            element: "ID_X".into(),
        }],
        improvement: None,
        rationale: String::new(),
    };
    let issues = replay_step(engine(), &state, &step).unwrap_err();
    assert!(
        issues[0].contains("Imaginary Slot") && issues[0].contains("Fighting Style"),
        "{issues:?}"
    );
}

#[test]
fn replay_rejects_wrong_level_numbers_and_wrong_elements() {
    let state = fighter(2, [15, 12, 14, 10, 10, 10]);
    let wrong_level = PlanStep {
        level: 5,
        path: LevelPath::Main,
        choices: vec![],
        improvement: None,
        rationale: String::new(),
    };
    assert!(replay_step(engine(), &state, &wrong_level).unwrap_err()[0].contains("next level is 3"));
    let wrong_element = PlanStep {
        level: 3,
        path: LevelPath::Main,
        choices: vec![PlanChoice {
            slot: "Martial Archetype".into(),
            ordinal: None,
            element: "ID_NOT_AN_ARCHETYPE".into(),
        }],
        improvement: None,
        rationale: String::new(),
    };
    let issues = replay_step(engine(), &state, &wrong_element).unwrap_err();
    assert!(
        issues[0].contains("not a legal choice") && issues[0].contains(&archetype_id()),
        "{issues:?}"
    );
}
