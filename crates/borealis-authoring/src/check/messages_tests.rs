//! Every code has a plain sentence (spec 8.2, 18.1 "Messages"). The variants are read from the enums' source, so a
//! code added without a sample here fails the test (and one added without a sentence fails to compile).

use super::*;
use crate::check::balance::BalanceFlag;
use crate::form::{NoLookup, RefInfo};
use borealis_content::issue::Severity;
use std::collections::HashMap;

/// The variant names of `pub enum <name>` in `src` (lines indented by four spaces inside the enum).
fn variants_of(src: &str, name: &str) -> Vec<String> {
    let start = src
        .find(&format!("pub enum {name} {{"))
        .unwrap_or_else(|| panic!("enum {name} not found"));
    src[start..]
        .lines()
        .skip(1)
        .take_while(|l| !l.starts_with('}'))
        .filter_map(|l| {
            let rest = l.strip_prefix("    ")?;
            let first = rest.chars().next()?;
            if !first.is_ascii_uppercase() {
                return None;
            }
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            Some(rest[..end].to_string())
        })
        .collect()
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// No ids, no code, no field keys; a sentence.
fn assert_plain(message: &str, what: &str) {
    assert!(!message.trim().is_empty(), "{what}: empty");
    assert!(!message.contains("ID_"), "{what}: an id in {message:?}");
    for bad in ['`', '{', '}', '[', ']'] {
        assert!(!message.contains(bad), "{what}: {bad:?} in {message:?}");
    }
    let snake_word = message
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|w| w.contains('_') && w.chars().any(char::is_alphabetic));
    assert!(!snake_word, "{what}: a key in {message:?}");
    assert!(
        message.ends_with('.'),
        "{what}: not a sentence: {message:?}"
    );
    let first = message.chars().next().unwrap_or(' ');
    assert!(
        first.is_uppercase() || first == '"',
        "{what}: {message:?} starts in lower case"
    );
}

fn names() -> HashMap<String, RefInfo> {
    [("ID_X_FEAT_SHIELD_MASTER", "Shield Master", "Feat")]
        .into_iter()
        .map(|(id, n, t)| {
            (
                id.to_string(),
                RefInfo {
                    name: n.into(),
                    ty: t.into(),
                },
            )
        })
        .collect()
}

#[test]
fn every_issue_code_has_a_plain_sentence() {
    let src = include_str!("../../../borealis-content/src/issue.rs");
    let variants = variants_of(src, "IssueCode");
    assert!(variants.len() >= 26, "{variants:?}");
    for v in variants {
        let code: IssueCode = serde_json::from_value(serde_json::json!(snake(&v)))
            .unwrap_or_else(|e| panic!("{v}: {e}"));
        for raw in [
            "raw validator text",
            "Spell element requires setter \"school\"",
            "grant references unknown element ID_X_FEAT_SHIELD_MASTER",
            "requirements: bad",
        ] {
            let issue = Issue::new(Severity::Error, code, raw);
            for ty in ["Spell", "Monster", ""] {
                assert_plain(&issue_message(&issue, ty, &names()), &v);
            }
        }
        assert_eq!(issue_code(code), snake(&v));
    }
}

