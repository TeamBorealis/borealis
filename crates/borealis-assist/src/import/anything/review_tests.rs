use super::*;
use crate::import::anything::types::{
    Detection, ElementCheck, ImportKind, ImportPlan, PlannedElement, SourceExcerpt,
};
use crate::import::anything::{Edition, SweepIssue};
use serde_json::json;

fn element(id: &str, ty: &str, json: Value) -> DraftElement {
    DraftElement {
        id: id.into(),
        name: json["name"].as_str().unwrap_or("Thing").into(),
        ty: ty.into(),
        role: Role::Standalone,
        level: None,
        json,
        status: ElementStatus::Ok,
        issues: Vec::new(),
        excerpt: Some(SourceExcerpt {
            start_line: 1,
            end_line: 2,
            snippet: String::new(),
        }),
        balance: None,
        repair_rounds: 0,
        revisions: 0,
        rule_errors: Vec::new(),
        normalised: Vec::new(),
        checks: Vec::new(),
    }
}

fn feat(id: &str, description: &str, rules: Value) -> DraftElement {
    element(
        id,
        "Feat",
        json!({"id": id, "type": "Feat", "name": "Brisk Walker", "description": description, "rules": rules}),
    )
}

fn draft(source: &str, elements: Vec<DraftElement>) -> ImportDraft {
    let plan_elements = elements
        .iter()
        .map(|e| PlannedElement {
            id: e.id.clone(),
            ty: e.ty.clone(),
            name: e.name.clone(),
            role: e.role,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: e.excerpt.clone(),
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        })
        .collect();
    ImportDraft {
        edition: Edition::Y2024,
        id_prefix: "t".into(),
        source_name: "Test".into(),
        pack_id: "t".into(),
        pack_name: "T".into(),
        source_kind: "text".into(),
        source_text: source.into(),
        detection: Detection {
            kind: ImportKind::Feat,
            edition: Edition::Y2024,
            confidence: 1.0,
            items: Vec::new(),
            notes: Vec::new(),
            other: Vec::new(),
        },
        plan: ImportPlan {
            edition: Edition::Y2024,
            groups: Vec::new(),
            elements: plan_elements,
            notes: Vec::new(),
        },
        elements,
        sweep_issues: Vec::new(),
        source_checks: Vec::new(),
    }
}

const SOURCE: &str = "Brisk Walker\nYou gain proficiency in Athletics. You like long walks.";
const DESCRIPTION: &str = "<p>You gain proficiency in Athletics. You like long walks.</p>";

#[test]
fn model_notes_alone_stay_ok_and_land_in_details() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.status = ElementStatus::NeedsReview;
    e.issues = vec!["The source gives no prerequisite, so I left it out.".into()];
    e.normalised = vec!["resource id normalised".into()];
    let d = draft(SOURCE, vec![e]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.state, ReviewLevel::Ok, "{s:?}");
    assert!(s.reason.is_empty());
    assert_eq!(s.details.len(), 2, "{s:?}");
    assert!(!s.text_only);
}

#[test]
fn no_output_is_not_imported_with_the_failure_as_reason() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.json = Value::Null;
    e.status = ElementStatus::Failed;
    e.issues = vec!["generation failed: the model timed out".into()];
    let d = draft(SOURCE, vec![e]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.state, ReviewLevel::NotImported);
    assert_eq!(s.code, code::FAILED);
    assert!(s.reason.contains("timed out"), "{}", s.reason);
    let mut p = d.clone();
    p.elements[0].status = ElementStatus::Pending;
    assert_eq!(review_state(&p.elements[0], &p).code, code::PENDING);
}

#[test]
fn a_failed_check_leads_over_rule_errors_and_both_are_kept() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.rule_errors = vec!["removed a rule of unknown kind \"teleport\"".into()];
    e.checks = vec![
        ElementCheck {
            code: "heading_covered".into(),
            message: "heading is covered".into(),
            failed: false,
        },
        ElementCheck {
            code: "table_row_missing".into(),
            message: "table row \"3rd\" is missing".into(),
            failed: true,
        },
    ];
    let d = draft(SOURCE, vec![e]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.state, ReviewLevel::Check);
    assert_eq!(s.code, "check:table_row_missing");
    assert_eq!(s.reason, "Table row \"3rd\" is missing");
    assert!(s.details[0].contains("removed"), "{:?}", s.details);
}

