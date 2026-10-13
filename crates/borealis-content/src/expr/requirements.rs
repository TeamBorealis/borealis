//! Requirements expressions.

use super::bool_expr::{parse_all, BoolExpr, Cursor};
use super::{sanitize, ExprError, ExprErrorKind};
use std::fmt;

/// The value side of a `[key:value]` test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestValue {
    /// An integer threshold (`[str:13]`): the test means `>=`.
    Number(i64),
    /// Any other text (`[type:spell]`, `[feature:Fighting Style]`,
    /// `[ranger:archetype:swarmkeeper]`, `[level:]`), trimmed. Only `type` (`engine.md` 5.1) and
    /// the Borealis `feature` key (a live element with that name) give text a meaning; every
    /// other key with a text value never holds. The Borealis `[spellcasting:N]` term is numeric
    /// (a class spell slot of level `N` or higher).
    Text(String),
}

impl fmt::Display for TestValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(t) => f.write_str(t),
        }
    }
}

/// A `[key:value]` (or `[key=value]`) test. Numeric comparisons mean `>=`; negate the
/// test with `!` for "below".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatTest {
    /// The statistic, ability, `character`, `level`, `type` or class name. May contain `:`.
    pub key: String,
    /// The threshold, or the text to compare (`[type:spell]`).
    pub value: TestValue,
}

impl StatTest {
    /// The key lower-cased, as element `<requirements>` evaluation treats it.
    #[must_use]
    pub fn normalized_key(&self) -> String {
        self.key.to_lowercase()
    }
}

impl fmt::Display for StatTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sep = if self.key.contains('=') { '=' } else { ':' };
        write!(f, "[{}{sep}{}]", self.key, self.value)
    }
}

/// An operand of a requirements expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementLeaf {
    /// True iff the element with this id is part of the character.
    Id(String),
    /// A numeric test.
    Test(StatTest),
}

impl fmt::Display for RequirementLeaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => f.write_str(id),
            Self::Test(t) => t.fmt(f),
        }
    }
}

fn parse_test(cursor: &mut Cursor) -> Result<RequirementLeaf, ExprError> {
    let start = cursor.position();
    cursor.bump(); // '['
    let body = cursor.take_while(|c| c != ']');
    if cursor.bump() != Some(']') {
        return Err(cursor.error_at(start, ExprErrorKind::InvalidTest(body)));
    }
    let separator = if body.contains('=') { '=' } else { ':' };
    let Some((key, value)) = body.rsplit_once(separator) else {
        return Err(cursor.error_at(start, ExprErrorKind::InvalidTest(body)));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(cursor.error_at(start, ExprErrorKind::InvalidTest(body)));
    }
    let value = value.trim();
    // Aurora evaluates any value: integers compare `>=`, `type` compares text, and every other
    // text value is simply false (engine.md 5.1), so text is never a parse error.
    let value = value
        .parse::<i64>()
        .map_or_else(|_| TestValue::Text(value.to_string()), TestValue::Number);
    Ok(RequirementLeaf::Test(StatTest {
        key: key.to_string(),
        value,
    }))
}

fn parse_leaf(cursor: &mut Cursor) -> Result<RequirementLeaf, ExprError> {
    match cursor.peek() {
        Some('[') => parse_test(cursor),
        Some('I') if cursor.peek_at(1) == Some('D') && cursor.peek_at(2) == Some('_') => {
            let start = cursor.position();
            let id = cursor.take_while(|c| c.is_ascii_alphanumeric() || c == '_');
            if id.chars().count() < "ID_".len() + 2 {
                return Err(cursor.error_at(start, ExprErrorKind::InvalidId(id)));
            }
            Ok(RequirementLeaf::Id(id))
        }
        Some(c) => Err(cursor.error(ExprErrorKind::UnexpectedChar(c))),
        None => Err(cursor.error(ExprErrorKind::MissingOperand)),
    }
}

/// A parsed requirements expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementsExpr {
    root: BoolExpr<RequirementLeaf>,
}

impl RequirementsExpr {
    /// Parse a requirements string. Blank input means "always true" and yields `Ok(None)`.
    ///
    /// # Errors
    /// Returns [`ExprError`] for English prose, empty parentheses (`()`), tests without a key or
    /// separator and other malformed input.
    pub fn parse(raw: &str) -> Result<Option<Self>, ExprError> {
        if raw.trim().is_empty() {
            return Ok(None);
        }
        let root = parse_all(&sanitize(raw), &parse_leaf)?;
        Ok(Some(Self { root }))
    }

    /// The expression tree.
    #[must_use]
    pub fn root(&self) -> &BoolExpr<RequirementLeaf> {
        &self.root
    }

    /// Every element id referenced, in order (duplicates preserved).
    #[must_use]
    pub fn referenced_ids(&self) -> Vec<&str> {
        self.root
            .leaves()
            .into_iter()
            .filter_map(|l| match l {
                RequirementLeaf::Id(id) => Some(id.as_str()),
                RequirementLeaf::Test(_) => None,
            })
            .collect()
    }