#[test]
fn the_spec_examples_read_as_written() {
    let n = names();
    let issue = |code, m: &str| Issue::new(Severity::Error, code, m);
    assert_eq!(
        issue_message(
            &issue(
                IssueCode::MissingRequiredSetter,
                "Spell element requires setter \"school\""
            ),
            "Spell",
            &n
        ),
        "A spell needs a school."
    );
    assert_eq!(
        issue_message(
            &issue(
                IssueCode::DanglingReference,
                "grant references unknown element ID_X_FEAT_SHIELD_MASTER"
            ),
            "Feat",
            &n
        ),
        "\"Shield Master\" isn't loaded any more (its pack was removed or turned off). Pick it again."
    );
    assert_eq!(
        issue_message(&issue(IssueCode::DuplicateSelectName, "x"), "Class", &n),
        "Two choices on this element have the same label. Give one a different label."
    );
    assert_eq!(
        issue_message(
            &issue(IssueCode::InvalidExpression, "requirements: x at 3"),
            "Feat",
            &n
        ),
        "This condition, kept from the original, can't be read. Rebuild it with the condition builder."
    );
    assert_eq!(
        rule_problem_message(&RuleProblem::UnknownKind("x".into())),
        "Kept from the original: Borealis doesn't use this rule."
    );
    let sweep = SweepFinding {
        level: 7,
        element_id: None,
        problem: SweepProblem::EmptySelect {
            name: "Arcane Shot".into(),
            ty: "Option".into(),
        },
    };
    assert_eq!(
        sweep_message(&sweep, "Class", &n),
        "At level 7 the choice \"Arcane Shot\" has nothing to offer."
    );
    assert_eq!(
        refusal_message("Species", "the species slot is taken", &n),
        "A test character couldn't take this species: the species slot is taken."
    );
}

#[test]
fn a_dangling_reference_without_a_known_name_says_so_plainly() {
    let m = issue_message(
        &Issue::warning(
            IssueCode::DanglingReference,
            "grant references unknown element ID_GONE",
        ),
        "Feat",
        &NoLookup,
    );
    assert!(m.starts_with("Something this element uses"), "{m}");
}

#[test]
fn every_rule_problem_has_a_plain_sentence() {
    let src = include_str!("../../../borealis-content/src/rule_kinds.rs");
    let samples = [
        RuleProblem::UnknownKind("select.ability_distribution".into()),
        RuleProblem::UnknownField {
            kind: "spell_free_cast".into(),
            field: "selection_mode".into(),
        },
    ];
    assert_eq!(variants_of(src, "RuleProblem").len(), samples.len());
    for p in &samples {
        assert_plain(&rule_problem_message(p), p.code());
    }
}

fn sweep_samples() -> Vec<SweepProblem> {
    vec![
        SweepProblem::Store(Issue::error(IssueCode::InvalidId, "x")),
        SweepProblem::NoClassSlot,
        SweepProblem::ClassRefused("element ID_X_FEAT_SHIELD_MASTER is not a class".into()),
        SweepProblem::LevelUpFails("no level slot".into()),
        SweepProblem::SubclassRefused("`ID_GONE` is not offered".into()),
        SweepProblem::SubclassNeverOffered,
        SweepProblem::Engine("stat out of range: ID_GONE".into()),
        SweepProblem::BrokenSelection("InvalidSelection { key: .. }".into()),
        SweepProblem::EmptySelect {
            name: "Fighting Style (Fighter)".into(),
            ty: "Class Feature".into(),
        },
        SweepProblem::MissingFeature("ID_X_FEAT_SHIELD_MASTER".into()),
        SweepProblem::NoSpellcasting,
        SweepProblem::SlotsShrink(5),
        SweepProblem::SlotsFlat,
        SweepProblem::NoSlots,
    ]
}

#[test]
fn every_sweep_finding_has_a_plain_sentence_and_its_own_code() {
    let src = include_str!("sweep.rs");
    let samples = sweep_samples();
    assert_eq!(variants_of(src, "SweepProblem").len(), samples.len());
    let mut codes = std::collections::HashSet::new();
    for p in samples {
        let f = SweepFinding {
            level: 3,
            element_id: None,
            problem: p,
        };
        let m = sweep_message(&f, "Class", &names());
        assert_plain(&m, &format!("{:?}", f.problem));
        assert!(codes.insert(sweep_code(&f.problem)), "{:?}", f.problem);
    }
}

#[test]
fn engine_reasons_show_names_not_ids() {
    let m = refusal_message(
        "Feat",
        "prerequisite of ID_X_FEAT_SHIELD_MASTER not met; `x`",
        &names(),
    );
    assert_eq!(
        m,
        "A test character couldn't take this feat: prerequisite of \"Shield Master\" not met; x."
    );
    let m = refusal_message("Feat", "ID_GONE refused.", &NoLookup);
    assert_eq!(
        m,
        "A test character couldn't take this feat: an element that isn't loaded refused."
    );
    assert_plain(&refusal_message("Item", "", &NoLookup), "empty reason");
}

