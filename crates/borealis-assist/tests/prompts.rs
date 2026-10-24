//! Prompt templates are snapshot-stable: a change to any prompt shows up as a diff of the files in
//! `tests/snapshots/`. Regenerate with `BOREALIS_UPDATE_SNAPSHOTS=1 cargo test -p borealis-assist`.

use std::path::PathBuf;

use borealis_ai::task::AiTask;
use borealis_assist::advisor::AdviseLevelUp;
use borealis_assist::explain::{render_trace, ExplainNumber};
use borealis_assist::options::{ImprovementSpec, LevelPath, LevelUpOption, OptionAction};
use borealis_engine::{StatTrace, StepKind, TraceStep};

fn check(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("missing snapshot {name}; run with BOREALIS_UPDATE_SNAPSHOTS=1")
    });
    assert_eq!(actual, expected, "prompt snapshot {name} changed");
}

fn render(t: &impl AiTask) -> String {
    format!(
        "== system ==\n{}\n\n== user ==\n{}\n\n== schema ==\n{}\n",
        t.system_prompt(),
        t.user_prompt(),
        serde_json::to_string_pretty(&t.output_schema()).unwrap()
    )
}

fn step(label: &str, value: i32, included: bool, reason: Option<&str>) -> TraceStep {
    TraceStep {
        group: "ac".into(),
        kind: if label == "Base" {
            StepKind::Seed
        } else {
            StepKind::Rule
        },
        label: label.into(),
        element_id: None,
        element_name: Some(label.into()),
        rule_kind: "stat".into(),
        value,
        raw_value: None,
        bonus: None,
        condition: None,
        included,
        reason: reason.map(Into::into),
        inputs: vec![],
    }
}

fn trace() -> StatTrace {
    StatTrace::build(
        "ac",
        vec![
            step("Base", 10, true, None),
            step("Chain Mail", 6, true, None),
            step("Shield", 2, true, None),
            step("Unarmored Defense", 3, false, Some("armor is equipped")),
        ],
        &[],
    )
}

fn options() -> Vec<LevelUpOption> {
    vec![
        LevelUpOption {
            id: "main:level".into(),
            group: "Which class to level".into(),
            label: "Take the next level in Fighter".into(),
            summary: String::new(),
            path: LevelPath::Main,
            action: OptionAction::TakeLevel,
        },
        LevelUpOption {
            id: "main:feat:ID_PHB_FEAT_GRAPPLER".into(),
            group: "Improvement Option (Fighter 4)".into(),
            label: "Feat: Grappler".into(),
            summary: "You have advantage on attack rolls against a creature you are grappling."
                .into(),
            path: LevelPath::Main,
            action: OptionAction::Improve {
                class: "Fighter".into(),
                level: 4,
                what: ImprovementSpec::Feat {
                    feat: "ID_PHB_FEAT_GRAPPLER".into(),
                },
            },
        },
    ]
}

#[test]
fn explain_number_prompt() {
    let t = ExplainNumber::new(trace());
    check("explain_number", &render(&t));
    check("explain_number_trace", &render_trace(&trace()));
}

#[test]
fn level_up_advisor_prompt() {
    let t = AdviseLevelUp::new(
        "level 3: Fighter 3; HP 28, AC 16, proficiency +2, STR 15 DEX 12".into(),
        "a tanky grappler".into(),
        options(),
        3,
    );
    check("level_up_advisor", &render(&t));
}