#[test]
fn an_engine_failure_with_output_is_check_and_names_the_engine_message() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.status = ElementStatus::Failed;
    e.issues = vec![
        "A model note.".into(),
        "level 3: grant points at nothing".into(),
    ];
    let mut d = draft(SOURCE, vec![e]);
    d.sweep_issues.push(SweepIssue {
        level: 3,
        element_id: Some("ID_T_FEAT_BRISK".into()),
        message: "grant points at nothing".into(),
    });
    let s = review_state(&d.elements[0], &d);
    assert_eq!(
        (s.state, s.code.as_str()),
        (ReviewLevel::Check, code::ENGINE)
    );
    assert!(s.reason.contains("grant points at nothing"), "{}", s.reason);
    assert!(
        s.details.iter().all(|x| !x.starts_with("level 3")),
        "{:?}",
        s.details
    );
}

#[test]
fn a_mechanic_in_the_text_without_a_rule_is_text_only_not_check() {
    let text = "<p>You have a flying speed of 30 feet.</p>";
    let src = "Brisk Walker\nYou have a flying speed of 30 feet.";
    let d = draft(src, vec![feat("ID_T_FEAT_BRISK", text, json!([]))]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.state, ReviewLevel::Ok, "{s:?}");
    assert!(s.text_only);
    assert!(
        s.text_only_reason.contains("speed"),
        "{}",
        s.text_only_reason
    );
}

#[test]
fn a_mechanic_the_source_states_but_the_element_lost_is_check() {
    let src = "Brisk Walker\nYou have a flying speed of 30 feet. You like long walks.";
    let d = draft(
        src,
        vec![feat(
            "ID_T_FEAT_BRISK",
            "<p>You like long walks.</p>",
            json!([]),
        )],
    );
    let s = review_state(&d.elements[0], &d);
    assert_eq!(
        (s.state, s.code.as_str()),
        (ReviewLevel::Check, code::MECHANIC_LOST)
    );
    assert!(s.reason.contains("30 feet"), "{}", s.reason);
    // The user says the sheet is not meant to compute it: no longer a gap.
    let mut o = ReviewOverrides::default();
    o.set_text_only("ID_T_FEAT_BRISK", Some(true));
    let s = review_all(&d, &o).remove(0);
    assert_eq!(s.state, ReviewLevel::Ok);
    assert!(s.text_only && s.text_only_override == Some(true));
}

#[test]
fn acceptance_holds_until_the_element_changes() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.rule_errors = vec!["removed a rule".into()];
    let mut d = draft(SOURCE, vec![e]);
    let mut o = ReviewOverrides::default();
    o.set_accepted(&d.elements[0], true);
    let s = review_all(&d, &o).remove(0);
    assert_eq!(
        (s.state, s.code.as_str()),
        (ReviewLevel::Ok, code::ACCEPTED)
    );
    assert!(s.accepted);
    assert!(s.details[0].contains("removed"), "{:?}", s.details);
    d.elements[0].json["description"] = json!("<p>Changed.</p>");
    assert_eq!(review_all(&d, &o)[0].state, ReviewLevel::Check);
    o.prune(&d);
    assert!(o.accepted.is_empty());
}

#[test]
fn text_only_override_false_clears_an_automatic_text_only() {
    let text = "<p>You have a flying speed of 30 feet.</p>";
    let d = draft(SOURCE, vec![feat("ID_T_FEAT_BRISK", text, json!([]))]);
    let mut o = ReviewOverrides::default();
    o.set_text_only("ID_T_FEAT_BRISK", Some(false));
    let s = review_all(&d, &o).remove(0);
    assert!(!s.text_only && s.text_only_reason.is_empty());
    o.set_text_only("ID_T_FEAT_BRISK", None);
    assert!(review_all(&d, &o)[0].text_only);
}

