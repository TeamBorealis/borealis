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

fn failed(code: &str, message: &str) -> ElementCheck {
    ElementCheck {
        code: code.into(),
        message: message.into(),
        failed: true,
    }
}

#[test]
fn a_failed_check_comes_after_removed_rules_and_both_are_kept() {
    // Arrange
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.rule_errors = vec!["removed a rule of unknown kind \"teleport\"".into()];
    e.checks = vec![
        ElementCheck {
            code: "heading_covered".into(),
            message: "heading is covered".into(),
            failed: false,
        },
        failed("table_row_missing", "table row \"3rd\" is missing"),
    ];
    let d = draft(SOURCE, vec![e]);
    // Act
    let s = review_state(&d.elements[0], &d);
    // Assert
    assert_eq!(s.state, ReviewLevel::Check);
    assert_eq!(s.code, code::RULE_REMOVED);
    assert!(
        s.details[0].contains("Table row \"3rd\" is missing"),
        "{:?}",
        s.details
    );
}

#[test]
fn a_failed_check_alone_is_check_with_its_message_as_reason() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.checks = vec![failed(
        "number_not_in_source",
        "the weight 9 is not in the source text",
    )];
    let d = draft(SOURCE, vec![e]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.state, ReviewLevel::Check);
    assert_eq!(s.code, "check:number_not_in_source");
    assert_eq!(s.reason, "The weight 9 is not in the source text");
}

#[test]
fn a_sweep_issue_leads_over_a_failed_check() {
    let mut e = feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]));
    e.checks = vec![failed(
        "reference_unresolved",
        "points at something that does not exist",
    )];
    let mut d = draft(SOURCE, vec![e]);
    d.sweep_issues = vec![SweepIssue {
        level: 0,
        element_id: Some("ID_T_FEAT_BRISK".into()),
        message: "unknown resource".into(),
    }];
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.code, code::ENGINE);
    assert!(
        s.details.iter().any(|x| x.contains("does not exist")),
        "{:?}",
        s.details
    );
}

#[test]
fn a_failed_untraced_check_and_the_fidelity_finding_are_one_reason() {
    let mut e = feat(
        "ID_T_FEAT_BRISK",
        "<p>A sentence the source never prints at all.</p>",
        json!([]),
    );
    e.checks = vec![failed(
        "description_untraced",
        "one sentence of the description is not in the source",
    )];
    let d = draft(SOURCE, vec![e]);
    let s = review_state(&d.elements[0], &d);
    assert_eq!(s.code, "check:description_untraced");
    assert!(
        !s.details
            .iter()
            .any(|x| x.contains("description sentence(s)")),
        "{:?}",
        s.details
    );
}

#[test]
fn failed_import_level_checks_become_one_note_per_code_with_the_names() {
    let mut d = draft(
        SOURCE,
        vec![feat("ID_T_FEAT_BRISK", DESCRIPTION, json!([]))],
    );
    d.source_checks = vec![
        failed(
            "heading_uncovered",
            "The section \"Camp Rules\" was not imported",
        ),
        ElementCheck {
            code: "heading_uncovered".into(),
            message: "The section \"Fine\" was imported".into(),
            failed: false,
        },
        failed(
            "heading_uncovered",
            "The section \"Road Lore\" was not imported",
        ),
    ];
    let notes = import_level_notes(&d);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].code, "heading_uncovered");
    assert_eq!(notes[0].summary, "These headings weren't imported");
    assert_eq!(notes[0].items, ["Camp Rules", "Road Lore"]);
    let report = review_report(&d, &ReviewOverrides::default(), &[]);
    assert_eq!(report.import_checks.len(), 1);
    assert_eq!(report.import_checks[0].count, 2);
    assert!(import_level_notes(&draft(SOURCE, vec![])).is_empty());
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

fn passed_check(code: &str) -> ElementCheck {
    ElementCheck {
        code: code.into(),
        message: "fine".into(),
        failed: false,
    }
}

