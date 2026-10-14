//! Parsers (not evaluators) for the four expression sub-languages of the content format.
//!
//! * [`supports`] - select `supports`, spellcasting `list`/`extend`, magic item `armor`/`weapon`
//! * [`requirements`] - rule and element `requirements`
//! * [`equipped`] - stat `equipped`
//! * [`template`] - `{{key}}` / `$(key)` text placeholders
//!
//! All boolean languages share [`BoolExpr`] and the [`sanitize`] step. Evaluation belongs to `borealis-engine`.

mod bool_expr;
pub mod equipped;
pub mod requirements;
mod sanitize;
pub mod supports;
pub mod template;

pub use bool_expr::{BoolExpr, MAX_NESTING};
pub use equipped::{EquippedExpr, EquippedTest};
pub use requirements::{RequirementLeaf, RequirementsExpr, StatTest, TestValue};
pub use sanitize::sanitize;
pub use supports::{Dynamic, SupportsExpr, SupportsLeaf};
pub use template::{Placeholder, PlaceholderSyntax, Template, TemplatePart};

/// What went wrong while parsing an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprErrorKind {
    /// Nothing but whitespace where an operand was required.
    MissingOperand,
    /// A character that is not valid at this position.
    UnexpectedChar(char),
    /// A `(` without matching `)`.
    UnbalancedParen,
    /// `()` with nothing inside.
    EmptyGroup,
    /// A malformed `[key:value]` test.
    InvalidTest(String),
    /// A test value that is not an integer.
    InvalidInteger(String),
    /// A malformed `ID_...` reference.
    InvalidId(String),
    /// A malformed `$(...)` placeholder.
    InvalidPlaceholder(String),
    /// Nested deeper than [`MAX_NESTING`] levels.
    TooDeep,
}

impl std::fmt::Display for ExprErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOperand => write!(f, "missing operand"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character {c:?}"),
            Self::UnbalancedParen => write!(f, "unbalanced parenthesis"),
            Self::EmptyGroup => write!(f, "empty parentheses"),
            Self::InvalidTest(t) => write!(f, "invalid test [{t}]"),
            Self::InvalidInteger(v) => write!(f, "test value {v:?} is not an integer"),
            Self::InvalidId(v) => write!(f, "invalid element reference {v:?}"),
            Self::InvalidPlaceholder(v) => write!(f, "invalid placeholder {v:?}"),
            Self::TooDeep => write!(f, "nested more than {} levels deep", bool_expr::MAX_NESTING),
        }
    }
}

/// Error from an expression parser, with the sanitized input and the offending position
/// (a character offset into the sanitized text).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at position {position} in {input:?}")]
pub struct ExprError {
    /// The category of failure.
    pub kind: ExprErrorKind,
    /// Character offset into [`ExprError::input`].
    pub position: usize,
    /// The sanitized expression text that failed to parse.
    pub input: String,
}
