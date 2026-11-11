//! "Also gained again at levels" (spec 4.5, the feature editor): level-gated copies of a block, as the SRD writes a
//! 2014 Ability Score Improvement (one `select` per level) or a cumulative counter (Sneak Attack dice, `stat` +1 at
//! levels 3, 5, 7...).
//!
//! A copy is the block's rule at another `level`, with the base level written as a whole number in its `name` and
//! `supports` replaced by the copy's ("Improvement Option (Wizard 4)" at 8 is "Improvement Option (Wizard 8)",
//! supports `Improvement Option,Wizard,8`). Reading finds them (only rules no field took, in kinds that repeat
//! blocks) and lists their levels in the base block's reserved field [`AGAIN_AT`]; the copies are not blocks.
//! Writing puts the stored copies back at their positions while the levels and the base rule are unchanged, else
//! writes fresh copies after the base rule.
//!
//! Deterministic, no AI (G7).

use super::block::Block;
use borealis_content::rule_kinds;
use borealis_content::Rule;
use serde_json::{json, Value};

/// The reserved block field holding the levels a block is gained again at (`HomebrewBlock.fields.againAt`).
pub const AGAIN_AT: &str = "againAt";

/// The memo keys of a repeated block: the levels read and the stored base rule and copies.
const MEMO_KEY: &str = "againAt";

/// Kinds whose blocks can be gained again at other levels (the features of a class; WB-18 adds subclass features).
pub(crate) const REPEAT_KINDS: &[&str] = &["classFeature", "subclassFeature"];

/// The kind repeats its blocks.
pub(crate) fn repeats(kind: &str) -> bool {
    REPEAT_KINDS.contains(&kind)
}

/// The rule kinds whose `level` is the class level they are gained at.
fn gated(rule: &Value) -> bool {
    match rule.get("kind").and_then(Value::as_str) {
        Some("grant" | "select" | "stat") => true,
        Some(k) => rule_kinds::spec(k).is_some_and(|s| s.gated),
        None => false,
    }
}

fn level_of(rule: &Value) -> u32 {
    rule.get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
}

/// `text` with every whole number `from` written as `to`.
fn renumber(text: &str, from: u32, to: u32) -> String {
    let from = from.to_string();
    let mut out = String::new();
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if *digits == from {
            out.push_str(&to.to_string());
        } else {
            out.push_str(digits);
        }
        digits.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush(&mut digits, &mut out);
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
}

/// The copy of `base` gained at level `to`; `None` for a rule kind that has no class level.
pub(crate) fn copy_at(base: &Rule, to: u32) -> Option<Rule> {
    let mut v = serde_json::to_value(base).ok()?;
    if !gated(&v) {
        return None;
    }
    let from = level_of(&v);
    let map = v.as_object_mut()?;
    map.insert("level".into(), json!(to));
    for key in ["name", "supports"] {
        if let Some(Value::String(t)) = map.get_mut(key) {
            *t = renumber(t, from, to);
        }
    }
    serde_json::from_value(v).ok()
}

/// The copies among `rules`: per base rule index, its copies' indices and levels. Only rules `free` holds are
/// looked at; a copy comes after its base, at a level the base and its other copies don't have.
pub(crate) fn detect(rules: &[Rule], free: &[bool]) -> Vec<(usize, Vec<(usize, u32)>)> {
    let values: Vec<Value> = rules
        .iter()
        .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        .collect();
    let mut taken = vec![false; rules.len()];
    let mut out = Vec::new();
    for (i, base) in rules.iter().enumerate() {
        if !free[i] || taken[i] || !gated(&values[i]) {
            continue;
        }
        let mut levels = vec![level_of(&values[i])];
        let mut copies = Vec::new();
        for j in i + 1..rules.len() {
            if !free[j] || taken[j] || values[j].get("level").is_none() {
                continue;
            }
            let at = level_of(&values[j]);
            if levels.contains(&at) {
                continue;
            }
            let same = copy_at(base, at)
                .and_then(|c| serde_json::to_value(c).ok())
                .is_some_and(|c| c == values[j]);
            if same {
                levels.push(at);
                copies.push((j, at));
                taken[j] = true;
            }
        }
        if !copies.is_empty() {
            out.push((i, copies));
        }
    }
    out
}

