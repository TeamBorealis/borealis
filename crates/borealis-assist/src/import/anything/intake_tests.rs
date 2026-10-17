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
            rolled: false,
            heading: Some("Battle Tricks".into()),
        }]
    );
    // The rule the detection names (and lists) wins over the heading; a rule it names that
    // the source neither prints nor lists does not.
    let mut named: Vec<OtherItem> = ["Low Sweep", "High Feint", "Shield Bash"]
        .iter()
        .map(|n| other(n, "option of Martial Tricks"))
        .collect();
    assert_eq!(option_tables(&named, TRICKS)[0].owner, "Battle Tricks");
    named.push(other("Martial Tricks", "rule"));
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

/// A glossary rule that says a character picks gambits, and the gambits as rows of a table
/// printed later under a caption of its own: a level column first, a wrapped cell, blank lines
/// between rows. The rows' `what` names their kind, not "option".
const GLOSSARY: &str = "# Glossary\n\n## Gambits\n\n\
A duelist knows two gambits of their choice from the Gambit Table.\n\n\
## Footwork\n\nStay light on your feet.\n\n\
## Gambit Table\n\n| Level | Gambit | Effect |\n|---|---|---|\n\
| 1st | Quick Step | You move 5 feet and the target\ncannot take reactions until your next turn.\n\n\
| 1st | Low Blow | The target falls prone. |\n\n\
| 3rd | Iron Feint | You gain advantage. |\n\n\
| 3rd | Last Word | You shout. |\n\n# Next\n\nOther.\n";

fn gambit_rows() -> Vec<OtherItem> {
    ["Quick Step", "Low Blow", "Iron Feint", "Last Word"]
        .iter()
        .map(|n| other(n, "gambit"))
        .collect()
}

#[test]
fn a_table_governed_by_a_glossary_rule_elsewhere_becomes_its_options() {
    let mut detected = vec![other("Gambits", "glossary entry")];
    detected.extend(gambit_rows());
    let tables = option_tables(&detected, GLOSSARY);
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(tables[0].owner, "Gambits");
    assert_eq!(tables[0].count, Some(2));
    assert_eq!(tables[0].rows.len(), 4);
    let mut groups: Vec<PlanGroup> = detected
        .iter()
        .map(|o| super::super::plan::text_group(&o.name, &o.what, ""))
        .collect();
    let notes = attach_option_tables(&mut groups, &tables);
    assert_eq!(notes.len(), 1);
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::super::plan::rebuild(&mut plan, &library(), "HB", GLOSSARY);
    let rule = plan
        .elements
        .iter()
        .find(|e| e.name == "Gambits")
        .expect("rule");
    let picks = rule
        .picks
        .as_ref()
        .expect("the glossary rule offers the gambits");
    assert_eq!(picks.choose[0].count, 2);
    // The options are rules entries like the rule that governs them.
    assert_eq!(picks.ty, "Rules Glossary Entry");
    let options: Vec<&str> = plan
        .elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(rule.id.as_str()))
        .inspect(|e| {
            assert_eq!(
                (e.role, e.ty.as_str()),
                (Role::Option, "Rules Glossary Entry")
            )
        })
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        options,
        vec!["Quick Step", "Low Blow", "Iron Feint", "Last Word"]
    );
    // Without any pick wording (and no "option" in `what`) the rows stay as they are.
    let plain = GLOSSARY.replace("knows two gambits of their choice from", "is described in");
    assert!(option_tables(&detected, &plain).is_empty());
}

/// An item, then a roll table of its versions under a caption naming the item in the plural.
const ITEM_TABLE: &str = "# Wonders\n\n#### Cloak of Hues\n\nWondrous item, rare\n\
This cloak shifts color. Roll on the table below to determine its hue.\n\n\
#### Types of cloaks of hues\n\nd4 | Hue | Effect\n\n1 | Ember | Fire resistance\n\n\
2 | Frost | Cold resistance\n\n3 | Storm | Lightning resistance\n\n4 | Dusk | Necrotic resistance\n\n\
#### Lantern of Echoes\n\nWondrous item, uncommon\nIt hums.\n";

