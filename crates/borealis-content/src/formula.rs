//! Content formulas: the `max`, `value` and `formula` fields of custom rules (`5*cleric level`,
//! `max(1, charisma:modifier)`, `ceil(druid level / 2)`, `proficiency + 2`).
//!
//! One grammar for the engine (which evaluates a [`Formula`] against a character) and the homebrew creator (which
//! checks and builds formulas without an engine). The parser reads exactly what the engine's evaluator always read,
//! quirks included, so moving the evaluator onto it changes no result:
//!
//! * `ceil(a / b)` is tried first, on the whole text; then the right-most top-level `+`/`-` splits (left-associative,
//!   lower precedence), then the right-most top-level `*`/`/`; an operator at the very start is a sign, not a split;
//! * `max(...)`, `min(...)`, `floor(...)` (comma-separated arguments; `floor` takes its first argument);
//! * a parenthesized formula; an integer; `proficiency` / `proficiency bonus`; `<ability>:modifier`; `<class> level`
//!   (case-insensitive).
//!
//! Text that does not fit is a [`FormulaError`]. Evaluation can still give no value: division by zero, `ceil` by a
//! divisor below 1, or an ability the [`FormulaContext`] does not know.

use std::fmt;

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/` (integer division, truncating; no value when dividing by zero)
    Div,
}

impl BinaryOp {
    fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(Self::Add),
            '-' => Some(Self::Sub),
            '*' => Some(Self::Mul),
            '/' => Some(Self::Div),
            _ => None,
        }
    }

    fn symbol(self) -> char {
        match self {
            Self::Add => '+',
            Self::Sub => '-',
            Self::Mul => '*',
            Self::Div => '/',
        }
    }

    /// Additive operators bind looser than multiplicative ones.
    fn is_additive(self) -> bool {
        matches!(self, Self::Add | Self::Sub)
    }
}

/// A function of the formula language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Func {
    /// `max(a, b, ...)`
    Max,
    /// `min(a, b, ...)`
    Min,
    /// `floor(a)`: the first argument (integer arithmetic is already floored).
    Floor,
}

impl Func {
    const ALL: [Self; 3] = [Self::Max, Self::Min, Self::Floor];

    /// The function's name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Min => "min",
            Self::Floor => "floor",
        }
    }
}

/// A parsed formula.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Formula {
    /// An integer.
    Number(i32),
    /// The proficiency bonus (`proficiency`, `proficiency bonus`).
    Proficiency,
    /// An ability modifier (`wisdom:modifier`); the ability as written, lower-cased and trimmed.
    AbilityModifier(String),
    /// A class level (`cleric level`); the class name lower-cased and trimmed.
    ClassLevel(String),
    /// `left op right`.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Formula>,
        /// Right operand.
        right: Box<Formula>,
    },
    /// `ceil(numerator / denominator)`.
    CeilDiv(Box<Formula>, Box<Formula>),
    /// `max(...)`, `min(...)`, `floor(...)`.
    Call(Func, Vec<Formula>),
    /// A parenthesized formula.
    Group(Box<Formula>),
}

/// The text is not a formula; `part` is the innermost piece that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot read \"{part}\" in a formula")]
pub struct FormulaError {
    /// The unreadable piece (trimmed).
    pub part: String,
}

/// What a formula reads from a character.
pub trait FormulaContext {
    /// The proficiency bonus.
    fn proficiency(&self) -> i32;
    /// The modifier of `ability` (as written: `wisdom`, `wis`), or `None` for an unknown ability.
    fn ability_modifier(&self, ability: &str) -> Option<i32>;
    /// The character's level in `class` (lower-case name), 0 without levels in it.
    fn class_level(&self, class: &str) -> i32;
}

/// Parse a formula.
///
/// # Errors
/// [`FormulaError`] naming the piece that is not part of the grammar.
pub fn parse(text: &str) -> Result<Formula, FormulaError> {
    parse_expr(text)
}

/// Split at the right-most `ops` character outside parentheses that is not the first character.
fn split_top<'a>(expr: &'a str, ops: &[char]) -> Option<(&'a str, char, &'a str)> {
    let mut depth = 0i32;
    for (i, ch) in expr.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth -= 1,
            c if depth == 0 && ops.contains(&c) && i > 0 => {
                return Some((expr[..i].trim(), c, expr[i + c.len_utf8()..].trim()));
            }
            _ => {}
        }
    }
    None
}

/// Split function arguments at top-level commas.
fn split_commas(inner: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut out = Vec::new();
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

fn binary(op: char, left: &str, right: &str) -> Result<Formula, FormulaError> {
    let op = BinaryOp::from_char(op).ok_or_else(|| FormulaError {
        part: op.to_string(),
    })?;
    Ok(Formula::Binary {
        op,
        left: Box::new(parse_expr(left)?),
        right: Box::new(parse_expr(right)?),
    })
}

fn parse_expr(expr: &str) -> Result<Formula, FormulaError> {
    let e = expr.trim();
    if let Some((l, _, r)) = e
        .strip_prefix("ceil(")
        .and_then(|x| x.strip_suffix(')'))
        .and_then(|x| split_top(x, &['/']))
    {
        return Ok(Formula::CeilDiv(
            Box::new(parse_expr(l)?),
            Box::new(parse_expr(r)?),
        ));
    }
    if let Some((l, op, r)) = split_top(e, &['+', '-']) {
        return binary(op, l, r);
    }
    if let Some((l, op, r)) = split_top(e, &['*', '/']) {
        return binary(op, l, r);
    }
    for f in Func::ALL {
        if let Some(inner) = e
            .strip_prefix(f.name())
            .and_then(|r| r.trim_start().strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
        {
            let args = split_commas(inner)
                .into_iter()
                .map(parse_expr)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Formula::Call(f, args));
        }
    }
    if let Some(inner) = e.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        return Ok(Formula::Group(Box::new(parse_expr(inner)?)));
    }
    if let Ok(n) = e.parse::<i32>() {
        return Ok(Formula::Number(n));
    }
    let lower = e.to_lowercase();
    if lower == "proficiency" || lower == "proficiency bonus" {
        return Ok(Formula::Proficiency);
    }
    if let Some(a) = lower.strip_suffix(":modifier") {
        return Ok(Formula::AbilityModifier(a.trim().to_string()));
    }
    if let Some(class) = lower.strip_suffix(" level") {
        return Ok(Formula::ClassLevel(class.trim().to_string()));
    }
    Err(FormulaError {
        part: e.to_string(),
    })
}

impl Formula {
    /// Evaluate against `cx`. `None`: division by zero, `ceil` by a divisor below 1, or an unknown ability.
    #[must_use]
    pub fn eval(&self, cx: &dyn FormulaContext) -> Option<i32> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Proficiency => Some(cx.proficiency()),
            Self::AbilityModifier(a) => cx.ability_modifier(a),
            Self::ClassLevel(c) => Some(cx.class_level(c)),
            Self::Binary { op, left, right } => {
                let (a, b) = (left.eval(cx)?, right.eval(cx)?);
                match op {
                    BinaryOp::Add => Some(a + b),
                    BinaryOp::Sub => Some(a - b),
                    BinaryOp::Mul => Some(a * b),
                    BinaryOp::Div => a.checked_div(b),
                }
            }
            Self::CeilDiv(num, den) => {
                let (a, b) = (num.eval(cx)?, den.eval(cx)?);
                (b > 0).then(|| (a + b - 1) / b)
            }
            Self::Call(f, args) => {
                let vals = args
                    .iter()
                    .map(|a| a.eval(cx))
                    .collect::<Option<Vec<i32>>>()?;
                match f {
                    Func::Max => vals.into_iter().max(),
                    Func::Min => vals.into_iter().min(),
                    Func::Floor => vals.first().copied(),
                }
            }
            Self::Group(inner) => inner.eval(cx),
        }
    }

    /// Wrap `self` in parentheses when it would split differently as an operand of `parent`.
    fn fmt_operand(
        &self,
        f: &mut fmt::Formatter<'_>,
        parent: BinaryOp,
        right: bool,
    ) -> fmt::Result {
        let needs = match self {
            Self::Binary { op, .. } => {
                op.is_additive() && !parent.is_additive()
                    || right && op.is_additive() == parent.is_additive()
            }
            Self::Number(n) => right && *n < 0,
            _ => false,
        };
        if needs {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }
}

/// The canonical text: `a + b`, `max(a, b)`, `ceil(a / b)`, `wisdom:modifier`, `cleric level`. A parsed formula
/// renders to text that parses back to the same formula.
impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Proficiency => f.write_str("proficiency"),
            Self::AbilityModifier(a) => write!(f, "{a}:modifier"),
            Self::ClassLevel(c) => write!(f, "{c} level"),
            Self::Binary { op, left, right } => {
                left.fmt_operand(f, *op, false)?;
                write!(f, " {} ", op.symbol())?;
                right.fmt_operand(f, *op, true)
            }
            Self::CeilDiv(num, den) => {
                // `ceil(` splits only at its right-most top-level `/`: the numerator is read as written, and the
                // denominator needs parentheses only when it has a `/` of its own.
                let den = den.to_string();
                if split_top(&den, &['/']).is_some() {
                    write!(f, "ceil({num} / ({den}))")
                } else {
                    write!(f, "ceil({num} / {den})")
                }
            }
            Self::Call(func, args) => {
                write!(f, "{}(", func.name())?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
            }
            Self::Group(inner) => write!(f, "({inner})"),
        }
    }
}

#[cfg(test)]
#[path = "formula_tests.rs"]
mod tests;
