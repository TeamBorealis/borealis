//! Monsters come from printed stat blocks only. A creature whose source passage prints no
//! stat block (its block was a picture nobody could read, or the source only tells its lore)
//! is kept as a text entry with its printed passage and reported; its armor class, hit points
//! and ability scores are never guessed. A generated stat block whose core numbers are not in
//! the passage is repaired once and, when most of them are still missing, dropped the same way.

use serde_json::Value;

use super::super::ingest::READING_NOTE;
use super::super::sweep::SweepIssue;
use super::super::types::{ElementStatus, ImportDraft, ImportKind, PlannedElement, Role};
use super::normalize;
use crate::import::chunk::norm_words;

/// Element type of a monster.
const MONSTER: &str = "Monster";
/// Element type a creature without a stat block is kept as.
const TEXT_TYPE: &str = "Rules Glossary Entry";
/// Different stat-block lines (armor class, hit points, speed, challenge, ability header) a
/// passage needs before it is a stat block.
const MIN_STAT_LINES: usize = 2;
/// Ability header abbreviations, as printed.
const ABILITIES: [&str; 6] = ["STR", "DEX", "CON", "INT", "WIS", "CHA"];
/// Of them, fewest that make an ability header.
const MIN_ABILITY_HEADS: usize = 4;
/// Ability score keys of a stat block's `data.abilities`.
const ABILITY_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

/// Why a creature is kept as text (element issue; the import report states it as a source
/// warning).
pub(crate) const STAT_BLOCK_ABSENT: &str = "stat block not found in the source text: kept as a text entry with its printed passage; no armor class, hit points or ability scores were generated";
/// Why a generated stat block was dropped.
const STAT_BLOCK_UNPRINTED: &str = "most numbers of the generated stat block are not printed in the source passage: kept as a text entry with its printed passage; no armor class, hit points or ability scores were kept";

/// Start of the issue on a monster whose XP is not the XP its stat block prints.
pub(crate) const XP_NOT_AS_PRINTED: &str = "XP not as printed";

/// The XP values `passage` prints ("Challenge 3 (1,100 XP)").
fn printed_xp(passage: &str) -> Vec<u64> {
    let tokens: Vec<&str> = passage
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ';' | '|'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut out: Vec<u64> = tokens
        .windows(2)
        .filter(|w| w[1].trim_end_matches(['.', ',']).eq_ignore_ascii_case("xp"))
        .filter_map(|w| w[0].replace(',', "").parse().ok())
        .collect();
    out.dedup();
    out
}

/// A generated monster whose XP differs from the one XP value its passage prints: the issue
/// (the engine checks XP against the challenge rating, so a printed value it rejects is
/// replaced; the mismatch is said, never hidden).
fn xp_mismatch(data: &Value, passage: &str) -> Option<String> {
    let xp = data.get("xp").and_then(Value::as_u64)?;
    let [printed] = printed_xp(passage)[..] else {
        return None;
    };
    (printed != xp).then(|| {
        format!("{XP_NOT_AS_PRINTED}: the stat block prints {printed} XP, the element has {xp} XP")
    })
}

fn is_number(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// `text` prints a stat block: at least [`MIN_STAT_LINES`] of an armor class, hit points,
/// a speed in feet, a challenge rating, and an ability header (STR DEX CON ...).
pub(crate) fn has_stat_block(text: &str) -> bool {
    let words = norm_words(text);
    let t: Vec<&str> = words.split(' ').collect();
    let at = |i: usize| t.get(i).copied().unwrap_or("");
    let mut kinds = [false; 5];
    for (i, word) in t.iter().enumerate() {
        match *word {
            "armor" | "armour" if at(i + 1) == "class" && is_number(at(i + 2)) => kinds[0] = true,
            "ac" if is_number(at(i + 1)) => kinds[0] = true,
            "hit" if at(i + 1) == "points" && is_number(at(i + 2)) => kinds[1] = true,
            "hp" if is_number(at(i + 1)) => kinds[1] = true,
            "speed" if is_number(at(i + 1)) && matches!(at(i + 2), "ft" | "feet") => {
                kinds[2] = true;
            }
            "challenge" | "cr" if is_number(at(i + 1)) => kinds[3] = true,
            _ => {}
        }
    }
    let heads: std::collections::HashSet<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| ABILITIES.contains(w))
        .collect();
    kinds[4] = heads.len() >= MIN_ABILITY_HEADS;
    kinds.iter().filter(|k| **k).count() >= MIN_STAT_LINES
}

/// The core numbers of a generated stat block (armor class, average hit points, ability
/// scores), labelled.
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

/// The labelled core numbers of `data` that `passage` does not print.
fn unprinted(data: &Value, passage: &str) -> (Vec<String>, usize) {
    let words = norm_words(passage);
    let printed: std::collections::HashSet<u64> =
        words.split(' ').filter_map(|w| w.parse().ok()).collect();
    let core = core_numbers(data);
    let missing = core
        .iter()
        .filter(|(_, v)| !printed.contains(v))
        .map(|(l, _)| l.clone())
        .collect();
    (missing, core.len())
}

/// Lines a located section needs before it stands alone (a heading and a line of text);
/// a shorter excerpt is read with the context generation gets.
const MIN_SECTION_LINES: usize = 2;

