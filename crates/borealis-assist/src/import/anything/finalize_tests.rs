//! Finalize: report shape and committing a draft with failed elements.

use super::*;

#[test]
fn reading_notes_are_source_warnings_not_element_warnings() {
    let mut draft: ImportDraft = serde_json::from_value(serde_json::json!({
        "edition": "2024", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "attachments", "source_text": "",
        "detection": {"kind": "feat", "edition": "2024", "confidence": 1.0, "items": []},
        "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
        "elements": [], "sweep_issues": []}))
    .unwrap();
    draft.detection.notes = vec![
        format!(
            "{}`a.pdf`: 2 page(s) were read visually",
            super::super::ingest::READING_NOTE
        ),
        "An unrelated detection note.".into(),
    ];
    let report = build_report(&draft, &[], &HashSet::new());
    assert_eq!(
        report.source_warnings,
        vec!["`a.pdf`: 2 page(s) were read visually".to_string()]
    );
    assert!(report.warnings.iter().all(|w| w.element != "source"));
}

fn library() -> ContentStoreBuilder {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .expect("core loads")
}

fn element(id: &str, ty: &str, status: &str, json: Value) -> Value {
    serde_json::json!({"id": id, "name": id.trim_start_matches("ID_T_").replace('_', " "),
        "ty": ty, "role": "standalone", "level": null, "json": json, "status": status,
        "issues": if status == "failed" { vec!["generation failed: the model gave up"] } else { vec![] },
        "excerpt": null, "balance": null, "repair_rounds": 0, "revisions": 0})
}

fn el_json(id: &str, ty: &str, rules: Value) -> Value {
    serde_json::json!({"id": id, "name": id.trim_start_matches("ID_T_").replace('_', " "),
        "type": ty, "source": "T", "description": "<p>Invented.</p>", "rules": rules})
}

/// A species granting two traits, one of which failed; a select over both traits; a failed feat
/// that still has (valid) output; a failed species whose output does not load.
fn draft_with_failures() -> ImportDraft {
    let race = el_json(
        "ID_T_RACE_GLASSFOLK",
        "Race",
        serde_json::json!([
            {"kind": "grant", "type": "Racial Trait", "id": "ID_T_TRAIT_PRISM"},
            {"kind": "grant", "type": "Racial Trait", "id": "ID_T_TRAIT_SHATTER"},
            {"kind": "select", "type": "Racial Trait", "name": "Facet",
             "supports": "ID_T_TRAIT_PRISM|ID_T_TRAIT_SHATTER"},
            {"kind": "select", "type": "Racial Trait", "name": "Lost Facet",
             "supports": "ID_T_TRAIT_SHATTER"}
        ]),
    );
    let elements = vec![
        element("ID_T_RACE_GLASSFOLK", "Race", "ok", race),
        element(
            "ID_T_TRAIT_PRISM",
            "Racial Trait",
            "ok",
            el_json("ID_T_TRAIT_PRISM", "Racial Trait", serde_json::json!([])),
        ),
        element("ID_T_TRAIT_SHATTER", "Racial Trait", "failed", Value::Null),
        element(
            "ID_T_FEAT_KEPT",
            "Feat",
            "failed",
            el_json("ID_T_FEAT_KEPT", "Feat", serde_json::json!([])),
        ),
        element("ID_T_RACE_BROKEN", "Race", "failed", {
            // A load error: a Race `names` setter needs a `type`.
            let mut j = el_json("ID_T_RACE_BROKEN", "Race", serde_json::json!([]));
            j["setters"] = serde_json::json!([{"name": "names", "value": "Ann"}]);
            j
        }),
    ];
    serde_json::from_value(serde_json::json!({
        "edition": "2024", "id_prefix": "ID_T", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "text", "source_text": "",
        "detection": {"kind": "species", "edition": "2024", "confidence": 1.0, "items": []},
        "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
        "elements": elements, "sweep_issues": []}))
    .expect("draft")
}

fn entry<'a>(report: &'a ImportReport, source: &str) -> &'a ReportEntry {
    report
        .entries
        .iter()
        .find(|e| e.source == source)
        .unwrap_or_else(|| panic!("no entry for {source}: {:?}", report.entries))
}

