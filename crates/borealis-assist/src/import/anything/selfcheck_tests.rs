use std::sync::atomic::{AtomicUsize, Ordering};

use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use serde_json::{json, Value};

use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};
use crate::import::anything::types::SourceExcerpt;

fn located(mut p: PlannedElement, start: usize, end: usize) -> PlannedElement {
    p.excerpt = Some(SourceExcerpt {
        start_line: start,
        end_line: end,
        snippet: String::new(),
    });
    p
}

fn core() -> ContentStoreBuilder {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .expect("core loads")
}

fn failed_codes(e: &DraftElement) -> Vec<(&str, &str)> {
    e.checks
        .iter()
        .filter(|c| c.failed)
        .map(|c| (c.code.as_str(), c.message.as_str()))
        .collect()
}

const FEAT_SOURCE: &str =
    "## Feats\n\n### Cinder Step\n\nWhen you hit a creature, it takes an extra \
1d8 fire damage, and you can move up to thirty feet. You can do so twenty-five times.\n";

fn cinder(rules: Value) -> ImportDraft {
    let p = located(
        planned(
            "ID_T_FEAT_CINDER_STEP",
            "Cinder Step",
            "Feat",
            Role::Standalone,
            None,
        ),
        3,
        5,
    );
    let mut d = draft(FEAT_SOURCE, vec![p], &["<p>When you hit a creature, it takes an extra 1d8 fire damage, and you can move up to thirty feet. You can do so twenty-five times.</p>"]);
    d.elements[0].json["rules"] = rules;
    d.elements[0].json["source"] = json!("T");
    d
}

#[test]
fn numbers_printed_as_dice_words_or_feet_pass() {
    // Arrange: "d8" for the printed "1d8", 30 for "thirty feet", 25 for "twenty-five".
    let mut d = cinder(json!([
        {"kind": "extra_damage", "dice": "d8"},
        {"kind": "speed", "speed": 30},
        {"kind": "resource", "id": "cinder", "name": "Cinder",
         "max": {"type": "fixed", "value": 25}, "recharge": ["long_rest"]}
    ]));
    // Act
    apply(&core(), &mut d);
    // Assert
    let e = &d.elements[0];
    assert!(failed_codes(e).is_empty(), "{:?}", e.checks);
    assert!(e
        .checks
        .iter()
        .any(|c| c.code == NUMBER_NOT_IN_SOURCE && !c.failed));
}

#[test]
fn a_number_the_passage_never_prints_fails_with_its_field() {
    let mut d = cinder(json!([
        {"kind": "extra_damage", "dice": "2d8"},
        {"kind": "sense", "sense": "darkvision", "range": 60}
    ]));
    apply(&core(), &mut d);
    let got = failed_codes(&d.elements[0]);
    assert!(
        got.contains(&(
            NUMBER_NOT_IN_SOURCE,
            "The dice 2d8 is not in the source text"
        )),
        "{got:?}"
    );
    assert!(
        got.contains(&(
            NUMBER_NOT_IN_SOURCE,
            "The range 60 is not in the source text"
        )),
        "{got:?}"
    );
}

#[test]
fn levels_and_ones_are_never_checked() {
    let mut d = cinder(json!([
        {"kind": "resource", "id": "cinder", "name": "Cinder", "level": 7,
         "max": {"type": "fixed", "value": 1}, "recharge": ["long_rest"]}
    ]));
    apply(&core(), &mut d);
    assert!(failed_codes(&d.elements[0]).is_empty());
}

const TABLE_SOURCE: &str =
    "## Gloam Lantern\n\nWhen you light it, roll on the table.\n\nd4 | Effect\n\
1 | The flame turns blue\n2 | A moth swarm gathers\n3 | Shadows lengthen around you\n\
4 | The lantern sings softly\n";