    /// Every `[key:value]` test in order.
    #[must_use]
    pub fn tests(&self) -> Vec<&StatTest> {
        self.root
            .leaves()
            .into_iter()
            .filter_map(|l| match l {
                RequirementLeaf::Test(t) => Some(t),
                RequirementLeaf::Id(_) => None,
            })
            .collect()
    }
}

impl fmt::Display for RequirementsExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.root.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> BoolExpr<RequirementLeaf> {
        BoolExpr::Leaf(RequirementLeaf::Id(s.into()))
    }

    fn test(k: &str, v: i64) -> BoolExpr<RequirementLeaf> {
        BoolExpr::Leaf(RequirementLeaf::Test(StatTest {
            key: k.into(),
            value: TestValue::Number(v),
        }))
    }

    fn text(k: &str, v: &str) -> BoolExpr<RequirementLeaf> {
        BoolExpr::Leaf(RequirementLeaf::Test(StatTest {
            key: k.into(),
            value: TestValue::Text(v.into()),
        }))
    }

    #[test]
    fn text_values_parse() {
        let e = RequirementsExpr::parse("[type:Spell]").unwrap().unwrap();
        assert_eq!(e.root(), &text("type", "Spell"));
        assert_eq!(e.to_string(), "[type:Spell]");
        let e = RequirementsExpr::parse("[ranger:archetype:beast master]||[shield:any]")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::Or(vec![
                text("ranger:archetype", "beast master"),
                text("shield", "any")
            ])
        );
        let e = RequirementsExpr::parse("[level:esper variant12:]")
            .unwrap()
            .unwrap();
        assert_eq!(e.root(), &text("level:esper variant12", ""));
        let again = RequirementsExpr::parse(&e.to_string()).unwrap().unwrap();
        assert_eq!(e, again);
    }

    #[test]
    fn ids_tests_and_operators() {
        let e = RequirementsExpr::parse("([str:13]||[dex:13])")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::Or(vec![test("str", 13), test("dex", 13)])
        );
        assert_eq!(e.to_string(), "[str:13]||[dex:13]");

        let e = RequirementsExpr::parse("!ID_WOTC_PHB_MULTICLASS_FIGHTER")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::Not(Box::new(id("ID_WOTC_PHB_MULTICLASS_FIGHTER")))
        );

        let e = RequirementsExpr::parse("[level:15],ID_WOTC_PHB_CLASS_FEATURE_X")
            .unwrap()
            .unwrap();
        assert_eq!(e.referenced_ids(), vec!["ID_WOTC_PHB_CLASS_FEATURE_X"]);
        assert_eq!(e.tests().len(), 1);
    }

    #[test]
    fn test_keys_may_contain_colons_and_equals_separator() {
        let e = RequirementsExpr::parse("[innate speed:climb:1]")
            .unwrap()
            .unwrap();
        assert_eq!(e.root(), &test("innate speed:climb", 1));
        let e = RequirementsExpr::parse("[wizard=3]").unwrap().unwrap();
        assert_eq!(e.root(), &test("wizard", 3));
        let e = RequirementsExpr::parse("![str:15]").unwrap().unwrap();
        assert_eq!(e.to_string(), "![str:15]");
        let e = RequirementsExpr::parse("[level:-2]").unwrap().unwrap();
        assert_eq!(e.root(), &test("level", -2));
    }

    #[test]
    fn prefix_sharing_ids_are_distinct_tokens() {
        let e = RequirementsExpr::parse("ID_A_B||ID_A_BC").unwrap().unwrap();
        assert_eq!(e.referenced_ids(), vec!["ID_A_B", "ID_A_BC"]);
    }

    #[test]
    fn display_round_trips() {
        for raw in [
            "!ID_AA&&(ID_BB||[str:13])",
            "ID_INTERNAL_OPTION_ALLOW_MULTICLASSING&&([str:13]||[dex:13])&&!ID_WOTC_PHB_MULTICLASS_FIGHTER",
            "!(ID_AA||ID_BB)",
        ] {
            let e = RequirementsExpr::parse(raw).unwrap().unwrap();
            let again = RequirementsExpr::parse(&e.to_string()).unwrap().unwrap();
            assert_eq!(e, again, "{raw}");
        }
    }

    #[test]
    fn blank_and_invalid() {
        assert_eq!(RequirementsExpr::parse("").unwrap(), None);
        for bad in [
            "()",
            "!CLASS&&()",
            "Strength 13 or higher",
            "[str]",
            "[:3]",
            "[str:3",
            "ID_A",
            "ID_AA&&",
            "(ID_AA",
        ] {
            assert!(
                RequirementsExpr::parse(bad).is_err(),
                "{bad} should not parse"
            );
        }
    }

    #[test]
    fn normalized_key_lowercases() {
        let t = StatTest {
            key: "Wizard".into(),
            value: TestValue::Number(1),
        };
        assert_eq!(t.normalized_key(), "wizard");
    }
}