fn finding_samples() -> Vec<Finding> {
    let b = || "Gives a spell".to_string();
    vec![
        Finding::NoName,
        Finding::RequiredField {
            subject: "a deity".into(),
            field: "Alignment".into(),
        },
        Finding::BlockRequired {
            block: b(),
            field: "Spell".into(),
        },
        Finding::BlockRange {
            block: b(),
            field: "From level".into(),
            min: 1,
            max: 20,
        },
        Finding::BlockDice {
            block: b(),
            field: "Damage".into(),
        },
        Finding::BlockChoice {
            block: b(),
            field: "How it is cast".into(),
        },
        Finding::BlockShape,
        Finding::DuplicateName {
            noun: "spell".into(),
            name: "Fire Bolt".into(),
        },
        Finding::EmptyDescription,
        Finding::Escape,
        Finding::SelectOffersNothing,
        Finding::SpellListEmpty,
        Finding::UnknownPool {
            pool: "ember points".into(),
        },
        Finding::NotOffered {
            noun: "subrace".into(),
        },
        Finding::UnknownSpellList {
            list: "Wizzard".into(),
            near: Some("Wizard".into()),
        },
        Finding::OnNoSpellList,
        Finding::GroupNotOffered {
            group: "Eldrich Invocation".into(),
            near: Some("Eldritch Invocation".into()),
        },
        Finding::TypeNotHere {
            ty: "Spell".into(),
            made_with: Some("New element > Spell".into()),
        },
    ]
}

#[test]
fn every_creator_finding_has_a_plain_sentence() {
    let src = include_str!("messages.rs");
    let samples = finding_samples();
    assert_eq!(variants_of(src, "Finding").len(), samples.len());
    for f in &samples {
        assert_plain(&f.message(), f.code());
        assert!(f.code().contains('.'), "{}", f.code());
    }
    assert_eq!(
        finding_samples()[1].message(),
        "A deity needs an alignment."
    );
    assert_eq!(
        finding_samples()[2].message(),
        "Gives a spell: fill in spell."
    );
}

#[test]
fn every_balance_metric_has_a_sentence_both_ways() {
    let src = include_str!("balance/report.rs");
    let emitted: Vec<&str> = src
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            s.split_once('.').is_some_and(|(a, b)| {
                ["spell", "feat", "item", "species", "monster"].contains(&a)
                    && !b.is_empty()
                    && b.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
        })
        .collect();
    assert!(emitted.len() >= 12, "{emitted:?}");
    for m in &emitted {
        assert!(
            BALANCE_METRICS.iter().any(|(x, ..)| x == m),
            "{m} has no sentence"
        );
    }
    for (metric, ..) in BALANCE_METRICS {
        for (value, severity) in [
            (99.0, BalanceSeverity::Medium),
            (999.0, BalanceSeverity::High),
            (0.0, BalanceSeverity::Medium),
        ] {
            let flag = BalanceFlag {
                metric: metric.into(),
                value,
                baseline_range: [10.0, 20.0],
                baseline_median: 15.0,
                severity,
                note: String::new(),
            };
            let m = balance_message(&flag).expect("flagged");
            assert_plain(&m, metric);
            assert!(m.starts_with("Compared with "), "{m}");
        }
    }
    let ok = BalanceFlag {
        metric: "feat.asi_total".into(),
        value: 1.0,
        baseline_range: [0.0, 2.0],
        baseline_median: 1.0,
        severity: BalanceSeverity::Low,
        note: String::new(),
    };
    assert_eq!(balance_message(&ok), None);
    let high = BalanceFlag {
        severity: BalanceSeverity::High,
        value: 9.0,
        ..ok
    };
    assert_eq!(
        balance_message(&high).as_deref(),
        Some("Compared with SRD feats: grants far more ability score increases than any.")
    );
}