#[test]
fn a_table_row_the_element_leaves_out_is_named() {
    let p = located(
        planned(
            "ID_T_MAGIC_ITEM_GLOAM",
            "Gloam Lantern",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        8,
    );
    let mut d = draft(
        TABLE_SOURCE,
        vec![p],
        &["<p>When you light it, roll on the table.</p><table><tr><td>1</td><td>The flame turns blue</td></tr>\
<tr><td>2</td><td>A moth swarm gathers</td></tr><tr><td>4</td><td>The lantern sings softly</td></tr></table>"],
    );
    apply(&core(), &mut d);
    let got = failed_codes(&d.elements[0]);
    assert_eq!(got, [(TABLE_ROW_MISSING, "The table row \"3\" is missing")]);
}

#[test]
fn a_table_carried_whole_passes_and_its_header_is_not_a_row() {
    let p = located(
        planned(
            "ID_T_MAGIC_ITEM_GLOAM",
            "Gloam Lantern",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        8,
    );
    let mut d = draft(
        TABLE_SOURCE,
        vec![p],
        &["<p>When you light it, roll on the table.</p><p>1 The flame turns blue. 2 A moth swarm gathers. \
3 Shadows lengthen around you. 4 The lantern sings softly.</p>"],
    );
    apply(&core(), &mut d);
    let e = &d.elements[0];
    assert!(failed_codes(e).is_empty(), "{:?}", e.checks);
    assert!(e
        .checks
        .iter()
        .any(|c| c.code == TABLE_ROW_MISSING && !c.failed));
}

const CLASS_SOURCE: &str = "## Lamplighter\n\nLamplighters keep the roads lit through the long winter nights.\n\n\
Level | Proficiency Bonus | Features\n1st | +2 | Kindle, Steady Glow\n2nd | +2 | Wick Craft\n\
3rd | +2 | Ability Score Improvement\n\n### Kindle\n\nYou light a flame.\n\n### Steady Glow\n\nYour flame holds.\n";

#[test]
fn a_progression_feature_without_an_element_is_named_with_its_level() {
    let class = located(
        planned("ID_T_CLASS_LAMP", "Lamplighter", "Class", Role::Class, None),
        1,
        16,
    );
    let kindle = located(
        planned(
            "ID_T_CLASS_FEATURE_KINDLE",
            "Kindle",
            "Class Feature",
            Role::ClassFeature,
            None,
        ),
        10,
        12,
    );
    let glow = located(
        planned(
            "ID_T_CLASS_FEATURE_STEADY_GLOW",
            "Steady Glow",
            "Class Feature",
            Role::ClassFeature,
            None,
        ),
        14,
        16,
    );
    let mut d = draft(
        CLASS_SOURCE,
        vec![class, kindle, glow],
        &[
            "<p>Lamplighters keep the roads lit through the long winter nights.</p>",
            "<p>You light a flame.</p>",
            "<p>Your flame holds.</p>",
        ],
    );
    d.elements[0].json["data"] = json!({"progression": {"proficiency_bonus": [2, 2, 2]}});
    apply(&core(), &mut d);
    let got = failed_codes(&d.elements[0]);
    assert!(
        got.contains(&(
            TABLE_ROW_MISSING,
            "No element for \"Wick Craft\" (level 2 in the table)"
        )),
        "{got:?}"
    );
    assert!(
        !got.iter().any(|(_, m)| m.contains("Ability Score")),
        "{got:?}"
    );
    assert!(!got.iter().any(|(_, m)| m.contains("Kindle")), "{got:?}");
}

const HEADING_SOURCE: &str = "# The Ember Book\n\nA small book of fire for every table, written over many long winters by the \
keepers of the old roads.\n\n# Chapter One\n\n## Cinder Step\n\nYou move through \
the embers without harm, stepping lightly over the coals of the road.\n\n## Lantern Lore\n\nThe old keepers of the \
roads lit their lamps at dusk and kept them burning until the sun rose over the eastern hills again.\n\n\
## Ash Paths\n\nThese are the paths of ash.\n\n### Grey Path\n\nWalkers of the grey path leave no footprint in \
the dust and are never followed by the hounds of the night.\n\n## Credits\n\nWriting by a lantern keeper, art by \
a moth, layout by a quiet friend of the roads and the hills.\n";

fn heading_draft(description: &str) -> ImportDraft {
    let p = located(
        planned(
            "ID_T_FEAT_CINDER_STEP",
            "Cinder Step",
            "Feat",
            Role::Standalone,
            None,
        ),
        7,
        9,
    );
    draft(HEADING_SOURCE, vec![p], &[description])
}

fn failed_headings(d: &ImportDraft) -> Vec<&str> {
    d.source_checks
        .iter()
        .filter(|c| c.failed)
        .map(|c| c.message.as_str())
        .collect()
}

#[test]
fn a_content_heading_no_element_carries_is_reported() {
    // Arrange: the title page (before the first element), the "Ash Paths" lead-in (a short
    // body before its sub-heading) and the credits are structure, not content.
    let mut d = heading_draft(
        "<p>You move through the embers without harm, stepping lightly over the coals of the road.</p>",
    );
    // Act
    apply(&core(), &mut d);
    // Assert
    assert_eq!(
        failed_headings(&d),
        [
            "The section \"Lantern Lore\" was not imported",
            "The section \"Grey Path\" was not imported"
        ]
    );
    assert!(d.source_checks.iter().all(|c| c.code == HEADING_UNCOVERED));
}

#[test]
fn a_heading_whose_body_is_in_a_description_is_covered() {
    let mut d = heading_draft(
        "<p>You move through the embers without harm, stepping lightly over the coals of the road.</p>\
<p>The old keepers of the roads lit their lamps at dusk and kept them burning until the sun rose over the eastern \
hills again.</p><p>Walkers of the grey path leave no footprint in the dust and are never followed by the hounds of \
the night.</p>",
    );
    apply(&core(), &mut d);
    assert!(failed_headings(&d).is_empty(), "{:?}", d.source_checks);
    assert!(d.source_checks.iter().any(|c| !c.failed));
}

const LABEL_SOURCE: &str =
    "## Lantern Path\n\nWick Ward | 3rd\nYou gain proficiency in the Insight skill \
while your lantern burns.\n\nGlow Trick | 6th\nYour lantern sheds light.\n";

#[test]
fn a_lone_label_row_is_not_a_table() {
    let p = located(
        planned(
            "ID_T_SUBCLASS_FEATURE_WICK_WARD",
            "Wick Ward",
            "Archetype Feature",
            Role::SubclassFeature,
            None,
        ),
        3,
        4,
    );
    let mut d = draft(
        LABEL_SOURCE,
        vec![p],
        &["<p>You gain proficiency in the Insight skill while your lantern burns.</p>"],
    );
    apply(&core(), &mut d);
    assert!(
        failed_codes(&d.elements[0]).is_empty(),
        "{:?}",
        d.elements[0].checks
    );
}

#[test]
fn rows_after_another_element_starts_are_never_an_earlier_elements() {
    // Arrange: the lantern's passage was located far too long, over the next item's table row
    // and a row no element was planned for.
    let src = "## Gloam Lantern\n\nIt glows.\n\n1 | Moth cloak | You can fly 10 feet.\n\
2 | Shard bell | A bell that rings twice at dawn.\n3 | Ember coat | You resist fire.\n";
    let lantern = located(
        planned(
            "ID_T_MAGIC_ITEM_GLOAM",
            "Gloam Lantern",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        7,
    );
    let cloak = located(
        planned(
            "ID_T_MAGIC_ITEM_MOTH",
            "Moth Cloak",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        5,
        5,
    );
    let mut d = draft(
        src,
        vec![lantern, cloak],
        &["<p>It glows.</p>", "<p>You can fly 10 feet.</p>"],
    );
    apply(&core(), &mut d);
    assert!(
        failed_codes(&d.elements[0]).is_empty(),
        "{:?}",
        d.elements[0].checks
    );
}

#[test]
fn a_grant_of_an_id_nobody_has_is_unresolved_and_named_readably() {
    let mut d = cinder(json!([{"kind": "grant", "type": "Feat", "id": "ID_T_FEAT_ASH_WALK"}]));
    d.id_prefix = "ID_T".into();
    apply(&core(), &mut d);
    let got = failed_codes(&d.elements[0]);
    assert_eq!(
        got,
        [(
            REFERENCE_UNRESOLVED,
            "Points at something that does not exist: \"Feat Ash Walk\""
        )]
    );
}

#[test]
fn a_description_sentence_not_in_the_source_fails_the_fidelity_check() {
    let mut d = cinder(json!([]));
    d.elements[0].json["description"] = json!(
        "<p>This feat was beloved by the ancient fire dancers of the southern isles long ago.</p>"
    );
    apply(&core(), &mut d);
    let got = failed_codes(&d.elements[0]);
    assert_eq!(
        got,
        [(
            DESCRIPTION_UNTRACED,
            "One sentence of the description is not in the source"
        )]
    );
}

#[test]
fn pending_and_failed_elements_get_no_checks() {
    let mut d = cinder(json!([{"kind": "extra_damage", "dice": "4d12"}]));
    d.elements[0].status = ElementStatus::Failed;
    apply(&core(), &mut d);
    assert!(d.elements[0].checks.is_empty());
}

// ---------------------------------------------------------------------------------------------
// The repair path (fake model)

/// A fake model that answers every repair with `answer` (or refuses when `None`) and records
/// the prompts it saw.
struct Fake {
    answer: Option<Value>,
    calls: AtomicUsize,
    prompts: std::sync::Mutex<Vec<String>>,
}

impl Fake {
    fn new(answer: Option<Value>) -> Self {
        Self {
            answer,
            calls: 0.into(),
            prompts: Vec::new().into(),
        }
    }
}

impl AiRunner for Fake {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts.lock().unwrap().push(req.prompt.clone());
        let answer = self.answer.clone();
        Box::pin(async move {
            match answer {
                Some(el) => Ok(AiOutcome {
                    text: json!({"element": el, "notes": []}).to_string(),
                    ..Default::default()
                }),
                None => Err(AiError::Invalid("the import's call budget is spent".into())),
            }
        })
    }
}

fn ctx() -> PackContext {
    PackContext {
        base: core(),
        ruleset: crate::import::anything::types::Edition::Y2024.ruleset(),
    }
}

/// Two feats: one whose damage dice the source never prints, one that passes every check.
fn two_feats() -> ImportDraft {
    let src = "## Feats\n\n### Cinder Step\n\nWhen you hit a creature, it takes an extra 1d8 fire damage.\n\n\
### Ash Ward\n\nYou gain a +2 bonus to your Armor Class while you stand in smoke.\n";
    let a = located(
        planned(
            "ID_T_FEAT_CINDER_STEP",
            "Cinder Step",
            "Feat",
            Role::Standalone,
            None,
        ),
        3,
        5,
    );
    let b = located(
        planned(
            "ID_T_FEAT_ASH_WARD",
            "Ash Ward",
            "Feat",
            Role::Standalone,
            None,
        ),
        7,
        9,
    );
    let mut d = draft(
        src,
        vec![a, b],
        &[
            "<p>When you hit a creature, it takes an extra 1d8 fire damage.</p>",
            "<p>You gain a +2 bonus to your Armor Class while you stand in smoke.</p>",
        ],
    );
    d.elements[0].json["source"] = json!("T");
    d.elements[1].json["source"] = json!("T");
    d.elements[0].json["rules"] = json!([{"kind": "extra_damage", "dice": "3d8", "type": "fire"}]);
    d
}

fn fixed_cinder() -> Value {
    json!({"id": "ID_T_FEAT_CINDER_STEP", "name": "Cinder Step", "type": "Feat", "source": "T",
        "description": "<p>When you hit a creature, it takes an extra 1d8 fire damage.</p>",
        "rules": [{"kind": "extra_damage", "dice": "1d8", "type": "fire"}]})
}

#[tokio::test]
async fn a_failing_element_gets_one_repair_with_its_failed_checks_and_keeps_the_better_answer() {
    // Arrange
    let mut d = two_feats();
    let runner = Fake::new(Some(fixed_cinder()));
    // Act
    heal(&runner, &ctx(), &mut d, &RunOptions::default(), None).await;
    // Assert: one call, for the failing element only, told what failed.
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    let prompt = runner.prompts.lock().unwrap()[0].clone();
    assert!(
        prompt.contains("The dice 3d8 is not in the source text"),
        "{prompt}"
    );
    let e = &d.elements[0];
    assert_eq!(e.json["rules"][0]["dice"], "1d8", "{}", e.json);
    assert_eq!(e.status, ElementStatus::Repaired);
    assert_eq!(e.repair_rounds, 1);
    assert!(failed_codes(e).is_empty(), "{:?}", e.checks);
    assert_eq!(d.elements[1].repair_rounds, 0);
}

#[tokio::test]
async fn a_repair_that_fails_as_many_checks_is_not_kept() {
    let mut d = two_feats();
    let mut worse = fixed_cinder();
    worse["rules"][0]["dice"] = json!("5d8");
    let runner = Fake::new(Some(worse));
    heal(&runner, &ctx(), &mut d, &RunOptions::default(), None).await;
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    let e = &d.elements[0];
    assert_eq!(e.json["rules"][0]["dice"], "3d8");
    assert_eq!(e.status, ElementStatus::Ok);
    assert_eq!(
        failed_codes(e),
        [(
            NUMBER_NOT_IN_SOURCE,
            "The dice 3d8 is not in the source text"
        )]
    );
}

#[tokio::test]
async fn a_refused_call_ends_the_repairs_and_keeps_the_draft() {
    let mut d = two_feats();
    let runner = Fake::new(None);
    heal(&runner, &ctx(), &mut d, &RunOptions::default(), None).await;
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    assert_eq!(d.elements[0].json["rules"][0]["dice"], "3d8");
    assert_eq!(d.elements[0].repair_rounds, 0);
}

#[tokio::test]
async fn a_draft_that_passes_every_check_costs_no_call() {
    let mut d = two_feats();
    d.elements[0].json = fixed_cinder();
    let runner = Fake::new(Some(fixed_cinder()));
    heal(&runner, &ctx(), &mut d, &RunOptions::default(), None).await;
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    assert!(d.elements.iter().all(|e| !e.checks.is_empty()));
}

// ---------------------------------------------------------------------------------------------
// Offline measurement tool

/// `BOREALIS_SELFCHECK_DRAFTS=<draft.json>[:<draft.json>...] cargo test -p borealis-assist --lib
/// -- --ignored selfcheck_stored_drafts --nocapture` prints every failed check of each stored
/// draft (no model); with `BOREALIS_SELFCHECK_OUT=<dir>` it also writes each draft with its
/// checks to `<dir>/<n>/draft.json` and the pack re-finalized without and with the checks to
/// `<dir>/<n>/{before,after}/imported/`.
#[test]
#[ignore = "needs BOREALIS_SELFCHECK_DRAFTS (stored draft.json files)"]
fn selfcheck_stored_drafts() {
    let Ok(paths) = std::env::var("BOREALIS_SELFCHECK_DRAFTS") else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    for (n, path) in paths.split(':').enumerate() {
        let text = std::fs::read_to_string(path).expect("draft reads");
        let mut d: ImportDraft = serde_json::from_str(&text).expect("draft parses");
        let before = d.clone();
        apply(&b, &mut d);
        println!("== draft {n}: {path}");
        let mut counts: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
        for c in d
            .elements
            .iter()
            .flat_map(|e| &e.checks)
            .chain(&d.source_checks)
        {
            let entry = counts.entry(c.code.as_str()).or_default();
            if c.failed {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
        for (code, (f, p)) in &counts {
            println!("count {code}: failed {f}, passed {p}");
        }
        for c in d.source_checks.iter().filter(|c| c.failed) {
            println!("FAIL [import] {}: {}", c.code, c.message);
        }
        for e in &d.elements {
            for c in e.checks.iter().filter(|c| c.failed) {
                println!(
                    "FAIL [{} | {} | {}] {}: {}",
                    e.id, e.ty, e.name, c.code, c.message
                );
            }
        }
        if let Ok(out) = std::env::var("BOREALIS_SELFCHECK_OUT") {
            let dir = std::path::Path::new(&out).join(n.to_string());
            std::fs::create_dir_all(&dir).expect("out dir");
            std::fs::write(dir.join("draft.json"), serde_json::to_string(&d).unwrap())
                .expect("draft writes");
            for (draft, to) in [(&before, "before"), (&d, "after")] {
                let done = crate::import::anything::finalize::finalize_draft(&b, draft)
                    .expect("finalizes");
                done.pack
                    .write_to_dir(&dir.join(to).join("imported"))
                    .expect("pack writes");
            }
        }
    }
}

#[test]
fn a_number_the_rules_library_prints_for_the_same_item_passes() {
    // Arrange: the source prices a weapon only; its dice come from the library's "Shortsword".
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let base = core()
        .add_pack_dir(&root.join("srd-5.1"))
        .expect("srd loads");
    let src =
        "## Smithing\n\nItem | Cost | Days\nShort Sword | 10 gp | 3\nHook Shield | 12 gp | 4\n";
    let sword = located(
        planned(
            "ID_T_WEAPON_SHORT_SWORD",
            "Short Sword",
            "Weapon",
            Role::Standalone,
            None,
        ),
        4,
        4,
    );
    let shield = located(
        planned(
            "ID_T_ARMOR_HOOK_SHIELD",
            "Hook Shield",
            "Armor",
            Role::Standalone,
            None,
        ),
        5,
        5,
    );
    let mut d = draft(
        src,
        vec![sword, shield],
        &["<p>Short sword.</p>", "<p>Hook shield.</p>"],
    );
    d.elements[0].json["data"] = json!({"damage": {"dice": "1d6"}, "cost": {"value": 10}});
    d.elements[1].json["data"] = json!({"weight": 9, "cost": {"value": 12}});
    // Act
    apply(&base, &mut d);
    // Assert: the shield's weight is printed nowhere.
    assert!(
        failed_codes(&d.elements[0]).is_empty(),
        "{:?}",
        d.elements[0].checks
    );
    assert_eq!(
        failed_codes(&d.elements[1]),
        [(
            NUMBER_NOT_IN_SOURCE,
            "The weight 9 is not in the source text"
        )]
    );
}

#[test]
fn only_the_feature_column_of_a_progression_table_names_features() {
    let src = "## Lamplighter\n\nLamplighters keep the roads lit through the long winter nights.\n\n\
Level | Features | Wicks\n1st | Kindle | 2\n2nd | Wick Craft | 3\n\n### Path Spells\n\n\
Level | Spells\n1st | faerie fire, light\n3rd | moonbeam, darkness\n\n### Kindle\n\nYou light a flame.\n";
    let class = located(
        planned("ID_T_CLASS_LAMP", "Lamplighter", "Class", Role::Class, None),
        1,
        18,
    );
    let kindle = located(
        planned(
            "ID_T_CLASS_FEATURE_KINDLE",
            "Kindle",
            "Class Feature",
            Role::ClassFeature,
            None,
        ),
        16,
        18,
    );
    let mut d = draft(
        src,
        vec![class, kindle],
        &[
            "<p>Lamplighters keep the roads lit through the long winter nights.</p>",
            "<p>You light a flame.</p>",
        ],
    );
    apply(&core(), &mut d);
    let got: Vec<&str> = failed_codes(&d.elements[0])
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    assert_eq!(
        got,
        ["No element for \"Wick Craft\" (level 2 in the table)"]
    );
}

#[test]
fn a_table_column_dumped_under_a_heading_is_not_a_section() {
    let src = "## Cinder Step\n\nYou move through the embers without harm, stepping lightly over the coals.\n\n\
### Requirement\n\nA lantern\nAny lamp\nAny lamp\nA torch or a candle\nAny lamp\nA brazier\nAny lamp\nA wick\n\
Any lamp except a broken one\nA lantern\nA candle stub\nAny lamp\n";
    let p = located(
        planned(
            "ID_T_FEAT_CINDER_STEP",
            "Cinder Step",
            "Feat",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(
        src,
        vec![p],
        &["<p>You move through the embers without harm, stepping lightly over the coals.</p>"],
    );
    apply(&core(), &mut d);
    assert!(failed_headings(&d).is_empty(), "{:?}", d.source_checks);
}
