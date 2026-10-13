//! Equipped expressions.

use super::bool_expr::{parse_all, BoolExpr, Cursor};
use super::{sanitize, ExprError, ExprErrorKind};
use std::fmt;

/// A boolean `[key:value]` equipment test. Key and value are stored lower-cased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquippedTest {
    /// `armor`, `shield`, `main`, `primary`, `offhand`, `secondary`, `attunement`, `item`...
    pub key: String,
    /// `any`, `none`, a category, a name or an id. Empty means "any".
    pub value: String,
}

impl fmt::Display for EquippedTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}]", self.key, self.value)
    }
}

fn parse_leaf(cursor: &mut Cursor) -> Result<EquippedTest, ExprError> {
    let start = cursor.position();
    match cursor.peek() {
        Some('[') => {}
        Some(c) => return Err(cursor.error(ExprErrorKind::UnexpectedChar(c))),
        None => return Err(cursor.error(ExprErrorKind::MissingOperand)),
    }
    cursor.bump();
    let body = cursor.take_while(|c| c != ']');
    if cursor.bump() != Some(']') {
        return Err(cursor.error_at(start, ExprErrorKind::InvalidTest(body)));
    }
    let (key, value) = body.split_once(':').unwrap_or((body.as_str(), ""));
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return Err(cursor.error_at(start, ExprErrorKind::InvalidTest(body)));
    }
    Ok(EquippedTest {
        key,
        value: value.trim().to_lowercase(),
    })
}

/// A parsed equipped expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquippedExpr {
    root: BoolExpr<EquippedTest>,
}

impl EquippedExpr {
    /// Parse an `equipped` attribute. Blank input yields `Ok(None)`. A string without any
    /// bracket is wrapped in one pair, so `armor:none` means `[armor:none]`.
    ///
    /// # Errors
    /// Returns [`ExprError`] for malformed tests or operators.
    pub fn parse(raw: &str) -> Result<Option<Self>, ExprError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let wrapped;
        let text = if trimmed.contains('[') {
            trimmed
        } else {
            wrapped = format!("[{trimmed}]");
            wrapped.as_str()
        };
        let root = parse_all(&sanitize(text), &parse_leaf)?;
        Ok(Some(Self { root }))
    }

    /// The expression tree.
    #[must_use]
    pub fn root(&self) -> &BoolExpr<EquippedTest> {
        &self.root
    }

    /// Every test in order.
    #[must_use]
    pub fn tests(&self) -> Vec<&EquippedTest> {
        self.root.leaves()
    }
}

impl fmt::Display for EquippedExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.root.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(k: &str, v: &str) -> BoolExpr<EquippedTest> {
        BoolExpr::Leaf(EquippedTest {
            key: k.into(),
            value: v.into(),
        })
    }

    #[test]
    fn data_shapes() {
        let e = EquippedExpr::parse("[armor:none],[shield:none]")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::And(vec![t("armor", "none"), t("shield", "none")])
        );
        assert_eq!(e.to_string(), "[armor:none]&&[shield:none]");

        let e = EquippedExpr::parse("![armor:heavy]").unwrap().unwrap();
        assert_eq!(e.to_string(), "![armor:heavy]");

        let e = EquippedExpr::parse("[Armor:Light]").unwrap().unwrap();
        assert_eq!(e.root(), &t("armor", "light"));
    }

    #[test]
    fn bracketless_input_is_wrapped() {
        let e = EquippedExpr::parse("armor:none").unwrap().unwrap();
        assert_eq!(e.root(), &t("armor", "none"));
        let e = EquippedExpr::parse("[armor]").unwrap().unwrap();
        assert_eq!(e.root(), &t("armor", ""));
        assert_eq!(e.to_string(), "[armor:]");
        let again = EquippedExpr::parse(&e.to_string()).unwrap().unwrap();
        assert_eq!(e, again);
    }

    #[test]
    fn invalid() {
        assert_eq!(EquippedExpr::parse("").unwrap(), None);
        for bad in ["[armor:none", "[]", "[armor:none]&&"] {
            assert!(EquippedExpr::parse(bad).is_err(), "{bad}");
        }
    }
}
