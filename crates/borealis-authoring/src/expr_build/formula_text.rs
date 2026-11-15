//! Formula text in each dialect: read the way the engine's reader of the field reads it, written so that reader reads
//! it back to the same model.

use super::formula::{class_problem, FormulaDialect, FormulaNode, Op};
use super::Ability;
use borealis_content::formula::{self, BinaryOp, Formula, Func};

const SPELL_SAVE_DC: &str = "spell save dc (";

/// Read `text` in `dialect`; `Err` is the plain reason it is kept as is.
pub(crate) fn parse_dialect(text: &str, dialect: FormulaDialect) -> Result<FormulaNode, String> {
    if text.trim().is_empty() {
        return Err("This formula is empty.".to_string());
    }
    match dialect {
        FormulaDialect::Formula => parse_formula(text),
        FormulaDialect::SaveDc => parse_save_dc(text),
        FormulaDialect::BreathDc => parse_formula(&breath_text(text)).and_then(dc_sum),
        FormulaDialect::NoteDc => parse_terms(text, note_term),
        FormulaDialect::Value => value_term(text).ok_or_else(|| unreadable(text)),
        FormulaDialect::ValueOrClassLevel => reduction_term(text).ok_or_else(|| unreadable(text)),
    }
}

fn unreadable(part: &str) -> String {
    format!("Borealis can't read \"{}\" in this formula.", part.trim())
}

fn parse_formula(text: &str) -> Result<FormulaNode, String> {
    let f = formula::parse(text).map_err(|e| unreadable(&e.part))?;
    from_formula(&f)
}

fn from_formula(f: &Formula) -> Result<FormulaNode, String> {
    let all = |args: &[Formula]| args.iter().map(from_formula).collect::<Result<Vec<_>, _>>();
    Ok(match f {
        Formula::Number(n) => FormulaNode::number(*n),
        Formula::Proficiency => FormulaNode::Proficiency,
        Formula::AbilityModifier(a) => {
            FormulaNode::ability(Ability::from_key(a).ok_or_else(|| {
                format!("\"{a}\" isn't an ability, so this formula never has a value.")
            })?)
        }
        // `ceil(a level / 2) + ceil(b level / 2)` reads as a division by a class named `2) + ceil(b`
        Formula::ClassLevel(c) if class_problem(c).is_some() => {
            return Err(format!(
                "This formula reads \"{c}\" as a class name, so it has no value."
            ))
        }
        Formula::ClassLevel(c) => FormulaNode::ClassLevel { class: c.clone() },
        Formula::Binary { op, left, right } => FormulaNode::binary(
            match op {
                BinaryOp::Add => Op::Add,
                BinaryOp::Sub => Op::Sub,
                BinaryOp::Mul => Op::Mul,
                BinaryOp::Div => Op::Div,
            },
            from_formula(left)?,
            from_formula(right)?,
        ),
        Formula::CeilDiv(n, d) => FormulaNode::CeilDiv {
            num: Box::new(from_formula(n)?),
            den: Box::new(from_formula(d)?),
        },
        Formula::Call(Func::Max, args) => FormulaNode::Max { args: all(args)? },
        Formula::Call(Func::Min, args) => FormulaNode::Min { args: all(args)? },
        Formula::Call(Func::Floor, args) if args.len() == 1 => FormulaNode::Floor {
            arg: Box::new(from_formula(&args[0])?),
        },
        Formula::Call(Func::Floor, _) => {
            return Err("This formula rounds down several values at once.".to_string())
        }
        Formula::Group(inner) => from_formula(inner)?,
    })
}

/// A save DC sum: numbers of 0 or more, the proficiency bonus, ability modifiers.
fn dc_sum(node: FormulaNode) -> Result<FormulaNode, String> {
    let ok = node.sum_terms().iter().all(|t| match t {
        FormulaNode::Number { value } => *value >= 0,
        FormulaNode::Proficiency | FormulaNode::Ability { .. } => true,
        _ => false,
    });
    if ok {
        Ok(node)
    } else {
        Err(
            "This save DC isn't a sum of numbers, the proficiency bonus and ability modifiers."
                .to_string(),
        )
    }
}

fn parse_terms(text: &str, term: fn(&str) -> Option<FormulaNode>) -> Result<FormulaNode, String> {
    let terms = text
        .split('+')
        .map(|t| term(t).ok_or_else(|| unreadable(t)))
        .collect::<Result<Vec<_>, _>>()?;
    FormulaNode::sum(terms).ok_or_else(|| unreadable(text))
}

fn dc_number(t: &str) -> Option<FormulaNode> {
    t.parse::<i32>()
        .ok()
        .filter(|n| *n >= 0)
        .map(FormulaNode::number)
}

/// `spell_rules::eval_save_dc_formula`: `spell save DC (<class>)`, or `+`-separated numbers, `proficiency`
/// (`proficiency bonus`) and full ability names, bare or with `:modifier`.
fn parse_save_dc(text: &str) -> Result<FormulaNode, String> {
    let t = text.trim();
    if let Some(rest) = t
        .get(..SPELL_SAVE_DC.len())
        .filter(|h| h.eq_ignore_ascii_case(SPELL_SAVE_DC))
        .and_then(|_| t.get(SPELL_SAVE_DC.len()..))
    {
        return Ok(FormulaNode::ClassSaveDc {
            class: rest.trim_end_matches(')').to_string(),
        });
    }
    parse_terms(text, |term| {
        let lower = term.trim().to_lowercase();
        if lower == "proficiency" || lower == "proficiency bonus" {
            return Some(FormulaNode::Proficiency);
        }
        let ability = lower
            .strip_suffix(":modifier")
            .map_or(lower.as_str(), str::trim);
        dc_number(&lower).or_else(|| Ability::from_full_name(ability).map(FormulaNode::ability))
    })
}

