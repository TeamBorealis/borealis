use std::sync::atomic::{AtomicUsize, Ordering};

use borealis_ai::task::{AiRunner, RunOptions};
use borealis_ai::AiError;
use serde_json::json;

use super::super::{fill_pending, pending};
use super::*;
use crate::homebrew::PackContext;
use crate::import::anything::fidelity::tests::planned;

const SOURCE: &str = "# Gloomwing\n\nGloomwings roost under old bridges and hum at dusk. A swarm has 40 hit points between them, the villagers say.\n\n# Reed Stalker\n\nReed stalkers wait in the shallows.\n**Reed Stalker**\nMedium beast, unaligned\nArmor Class 14 (natural armor)\nHit Points 22 (4d8 + 4)\nSpeed 30 ft., swim 30 ft.\nSTR | DEX | CON | INT | WIS | CHA\n14 (+2) | 15 (+2) | 12 (+1) | 3 (-4) | 12 (+1) | 6 (-2)\nChallenge 1 (200 XP)";

fn monster(id: &str, name: &str, from: usize, to: usize) -> PlannedElement {
    let mut p = planned(id, name, MONSTER, Role::Standalone, Some(from));
    p.excerpt.as_mut().unwrap().end_line = to;
    p
}

fn draft() -> ImportDraft {
    let mut d: ImportDraft = serde_json::from_value(json!({"edition": "2014",
        "id_prefix": "ID_T", "source_name": "T", "pack_id": "t", "pack_name": "T",
        "source_kind": "text", "source_text": SOURCE,
        "detection": {"kind": "monster", "edition": "2014", "confidence": 1.0, "items": []},
        "plan": {"edition": "2014", "groups": [
            {"kind": "monster", "name": "Gloomwing", "root_id": "M1", "outline": null, "parent": null, "choice": null},
            {"kind": "monster", "name": "Reed Stalker", "root_id": "M2", "outline": null, "parent": null, "choice": null}],
            "elements": [], "notes": []},
        "elements": [], "sweep_issues": []}))
    .unwrap();
    d.plan.elements = vec![
        monster("M1", "Gloomwing", 1, 3),
        monster("M2", "Reed Stalker", 5, 15),
    ];
    d.elements = pending(&d.plan);
    d
}

/// A generated stat block with these core numbers.
fn generated(ac: u64, hp: u64, abilities: [u64; 6]) -> Value {
    let [s, d, c, i, w, h] = abilities;
    json!({"id": "M2", "name": "Reed Stalker", "type": "Monster", "data": {
        "ac": [{"value": ac}], "hp": {"average": hp, "formula": "4d8+4"},
        "abilities": {"str": s, "dex": d, "con": c, "int": i, "wis": w, "cha": h}}})
}

fn with(d: &mut ImportDraft, id: &str, json: Value) {
    let e = d.elements.iter_mut().find(|e| e.id == id).unwrap();
    e.json = json;
    e.status = ElementStatus::Ok;
}

#[test]
fn a_stat_block_needs_two_kinds_of_stat_lines() {
    assert!(has_stat_block(SOURCE));
    assert!(has_stat_block("AC 13, HP 9 (2d6 + 2), Speed 10 ft."));
    assert!(has_stat_block(
        "STR DEX CON INT WIS CHA\n8 14 10 6 12 7\nCR 1/4"
    ));
    // Lore that mentions hit points once, or a stat word in passing, is not a stat block.
    assert!(!has_stat_block(
        "Gloomwings roost under old bridges. A swarm has 40 hit points between them."
    ));
    assert!(!has_stat_block(
        "Its armor class is legendary and its speed is 30 feet, they say."
    ));
}

/// Counts calls and refuses them all.
struct NoModel(AtomicUsize);

impl AiRunner for NoModel {
    fn run<'a>(
        &'a self,
        _req: &'a borealis_ai::AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<borealis_ai::AiEvent>>,
    ) -> borealis_ai::task::BoxFuture<'a, Result<borealis_ai::AiOutcome, AiError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(AiError::Invalid("no model in this test".into())) })
    }
}

