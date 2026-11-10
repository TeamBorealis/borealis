//! The formula builder (spec 6.3): number, `+ - * /`, ability modifier, proficiency bonus, class level, `max`, `min`,
//! `floor`, `ceil(a / b)`, and the presets.
//!
//! The engine reads formula fields with five different readers, so the builder writes each field in the dialect of
//! its reader ([`FormulaDialect`], [`dialect_of`]). Each dialect offers only what its reader evaluates, and the text
//! it writes is read back by that reader to exactly the model (property-tested against the engine's readers):
//!
//! | Dialect | Fields | Reader | Offers |
//! |---|---|---|---|
//! | `formula` | resource `max.expr`, values of auras, Wild Shape, healing, regeneration, cantrip and spell damage... | `borealis_content::formula` (`play_rules::eval_formula`) | everything but the two below |
//! | `saveDc` | `save_dc.formula` | `spell_rules::eval_save_dc_formula` | a sum of numbers, the proficiency bonus and ability modifiers (`8 + proficiency + wisdom:modifier`), or a class's spell save DC (`spell save DC (Cleric)`) |
//! | `breathDc` | element `data.dc` (2024 breath weapons) | `breath::dc_formula_text` + `formula` | the same sums |
//! | `noteDc` | `cunning_strike.save_dc` | `notes_d::eval_note_dc` | the same sums, abilities written bare (`8 + dexterity + proficiency`) |
//! | `value` | `skill_bonus.value`, `skill_bonus_unproficient.value`, `attack_bonus.value` | `rule_view::eval_value` | one number, the proficiency bonus, half of it, or an ability modifier |
//! | `valueOrClassLevel` | each `damage_reduction.plus` term | `combat::reduction_term` | a value, or a class level |
//!
//! The save DC sums are the subset the save-DC reader and the shared formula grammar both read, so the creator can
//! show their value without a character; a class's spell save DC has a value only on a character.

