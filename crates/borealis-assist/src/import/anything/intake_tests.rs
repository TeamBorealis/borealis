//! Intake on invented sources: narrative, library mentions, option tables, and whole imports
//! whose detection found no element kind (scripted fake model, no CLI).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use borealis_content::{ContentStore, ContentStoreBuilder};
use serde_json::{json, Value};

use super::*;
use crate::import::anything::types::{ImportPlan, ImportSource, Role};
use crate::import::anything::{
    finalize_draft, import_anything, AnythingRequest, ImportStep, VisionRunner,
};

fn library_builder() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    b
}

fn library() -> ContentStore {
    library_builder().build()
}

fn other(name: &str, what: &str) -> OtherItem {
    OtherItem {
        name: name.into(),
        what: what.into(),
        note: String::new(),
    }
}

fn item(name: &str) -> DetectedItem {
    DetectedItem {
        kind: ImportKind::Item,
        name: name.into(),
        parent_hint: None,
        item_type: None,
    }
}

const TRICKS: &str = "# Battle Tricks\n\n\
Any character trained in martial weapons can learn two tricks from the table below and use one per turn.\n\n\
| Trick | Effect |\n\
|---|---|\n\
| Low Sweep | The target falls prone on a failed save. |\n\
| High Feint | You gain advantage on your next attack. |\n\
| Shield Bash | The target is pushed 5 feet. |\n\n\
# Notes\n\nNothing else.\n";

#[test]
fn narrative_sections_are_told_apart_from_rules_and_lore() {
    for what in [
        "narrative",
        "keyed area",
        "NPC description",
        "location",
        "read-aloud text",
        "adventure section",
    ] {
        assert!(is_narrative(what), "{what}");
    }
    for what in [
        "lore",
        "lore section",
        "rule",
        "optional rule",
        "table row",
        "upgrade",
    ] {
        assert!(!is_narrative(what), "{what}");
    }
}

#[test]
fn an_item_the_source_only_mentions_is_a_library_reference() {
    // Arrange: one mention in a treasure sentence, one item the source defines itself.
    let store = library();
    let source = "## The Vault\n\nThe chest holds 30 gp and a potion of healing.\n\n\
## Potion of Climbing\n\nA homebrew brew that tastes of moss and lets you climb.\n";
    let items = [item("Potion of Healing"), item("Potion of Climbing")];

    // Act
    let refs = library_references(&store, Edition::Y2014, &items, source);

    // Assert
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].0, "Potion of Healing");
    assert!(refs[0].1.starts_with("ID_"));
    let note = library_note(&refs[0].0, &refs[0].1);
    assert_eq!(
        parse_library_note(&note),
        Some(("Potion of Healing", refs[0].1.as_str()))
    );
}

#[test]
fn table_rows_headings_and_run_ins_define_an_item_but_bullets_and_prose_do_not() {
    for source in [
        "| Club | 1 sp | 1d4 bludgeoning |",
        "Club (2) | 2 sp | 1d4 bludgeoning",
        "### Club (homebrew)",
        "**Club.** A heavier club.",
        "Club\nSimple melee weapon",
    ] {
        assert!(defines(source, "Club"), "{source}");
    }
    for source in ["- club", "He swings a club at you.", "• Club"] {
        assert!(!defines(source, "Club"), "{source}");
    }
}

#[test]
fn named_options_in_one_table_belong_to_the_heading_that_prints_it() {
    let rows = [
        other("Low Sweep", "table row (option)"),
        other("High Feint", "table row (option)"),
        other("Shield Bash", "table row (option)"),
    ];
    let tables = option_tables(&rows, TRICKS);
    assert_eq!(
        tables,
        vec![OptionTable {
            owner: "Battle Tricks".into(),
            rows: vec![
                "Low Sweep".into(),
                "High Feint".into(),
                "Shield Bash".into()
            ],
            count: Some(2),
        }]
    );
    // The rule the detection names wins over the heading; plain table rows are no options.
    let named: Vec<OtherItem> = ["Low Sweep", "High Feint", "Shield Bash"]
        .iter()
        .map(|n| other(n, "option of Martial Tricks"))
        .collect();
    assert_eq!(option_tables(&named, TRICKS)[0].owner, "Martial Tricks");
    // Plain table rows are options when the rule prints a pick count, else not.
    let plain: Vec<OtherItem> = ["Low Sweep", "High Feint", "Shield Bash"]
        .iter()
        .map(|n| other(n, "table row"))
        .collect();
    assert_eq!(option_tables(&plain, TRICKS)[0].count, Some(2));
    let no_pick = TRICKS.replace(
        "can learn two tricks from the table below and use",
        "may use",
    );
    assert!(option_tables(&plain, &no_pick).is_empty());
}