fn ctx() -> PackContext {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let base = borealis_content::ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .unwrap();
    PackContext {
        base,
        ruleset: super::super::super::types::Edition::Y2014.ruleset(),
    }
}

#[tokio::test]
async fn a_creature_without_a_printed_stat_block_is_kept_as_text_without_a_model_call() {
    // Arrange: only the lore-only creature is planned.
    let mut d = draft();
    d.plan.elements.truncate(1);
    d.elements.truncate(1);
    let runner = NoModel(0.into());
    // Act
    fill_pending(&runner, &ctx(), &mut d, &RunOptions::default(), None).await;
    // Assert: no call, a text entry with the printed passage and no stat block, reported.
    assert_eq!(runner.0.load(Ordering::SeqCst), 0);
    let e = &d.elements[0];
    assert_eq!(e.ty, TEXT_TYPE);
    assert_eq!(e.role, Role::Text);
    assert_eq!(e.status, ElementStatus::NeedsReview);
    assert_eq!(e.issues, vec![STAT_BLOCK_ABSENT.to_string()]);
    assert_eq!(e.json["type"], TEXT_TYPE);
    assert!(e.json.get("data").is_none(), "{}", e.json);
    let desc = e.json["description"].as_str().unwrap();
    assert!(desc.contains("roost under old bridges"), "{desc}");
    assert_eq!(d.plan.elements[0].ty, TEXT_TYPE);
    assert_eq!(d.plan.groups[0].kind, ImportKind::Rules);
    assert!(
        d.detection
            .notes
            .iter()
            .any(|n| n.starts_with(READING_NOTE) && n.contains("\"Gloomwing\"")),
        "{:?}",
        d.detection.notes
    );
}

#[test]
fn a_printed_stat_block_copied_exactly_passes() {
    let mut d = draft();
    with(&mut d, "M2", generated(14, 22, [14, 15, 12, 3, 12, 6]));
    assert!(sweep(&d).is_empty());
    settle(&mut d);
    let e = d.element("M2").unwrap();
    assert_eq!(e.ty, MONSTER);
    assert_eq!(e.status, ElementStatus::Ok);
    assert!(e.issues.is_empty());
}

#[test]
fn a_few_unprinted_numbers_are_repaired_then_marked_for_review() {
    let mut d = draft();
    with(&mut d, "M2", generated(16, 22, [14, 15, 12, 3, 12, 8]));
    let issues = sweep(&d);
    assert_eq!(issues.len(), 1);
    assert!(
        issues[0].message.contains("AC 16, CHA 8"),
        "{}",
        issues[0].message
    );
    settle(&mut d);
    let e = d.element("M2").unwrap();
    assert_eq!(e.ty, MONSTER);
    assert_eq!(e.status, ElementStatus::NeedsReview);
    assert!(e.issues[0].contains("AC 16, CHA 8"), "{:?}", e.issues);
}

#[test]
fn an_invented_stat_block_is_dropped_and_the_creature_kept_as_text() {
    // Most numbers are not printed: the stat block is not the printed one.
    let mut d = draft();
    with(&mut d, "M2", generated(17, 95, [19, 11, 17, 5, 13, 9]));
    settle(&mut d);
    let e = d.element("M2").unwrap();
    assert_eq!(e.ty, TEXT_TYPE);
    assert_eq!(e.issues, vec![STAT_BLOCK_UNPRINTED.to_string()]);
    assert!(e.json["description"]
        .as_str()
        .unwrap()
        .contains("Armor Class 14"));
    // A stat block generated where none is printed (a revision, a regeneration) goes too.
    let mut d = draft();
    let mut lore = generated(13, 9, [3, 16, 10, 2, 12, 4]);
    lore["id"] = json!("M1");
    with(&mut d, "M1", lore);
    d.sweep_issues.push(SweepIssue {
        level: 0,
        element_id: Some("M1".into()),
        message: "invalid monster data".into(),
    });
    settle(&mut d);
    let e = d.element("M1").unwrap();
    assert_eq!(e.ty, TEXT_TYPE);
    assert_eq!(e.issues, vec![STAT_BLOCK_ABSENT.to_string()]);
    assert!(d.sweep_issues.is_empty());
}
