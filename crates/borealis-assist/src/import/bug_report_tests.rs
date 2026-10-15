//! Tests for the bug report payload: no source prose survives (BR2), codes group failures.

use super::*;
use serde_json::json;

/// Invented prose; every sentence contains the marker word so a leak is easy to find.
const PROSE: &str = "Zorblax the wandering storm whispers across the field and every ally gains a shimmering ward until dawn";
const MARK: &str = "Zorblax";

fn app() -> AppInfo {
    AppInfo {
        version: "1.2.3".into(),
        os: "macos".into(),
        arch: "aarch64".into(),
    }
}

fn content() -> ContentFile {
    let doc = json!({
        "format": borealis_content::file::ContentFormat::ID,
        "elements": [{
            "id": "ID_HB_T_FEAT_STORM_WARD", "name": "Storm Ward", "type": "Feat", "source": "Homebrew",
            "description": format!("<p>{PROSE}</p>"),
            "prerequisite": PROSE,
            "requirements": "[level:4],ID_HB_T_CLASS_STORMCALLER",
            "sheet": {"alt": "Ward", "descriptions": [{"text": PROSE}]},
            "setters": [{"name": "short", "value": PROSE}, {"name": "names-format", "value": "{{name}}"}],
            "rules": [
                {"kind": "select", "type": "Feat", "name": PROSE, "supports": "Storm Ward Option"},
                {"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_ARCANA"},
                {"kind": "stat", "name": "ac", "value": "1"},
                {"kind": "invented_aura", "radius": 10, "text": PROSE}
            ]
        }]
    });
    ContentFile::from_json_str(&doc.to_string()).expect("content loads")
}

fn report() -> ImportReport {
    ImportReport {
        errors: vec![ReportIssue {
            element: "ID_HB_T_FEAT_STORM_WARD".into(),
            message: format!("select rule removed: \"{PROSE}\" names no type"),
        }],
        warnings: vec![ReportIssue {
            element: "ID_HB_T_FEAT_STORM_WARD".into(),
            message: "the section was not located in the source".into(),
        }],
        entries: vec![ReportEntry {
            source: "Weather Table".into(),
            outcome: "unsupported".into(),
            elements: Vec::new(),
            note: format!("no element kind for tables; not imported. {PROSE}. {PROSE}. {PROSE}. {PROSE}. {PROSE}"),
        }],
        ..ImportReport::default()
    }
}

fn pack_report(options: ReportOptions) -> BugReport {
    let diag = vec![Diagnostic {
        level: Some(4),
        element: Some("ID_HB_T_FEAT_STORM_WARD".into()),
        message: "settle invariant: select has no options".into(),
    }];
    bug_report_from_pack(
        app(),
        Some("dnd2024".into()),
        &report(),
        &content(),
        diag,
        options,
    )
    .expect("report builds")
}

