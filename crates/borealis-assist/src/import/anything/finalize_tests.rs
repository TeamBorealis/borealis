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
        report.source_warnings[0],
        "`a.pdf`: 2 page(s) were read visually"
    );
    assert!(!report
        .source_warnings
        .iter()
        .any(|w| w.contains("unrelated")));
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

#[test]
fn tables_and_rows_folded_into_text_or_imported_as_items_are_not_reported() {
    // Arrange: detect listed two tables and a table's rows as "other". The first table's rows
    // are items; the second table and its rows are printed inside a text entry; "Credits" and
    // "Loot Table" are in no element.
    let source = "## Gear\n\n#### Hand Tools\n\nName | Cost\n\nMallet (2) | 1 gp\n\nAwl | 2 gp\n\n\
        #### Loot Table\n\nName | Cost\n\nGold Idol | 9 gp\n\n#### Tiers\n\nTier | Cost\n\nLow | 5 gp\n";
    let text = "<p><strong>Gear</strong></p><p><strong>Tiers</strong></p><p>Tier | Cost</p>\
        <p>Low | 5 gp</p>";
    let draft: ImportDraft = serde_json::from_value(serde_json::json!({
        "edition": "2014", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "attachments", "source_text": source,
        "detection": {"kind": "item", "edition": "2014", "confidence": 1.0, "items": [],
            "other": [
                {"name": "Hand Tools", "what": "table", "note": ""},
                {"name": "Tiers", "what": "table", "note": ""},
                {"name": "Low", "what": "table row", "note": "Tiers"},
                {"name": "Loot Table", "what": "table", "note": ""},
                {"name": "Credits", "what": "lore section", "note": ""}]},
        "plan": {"edition": "2014", "groups": [], "elements": [], "notes": []},
        "elements": [
            element("ID_T_MALLET", "Item", "ok", Value::Null),
            element("ID_T_AWL", "Item", "ok", Value::Null),
            element("ID_T_GEAR", "Rules Glossary Entry", "ok",
                serde_json::json!({"description": text}))],
        "sweep_issues": []}))
    .unwrap();
    // Act
    let report = build_report(&draft, &[], &HashSet::new());
    // Assert: only the real gaps are reported.
    let unsupported: Vec<&str> = report
        .entries
        .iter()
        .filter(|e| e.outcome == "unsupported")
        .map(|e| e.source.as_str())
        .collect();
    assert_eq!(unsupported, vec!["Loot Table", "Credits"]);
    // The text entry left out of the pack: its table is reported again.
    let left: HashSet<String> = ["ID_T_GEAR".to_string()].into();
    let again = build_report(&draft, &[], &left);
    assert!(again.entries.iter().any(|e| e.source == "Tiers"));
}

#[test]
fn index_captions_with_their_column_header_glued_on_are_not_reported() {
    // Arrange: an index whose captions carry the one-column table's header ("Tools Item"),
    // names marked with a footnote "*". "Tools" is printed in a text entry; "Bags" only in the
    // source, its rows are items; "Loot" is a real gap.
    let source = "### Gear by Kind\n\n#### Tools Item\n\nMallet*\nAwl\n\n#### Bags Item\n\nSack*\n\n\
        #### Cases Item\n\nTin\n\n#### Weapons\n\nItem | Cost\nClub* | 1 gp\n\n#### Loot\n\nGold Idol\n";
    let text = "<p><strong>Gear by Kind</strong></p><p><strong>Tools Item</strong></p>\
        <p>Mallet* Awl</p>";
    let draft: ImportDraft = serde_json::from_value(serde_json::json!({
        "edition": "2014", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "attachments", "source_text": source,
        "detection": {"kind": "item", "edition": "2014", "confidence": 1.0, "items": [],
            "other": [
                {"name": "Tools", "what": "table", "note": ""},
                {"name": "Bags", "what": "table", "note": ""},
                {"name": "Loot", "what": "table", "note": ""}]},
        "plan": {"edition": "2014", "groups": [], "elements": [], "notes": []},
        "elements": [
            element("ID_T_SACK", "Item", "ok", Value::Null),
            element("ID_T_GEAR", "Rules Glossary Entry", "ok",
                serde_json::json!({"description": text}))],
        "sweep_issues": []}))
    .unwrap();
    // Act
    let report = build_report(&draft, &[], &HashSet::new());
    // Assert
    let unsupported: Vec<&str> = report
        .entries
        .iter()
        .filter(|e| e.outcome == "unsupported")
        .map(|e| e.source.as_str())
        .collect();
    assert_eq!(unsupported, vec!["Loot"]);
    // A caption word that is no column header still counts as part of the name.
    let headers = header_words(source);
    assert!(headers.contains("item"));
    assert!(!printed_in("<p>Tools Kit</p>", "Tools", &headers));
}

#[test]
fn a_section_whose_prose_is_already_reported_is_not_reported_twice() {
    // Arrange: the uncovered prose entry quotes the section's run-in ("Preface (...): ...").
    let skipped = ReportEntry {
        source: "Some Book".into(),
        outcome: "skipped".into(),
        elements: Vec::new(),
        note: "prose in no element's description (check it is not rules text): \"Preface (short): This book adds ...\"".into(),
    };
    // Act + Assert: only the section that prose opens is covered, by whole word.
    assert!(reported_as_prose(std::slice::from_ref(&skipped), "Preface"));
    assert!(!reported_as_prose(std::slice::from_ref(&skipped), "Pref"));
    assert!(!reported_as_prose(
        std::slice::from_ref(&skipped),
        "Credits"
    ));
    assert!(reported_as_prose(
        std::slice::from_ref(&skipped),
        "Some Book"
    ));
}