/// `notes_d::eval_note_dc`: `+`-separated numbers, `proficiency` and bare ability names (full or short).
fn note_term(term: &str) -> Option<FormulaNode> {
    let p = term.trim().to_lowercase();
    if p == "proficiency" {
        return Some(FormulaNode::Proficiency);
    }
    if p.contains(':') {
        return None;
    }
    dc_number(&p).or_else(|| Ability::from_key(&p).map(FormulaNode::ability))
}

/// `breath::dc_formula_text`: each `+` term that is a full ability name gets `:modifier`.
fn breath_text(text: &str) -> String {
    text.split('+')
        .map(|t| {
            let t = t.trim();
            if Ability::from_full_name(t).is_some_and(|a| a.full() == t.to_lowercase()) {
                format!("{t}:modifier")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// `rule_view::eval_value`: a number, `proficiency`, `proficiency:half`, `<ability>:modifier`.
fn value_term(text: &str) -> Option<FormulaNode> {
    let e = text.trim().to_lowercase();
    if let Ok(n) = e.parse::<i32>() {
        return Some(FormulaNode::number(n));
    }
    match e.as_str() {
        "proficiency" => return Some(FormulaNode::Proficiency),
        "proficiency:half" => return Some(FormulaNode::HalfProficiency),
        _ => {}
    }
    Ability::from_key(e.strip_suffix(":modifier")?).map(FormulaNode::ability)
}

/// `combat::reduction_term`: a value read with a proficiency bonus of 0 (so only numbers and ability modifiers keep
/// their meaning), else `<class> level`.
fn reduction_term(text: &str) -> Option<FormulaNode> {
    match value_term(text) {
        Some(FormulaNode::Proficiency | FormulaNode::HalfProficiency) => return None,
        Some(v) => return Some(v),
        None => {}
    }
    let class = text.trim_end_matches(" level");
    (class.len() < text.len() && !class.is_empty() && class.trim() == class).then(|| {
        FormulaNode::ClassLevel {
            class: class.to_lowercase(),
        }
    })
}

/// The text of `node` in `dialect`.
pub(crate) fn render_dialect(node: &FormulaNode, dialect: FormulaDialect) -> String {
    match dialect {
        FormulaDialect::SaveDc | FormulaDialect::BreathDc => {
            render_sum(node, |a| format!("{}:modifier", a.full()))
        }
        FormulaDialect::NoteDc => render_sum(node, |a| a.full().to_string()),
        // a single term: a negative number stays bare (`-1`; the value readers don't read parentheses)
        FormulaDialect::Value | FormulaDialect::ValueOrClassLevel => match node {
            FormulaNode::Number { value } => value.to_string(),
            other => unit(other),
        },
        FormulaDialect::Formula => unit(node),
    }
}

fn render_sum(node: &FormulaNode, ability: fn(Ability) -> String) -> String {
    if let FormulaNode::ClassSaveDc { class } = node {
        return format!("spell save DC ({class})");
    }
    node.sum_terms()
        .into_iter()
        .map(|t| match t {
            FormulaNode::Ability { ability: a } => ability(*a),
            other => unit(other),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn paren(s: &str) -> String {
    format!("({s})")
}

/// A `/` outside parentheses: the reader splits `ceil(...)` at its right-most one.
fn has_top_slash(text: &str) -> bool {
    let mut depth = 0i32;
    text.chars().any(|c| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        c == '/' && depth == 0
    })
}

/// Canonical text of one node, safe wherever the grammar reads a whole piece (the formula, an operand, an argument,
/// a parenthesis): it starts with `ceil(` only when the node is a `ceil`, a negative number is parenthesized (a sign
/// after an operator splits there), operands are parenthesized when the right-most-split reader would group them
/// otherwise, and a `ceil` left operand is parenthesized (`ceil(a / b) + ceil(c / d)` would read as one `ceil`).
fn unit(node: &FormulaNode) -> String {
    match node {
        FormulaNode::Number { value } if *value < 0 => paren(&value.to_string()),
        FormulaNode::Number { value } => value.to_string(),
        FormulaNode::Proficiency => "proficiency".to_string(),
        FormulaNode::HalfProficiency => "proficiency:half".to_string(),
        FormulaNode::Ability { ability } => format!("{}:modifier", ability.full()),
        FormulaNode::ClassLevel { class } => format!("{} level", class.trim().to_lowercase()),
        FormulaNode::ClassSaveDc { class } => format!("spell save DC ({class})"),
        FormulaNode::Binary { op, left, right } => {
            let l = unit(left);
            let l = match left.as_ref() {
                FormulaNode::CeilDiv { .. } => paren(&l),
                FormulaNode::Binary { op: lo, .. } if lo.is_additive() && !op.is_additive() => {
                    paren(&l)
                }
                _ => l,
            };
            let r = unit(right);
            let r = match right.as_ref() {
                FormulaNode::Binary { op: ro, .. }
                    if ro.is_additive() && !op.is_additive()
                        || ro.is_additive() == op.is_additive() =>
                {
                    paren(&r)
                }
                _ => r,
            };
            format!("{l} {} {r}", op.symbol())
        }
        FormulaNode::Max { args } => call("max", args),
        FormulaNode::Min { args } => call("min", args),
        FormulaNode::Floor { arg } => format!("floor({})", unit(arg)),
        FormulaNode::CeilDiv { num, den } => {
            let d = unit(den);
            let d = if has_top_slash(&d) { paren(&d) } else { d };
            format!("ceil({} / {d})", unit(num))
        }
    }
}

fn call(name: &str, args: &[FormulaNode]) -> String {
    let parts: Vec<String> = args.iter().map(unit).collect();
    format!("{name}({})", parts.join(", "))
}