#[test]
fn an_items_roll_table_gives_its_versions_never_options() {
    let rows: Vec<OtherItem> = ["Ember", "Frost", "Storm", "Dusk"]
        .iter()
        .map(|n| other(n, "option of Types of cloaks of hues"))
        .collect();
    let mut detected = vec![other("Types of cloaks of hues", "table")];
    detected.extend(rows.clone());
    let tables = option_tables(&detected, ITEM_TABLE);
    assert_eq!(tables.len(), 1);
    assert!(tables[0].rolled, "a die column and no pick count");
    let mut groups = vec![super::super::plan::standalone(
        ImportKind::Item,
        "Cloak of Hues",
        Some(super::super::types::ItemType::MagicItem),
    )];
    groups.extend(
        detected
            .iter()
            .map(|o| super::super::plan::text_group(&o.name, &o.what, "")),
    );
    let notes = attach_option_tables(&mut groups, &tables);
    assert_eq!(notes.len(), 1, "{notes:?}");
    let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Cloak of Hues",
            "Cloak of Hues, Ember",
            "Cloak of Hues, Frost",
            "Cloak of Hues, Storm",
            "Cloak of Hues, Dusk",
            "Types of cloaks of hues"
        ]
    );
    assert!(groups
        .iter()
        .filter(|g| g.name.starts_with("Cloak of Hues,"))
        .all(|g| g.kind == ImportKind::Item
            && g.item_type == Some(super::super::types::ItemType::MagicItem)));
    assert!(groups
        .iter()
        .all(|g| g.outline.as_ref().map_or(true, |o| o.features.is_empty())));
    // With no item planned, a roll table is no options table either.
    let mut texts: Vec<PlanGroup> = detected
        .iter()
        .map(|o| super::super::plan::text_group(&o.name, &o.what, ""))
        .collect();
    assert!(attach_option_tables(&mut texts, &tables).is_empty());
    assert_eq!(texts.len(), detected.len());
}

/// A class whose feature prints its table of options; the rows' `what` names the feature.
const FEATURE_TABLE: &str = "# Duelist\n\n## Fencing Arts\n\n\
At 2nd level you learn two arts from the table below.\n\n\
| Art | Effect |\n|---|---|\n| Parry Back | You strike back. |\n\
| Blade Lock | You hold the blade. |\n| Long Reach | You reach 5 feet more. |\n\n# Next\n";

#[test]
fn a_table_of_a_planned_feature_becomes_that_features_options() {
    let rows: Vec<OtherItem> = ["Parry Back", "Blade Lock", "Long Reach"]
        .iter()
        .map(|n| other(n, "option of Fencing Arts"))
        .collect();
    let mut class = super::super::plan::text_group("Duelist", "class", "");
    class.kind = ImportKind::Class;
    if let Some(o) = class.outline.as_mut() {
        o.features = vec![OutlineFeature {
            level: 2,
            name: "Fencing Arts".into(),
            summary: String::new(),
            quote: String::new(),
            parts: Vec::new(),
            options: vec![part("Parry Back")],
            choose: Vec::new(),
            spells: Vec::new(),
        }];
    }
    let mut groups = vec![class];
    groups.extend(
        rows.iter()
            .map(|o| super::super::plan::text_group(&o.name, &o.what, "")),
    );
    let notes = attach_option_tables(&mut groups, &option_tables(&rows, FEATURE_TABLE));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(groups.len(), 1, "the rows' text groups go");
    let f = &groups[0].outline.as_ref().expect("outline").features[0];
    let options: Vec<&str> = f.options.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(options, vec!["Parry Back", "Blade Lock", "Long Reach"]);
    assert_eq!(f.choose, vec![PickCount { level: 2, count: 2 }]);
}

/// A rule whose heading prints no text of its own and whose table is split by tier captions
/// and blank lines, printed under the next section.
const TIERED: &str = "# Gear\n\n### Upgrades\n\n### Fitting\n\n\
Upgrades are fitted at a forge.\n\n\
Tag | Prerequisite | Effect\n1st Tier (cost: 100 gp)\n\
Keen Edge | Any blade | You gain a +1 bonus to damage rolls.\n\n\
Light Grip | Any weapon | You draw it as part of moving.\n2nd Tier (cost: 1,000 gp)\n\
Rune Slot | A keen blade | A rune can be set in it.\n\n\
Deep Etch | Any weapon | The rune lasts a day longer.\n\n# Next\n\nOther.\n";