#[test]
fn options_of_a_table_flattened_into_lines_are_found_by_their_names() {
    let flat = "# Battle Tricks

Choose three tricks.

Low Sweep The target falls prone.
High Feint You gain advantage.
Shield Bash The target is pushed.

# Notes
";
    let rows: Vec<OtherItem> = ["Low Sweep", "High Feint", "Shield Bash"]
        .iter()
        .map(|n| other(n, "option"))
        .collect();
    let tables = option_tables(&rows, flat);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].owner, "Battle Tricks");
    assert_eq!(tables[0].count, Some(3));
}

fn option_plan() -> ImportPlan {
    let rows = [
        other("Low Sweep", "table row (option)"),
        other("High Feint", "table row (option)"),
        other("Shield Bash", "table row (option)"),
    ];
    let mut groups: Vec<PlanGroup> = rows
        .iter()
        .map(|o| super::super::plan::text_group(&o.name, &o.what, ""))
        .collect();
    let tables = option_tables(&rows, TRICKS);
    let notes = attach_option_tables(&mut groups, &tables);
    assert_eq!(notes.len(), 1);
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::super::plan::rebuild(&mut plan, &library(), "HB", TRICKS);
    plan
}

#[test]
fn a_table_of_options_becomes_options_of_its_rule_not_loose_text() {
    let plan = option_plan();
    let owner = plan
        .elements
        .iter()
        .find(|e| e.name == "Battle Tricks")
        .expect("the rule is planned");
    assert_eq!(owner.role, Role::Text);
    let picks = owner.picks.as_ref().expect("the rule offers its options");
    assert_eq!(picks.key, "Battle Tricks");
    let options: Vec<&str> = plan
        .elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(owner.id.as_str()))
        .inspect(|e| {
            assert_eq!(e.role, Role::Option);
            assert_eq!(e.ty, super::super::plan::RULE_OPTION_TYPE);
        })
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(options, vec!["Low Sweep", "High Feint", "Shield Bash"]);
    // No row is planned twice as a text entry.
    assert_eq!(
        plan.elements
            .iter()
            .filter(|e| e.role == Role::Text)
            .count(),
        1
    );
}

#[test]
fn a_table_owned_by_an_element_kind_keeps_its_rows() {
    let rows = [
        other("Low Sweep", "option of Battle Tricks"),
        other("High Feint", "option of Battle Tricks"),
        other("Shield Bash", "option of Battle Tricks"),
    ];
    let mut groups = vec![super::super::plan::standalone(
        ImportKind::Feat,
        "Battle Tricks",
        None,
    )];
    groups.extend(
        rows.iter()
            .map(|o| super::super::plan::text_group(&o.name, &o.what, "")),
    );
    let notes = attach_option_tables(&mut groups, &option_tables(&rows, TRICKS));
    assert!(notes.is_empty());
    assert_eq!(groups.len(), 4);
}

// ---- whole imports with a scripted model -------------------------------------------------

/// Answers detection with `detect`; every other call fails (none is expected).
struct Fake {
    detect: Value,
    other_calls: AtomicUsize,
}

impl AiRunner for Fake {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        let is_detect = req
            .schema
            .as_ref()
            .is_some_and(|s| s.pointer("/properties/other").is_some());
        let out = if is_detect {
            Ok(AiOutcome {
                text: self.detect.to_string(),
                ..Default::default()
            })
        } else {
            self.other_calls.fetch_add(1, Ordering::SeqCst);
            Err(AiError::Invalid("no call expected".into()))
        };
        Box::pin(async move { out })
    }
}

impl VisionRunner for Fake {}

fn request(source: &str) -> AnythingRequest {
    AnythingRequest {
        source: ImportSource::Text(source.into()),
        id_prefix: "HB".into(),
        source_name: "Test Source".into(),
        pack_id: "test-pack".into(),
        pack_name: "Test Pack".into(),
        edition_hint: Edition::Y2014,
    }
}

async fn import(source: &str, other: Value) -> super::super::types::ImportDraft {
    let fake = Fake {
        detect: json!({"kind": "mixed", "edition": "2014", "confidence": 0.9, "items": [],
            "other": other}),
        other_calls: 0.into(),
    };
    let step = import_anything(
        &fake,
        &library_builder(),
        &request(source),
        &RunOptions::default(),
        None,
    )
    .await
    .expect("an import with no element kinds still finishes");
    assert_eq!(fake.other_calls.load(Ordering::SeqCst), 0);
    match step {
        ImportStep::Draft(d) => *d,
        ImportStep::NeedsChoice(_) => panic!("no choice expected"),
    }
}