#[test]
fn source_passage_is_one_based_and_inclusive() {
    let d = draft(
        SOURCE,
        vec![feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]))],
    );
    let (s, e, text) = source_passage(&d.elements[0], &d).unwrap();
    assert_eq!((s, e), (1, 2));
    assert_eq!(text, SOURCE);
}

#[test]
fn counts_add_up() {
    let mut gone = feat("ID_T_FEAT_GONE", DESCRIPTION, json!([]));
    gone.json = Value::Null;
    gone.status = ElementStatus::Failed;
    let d = draft(
        SOURCE,
        vec![feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([])), gone],
    );
    let c = ReviewCounts::of(&review_all(&d, &ReviewOverrides::default()));
    assert_eq!((c.ok, c.check, c.not_imported), (1, 0, 1));
}

/// Offline measurement: `BOREALIS_REVIEW_DRAFTS=<a.json>,<b.json> cargo test -p borealis-assist
/// --lib -- --ignored review_counts_of_stored_drafts --nocapture`.
#[test]
#[ignore = "tool: needs stored drafts"]
fn review_counts_of_stored_drafts() {
    let paths = std::env::var("BOREALIS_REVIEW_DRAFTS").unwrap_or_default();
    for path in paths.split(',').filter(|p| !p.is_empty()) {
        let text = std::fs::read_to_string(path).unwrap();
        let d: ImportDraft = serde_json::from_str(&text).unwrap();
        let states = review_all(&d, &ReviewOverrides::default());
        let c = ReviewCounts::of(&states);
        let notes: usize = d.elements.iter().map(|e| e.issues.len()).sum();
        println!(
            "DRAFT {path}: elements {} notes {notes} ok {} check {} not_imported {} text_only {}",
            states.len(),
            c.ok,
            c.check,
            c.not_imported,
            c.text_only
        );
        for (e, s) in d.elements.iter().zip(&states) {
            if s.state != ReviewLevel::Ok || s.text_only {
                println!("  {:?} {} {} | {}", s.state, s.code, e.id, s.reason);
            }
        }
    }
}

#[test]
fn a_species_without_any_speed_is_incomplete() {
    let race = element(
        "ID_T_RACE_ROCKLING",
        "Race",
        json!({"id": "ID_T_RACE_ROCKLING", "type": "Race", "name": "Rockling",
               "description": "<p>Rocklings are small.</p>", "rules": []}),
    );
    let d = draft("Rockling\nRocklings are small.", vec![race]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(
        (s.state, s.code.as_str()),
        (ReviewLevel::Check, code::INCOMPLETE)
    );
    let mut fixed = d.clone();
    fixed.elements[0].json["rules"] =
        json!([{"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"}]);
    assert_eq!(
        review_state(&fixed.elements[0], &fixed).state,
        ReviewLevel::Ok
    );
}

#[test]
fn the_review_report_carries_codes_and_overrides_but_no_text() {
    let src = "Brisk Walker\nYou have a flying speed of 30 feet. You like long walks.";
    let lost = feat("ID_T_FEAT_LOST", "<p>You like long walks.</p>", json!([]));
    let fine = feat("ID_T_FEAT_FINE", "<p>You like long walks.</p>", json!([]));
    let mut d = draft(src, vec![lost, fine]);
    d.elements[1].excerpt = None;
    d.plan.elements[1].excerpt = None;
    let mut o = ReviewOverrides::default();
    o.set_text_only("ID_T_FEAT_FINE", Some(true));
    let renamed = vec![("ID_T_FEAT_LOST".to_string(), "ID_T_FEAT_LOST_2".to_string())];
    let r = review_report(&d, &o, &renamed);
    assert_eq!(r.check, 2, "{r:?}");
    assert_eq!(r.elements[0].element, "ID_T_FEAT_LOST_2");
    assert_eq!(r.elements[0].code, code::MECHANIC_LOST);
    assert_eq!(r.elements[1].text_only_override, Some(true));
    let text = serde_json::to_string(&r).unwrap();
    assert!(
        !text.contains("30 feet") && !text.contains("walks"),
        "{text}"
    );
}