/// A species whose description carries its lore sections (one with "&" in its name, one under
/// no heading of its own), a contents page, a narrative section with a reward, and a section no
/// element carries.
fn lore_draft() -> ImportDraft {
    let source = "# Ashborn\n\n## Contents\n\nAshborn..........pg.2\nKeys . . . . . 3\nLater… 4\n\n\
## The Ashborn\n\nThe ashborn rise from cold hearths and wander the roads alone for many long years.\n\n\
## Hearth & Home\n\nThey keep no homes but carry a coal from the fire that woke them, wrapped in cloth.\n\n\
## The Burned Mill\n\nThe miller's chest holds a +1 dagger and 120 gp under the floor. Rats nest in the walls.\n\n\
## Old Customs\n\nTravellers leave a stone at every crossroads they pass, one for each friend they lost.\n\n\
## Omens\n\nd4 | Omen\n1 | A crow lands on your shoulder.\n";
    let description = "<p>A crow lands on your shoulder.</p><p>The ashborn rise from cold hearths and wander the roads alone for many long years.</p><h4>Hearth &amp; Home</h4><p>They keep no homes but carry a coal from the fire that woke them, wrapped in cloth.</p>";
    let race = serde_json::json!({"id": "ID_T_RACE_ASHBORN", "name": "Ashborn", "type": "Race",
        "source": "T", "description": description, "rules": []});
    let other = |name: &str, what: &str| serde_json::json!({"name": name, "what": what});
    serde_json::from_value(serde_json::json!({
        "edition": "2024", "id_prefix": "ID_T", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "text", "source_text": source,
        "detection": {"kind": "species", "edition": "2024", "confidence": 1.0, "items": [],
            "other": [other("Contents", "table of contents"), other("The Ashborn", "lore section"),
                other("Hearth & Home", "lore section"), other("Keys", "lore section"),
                other("The Burned Mill", "narrative (keyed area)"),
                other("Old Customs", "lore section"),
                other("A crow lands on your shoulder.", "option of Omens")]},
        "plan": {"edition": "2024", "groups": [], "elements": [],
            "notes": ["contents page: Contents"]},
        "elements": [element("ID_T_RACE_ASHBORN", "Race", "ok", race)], "sweep_issues": []}))
    .expect("draft")
}

#[test]
fn a_report_says_not_imported_only_for_text_no_element_carries() {
    let report = build_report(&lore_draft(), &[], &HashSet::new());
    let about = |name: &str| -> Vec<&ReportEntry> {
        report.entries.iter().filter(|e| e.source == name).collect()
    };
    // Carried by the species (one name printed as "&amp;", one under the species heading).
    assert!(about("The Ashborn").is_empty(), "{:?}", report.entries);
    assert!(about("Hearth & Home").is_empty(), "{:?}", report.entries);
    // A row printing a sentence as its name, carried by the species.
    assert!(about("A crow lands on your shoulder.").is_empty());
    // Really not imported.
    assert_eq!(about("Old Customs")[0].outcome, "unsupported");
    assert_eq!(about("Contents")[0].outcome, "skipped");
    assert!(about("Contents")[0].note.starts_with("a contents page"));
    assert!(about("Keys")[0]
        .note
        .starts_with("listed only on a contents page"));
    // A skipped narrative section keeps its reward as a line of its own.
    let mill = about("The Burned Mill");
    assert!(mill[0].note.starts_with("narrative text"));
    assert_eq!(mill.len(), 2, "{mill:?}");
    assert!(mill[1].note.contains("+1 dagger and 120 gp"));
}

/// A synthetic CR 1/4 creature (the bundled goblin's stat block) as draft element JSON, with
/// `xp` XP.
fn quillback_json(xp: u64) -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("srd-5.1-monsters"))
        .expect("monsters load")
        .build();
    let mut e = store
        .by_type("Monster")
        .find(|s| s.element.name == "Goblin")
        .expect("goblin")
        .element
        .clone();
    e.id = "ID_T_MONSTER_FEN_QUILLBACK".into();
    e.name = "Fen Quillback".into();
    e.source = "T".into();
    e.data.insert("xp".into(), Value::from(xp));
    serde_json::to_value(&e).expect("element json")
}

#[test]
fn a_printed_xp_off_the_table_is_committed_as_printed() {
    // Arrange: CR 1/4 (standard 50 XP) printed with 75 XP (decision HB4).
    let mut draft = draft_with_failures();
    draft.elements = serde_json::from_value(serde_json::json!([element(
        "ID_T_MONSTER_FEN_QUILLBACK",
        "Monster",
        "ok",
        quillback_json(75)
    )]))
    .expect("elements");
    // Act
    let done = finalize_draft(&library(), &draft).expect("commits");
    // Assert: the content warning does not block the commit, the printed XP is kept.
    let kept = done
        .pack
        .content
        .elements
        .iter()
        .find(|e| e.id == "ID_T_MONSTER_FEN_QUILLBACK")
        .expect("monster committed");
    assert_eq!(kept.data["xp"], Value::from(75));
}
