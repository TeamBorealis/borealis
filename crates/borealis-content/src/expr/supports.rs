//! Supports expressions.

use super::bool_expr::{parse_all, BoolExpr, Cursor};
use super::{sanitize, ExprError, ExprErrorKind};
use std::fmt;

/// A dynamic `$(...)` placeholder inside a supports expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dynamic {
    /// `$(spellcasting:list)`: the owning spellcasting node's list expression.
    SpellcastingList,
    /// `$(spellcasting:slots)`: the spell levels the class has slots for.
    SpellcastingSlots,
    /// Any other `$(name)`; kept verbatim.
    Other(String),
}

impl Dynamic {
    fn parse(inner: &str) -> Self {
        match inner.trim() {
            "spellcasting:list" => Self::SpellcastingList,
            "spellcasting:slots" => Self::SpellcastingSlots,
            other => Self::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Dynamic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpellcastingList => f.write_str("$(spellcasting:list)"),
            Self::SpellcastingSlots => f.write_str("$(spellcasting:slots)"),
            Self::Other(name) => write!(f, "$({name})"),
        }
    }
}

/// An operand of a supports expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportsLeaf {
    /// A support tag (or, in ID-range mode, an element id).
    Tag(String),
    /// A dynamic placeholder.
    Dynamic(Dynamic),
}

impl fmt::Display for SupportsLeaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(t) => f.write_str(t),
            Self::Dynamic(d) => d.fmt(f),
        }
    }
}

/// A tag character (spec 8.1 as amended by IM13, GitHub #11): a letter or digit of any script,
/// or one of `_ . / $ -`, space, `:` `'` `’`. Never an operator (`, & | ! ( )`).
fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | ' ' | '.' | '/' | '$' | '-' | ':' | '\'' | '’')
}

fn parse_leaf(cursor: &mut Cursor) -> Result<SupportsLeaf, ExprError> {
    let start = cursor.position();
    if cursor.peek() == Some('$') && cursor.peek_at(1) == Some('(') {
        cursor.bump();
        cursor.bump();
        let inner = cursor.take_while(|c| c != ')');
        if cursor.bump() != Some(')') {
            return Err(cursor.error_at(start, ExprErrorKind::InvalidPlaceholder(inner)));
        }
        return Ok(SupportsLeaf::Dynamic(Dynamic::parse(&inner)));
    }
    let mut token = String::new();
    while let Some(c) = cursor.peek() {
        let starts_placeholder = c == '$' && cursor.peek_at(1) == Some('(');
        if !is_token_char(c) || starts_placeholder {
            break;
        }
        token.push(c);
        cursor.bump();
    }
    let token = token.trim();
    if token.is_empty() {
        return Err(match cursor.peek() {
            Some(c) => cursor.error(ExprErrorKind::UnexpectedChar(c)),
            None => cursor.error(ExprErrorKind::MissingOperand),
        });
    }
    Ok(SupportsLeaf::Tag(token.to_string()))
}

/// A parsed supports expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportsExpr {
    root: BoolExpr<SupportsLeaf>,
    id_range_mode: bool,
}

impl SupportsExpr {
    /// Parse a raw `supports` string. Blank input means "no filter" and yields `Ok(None)`.
    ///
    /// # Errors
    /// Returns [`ExprError`] when the sanitized text is not a valid expression.
    pub fn parse(raw: &str) -> Result<Option<Self>, ExprError> {
        if raw.trim().is_empty() {
            return Ok(None);
        }
        let id_range_mode = raw.contains('|') && !raw.contains("||");
        let sanitized = sanitize(raw);
        let root = parse_all(&sanitized, &parse_leaf)?;
        Ok(Some(Self {
            root,
            id_range_mode,
        }))
    }

    /// The expression tree.
    #[must_use]
    pub fn root(&self) -> &BoolExpr<SupportsLeaf> {
        &self.root
    }

    /// True when the raw text contained a single `|` and no `||`: tokens starting with
    /// `ID_` are then compared against candidate ids instead of support tags.
    #[must_use]
    pub fn is_id_range_mode(&self) -> bool {
        self.id_range_mode
    }

    /// All plain tag operands in order.
    #[must_use]
    pub fn tags(&self) -> Vec<&str> {
        self.root
            .leaves()
            .into_iter()
            .filter_map(|l| match l {
                SupportsLeaf::Tag(t) => Some(t.as_str()),
                SupportsLeaf::Dynamic(_) => None,
            })
            .collect()
    }

    /// All dynamic placeholders in order.
    #[must_use]
    pub fn dynamics(&self) -> Vec<&Dynamic> {
        self.root
            .leaves()
            .into_iter()
            .filter_map(|l| match l {
                SupportsLeaf::Dynamic(d) => Some(d),
                SupportsLeaf::Tag(_) => None,
            })
            .collect()
    }

    /// True if the expression contains any `$(...)` placeholder.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        !self.dynamics().is_empty()
    }
}

