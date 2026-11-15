//! Plain words for the values inside blocks: element references by name, statistics by label, formulas, conditions,
//! tag filters and "while wearing" tests. No id, expression, token or JSON reaches a sentence (spec 5.1, 6).

use super::sentence::SentenceContext;
use crate::expr_build::condition::{ConditionContext, ConditionLanguage, ConditionRow};
use crate::expr_build::equipped::{EquippedLanguage, EquippedRow, Slot, SlotTest};
use crate::expr_build::formula::{FormulaDialect, FormulaLanguage, FormulaNode, Op};
use crate::expr_build::tag_filter::{TagFilterLanguage, TagRow};
use crate::expr_build::{Built, ExprLanguage, GroupMode, Node};

/// The name of an element id, or "a missing element".
pub(super) fn element(cx: &dyn SentenceContext, id: &str) -> String {
    cx.element(id)
        .map_or_else(|| "a missing element".to_string(), |(name, _)| name)
}

/// Tokens (`short_rest`, `bonus_action`) as words.
pub(super) fn token(s: &str) -> String {
    s.replace('_', " ")
}

/// `a`, `a and b`, `a, b and c`.
pub(super) fn and_list(items: &[String]) -> String {
    join(items, "and")
}

/// `a`, `a or b`, `a, b or c`.
pub(super) fn or_list(items: &[String]) -> String {
    join(items, "or")
}

fn join(items: &[String], word: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} {word} {last}", init.join(", ")),
    }
}

/// First letter upper case.
pub(super) fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn title(s: &str) -> String {
    s.split(' ').map(cap).collect::<Vec<_>>().join(" ")
}

fn group<R>(node: &Node<R>, row: &dyn Fn(&R) -> String, top: bool) -> String {
    match node {
        Node::Row { row: r } => row(r),
        Node::Group { mode, items } => {
            let parts: Vec<String> = items
                .iter()
                .filter(|n| !n.is_empty())
                .map(|n| group(n, row, false))
                .collect();
            let text = match mode {
                GroupMode::All => and_list(&parts),
                GroupMode::Any => or_list(&parts),
                GroupMode::None if parts.len() == 1 => format!("not {}", parts[0]),
                GroupMode::None => format!("none of {}", or_list(&parts)),
            };
            if top || parts.len() < 2 {
                text
            } else {
                format!("({text})")
            }
        }
    }
}

fn condition_row(cx: &dyn SentenceContext, row: &ConditionRow) -> String {
    match row {
        ConditionRow::Has { id } => match cx.element(id) {
            Some((name, ty)) if ty == "Multiclass" => format!("{name} is taken as a multiclass"),
            Some((name, _)) => format!("has {name}"),
            None => "has a missing element".to_string(),
        },
        ConditionRow::Lacks { id } => match cx.element(id) {
            Some((_, ty)) if ty == "Multiclass" => "first class only".to_string(),
            Some((name, _)) => format!("doesn't have {name}"),
            None => "doesn't have a missing element".to_string(),
        },
        ConditionRow::Ability { ability, min } => {
            format!("{} {min} or higher", cap(ability.full()))
        }
        ConditionRow::CharacterLevel { min } => format!("character level {min} or higher"),
        ConditionRow::Level { min } => format!("level {min} or higher"),
        ConditionRow::ClassLevel { class, min } => {
            format!("{} level {min} or higher", title(class))
        }
        ConditionRow::Feature { name } => format!("has a feature named {name}"),
        ConditionRow::Spellcasting { level } => format!("can cast level {level} spells"),
        ConditionRow::Type { value } => format!("has a {}", value.to_lowercase()),
        ConditionRow::Statistic { stat, min } => {
            format!("{} at least {min}", crate::stats::label(stat))
        }
        ConditionRow::Kept { .. } => "a condition kept as is".to_string(),
    }
}

/// A `requirements` text in words ("Strength 13 or higher and first class only").
pub(super) fn condition(cx: &dyn SentenceContext, text: &str) -> String {
    let ccx = ConditionContext::new(cx.class_names());
    match ConditionLanguage::parse(text, &ccx) {
        Built::Model { model } => group(&model, &|r| condition_row(cx, r), true),
        Built::Kept { .. } => "a condition kept as is".to_string(),
    }
}

fn tag_row(cx: &dyn SentenceContext, row: &TagRow) -> String {
    match row {
        TagRow::Tag { tag } => tag.clone(),
        TagRow::Element { id } => element(cx, id),
        TagRow::ClassList => "the class's spell list".to_string(),
        TagRow::CastableLevel => "a level the class can cast".to_string(),
        TagRow::Choice { name } => format!("the answer to {name}"),
        TagRow::Kept { .. } => "a filter kept as is".to_string(),
    }
}

/// A `supports` text in words ("Exotic or Standard").
pub(super) fn tag_filter(cx: &dyn SentenceContext, text: &str) -> String {
    match TagFilterLanguage::parse(text, &()) {
        Built::Model { model } => group(&model.root, &|r| tag_row(cx, r), true),
        Built::Kept { .. } => "a filter kept as is".to_string(),
    }
}

fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Armor => "armor",
        Slot::Shield => "a shield",
        Slot::MainHand => "the main hand",
        Slot::OffHand => "the off hand",
    }
}

fn equipped_row(cx: &dyn SentenceContext, row: &EquippedRow) -> String {
    match row {
        EquippedRow::Slot { slot, test } => match (slot, test) {
            (Slot::Armor, SlotTest::None) => "wearing no armor".to_string(),
            (Slot::Armor, SlotTest::Any) => "wearing armor".to_string(),
            (Slot::Armor, SlotTest::Light) => "wearing light armor".to_string(),
            (Slot::Armor, SlotTest::Medium) => "wearing medium armor".to_string(),
            (Slot::Armor, SlotTest::Heavy) => "wearing heavy armor".to_string(),
            (Slot::Shield, SlotTest::None) => "without a shield".to_string(),
            (Slot::Shield, SlotTest::Any) => "holding a shield".to_string(),
            (s, SlotTest::None) => format!("nothing in {}", slot_name(*s)),
            (s, SlotTest::Any) => format!("something in {}", slot_name(*s)),
            (s, SlotTest::Weapon) => format!("a weapon in {}", slot_name(*s)),
            (_, SlotTest::Versatile) => "a versatile weapon in both hands".to_string(),
            (s, SlotTest::Item { item }) => format!("{} in {}", item_name(cx, item), slot_name(*s)),
            (s, t) => format!(
                "{} in {}",
                token(&format!("{t:?}").to_lowercase()),
                slot_name(*s)
            ),
        },
        EquippedRow::Attuned { item } => format!("attuned to {}", item_name(cx, item)),
        EquippedRow::Wearing { item } => format!("wearing {}", item_name(cx, item)),
        EquippedRow::Kept { .. } => "a test kept as is".to_string(),
    }
}

/// Equipped tests hold ids in lower case: the context resolves them case-insensitively.
fn item_name(cx: &dyn SentenceContext, item: &str) -> String {
    cx.element(&item.to_uppercase())
        .or_else(|| cx.element(item))
        .map_or_else(|| "an item".to_string(), |(n, _)| n)
}

/// An `equipped` text in words ("wearing no armor and without a shield").
pub(super) fn equipped(cx: &dyn SentenceContext, text: &str) -> String {
    match EquippedLanguage::parse(text, &()) {
        Built::Model { model } => group(&model, &|r| equipped_row(cx, r), true),
        Built::Kept { .. } => "a test kept as is".to_string(),
    }
}

fn op_word(op: Op) -> &'static str {
    match op {
        Op::Add => "+",
        Op::Sub => "-",
        Op::Mul => "×",
        Op::Div => "÷",
    }
}

fn formula_node(n: &FormulaNode) -> String {
    match n {
        FormulaNode::Number { value } => value.to_string(),
        FormulaNode::Proficiency => "proficiency bonus".to_string(),
        FormulaNode::HalfProficiency => "half the proficiency bonus".to_string(),
        FormulaNode::Ability { ability } => format!("{} modifier", cap(ability.full())),
        FormulaNode::ClassLevel { class } => format!("{} level", title(class)),
        FormulaNode::ClassSaveDc { class } => format!("{} spell save DC", title(class)),
        FormulaNode::Binary { op, left, right } => {
            let prec = |o: Op| if o.is_additive() { 1 } else { 2 };
            let side = |x: &FormulaNode, right_side: bool| match x {
                FormulaNode::Binary { op: inner, .. }
                    if prec(*inner) < prec(*op)
                        || (right_side
                            && prec(*inner) == prec(*op)
                            && matches!(op, Op::Sub | Op::Div)) =>
                {
                    format!("({})", formula_node(x))
                }
                _ => formula_node(x),
            };
            format!(
                "{} {} {}",
                side(left, false),
                op_word(*op),
                side(right, true)
            )
        }
        FormulaNode::Max { args } => {
            let a: Vec<String> = args.iter().map(formula_node).collect();
            format!("the higher of {}", and_list(&a))
        }
        FormulaNode::Min { args } => {
            let a: Vec<String> = args.iter().map(formula_node).collect();
            format!("the lower of {}", and_list(&a))
        }
        FormulaNode::Floor { arg } => format!("{} rounded down", formula_node(arg)),
        FormulaNode::CeilDiv { num, den } => {
            format!("{} ÷ {} rounded up", formula_node(num), formula_node(den))
        }
    }
}

/// A formula text in words ("the higher of 1 and Charisma modifier"); numbers stay numbers.
pub(super) fn formula(text: &str, dialect: FormulaDialect) -> String {
    match FormulaLanguage::parse(text, &dialect) {
        Built::Model { model } => formula_node(&model),
        Built::Kept { .. } => "a formula kept as is".to_string(),
    }
}

/// A `stat` value: a signed number ("+2") or a statistic in words ("Wisdom modifier", "minus Dexterity modifier").
pub(super) fn stat_value(text: &str) -> String {
    let t = text.trim();
    match t.parse::<i64>() {
        Ok(n) => format!("{n:+}"),
        Err(_) => crate::stats::label(t),
    }
}