#[test]
fn an_unlocated_element_whose_long_description_is_traced_is_ok() {
    // Arrange: no located passage, but every sentence is the source's (a name wrapped over two
    // lines, a table read in columns).
    let src = "Lantern of\nQuiet Dusk\nThis lantern burns with a pale flame that never warms \
the hand, and its light reveals the tracks of small animals for a mile.";
    let desc = "<p>This lantern burns with a pale flame that never warms the hand, and its light \
reveals the tracks of small animals for a mile.</p>";
    let mut e = element(
        "ID_T_LANTERN",
        "Magic Item",
        json!({"id": "ID_T_LANTERN", "type": "Magic Item", "name": "Lantern of Quiet Dusk", "description": desc}),
    );
    e.excerpt = None;
    e.checks = vec![passed_check("description_untraced")];
    let mut d = draft(src, vec![e]);
    // Act
    let traced = review_state(&d.elements[0], &d);
    d.elements[0].checks.clear();
    let unchecked = review_state(&d.elements[0], &d);
    // Assert: traced -> ok; without the passed fidelity check it still asks for a look.
    assert_eq!(traced.state, ReviewLevel::Ok, "{traced:?}");
    assert_eq!(unchecked.code, code::UNLOCATED, "{unchecked:?}");
}

#[test]
fn a_mechanic_of_a_child_inside_the_parent_passage_is_not_the_parent_gap() {
    // Arrange: the species passage holds its subrace, whose own element states the increase.
    let src = "## Glassborn\nYou like quiet places.\n### Deep Glassborn\nYour Strength score increases by 2.";
    let parent = element(
        "ID_T_RACE_GLASSBORN",
        "Race",
        json!({"id": "ID_T_RACE_GLASSBORN", "type": "Race", "name": "Glassborn", "description": "<p>You like quiet places.</p>", "rules": [{"kind": "stat", "name": "innate speed", "value": "30", "bonus": "base"}]}),
    );
    let mut child = element(
        "ID_T_SUBRACE_DEEP",
        "Sub Race",
        json!({"id": "ID_T_SUBRACE_DEEP", "type": "Sub Race", "name": "Deep Glassborn", "description": "<p>Your Strength score increases by 2.</p>", "rules": [{"kind": "stat", "name": "strength", "value": "2"}]}),
    );
    let mut parent = parent;
    parent.excerpt = Some(SourceExcerpt {
        start_line: 1,
        end_line: 4,
        snippet: String::new(),
    });
    child.excerpt = Some(SourceExcerpt {
        start_line: 3,
        end_line: 4,
        snippet: String::new(),
    });
    let d = draft(src, vec![parent, child]);
    // Act
    let s = review_state(&d.elements[0], &d);
    // Assert
    assert_eq!(s.state, ReviewLevel::Ok, "{s:?}");
}

#[test]
fn a_sentence_wrapped_over_two_lines_is_read_whole_for_gaps() {
    // Arrange: "a number of ... equal to your Charisma modifier" is no count once read whole.
    let src = "Brisk Walker\nWhen you rest, choose a number of routes\nequal to your Charisma modifier (minimum of one route).";
    let desc = "<p>When you rest, choose a number of routes equal to your Charisma modifier (minimum of one route).</p>";
    let mut e = feat("ID_T_FEAT_BRISK", desc, json!([]));
    e.excerpt = Some(SourceExcerpt {
        start_line: 1,
        end_line: 3,
        snippet: String::new(),
    });
    let d = draft(src, vec![e]);
    // Act
    let s = review_state(&d.elements[0], &d);
    // Assert
    assert_ne!(s.code, code::MECHANIC_LOST, "{s:?}");
}

#[test]
fn a_section_kept_as_a_text_entry_is_text_only() {
    // Arrange: the root of a rules group (the report's text_only outcome).
    let mut e = element(
        "ID_T_RULE_CAMPING",
        "Rules Glossary Entry",
        json!({"id": "ID_T_RULE_CAMPING", "type": "Rules Glossary Entry", "name": "Camping", "description": "<p>Pitch camp at dusk.</p>"}),
    );
    e.role = Role::Text;
    let mut d = draft("Camping\nPitch camp at dusk.", vec![e]);
    d.plan.groups = vec![serde_json::from_value(json!({
        "name": "Camping", "kind": "rules", "root_id": "ID_T_RULE_CAMPING"
    }))
    .expect("group parses")];
    // Act
    let states = review_all(&d, &ReviewOverrides::default());
    // Assert
    assert_eq!(states[0].state, ReviewLevel::Ok);
    assert!(states[0].text_only, "{:?}", states[0]);
    assert!(states[0].text_only_reason.contains("text entry"));
    assert_eq!(ReviewCounts::of(&states).text_only, 1);
}