/// The passage a monster's stat block must be printed in: its located section (the padded
/// generation window could reach the next creature's stat block), or what generation reads
/// when no section of its own was found.
fn window(draft: &ImportDraft, p: &PlannedElement) -> String {
    let own = p
        .excerpt
        .as_ref()
        .filter(|ex| ex.end_line >= ex.start_line + MIN_SECTION_LINES)
        .and_then(|_| super::super::source::section_text(&draft.source_text, p));
    own.unwrap_or_else(|| super::source_window(&draft.source_text, p))
}

/// Turn the monster `id` into a text entry with its printed passage (plan, group and draft
/// element), with `why` on the element and a source warning naming it.
fn keep_as_text(draft: &mut ImportDraft, id: &str, why: &str) {
    let Some(planned) = draft.plan.elements.iter().find(|p| p.id == id).cloned() else {
        return;
    };
    let text = PlannedElement {
        ty: TEXT_TYPE.into(),
        role: Role::Text,
        ..planned
    };
    if let Some(p) = draft.plan.elements.iter_mut().find(|p| p.id == id) {
        *p = text.clone();
    }
    if let Some(g) = draft
        .plan
        .groups
        .iter_mut()
        .find(|g| g.root_id.as_deref() == Some(id))
    {
        g.kind = ImportKind::Rules;
    }
    let mut json = super::super::parts::text_element(&text, &draft.plan, &draft.source_text);
    normalize(&mut json, &text, &draft.plan, None, &draft.source_name);
    if let Some(e) = draft.elements.iter_mut().find(|e| e.id == id) {
        e.ty = TEXT_TYPE.into();
        e.role = Role::Text;
        e.json = json;
        e.status = ElementStatus::NeedsReview;
        e.issues = vec![why.to_string()];
        e.rule_errors.clear();
        e.balance = None;
    }
    draft
        .sweep_issues
        .retain(|i| i.element_id.as_deref() != Some(id));
    let note = format!(
        "{READING_NOTE}the stat block of \"{}\" was not found in the source text (it may be printed as a picture that could not be read); it was imported as text, without armor class, hit points or ability scores",
        text.name
    );
    if !draft.detection.notes.contains(&note) {
        draft.detection.notes.push(note);
    }
}

/// Before generation: every pending monster whose source prints no stat block becomes a text
/// entry (no model call, nothing to invent).
pub(crate) fn keep_unprinted_as_text(draft: &mut ImportDraft) {
    let ids: Vec<String> = draft
        .plan
        .elements
        .iter()
        .filter(|p| p.ty == MONSTER)
        .filter(|p| {
            draft
                .element(&p.id)
                .is_some_and(|e| e.status == ElementStatus::Pending)
        })
        .filter(|p| !has_stat_block(&window(draft, p)))
        .map(|p| p.id.clone())
        .collect();
    for id in ids {
        keep_as_text(draft, &id, STAT_BLOCK_ABSENT);
    }
}

/// Generated monsters with their passage and stat block.
fn generated(draft: &ImportDraft) -> Vec<(String, String, Value)> {
    draft
        .elements
        .iter()
        .filter(|e| e.ty == MONSTER && !e.json.is_null())
        .filter_map(|e| {
            let p = draft.plan.elements.iter().find(|p| p.id == e.id)?;
            let data = e.json.get("data").cloned().unwrap_or(Value::Null);
            Some((e.id.clone(), window(draft, p), data))
        })
        .collect()
}

/// Repairable issues: a generated stat block with core numbers its printed stat block lacks.
pub(crate) fn sweep(draft: &ImportDraft) -> Vec<SweepIssue> {
    generated(draft)
        .into_iter()
        .filter(|(_, passage, _)| has_stat_block(passage))
        .filter_map(|(id, passage, data)| {
            let (missing, _) = unprinted(&data, &passage);
            (!missing.is_empty()).then(|| SweepIssue {
                level: 0,
                element_id: Some(id),
                message: format!(
                    "stat block numbers not printed in the source passage: {}. Copy every number of the printed stat block exactly; never estimate or complete one",
                    missing.join(", ")
                ),
            })
        })
        .collect()
}

/// After the repairs: a monster without a printed stat block, or whose stat block is mostly
/// not printed, becomes a text entry; one with a few unprinted numbers is marked for review.
pub(crate) fn settle(draft: &mut ImportDraft) {
    for (id, passage, data) in generated(draft) {
        if !has_stat_block(&passage) {
            keep_as_text(draft, &id, STAT_BLOCK_ABSENT);
            continue;
        }
        if let Some(msg) = xp_mismatch(&data, &passage) {
            if let Some(e) = draft.elements.iter_mut().find(|e| e.id == id) {
                if !e.issues.contains(&msg) {
                    e.issues.push(msg);
                }
                if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
                    e.status = ElementStatus::NeedsReview;
                }
            }
        }
        let (missing, total) = unprinted(&data, &passage);
        if total == 0 {
            // No stat block was generated: the engine checks report it.
            continue;
        }
        if missing.len() * 2 > total {
            keep_as_text(draft, &id, STAT_BLOCK_UNPRINTED);
        } else if !missing.is_empty() {
            if let Some(e) = draft.elements.iter_mut().find(|e| e.id == id) {
                let msg = format!(
                    "stat block numbers not printed in the source passage: {}",
                    missing.join(", ")
                );
                if !e.issues.contains(&msg) {
                    e.issues.push(msg);
                }
                if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
                    e.status = ElementStatus::NeedsReview;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "stat_blocks_tests.rs"]
mod tests;