/// Put the levels a block is gained again at into its fields, and the stored copies into its memo.
pub(crate) fn attach(block: &mut Block, copies: &[(usize, u32)], rules: &[Rule], base: &Rule) {
    let levels: Vec<u32> = copies.iter().map(|(_, l)| *l).collect();
    block.fields.insert(AGAIN_AT.into(), json!(levels));
    let stored: Vec<Value> = copies
        .iter()
        .map(|(i, _)| serde_json::to_value(&rules[*i]).unwrap_or(Value::Null))
        .collect();
    let slots: Vec<usize> = copies.iter().map(|(i, _)| *i).collect();
    let kind = block.kind.clone();
    let memo = block.memo.get_or_insert_with(|| super::block::BlockMemo {
        kind,
        ..Default::default()
    });
    memo.read.insert(MEMO_KEY.into(), json!(levels));
    memo.raw.insert(
        MEMO_KEY.into(),
        json!({ "base": serde_json::to_value(base).unwrap_or(Value::Null), "copies": stored, "slots": slots }),
    );
}

/// The levels a block is gained again at (none, or a value of another shape: nothing to copy).
pub(crate) fn levels(block: &Block) -> Vec<u32> {
    let mut out: Vec<u32> = block
        .fields
        .get(AGAIN_AT)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|n| u32::try_from(n).ok())
        .collect();
    out.dedup();
    out
}

/// What a repeated block writes besides its own rule: the stored copies at their positions (`Some(slots)`) while
/// nothing changed, else fresh copies (`None`).
pub(crate) fn copies(block: &Block, base: &Rule) -> (Vec<Rule>, Option<Vec<usize>>) {
    let levels = levels(block);
    if levels.is_empty() {
        return (Vec::new(), None);
    }
    let memo = block.memo.as_ref().filter(|m| m.kind == block.kind);
    let stored = memo.and_then(|m| {
        let read = m.read.get(MEMO_KEY)?;
        let raw = m.raw.get(MEMO_KEY)?;
        let same_levels = read == &json!(levels);
        let same_base = raw.get("base") == serde_json::to_value(base).ok().as_ref();
        (same_levels && same_base).then_some(raw)
    });
    if let Some(raw) = stored {
        let rules: Option<Vec<Rule>> = raw["copies"].as_array().map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        });
        let slots: Option<Vec<usize>> = serde_json::from_value(raw["slots"].clone()).ok();
        if let (Some(rules), Some(slots)) = (rules, slots) {
            if rules.len() == slots.len() {
                return (rules, Some(slots));
            }
        }
    }
    let base_level = serde_json::to_value(base).map_or(1, |v| level_of(&v));
    let fresh = levels
        .iter()
        .filter(|l| **l != base_level)
        .filter_map(|l| copy_at(base, *l))
        .collect();
    (fresh, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(v: Value) -> Rule {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn renumbers_whole_numbers_only() {
        assert_eq!(
            renumber("Improvement Option (Wizard 4)", 4, 8),
            "Improvement Option (Wizard 8)"
        );
        assert_eq!(
            renumber("Improvement Option,Wizard,4", 4, 12),
            "Improvement Option,Wizard,12"
        );
        assert_eq!(renumber("14 and 4 and 40", 4, 6), "14 and 6 and 40");
    }

    #[test]
    fn finds_the_copies_of_a_2014_improvement_option() {
        let base = json!({"kind": "select", "type": "Class Feature", "name": "Improvement Option (Wizard 4)",
            "level": 4, "supports": "Improvement Option,Wizard,4"});
        let rules: Vec<Rule> = [4, 8, 12]
            .iter()
            .map(|l| rule(json!({"kind": "select", "type": "Class Feature", "name": format!("Improvement Option (Wizard {l})"),
                "level": l, "supports": format!("Improvement Option,Wizard,{l}")})))
            .chain([rule(json!({"kind": "stat", "name": "hp", "value": "1"}))])
            .collect();
        assert_eq!(rules[0], rule(base));
        let found = detect(&rules, &[true; 4]);
        assert_eq!(found, vec![(0, vec![(1, 8), (2, 12)])]);
        assert_eq!(copy_at(&rules[0], 12), Some(rules[2].clone()));
    }

    #[test]
    fn a_rule_without_a_class_level_is_not_copied() {
        let r = rule(json!({"kind": "unknown", "tag": "x"}));
        assert_eq!(copy_at(&r, 5), None);
    }
}