#[test]
fn a_table_split_by_captions_is_one_table_and_its_rule_keeps_its_options() {
    let mut detected = vec![other("Upgrades", "optional rule"), other("Fitting", "rule")];
    detected.extend(
        ["Keen Edge", "Light Grip", "Rune Slot", "Deep Etch"]
            .iter()
            .map(|n| other(n, "option of Upgrades")),
    );
    let tables = option_tables(&detected, TIERED);
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(tables[0].rows.len(), 4, "{tables:?}");
    let mut groups: Vec<PlanGroup> = detected
        .iter()
        .map(|o| super::super::plan::text_group(&o.name, &o.what, ""))
        .collect();
    attach_option_tables(&mut groups, &tables);
    let mut plan = ImportPlan {
        edition: Edition::Y2014,
        groups,
        elements: Vec::new(),
        notes: Vec::new(),
    };
    super::super::plan::rebuild(&mut plan, &library(), "HB", TIERED);
    let rule = plan
        .elements
        .iter()
        .find(|e| e.name == "Upgrades")
        .expect("the rule is kept");
    let ids: Vec<&str> = plan.elements.iter().map(|e| e.id.as_str()).collect();
    let options = plan
        .elements
        .iter()
        .filter(|e| e.role == Role::Option)
        .inspect(|e| {
            assert!(
                ids.contains(&e.parent.as_deref().unwrap_or("")),
                "no dangling parent"
            )
        })
        .filter(|e| e.parent.as_deref() == Some(rule.id.as_str()))
        .count();
    assert_eq!(options, 4);
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

/// Offline replay of a stored draft (no model): `BOREALIS_INTAKE_REPLAY=<draft.json>
/// BOREALIS_INTAKE_OUT=<dir> [BOREALIS_INTAKE_REPLAN=1] cargo test -p borealis-assist --lib --
/// --ignored replay_option_tables_of_a_stored_draft --nocapture`. With REPLAN, the stored
/// option-table placement is undone and redone with the current code and the plan rebuilt
/// before finalizing; elements the new plan adds have no generated text (needs live).
#[test]
#[ignore = "needs BOREALIS_INTAKE_REPLAY (a stored draft.json)"]
fn replay_option_tables_of_a_stored_draft() {
    let Ok(path) = std::env::var("BOREALIS_INTAKE_REPLAY") else {
        return;
    };
    let text = std::fs::read_to_string(&path).expect("draft reads");
    let mut draft: ImportDraft = serde_json::from_str(&text).expect("draft parses");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    if std::env::var("BOREALIS_INTAKE_REPLAN").is_ok() {
        let src = draft.source_text.clone();
        let mut groups = draft.plan.groups.clone();
        let mut rows: Vec<String> = Vec::new();
        for g in groups.iter_mut().filter(|g| g.kind == ImportKind::Rules) {
            if let Some(o) = g.outline.as_mut() {
                rows.extend(
                    o.features
                        .iter()
                        .flat_map(|f| f.options.iter().map(|p| p.name.clone())),
                );
                o.features.clear();
            }
        }
        for o in draft
            .detection
            .other
            .iter()
            .filter(|o| rows.contains(&o.name))
        {
            if !groups.iter().any(|g| g.name == o.name) {
                groups.push(super::super::plan::text_group(&o.name, &o.what, &o.note));
            }
        }
        let tables = option_tables(&draft.detection.other, &src);
        for t in &tables {
            println!("table of {} (rolled {}): {:?}", t.owner, t.rolled, t.rows);
        }
        let notes = attach_option_tables(&mut groups, &tables);
        let mut plan = draft.plan.clone();
        plan.groups = groups;
        super::super::plan::rebuild(&mut plan, &b.clone().build(), &draft.id_prefix, &src);
        plan.notes.extend(notes.iter().cloned());
        for n in &notes {
            println!("intake: {n}");
        }
        let before: Vec<&str> = draft
            .plan
            .elements
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        let after: Vec<&str> = plan.elements.iter().map(|e| e.name.as_str()).collect();
        for e in &plan.elements {
            if !before.contains(&e.name.as_str()) {
                println!("plan adds: {} ({}, {:?})", e.name, e.ty, e.role);
            } else if let Some(o) = draft
                .plan
                .elements
                .iter()
                .find(|o| o.name == e.name && o.ty != e.ty)
            {
                println!("plan retypes: {} {} -> {}", e.name, o.ty, e.ty);
            }
        }
        for e in &plan.elements {
            let old = draft.plan.elements.iter().find(|o| o.id == e.id);
            let span = |x: &super::super::types::PlannedElement| {
                x.excerpt.as_ref().map(|x| (x.start_line, x.end_line))
            };
            if let Some(o) = old.filter(|o| span(o) != span(e)) {
                println!("plan moves: {} {:?} -> {:?}", e.name, span(o), span(e));
            }
        }
        for n in plan
            .notes
            .iter()
            .filter(|n| n.starts_with("contents page: "))
        {
            println!("note: {n}");
        }
        for n in before.iter().filter(|n| !after.contains(n)) {
            println!("plan drops: {n}");
        }
        // A retyped element has a new id: its stored generation is carried over to it.
        let moved: Vec<(String, String)> = plan
            .elements
            .iter()
            .filter_map(|e| {
                let old = draft
                    .plan
                    .elements
                    .iter()
                    .find(|o| o.name == e.name && o.role == e.role && o.id != e.id)?;
                (!draft.elements.iter().any(|d| d.id == e.id))
                    .then(|| (old.id.clone(), e.id.clone()))
            })
            .collect();
        for (old, new) in moved {
            if let Some(d) = draft.elements.iter().find(|d| d.id == old).cloned() {
                let mut d = d;
                d.id = new.clone();
                if let Some(o) = d.json.as_object_mut() {
                    o.insert("id".into(), json!(new));
                }
                draft.elements.push(d);
            }
        }
        draft.plan = plan;
    }
    let done = finalize_draft(&b, &draft).expect("finalizes");
    if let Ok(out) = std::env::var("BOREALIS_INTAKE_OUT") {
        done.pack
            .write_to_dir(Path::new(&out))
            .expect("pack writes");
        println!("written to {out}");
    }
}
