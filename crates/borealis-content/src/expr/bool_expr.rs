use super::{ExprError, ExprErrorKind};
use std::fmt;

/// Boolean expression tree shared by every boolean sub-language, generic over its leaf.
///
/// `And` and `Or` are n-ary and never nest directly inside the same operator (the parser
/// flattens `(a&&b)&&c`), so `Display` output re-parses to an equal tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolExpr<L> {
    /// A language specific operand.
    Leaf(L),
    /// `!x`
    Not(Box<BoolExpr<L>>),
    /// `a && b && ...` (at least two operands)
    And(Vec<BoolExpr<L>>),
    /// `a || b || ...` (at least two operands)
    Or(Vec<BoolExpr<L>>),
}

impl<L> BoolExpr<L> {
    /// Build an `And`, flattening nested `And`s and unwrapping a single operand.
    #[must_use]
    pub fn and(items: Vec<Self>) -> Self {
        Self::nary(items, true)
    }

    /// Build an `Or`, flattening nested `Or`s and unwrapping a single operand.
    #[must_use]
    pub fn or(items: Vec<Self>) -> Self {
        Self::nary(items, false)
    }

    fn nary(items: Vec<Self>, is_and: bool) -> Self {
        let mut flat = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Self::And(inner) if is_and => flat.extend(inner),
                Self::Or(inner) if !is_and => flat.extend(inner),
                other => flat.push(other),
            }
        }
        if flat.len() == 1 {
            return flat.remove(0);
        }
        if is_and {
            Self::And(flat)
        } else {
            Self::Or(flat)
        }
    }

    /// All leaves in left-to-right order.
    #[must_use]
    pub fn leaves(&self) -> Vec<&L> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a L>) {
        match self {
            Self::Leaf(l) => out.push(l),
            Self::Not(inner) => inner.collect_leaves(out),
            Self::And(items) | Self::Or(items) => {
                items.iter().for_each(|i| i.collect_leaves(out));
            }
        }
    }
}

const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;

impl<L: fmt::Display> BoolExpr<L> {
    /// Render with a custom `Or` separator (`||` normally, `|` for ID-range mode).
    #[must_use]
    pub fn render(&self, or_sep: &str) -> String {
        let mut out = String::new();
        self.write_to(&mut out, or_sep, 0);
        out
    }

    fn write_to(&self, out: &mut String, or_sep: &str, parent: u8) {
        use std::fmt::Write;
        match self {
            Self::Leaf(l) => {
                let _ = write!(out, "{l}");
            }
            Self::Not(inner) => {
                out.push('!');
                inner.write_to(out, or_sep, u8::MAX);
            }
            Self::And(items) => {
                let wrap = parent > PREC_AND;
                if wrap {
                    out.push('(');
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str("&&");
                    }
                    item.write_to(out, or_sep, PREC_AND);
                }
                if wrap {
                    out.push(')');
                }
            }
            Self::Or(items) => {
                let wrap = parent > PREC_OR;
                if wrap {
                    out.push('(');
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(or_sep);
                    }
                    item.write_to(out, or_sep, PREC_OR);
                }
                if wrap {
                    out.push(')');
                }
            }
        }
    }
}

impl<L: fmt::Display> fmt::Display for BoolExpr<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render("||"))
    }
}

/// Character cursor over a sanitized expression.
pub(crate) struct Cursor {
    chars: Vec<char>,
    pos: usize,
    input: String,
}

impl Cursor {
    pub(crate) fn new(sanitized: &str) -> Self {
        Self {
            chars: sanitized.chars().collect(),
            pos: 0,
            input: sanitized.to_string(),
        }
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    pub(crate) fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    pub(crate) fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }

    pub(crate) fn error(&self, kind: ExprErrorKind) -> ExprError {
        self.error_at(self.pos, kind)
    }

    pub(crate) fn error_at(&self, position: usize, kind: ExprErrorKind) -> ExprError {
        ExprError {
            kind,
            position,
            input: self.input.clone(),
        }
    }

    /// Consume characters while `pred` holds and return them.
    pub(crate) fn take_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let start = self.pos;
        while self.peek().is_some_and(&pred) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }
}

/// Parse a full boolean expression from `cursor`, calling `leaf` for operands.
pub(crate) fn parse_or<L>(
    cursor: &mut Cursor,
    leaf: &dyn Fn(&mut Cursor) -> Result<L, ExprError>,
) -> Result<BoolExpr<L>, ExprError> {
    let mut items = vec![parse_and(cursor, leaf)?];
    loop {
        cursor.skip_ws();
        if cursor.starts_with("||") {
            cursor.bump();
            cursor.bump();
            items.push(parse_and(cursor, leaf)?);
        } else {
            break;
        }
    }
    Ok(BoolExpr::or(items))
}

fn parse_and<L>(
    cursor: &mut Cursor,
    leaf: &dyn Fn(&mut Cursor) -> Result<L, ExprError>,
) -> Result<BoolExpr<L>, ExprError> {
    let mut items = vec![parse_unary(cursor, leaf)?];
    loop {
        cursor.skip_ws();
        if cursor.starts_with("&&") {
            cursor.bump();
            cursor.bump();
            items.push(parse_unary(cursor, leaf)?);
        } else {
            break;
        }
    }
    Ok(BoolExpr::and(items))
}

fn parse_unary<L>(
    cursor: &mut Cursor,
    leaf: &dyn Fn(&mut Cursor) -> Result<L, ExprError>,
) -> Result<BoolExpr<L>, ExprError> {
    cursor.skip_ws();
    match cursor.peek() {
        Some('!') => {
            cursor.bump();
            Ok(BoolExpr::Not(Box::new(parse_unary(cursor, leaf)?)))
        }
        Some('(') => {
            let open = cursor.position();
            cursor.bump();
            cursor.skip_ws();
            if cursor.peek() == Some(')') {
                return Err(cursor.error(ExprErrorKind::EmptyGroup));
            }
            let inner = parse_or(cursor, leaf)?;
            cursor.skip_ws();
            if cursor.peek() == Some(')') {
                cursor.bump();
                Ok(inner)
            } else {
                Err(cursor.error_at(open, ExprErrorKind::UnbalancedParen))
            }
        }
        None => Err(cursor.error(ExprErrorKind::MissingOperand)),
        Some(_) => leaf(cursor).map(BoolExpr::Leaf),
    }
}

/// Run the full grammar over `sanitized` and require all input to be consumed.
pub(crate) fn parse_all<L>(
    sanitized: &str,
    leaf: &dyn Fn(&mut Cursor) -> Result<L, ExprError>,
) -> Result<BoolExpr<L>, ExprError> {
    let mut cursor = Cursor::new(sanitized);
    let expr = parse_or(&mut cursor, leaf)?;
    cursor.skip_ws();
    match cursor.peek() {
        None => Ok(expr),
        Some(')') => Err(cursor.error(ExprErrorKind::UnbalancedParen)),
        Some(c) => Err(cursor.error(ExprErrorKind::UnexpectedChar(c))),
    }
}