#[test]
fn no_description_or_prose_survives() {
    let r = pack_report(ReportOptions::default());
    let text = serde_json::to_string(&r).unwrap();
    assert!(!text.contains(MARK), "prose leaked: {text}");
    let e = &r.elements[0];
    assert_eq!(e.name, "Storm Ward", "names are kept for triage");
    assert!(e.description_chars > PROSE.len());
    assert!(e.structure.get("description").is_none());
    assert!(e.structure.get("prerequisite").is_none());
    // Structure stays: rule kinds, grant ids, requirements, short setters and labels.
    assert!(text.contains("ID_PROFICIENCY_SKILL_ARCANA"));
    assert!(text.contains("[level:4],ID_HB_T_CLASS_STORMCALLER"));
    assert!(text.contains("Storm Ward Option"));
    assert!(text.contains("{{name}}"));
    let rules = e.structure["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 4);
    assert_eq!(
        rules[0]["name"],
        json!(format!("[{} chars removed]", PROSE.chars().count()))
    );
    assert_eq!(e.structure["sheet"], json!({"alt": "Ward"}));
}

#[test]
fn codes_group_errors_engine_findings_and_invented_kinds() {
    let r = pack_report(ReportOptions {
        flagged: vec!["ID_HB_T_FEAT_STORM_WARD".into()],
        ..ReportOptions::default()
    });
    let codes: Vec<(&str, &str)> = r
        .codes
        .iter()
        .map(|c| (c.code.as_str(), c.pattern.as_str()))
        .collect();
    assert!(
        codes.contains(&("invented_rule_kind", "invented_aura")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("engine", "settle invariant: select has no options")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("rule_error", "select rule removed: <q> names no type")),
        "{codes:?}"
    );
    assert!(codes.contains(&("user_flagged", "Feat")), "{codes:?}");
    assert!(codes.iter().any(|(c, _)| *c == "unlocated"));
    assert!(codes.iter().any(|(c, _)| *c == "unsupported"));
    assert_eq!(r.summary["unsupported"], 1);
    assert_eq!(r.summary["flagged"], 1);
    assert!(self_checks_failed(&r));
    assert_eq!(r.format, BUG_REPORT_FORMAT);
    assert_eq!(r.origin, "pack");
}

#[test]
fn source_is_attached_only_when_asked_and_capped() {
    assert!(pack_report(ReportOptions::default()).source.is_none());
    let with = pack_report(ReportOptions {
        source: Some((
            "storm.txt".into(),
            "text".into(),
            "Storm Ward. A feat.".into(),
        )),
        ..ReportOptions::default()
    });
    let s = with.source.expect("attached");
    assert_eq!((s.name.as_str(), s.chars), ("storm.txt", 19));
    let big = bug_report_from_pack(
        app(),
        None,
        &ImportReport::default(),
        &content(),
        Vec::new(),
        ReportOptions {
            source: Some((
                "big.txt".into(),
                "text".into(),
                "x".repeat(MAX_SOURCE_BYTES + 1),
            )),
            ..ReportOptions::default()
        },
    );
    assert!(matches!(big, Err(AssistError::Invalid(_))));
}

#[test]
fn note_is_trimmed_and_capped() {
    let r = pack_report(ReportOptions {
        note: format!("  {}  ", "n".repeat(MAX_NOTE_CHARS + 50)),
        ..ReportOptions::default()
    });
    assert_eq!(r.note.chars().count(), MAX_NOTE_CHARS);
}

#[test]
fn draft_report_keeps_structure_status_and_sweep_issues() {
    let draft: ImportDraft = serde_json::from_value(json!({
        "edition": "2024", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "text", "source_text": PROSE,
        "detection": {"kind": "feat", "edition": "2024", "confidence": 1.0, "items": [],
            "other": [{"name": "Weather Table", "what": "table", "note": PROSE}]},
        "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
        "elements": [{
            "id": "t-storm", "name": "Storm Ward", "ty": "feat", "role": "standalone", "level": null,
            "json": {"id": "t-storm", "type": "feat", "name": "Storm Ward", "description": PROSE,
                "rules": [{"kind": "grant", "type": "Proficiency", "id": "ID_X"}]},
            "status": "failed", "issues": [format!("model said \"{PROSE}\"")], "excerpt": null,
            "balance": null, "repair_rounds": 2, "revisions": 0,
            "rule_errors": ["grant references an unknown id ID_X"]
        }],
        "sweep_issues": [{"level": 3, "element_id": "t-storm", "message": "level 3: grant failed"}]
    }))
    .expect("draft fixture");
    let base = ContentStoreBuilder::new();
    let r = bug_report_from_draft(app(), &base, &draft, ReportOptions::default()).unwrap();
    let text = serde_json::to_string(&r).unwrap();
    assert!(!text.contains(MARK), "prose leaked: {text}");
    assert_eq!(r.origin, "draft");
    assert_eq!(r.edition.as_deref(), Some("dnd2024"));
    assert_eq!(r.elements[0].status.as_deref(), Some("failed"));
    assert_eq!(r.diagnostics[0].level, Some(3));
    assert_eq!(r.report.errors.len(), 1);
    let codes: Vec<&str> = r.codes.iter().map(|c| c.code.as_str()).collect();
    for want in [
        "element_failed",
        "engine",
        "rule_error",
        "unsupported",
        "element_issue",
    ] {
        assert!(codes.contains(&want), "{want} missing from {codes:?}");
    }
}

#[test]
fn prose_detection_keeps_expressions_and_labels() {
    assert!(is_prose(PROSE));
    assert!(!is_prose("[level:4],ID_A || ID_B && ID_C"));
    assert!(!is_prose("$(level:5) + 2 * 3"));
    assert_eq!(scrub_str("Fighting Style"), "Fighting Style");
    assert_eq!(
        clean_message("x \"short\" y"),
        "x \"short\" y",
        "short quotes are labels"
    );
}

#[test]
fn logs_are_attached_only_when_opted_in_and_capped() {
    let many: String = (0..500).map(|i| format!("INFO line {i}\n")).collect();
    assert!(pack_report(ReportOptions::default()).logs.is_none());
    let json = serde_json::to_value(pack_report(ReportOptions::default())).unwrap();
    assert!(json.get("logs").is_none(), "absent, not null, when off");

    let with = pack_report(ReportOptions {
        logs: Some(many),
        ..ReportOptions::default()
    });
    let logs = with.logs.expect("logs attached");
    assert_eq!(logs.lines, MAX_LOG_LINES);
    assert_eq!(logs.text.lines().count(), MAX_LOG_LINES);
    assert!(logs.text.ends_with("INFO line 499"));
    assert!(!logs.text.contains("INFO line 299\n"));

    let huge = format!("{}\n", "x".repeat(MAX_LOG_BYTES / 2)).repeat(4);
    let capped = pack_report(ReportOptions {
        logs: Some(huge),
        ..ReportOptions::default()
    });
    assert!(capped.logs.unwrap().text.len() <= MAX_LOG_BYTES);
    assert!(pack_report(ReportOptions {
        logs: Some(String::new()),
        ..ReportOptions::default()
    })
    .logs
    .is_none());
}