use super::formula_text::{parse_dialect, render_dialect};
use super::{Ability, BuilderProblem, Built, ExprLanguage};
use serde::{Deserialize, Serialize};

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Op {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/` (whole-number division, rounding toward zero)
    Div,
}

impl Op {
    pub(crate) fn symbol(self) -> char {
        match self {
            Self::Add => '+',
            Self::Sub => '-',
            Self::Mul => '*',
            Self::Div => '/',
        }
    }

    pub(crate) fn is_additive(self) -> bool {
        matches!(self, Self::Add | Self::Sub)
    }
}

/// One node of a formula.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "term", rename_all = "camelCase")]
pub enum FormulaNode {
    /// A whole number.
    Number {
        /// The number.
        value: i32,
    },
    /// The proficiency bonus.
    Proficiency,
    /// Half the proficiency bonus, rounded down (`value` dialects only).
    HalfProficiency,
    /// An ability modifier.
    Ability {
        /// The ability.
        ability: Ability,
    },
    /// Levels in a class (`cleric level`).
    ClassLevel {
        /// The class name, lower case.
        class: String,
    },
    /// A class's spell save DC (`saveDc` only).
    ClassSaveDc {
        /// The class name, as written.
        class: String,
    },
    /// `left op right`.
    Binary {
        /// The operator.
        op: Op,
        /// Left operand.
        left: Box<FormulaNode>,
        /// Right operand.
        right: Box<FormulaNode>,
    },
    /// The largest of the arguments.
    Max {
        /// At least one argument.
        args: Vec<FormulaNode>,
    },
    /// The smallest of the arguments.
    Min {
        /// At least one argument.
        args: Vec<FormulaNode>,
    },
    /// Rounded down (whole-number arithmetic already rounds toward zero).
    Floor {
        /// The argument.
        arg: Box<FormulaNode>,
    },
    /// `ceil(num / den)`: the division rounded up.
    CeilDiv {
        /// Numerator.
        num: Box<FormulaNode>,
        /// Denominator (no value when below 1).
        den: Box<FormulaNode>,
    },
}

impl FormulaNode {
    /// A number.
    #[must_use]
    pub fn number(value: i32) -> Self {
        Self::Number { value }
    }

    /// An ability modifier.
    #[must_use]
    pub fn ability(ability: Ability) -> Self {
        Self::Ability { ability }
    }

    /// A class level.
    #[must_use]
    pub fn class_level(class: &str) -> Self {
        Self::ClassLevel {
            class: class.trim().to_lowercase(),
        }
    }

    /// `left op right`.
    #[must_use]
    pub fn binary(op: Op, left: Self, right: Self) -> Self {
        Self::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// The terms of a sum (`a + b + c`), left to right; a node that is not a sum is one term.
    #[must_use]
    pub fn sum_terms(&self) -> Vec<&Self> {
        match self {
            Self::Binary {
                op: Op::Add,
                left,
                right,
            } => {
                let mut out = left.sum_terms();
                out.extend(right.sum_terms());
                out
            }
            other => vec![other],
        }
    }

    /// A left-to-right sum of terms; `None` for no terms.
    #[must_use]
    pub fn sum(terms: Vec<Self>) -> Option<Self> {
        terms
            .into_iter()
            .reduce(|acc, t| Self::binary(Op::Add, acc, t))
    }
}

/// Which engine reader a formula field is read by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormulaDialect {
    /// The shared formula grammar.
    Formula,
    /// The `save_dc` rule's formula.
    SaveDc,
    /// A breath weapon's `data.dc`.
    BreathDc,
    /// A save DC shown in a note (`cunning_strike.save_dc`).
    NoteDc,
    /// A small value (`skill_bonus.value`).
    Value,
    /// A damage reduction term (`damage_reduction.plus`).
    ValueOrClassLevel,
}

impl FormulaDialect {
    /// Every dialect.
    pub const ALL: [Self; 6] = [
        Self::Formula,
        Self::SaveDc,
        Self::BreathDc,
        Self::NoteDc,
        Self::Value,
        Self::ValueOrClassLevel,
    ];

    /// A save DC sum dialect.
    #[must_use]
    pub fn is_dc(self) -> bool {
        matches!(self, Self::SaveDc | Self::BreathDc | Self::NoteDc)
    }
}

/// Formula fields of custom rules and the dialect of their reader: `(kind, field path, dialect)`. A path with `[]`
/// is a list of formulas; `data.dc` is the element's data key.
pub const FORMULA_FIELDS: &[(&str, &str, FormulaDialect)] = &[
    ("resource", "max.expr", FormulaDialect::Formula),
    (
        "aura",
        "bonus_saving_throws.formula",
        FormulaDialect::Formula,
    ),
    ("beast_forms", "duration_hours", FormulaDialect::Formula),
    ("beast_forms", "temp_hp", FormulaDialect::Formula),
    ("cantrip_damage_bonus", "value", FormulaDialect::Formula),
    ("damage_bonus_spell", "value", FormulaDialect::Formula),
    (
        "channel_divinity_option",
        "attack_bonus",
        FormulaDialect::Formula,
    ),
    (
        "channel_divinity_option",
        "pool_hit_points",
        FormulaDialect::Formula,
    ),
    ("healing", "plus", FormulaDialect::Formula),
    ("regeneration", "amount", FormulaDialect::Formula),
    (
        "slot_recovery",
        "max_combined_level",
        FormulaDialect::Formula,
    ),
    ("save_dc", "formula", FormulaDialect::SaveDc),
    ("cunning_strike", "save_dc", FormulaDialect::NoteDc),
    ("skill_bonus", "value", FormulaDialect::Value),
    ("skill_bonus_unproficient", "value", FormulaDialect::Value),
    ("attack_bonus", "value", FormulaDialect::Value),
    (
        "damage_reduction",
        "plus[]",
        FormulaDialect::ValueOrClassLevel,
    ),
    ("element", "data.dc", FormulaDialect::BreathDc),
];

/// The dialect of a custom rule's formula field (`kind`, `field` as in [`FORMULA_FIELDS`]).
#[must_use]
pub fn dialect_of(kind: &str, field: &str) -> Option<FormulaDialect> {
    FORMULA_FIELDS
        .iter()
        .find(|(k, f, _)| *k == kind && *f == field)
        .map(|(_, _, d)| *d)
}

/// The formula builder; its context is the field's dialect.
pub struct FormulaLanguage;

impl ExprLanguage for FormulaLanguage {
    type Model = FormulaNode;
    type Context = FormulaDialect;

    fn parse(text: &str, dialect: &FormulaDialect) -> Built<FormulaNode> {
        match parse_dialect(text, *dialect) {
            Ok(model) => Built::Model { model },
            Err(reason) => Built::Kept {
                text: text.to_string(),
                reason,
            },
        }
    }

    fn render(model: &FormulaNode, dialect: &FormulaDialect) -> String {
        render_dialect(model, *dialect)
    }

    fn means_same(model: &FormulaNode, text: &str, dialect: &FormulaDialect) -> bool {
        parse_dialect(text, *dialect).is_ok_and(|m| &m == model)
    }

    fn check(model: &FormulaNode, dialect: &FormulaDialect) -> Vec<BuilderProblem> {
        let mut out = Vec::new();
        if dialect.is_dc() {
            check_dc(model, *dialect, &mut out);
        } else if matches!(
            dialect,
            FormulaDialect::Value | FormulaDialect::ValueOrClassLevel
        ) {
            check_value(model, *dialect, &mut out);
        } else {
            check_node(model, "", &mut out);
        }
        out
    }
}

/// The largest number a formula may hold: keeps every sum and product of a few terms inside the engine's integers.
pub const MAX_NUMBER: i32 = 999;

fn child(path: &str, part: &str) -> String {
    if path.is_empty() {
        part.to_string()
    } else {
        format!("{path}.{part}")
    }
}

fn push(out: &mut Vec<BuilderProblem>, path: &str, message: &str) {
    out.push(BuilderProblem::new(path, message));
}

fn check_number(value: i32, min: i32, path: &str, out: &mut Vec<BuilderProblem>) {
    if !(min..=MAX_NUMBER).contains(&value) {
        push(
            out,
            path,
            &format!("Use a number from {min} to {MAX_NUMBER}."),
        );
    }
}

/// A class name the formula text can hold: no operator, parenthesis or comma; not empty.
pub(crate) fn class_problem(class: &str) -> Option<&'static str> {
    let c = class.trim();
    if c.is_empty() {
        Some("Pick a class.")
    } else if c.contains(['+', '-', '*', '/', '(', ')', ',']) || c != class {
        Some("This class name can't be used in a formula (it has + - * / ( ) or a comma).")
    } else if c.ends_with(" level") || c == "proficiency" || c == "proficiency bonus" {
        Some("This class name can't be used in a formula.")
    } else {
        None
    }
}

fn check_node(node: &FormulaNode, path: &str, out: &mut Vec<BuilderProblem>) {
    match node {
        FormulaNode::Number { value } => check_number(*value, -MAX_NUMBER, path, out),
        FormulaNode::Proficiency | FormulaNode::Ability { .. } => {}
        FormulaNode::ClassLevel { class } => {
            if let Some(m) = class_problem(class) {
                push(out, path, m);
            }
        }
        FormulaNode::HalfProficiency => {
            push(out, path, "Half the proficiency bonus can't be used here.")
        }
        FormulaNode::ClassSaveDc { .. } => {
            push(
                out,
                path,
                "A class's spell save DC can be used only as a feature's save DC.",
            );
        }
        FormulaNode::Binary { left, right, .. } => {
            check_node(left, &child(path, "left"), out);
            check_node(right, &child(path, "right"), out);
        }
        FormulaNode::Max { args } | FormulaNode::Min { args } => {
            if args.is_empty() {
                push(out, path, "Add at least one value.");
            }
            for (i, a) in args.iter().enumerate() {
                check_node(a, &child(path, &format!("args.{i}")), out);
            }
        }
        FormulaNode::Floor { arg } => check_node(arg, &child(path, "arg"), out),
        FormulaNode::CeilDiv { num, den } => {
            check_node(num, &child(path, "num"), out);
            check_node(den, &child(path, "den"), out);
        }
    }
}

fn check_dc(node: &FormulaNode, dialect: FormulaDialect, out: &mut Vec<BuilderProblem>) {
    if let FormulaNode::ClassSaveDc { class } = node {
        if dialect != FormulaDialect::SaveDc {
            push(
                out,
                "",
                "A class's spell save DC can be used only as a feature's save DC.",
            );
        } else if class.trim().is_empty() || class.trim() != class || class.contains(['(', ')']) {
            push(out, "", "Pick a class.");
        }
        return;
    }
    for (i, term) in node.sum_terms().into_iter().enumerate() {
        let path = if i == 0 {
            String::new()
        } else {
            format!("terms.{i}")
        };
        match term {
            FormulaNode::Number { value } => check_number(*value, 0, &path, out),
            FormulaNode::Proficiency | FormulaNode::Ability { .. } => {}
            _ => push(
                out,
                &path,
                "A save DC adds numbers, the proficiency bonus and ability modifiers only.",
            ),
        }
    }
}

fn check_value(node: &FormulaNode, dialect: FormulaDialect, out: &mut Vec<BuilderProblem>) {
    match node {
        FormulaNode::Number { value } => check_number(*value, -MAX_NUMBER, "", out),
        FormulaNode::Proficiency | FormulaNode::HalfProficiency | FormulaNode::Ability { .. } => {}
        FormulaNode::ClassLevel { class } if dialect == FormulaDialect::ValueOrClassLevel => {
            if let Some(m) = class_problem(class) {
                push(out, "", m);
            }
        }
        _ if dialect == FormulaDialect::ValueOrClassLevel => push(
            out,
            "",
            "This value can be a number, the proficiency bonus, half of it, an ability modifier or a class level.",
        ),
        _ => push(
            out,
            "",
            "This value can be a number, the proficiency bonus, half of it or an ability modifier.",
        ),
    }
}

/// One-tap formula shapes (spec 6.3).
pub mod presets {
    use super::{Ability, FormulaNode, Op};

    /// `N × class level` (`5 * paladin level`).
    #[must_use]
    pub fn times_class_level(n: i32, class: &str) -> FormulaNode {
        FormulaNode::binary(
            Op::Mul,
            FormulaNode::number(n),
            FormulaNode::class_level(class),
        )
    }

    /// An ability modifier, at least 1 (`max(1, charisma:modifier)`).
    #[must_use]
    pub fn ability_min_one(ability: Ability) -> FormulaNode {
        FormulaNode::Max {
            args: vec![FormulaNode::number(1), FormulaNode::ability(ability)],
        }
    }

    /// `8 + proficiency + <ability> modifier`, the usual save DC.
    #[must_use]
    pub fn save_dc(ability: Ability) -> FormulaNode {
        FormulaNode::binary(
            Op::Add,
            FormulaNode::binary(Op::Add, FormulaNode::number(8), FormulaNode::Proficiency),
            FormulaNode::ability(ability),
        )
    }

    /// Half the class level, rounded up (`ceil(druid level / 2)`).
    #[must_use]
    pub fn half_class_level_up(class: &str) -> FormulaNode {
        FormulaNode::CeilDiv {
            num: Box::new(FormulaNode::class_level(class)),
            den: Box::new(FormulaNode::number(2)),
        }
    }

    /// Half the class level, rounded down (`sorcerer level / 2`).
    #[must_use]
    pub fn half_class_level_down(class: &str) -> FormulaNode {
        FormulaNode::binary(
            Op::Div,
            FormulaNode::class_level(class),
            FormulaNode::number(2),
        )
    }
}
