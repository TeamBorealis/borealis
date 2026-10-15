//! The stat block check: a creature comes from a printed stat block. It fails when generation
//! kept a creature as text because its passage prints no stat block (the generation issues say
//! so), or when most core numbers of a generated stat block (armor class, average hit points,
//! ability scores) are not printed in the creature's passage: those were made up.

use serde_json::Value;

use super::{failed, passed, tokens};
use super::{DraftElement, ElementCheck, ImportDraft, STAT_BLOCK_ABSENT};

/// Element type of a creature.
const MONSTER: &str = "Monster";
/// Starts of the generation issues of a creature kept as text (`generate`'s stat block guard):
/// no stat block in its passage, or a generated one mostly not printed there.
const KEPT_AS_TEXT: [&str; 2] = [
    "stat block not found in the source text",
    "most numbers of the generated stat block are not printed",
];
/// Ability score keys of a stat block's `data.abilities`.
const ABILITY_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// Fewest core numbers a stat block needs before its share of unprinted ones means anything.
const MIN_CORE_NUMBERS: usize = 3;
/// Most core numbers named in one message.
const MAX_SHOWN: usize = 4;

/// The labelled core numbers of a generated stat block.
fn core_numbers(data: &Value) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for ac in data
        .get("ac")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(v) = ac.get("value").and_then(Value::as_u64) {
            out.push((format!("AC {v}"), v));
        }
    }
    if let Some(v) = data.pointer("/hp/average").and_then(Value::as_u64) {
        out.push((format!("hit points {v}"), v));
    }
    for k in ABILITY_KEYS {
        if let Some(v) = data
            .pointer(&format!("/abilities/{k}"))
            .and_then(Value::as_u64)
        {
            out.push((format!("{} {v}", k.to_uppercase()), v));
        }
    }
    out
}

/// The stat block check of `e` (no check for anything but a creature).
pub(super) fn stat_block_checks(draft: &ImportDraft, e: &DraftElement) -> Vec<ElementCheck> {
    if e.issues
        .iter()
        .any(|i| KEPT_AS_TEXT.iter().any(|k| i.starts_with(k)))
    {
        return vec![failed(
            STAT_BLOCK_ABSENT,
            "No printed stat block was found for this creature in the source text: it was kept as text, with no numbers made up. Point to its stat block (it may be a picture) or add it".into(),
        )];
    }
    if e.ty != MONSTER {
        return Vec::new();
    }
    let core = core_numbers(e.json.get("data").unwrap_or(&Value::Null));
    let Some(text) = draft
        .plan
        .elements
        .iter()
        .find(|p| p.id == e.id)
        .and_then(|p| super::super::source::section_text(&draft.source_text, p))
    else {
        return Vec::new();
    };
    let printed: std::collections::HashSet<u64> = tokens(&text)
        .iter()
        .filter_map(|t| t.parse().ok())
        .collect();
    let missing: Vec<&str> = core
        .iter()
        .filter(|(_, v)| !printed.contains(v))
        .map(|(l, _)| l.as_str())
        .collect();
    if core.len() < MIN_CORE_NUMBERS || missing.len() * 2 <= core.len() {
        return vec![passed(
            STAT_BLOCK_ABSENT,
            "Its stat block is the printed one",
        )];
    }
    let shown = missing
        .iter()
        .take(MAX_SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    vec![failed(
        STAT_BLOCK_ABSENT,
        format!(
            "{} of its {} stat block numbers are not in the source text ({shown}): they were probably made up. Point to its printed stat block (it may be a picture) or remove the numbers",
            missing.len(),
            core.len()
        ),
    )]
}