#[test]
fn a_failed_element_is_left_out_and_nothing_points_at_it() {
    let done = finalize_draft(&library(), &draft_with_failures()).expect("commits");
    let content = &done.pack.content;
    let ids: Vec<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"ID_T_RACE_GLASSFOLK") && ids.contains(&"ID_T_TRAIT_PRISM"));
    assert!(!ids.contains(&"ID_T_TRAIT_SHATTER") && !ids.contains(&"ID_T_RACE_BROKEN"));
    let text = serde_json::to_string(content).unwrap();
    assert!(!text.contains("ID_T_TRAIT_SHATTER"), "{text}");
    assert!(text.contains("\"supports\":\"ID_T_TRAIT_PRISM\""), "{text}");
    assert!(
        !text.contains("Lost Facet"),
        "an emptied select goes: {text}"
    );
    let report = done.pack.report.expect("report");
    let gone = entry(&report, "TRAIT SHATTER");
    assert_eq!(gone.outcome, DROPPED);
    assert!(gone.note.contains("the model gave up") && gone.note.contains("3 reference(s)"));
    assert_eq!(entry(&report, "RACE BROKEN").outcome, DROPPED);
}

#[test]
fn a_failed_element_with_valid_output_is_kept_for_review() {
    let done = finalize_draft(&library(), &draft_with_failures()).expect("commits");
    assert!(done
        .pack
        .content
        .elements
        .iter()
        .any(|e| e.id == "ID_T_FEAT_KEPT"));
    let report = done.pack.report.expect("report");
    let kept = entry(&report, "FEAT KEPT");
    assert_eq!(kept.outcome, UNCHECKED);
    assert_eq!(kept.elements, vec!["ID_T_FEAT_KEPT".to_string()]);
}

#[test]
fn dropped_elements_fail_the_bug_report_self_check() {
    let draft = draft_with_failures();
    let report = crate::import::bug_report::bug_report_from_draft(
        crate::import::bug_report::AppInfo {
            version: "0".into(),
            os: "t".into(),
            arch: "t".into(),
        },
        &library(),
        &draft,
        crate::import::bug_report::ReportOptions::default(),
    )
    .expect("report");
    assert!(report.codes.iter().any(|c| c.code == DROPPED));
    assert!(crate::import::bug_report::self_checks_failed(&report));
}

#[test]
fn a_pending_element_still_blocks_the_commit() {
    let mut draft = draft_with_failures();
    draft.elements[1].status = ElementStatus::Pending;
    let err = finalize_draft(&library(), &draft).unwrap_err().to_string();
    assert!(err.contains("not generated yet"), "{err}");
}

#[test]
fn table_rows_imported_as_elements_are_not_reported_as_not_imported() {
    // Arrange: detect listed a table's rows as "other"; the plan made them option traits.
    let draft: ImportDraft = serde_json::from_value(serde_json::json!({
        "edition": "2014", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "attachments", "source_text": "",
        "detection": {"kind": "species", "edition": "2014", "confidence": 1.0, "items": [],
            "other": [
                {"name": "Spiral Grip", "what": "table row", "note": "Clawform option"},
                {"name": "Hooked Grip", "what": "table row", "note": "Clawform option"},
                {"name": "Credits", "what": "lore section", "note": ""}]},
        "plan": {"edition": "2014", "groups": [], "elements": [], "notes": []},
        "elements": [
            element("ID_T_SPIRAL_GRIP", "Racial Trait", "ok", Value::Null),
            element("ID_T_HOOKED_GRIP", "Racial Trait", "ok", Value::Null)],
        "sweep_issues": []}))
    .unwrap();
    // Act
    let report = build_report(&draft, &[], &HashSet::new());
    let unsupported = |r: &ImportReport| -> Vec<String> {
        r.entries
            .iter()
            .filter(|e| e.outcome == "unsupported")
            .map(|e| e.source.clone())
            .collect()
    };
    // Assert: only the object no element has is reported.
    assert_eq!(unsupported(&report), vec!["Credits".to_string()]);
    // An element left out of the pack is not imported.
    let left: HashSet<String> = ["ID_T_HOOKED_GRIP".to_string()].into();
    assert_eq!(
        unsupported(&build_report(&draft, &[], &left)),
        vec!["Hooked Grip".to_string(), "Credits".to_string()]
    );
}