impl fmt::Display for SupportsExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sep = if self.id_range_mode { "|" } else { "||" };
        f.write_str(&self.root.render(sep))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str) -> BoolExpr<SupportsLeaf> {
        BoolExpr::Leaf(SupportsLeaf::Tag(s.into()))
    }

    #[test]
    fn blank_is_no_filter() {
        assert_eq!(SupportsExpr::parse("  ").unwrap(), None);
    }

    #[test]
    fn comma_is_and_and_spaces_inside_tokens_survive() {
        let e = SupportsExpr::parse("Skill, Sleight of Hand")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::And(vec![tag("Skill"), tag("Sleight of Hand")])
        );
        assert_eq!(e.to_string(), "Skill&&Sleight of Hand");
    }

    #[test]
    fn precedence_and_binds_tighter_than_or() {
        let e = SupportsExpr::parse("A||B&&C").unwrap().unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::Or(vec![tag("A"), BoolExpr::And(vec![tag("B"), tag("C")])])
        );
        assert_eq!(e.to_string(), "A||B&&C");
    }

    #[test]
    fn parentheses_and_not() {
        let e = SupportsExpr::parse("(A||B), !C").unwrap().unwrap();
        assert_eq!(e.to_string(), "(A||B)&&!C");
        let e2 = SupportsExpr::parse(&e.to_string()).unwrap().unwrap();
        assert_eq!(e, e2);
    }

    #[test]
    fn dynamic_placeholders() {
        let e = SupportsExpr::parse("$(spellcasting:list), 0")
            .unwrap()
            .unwrap();
        assert!(e.is_dynamic());
        assert_eq!(e.dynamics(), vec![&Dynamic::SpellcastingList]);
        assert_eq!(e.tags(), vec!["0"]);
        assert_eq!(e.to_string(), "$(spellcasting:list)&&0");

        let e = SupportsExpr::parse("(0)||($(spellcasting:slots))")
            .unwrap()
            .unwrap();
        assert_eq!(e.dynamics(), vec![&Dynamic::SpellcastingSlots]);
        assert_eq!(e.to_string(), "0||$(spellcasting:slots)");

        let e = SupportsExpr::parse("$(custom:thing)").unwrap().unwrap();
        assert_eq!(e.dynamics(), vec![&Dynamic::Other("custom:thing".into())]);
    }

    #[test]
    fn id_range_mode_uses_single_pipes() {
        let e = SupportsExpr::parse("ID_A|ID_B|ID_C").unwrap().unwrap();
        assert!(e.is_id_range_mode());
        assert_eq!(e.tags(), vec!["ID_A", "ID_B", "ID_C"]);
        assert_eq!(e.to_string(), "ID_A|ID_B|ID_C");
        let again = SupportsExpr::parse(&e.to_string()).unwrap().unwrap();
        assert_eq!(e, again);

        let e = SupportsExpr::parse("Standard||Exotic").unwrap().unwrap();
        assert!(!e.is_id_range_mode());
        let e = SupportsExpr::parse("A|B||C").unwrap().unwrap();
        assert!(!e.is_id_range_mode());
    }

    #[test]
    fn flattening_makes_display_a_fixed_point() {
        let e = SupportsExpr::parse("(A&&B)&&C").unwrap().unwrap();
        assert_eq!(e.root(), &BoolExpr::And(vec![tag("A"), tag("B"), tag("C")]));
        assert_eq!(e.to_string(), "A&&B&&C");
    }

    #[test]
    fn invalid_expressions_are_rejected() {
        // IM13 (spec 8.1 as amended 2026-10-31): `:` `'` `’` and letters are tag characters now,
        // so "Smith’s Tools" and "A:B" moved to `tags_accept_any_letter_colon_and_apostrophes`;
        // operator characters still end a tag
        for bad in [
            "A&&",
            "(A",
            "A)",
            "()",
            "!",
            "A B C (",
            "Smith’s (Tools",
            "A:B)",
            "Senryōka (",
            "A;B",
            "A\"B",
        ] {
            assert!(SupportsExpr::parse(bad).is_err(), "{bad} should not parse");
        }
        let err = SupportsExpr::parse("$(oops").unwrap_err();
        assert!(matches!(err.kind, ExprErrorKind::InvalidPlaceholder(_)));
    }

    /// IM13 (GitHub #11): Aurora homebrew copies its tags verbatim into selects, and Aurora's own
    /// parser throws on these; Borealis reads them as the single tags they plainly are.
    #[test]
    fn tags_accept_any_letter_colon_and_apostrophes() {
        for text in [
            "Companion 4: Ability Boost",
            "Spirit Bonded Senryōka",
            "Spider's Cunning",
            "Smith’s Tools",
            "A:B",
            "Ünïcödé Tag",
            "Ōkami: Ninjutsu",
        ] {
            let e = SupportsExpr::parse(text).unwrap().unwrap();
            assert_eq!(e.root(), &tag(text), "{text}");
            assert_eq!(e.to_string(), text);
            assert_eq!(SupportsExpr::parse(&e.to_string()).unwrap().unwrap(), e);
        }
        let e = SupportsExpr::parse("!A:B").unwrap().unwrap();
        assert_eq!(e.root(), &BoolExpr::Not(Box::new(tag("A:B"))));
        let e = SupportsExpr::parse("A,B:C").unwrap().unwrap();
        assert_eq!(e.root(), &BoolExpr::And(vec![tag("A"), tag("B:C")]));
        let e = SupportsExpr::parse("Spider's Cunning||Senryōka: Path")
            .unwrap()
            .unwrap();
        assert_eq!(
            e.root(),
            &BoolExpr::Or(vec![tag("Spider's Cunning"), tag("Senryōka: Path")])
        );
        // the spell placeholders keep their meaning
        let e = SupportsExpr::parse("$(spellcasting:list), Druid: Circle")
            .unwrap()
            .unwrap();
        assert_eq!(e.dynamics(), vec![&Dynamic::SpellcastingList]);
        assert_eq!(e.tags(), vec!["Druid: Circle"]);
    }
}