// The narrative section comes first: an unplanned heading after a text entry continues it
// (a flattened FAQ), which is the passage code's call, not the intake's.
const GUIDE: &str = "# The Drowned Bell\n\n\
Read aloud: A bell tolls beneath the water as you step onto the pier and the lamps gutter out.\n\n\
# Founding of Lowmarsh\n\n\
The town was raised on stilts after the third flood, and its people still count years from that night.\n";

#[tokio::test]
async fn a_lore_only_source_is_a_text_pack_with_a_report_not_an_error() {
    let draft = import(
        GUIDE,
        json!([{"name": "Founding of Lowmarsh", "what": "lore section"},
               {"name": "The Drowned Bell", "what": "narrative (keyed area)"}]),
    )
    .await;
    assert_eq!(draft.detection.kind, ImportKind::Rules);
    let names: Vec<&str> = draft.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Founding of Lowmarsh"]);
    let done = finalize_draft(&library_builder(), &draft).expect("finalizes");
    let report = done.pack.report.expect("a report");
    assert!(
        report
            .source_warnings
            .iter()
            .any(|w| w.starts_with("nothing mechanical was found")),
        "{:?}",
        report.source_warnings
    );
    let bell = report
        .entries
        .iter()
        .find(|e| e.source == "The Drowned Bell")
        .expect("the narrative is reported");
    assert_eq!(bell.outcome, "skipped");
    assert!(bell.note.starts_with("narrative text"), "{}", bell.note);
}

#[tokio::test]
async fn a_source_with_nothing_detected_is_an_empty_pack_with_a_report() {
    let draft = import("Just a sentence with no heading at all.", json!([])).await;
    assert!(draft.elements.is_empty());
    let done = finalize_draft(&library_builder(), &draft).expect("an empty pack finalizes");
    let report = done.pack.report.expect("a report");
    assert!(report.source_warnings[0].contains("the pack is empty"));
}

#[tokio::test]
async fn options_named_only_in_an_options_table_are_planned_as_options() {
    // Generating the options needs the model (needs live); the plan is checked here.
    let fake = Fake {
        detect: json!({"kind": "mixed", "edition": "2014", "confidence": 0.9, "items": [],
            "other": [{"name": "Low Sweep", "what": "table row (option)"},
                      {"name": "High Feint", "what": "table row (option)"},
                      {"name": "Shield Bash", "what": "table row (option)"}]}),
        other_calls: 0.into(),
    };
    let (detection, text) = super::super::detect_source(
        &fake,
        &ImportSource::Text(TRICKS.into()),
        Edition::Y2014,
        &RunOptions::default(),
    )
    .await
    .expect("detection with only `other` entries is no error");
    let plan = super::super::plan_import(
        &fake,
        &library_builder(),
        &detection,
        &text,
        "HB",
        &RunOptions::default(),
    )
    .await
    .expect("plans");
    let options = plan
        .elements
        .iter()
        .filter(|e| e.role == Role::Option)
        .count();
    assert_eq!(options, 3);
    assert!(plan.notes.iter().any(|n| n.contains("options it offers")));
}

const BOOK: &str = "# Book of Embers\n\n## Table of Contents\n\nForeword..........pg.2\n\
Ember Rules . . . . . . 5\nAppendix… 9\n\
Dear reader, this book was written over many long winters by a tired author who wanted \
to share every rule he ever made with the world, and he hopes you enjoy reading it.\n\n\
## Ember Rules\n\nAn ember burns for one minute and sheds dim light in a 5-foot radius.\n\n\
## Appendix\n\nA closing note on the embers and where they came from in the old days.\n";

#[tokio::test]
async fn a_contents_page_is_not_imported_and_the_text_after_it_is_the_entry_it_lists() {
    let draft = import(
        BOOK,
        json!([{"name": "Table of Contents", "what": "table of contents"},
               {"name": "Foreword", "what": "lore section"},
               {"name": "Ember Rules", "what": "rules section"},
               {"name": "Appendix", "what": "lore section"}]),
    )
    .await;
    let names: Vec<&str> = draft.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&"Table of Contents"), "{names:?}");
    let foreword = draft
        .elements
        .iter()
        .find(|e| e.name == "Foreword")
        .unwrap_or_else(|| panic!("{names:?} {:?}", draft.plan.notes));
    let text = foreword.json["description"].as_str().unwrap();
    assert!(text.contains("Dear reader"), "{text}");
    assert!(
        !text.contains("pg.2") && !text.contains("embers and where"),
        "{text}"
    );
    let done = finalize_draft(&library_builder(), &draft).expect("finalizes");
    let report = done.pack.report.expect("a report");
    let toc = report
        .entries
        .iter()
        .find(|e| e.source == "Table of Contents")
        .expect("the contents page is reported");
    assert_eq!(toc.outcome, "skipped");
    assert!(toc.note.starts_with("a contents page"), "{}", toc.note);
}