#[test]
fn a_model_note_keeping_a_mechanic_in_the_text_makes_it_text_only() {
    // Arrange
    let mut e = feat(
        "ID_T_FEAT_BRISK",
        "<p>For 1 minute you shrug off cuts.</p>",
        json!([]),
    );
    e.issues = vec!["The 1-minute resistance is kept in the description only.".into()];
    let d = draft("Brisk Walker\nFor 1 minute you shrug off cuts.", vec![e]);
    // Act
    let s = review_state(&d.elements[0], &d);
    // Assert
    assert!(s.text_only, "{s:?}");
    assert!(s.text_only_reason.contains("description only"));
}

#[test]
fn a_text_fragment_note_has_its_own_summary() {
    // Arrange
    let mut d = draft("x", Vec::new());
    d.source_checks = vec![failed(
        "text_uncovered",
        "The text starting \"I always knew the\" was not imported",
    )];
    // Act
    let notes = import_level_notes(&d);
    // Assert
    assert_eq!(notes[0].summary, "This text wasn't imported");
    assert_eq!(notes[0].items, vec!["I always knew the".to_string()]);
}

/// Offline probe over stored drafts (not a test): `BOREALIS_REVIEW_PROBE=<draft.json>[:...]`
/// prints each draft's review counts and every element that is not plainly `ok`, after
/// re-running the source checks over the bundled packs. Reference checks are left out: the
/// bundled packs are not the user's library, so they would dangle.
#[test]
#[ignore = "offline probe over stored drafts"]
fn probe_review_states() {
    let Ok(paths) = std::env::var("BOREALIS_REVIEW_PROBE") else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut base = borealis_content::ContentStoreBuilder::new();
    for p in [
        "core",
        "srd-5.1",
        "srd-5.1-monsters",
        "srd-5.2.1",
        "srd-5.2.1-monsters",
    ] {
        base = base
            .add_pack_dir(&root.join(p))
            .expect("bundled pack loads");
    }
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).expect("draft readable");
        let mut d: ImportDraft = serde_json::from_str(&text).expect("draft loads");
        super::super::selfcheck::apply(&base, &mut d);
        for e in &mut d.elements {
            e.checks.retain(|c| c.code != "reference_unresolved");
        }
        let states = review_all(&d, &ReviewOverrides::default());
        println!("== {path} {:?}", ReviewCounts::of(&states));
        for n in import_level_notes(&d) {
            println!("  note {} {}: {:?}", n.code, n.items.len(), n.items);
        }
        for (e, s) in d.elements.iter().zip(&states) {
            if s.state != ReviewLevel::Ok || s.text_only {
                println!(
                    "  {:?}\t{}\t{}\t{}\tTO={}\t{}",
                    s.state,
                    s.code,
                    e.id,
                    e.ty,
                    u8::from(s.text_only),
                    s.reason.chars().take(160).collect::<String>()
                );
            }
        }
    }
}

#[test]
fn a_creature_whose_stat_block_was_never_read_is_not_imported() {
    // Arrange: a detected creature kept as text with only its name line.
    let p = crate::import::anything::fidelity::tests::planned(
        "ID_T_GLOOM",
        "Gloom Stalker",
        "Monster",
        Role::Standalone,
        Some(1),
    );
    let mut d = crate::import::anything::fidelity::tests::draft(
        "# Gloom Stalker\n",
        vec![p],
        &["<p>A gloom stalker.</p>"],
    );
    let absent = crate::import::anything::types::ElementCheck {
        code: "stat_block_absent".into(),
        message: "No printed stat block was found for this creature in the source text".into(),
        failed: true,
    };
    d.elements[0].checks = vec![absent.clone()];
    let mut long = d.clone();
    long.elements[0].json["description"] =
        serde_json::json!("<p>It hunts at dusk. It never leaves the marsh. Its eyes glow.</p>");
    // Act
    let short = review_state(&d.elements[0], &d);
    let lore = review_state(&long.elements[0], &long);
    // Assert
    assert_eq!(short.state, ReviewLevel::NotImported);
    assert_eq!(short.code, code::STAT_BLOCK_NOT_READ);
    assert_eq!(lore.state, ReviewLevel::Check);
}
