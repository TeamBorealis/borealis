//! The All of / Any of / None of tree the three boolean builders share (spec 6.3), and its exact mapping to
//! `borealis_content::expr::BoolExpr`.

use borealis_content::expr::BoolExpr;
use serde::{Deserialize, Serialize};

/// How a group joins its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupMode {
    /// Every item holds (`a&&b`).
    All,
    /// At least one item holds (`a||b`).
    Any,
    /// No item holds (`!(a||b)`; one item: `!a`).
    None,
}

/// One node of a builder tree: a row (a chip) or a group of nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "camelCase")]
pub enum Node<R> {
    /// One row.
    Row {
        /// The row.
        row: R,
    },
    /// A group.
    Group {
        /// How the items join.
        mode: GroupMode,
        /// The items; an empty group writes nothing.
        items: Vec<Node<R>>,
    },
}

impl<R> Node<R> {
    /// A row node.
    #[must_use]
    pub fn row(row: R) -> Self {
        Self::Row { row }
    }

    /// A group node.
    #[must_use]
    pub fn group(mode: GroupMode, items: Vec<Self>) -> Self {
        Self::Group { mode, items }
    }

    /// The empty tree (no condition).
    #[must_use]
    pub fn empty() -> Self {
        Self::group(GroupMode::All, Vec::new())
    }

    /// True when the tree writes nothing (no rows anywhere).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Row { .. } => false,
            Self::Group { items, .. } => items.iter().all(Self::is_empty),
        }
    }

    /// Every row with its path (`items.0.items.2`), left to right.
    #[must_use]
    pub fn rows(&self) -> Vec<(String, &R)> {
        let mut out = Vec::new();
        self.collect_rows(String::new(), &mut out);
        out
    }

    fn collect_rows<'a>(&'a self, path: String, out: &mut Vec<(String, &'a R)>) {
        match self {
            Self::Row { row } => out.push((path, row)),
            Self::Group { items, .. } => {
                for (i, item) in items.iter().enumerate() {
                    let p = if path.is_empty() {
                        format!("items.{i}")
                    } else {
                        format!("{path}.items.{i}")
                    };
                    item.collect_rows(p, out);
                }
            }
        }
    }

    /// The expression tree this node means; `None` for a tree without rows. Single-item groups disappear (`All` and
    /// `Any` of one item are the item), so the result is exactly what the content parser builds from the text.
    pub(crate) fn to_bool<L>(&self, leaf: &dyn Fn(&R) -> BoolExpr<L>) -> Option<BoolExpr<L>> {
        match self {
            Self::Row { row } => Some(leaf(row)),
            Self::Group { mode, items } => {
                let mut parts: Vec<BoolExpr<L>> =
                    items.iter().filter_map(|i| i.to_bool(leaf)).collect();
                if parts.is_empty() {
                    return None;
                }
                Some(match mode {
                    GroupMode::All => BoolExpr::and(parts),
                    GroupMode::Any => BoolExpr::or(parts),
                    GroupMode::None if parts.len() == 1 => BoolExpr::Not(Box::new(parts.remove(0))),
                    GroupMode::None => BoolExpr::Not(Box::new(BoolExpr::or(parts))),
                })
            }
        }
    }

    /// The builder tree of a parsed expression. `negated` turns `!leaf` into one row when the language has a row
    /// for it (`!ID_X` is "Doesn't have X").
    pub(crate) fn from_bool<L>(
        expr: &BoolExpr<L>,
        leaf: &dyn Fn(&L) -> R,
        negated: &dyn Fn(&L) -> Option<R>,
    ) -> Self {
        let all = |items: &[BoolExpr<L>], mode| {
            Self::group(
                mode,
                items
                    .iter()
                    .map(|e| Self::from_bool(e, leaf, negated))
                    .collect(),
            )
        };
        match expr {
            BoolExpr::Leaf(l) => Self::row(leaf(l)),
            BoolExpr::And(items) => all(items, GroupMode::All),
            BoolExpr::Or(items) => all(items, GroupMode::Any),
            BoolExpr::Not(inner) => match inner.as_ref() {
                BoolExpr::Leaf(l) => negated(l).map_or_else(
                    || Self::group(GroupMode::None, vec![Self::row(leaf(l))]),
                    Self::row,
                ),
                BoolExpr::Or(items) => all(items, GroupMode::None),
                other => Self::group(GroupMode::None, vec![Self::from_bool(other, leaf, negated)]),
            },
        }
    }

    /// [`Node::from_bool`] for a whole expression: the root is always a group, so the creator always shows an
    /// All of / Any of / None of heading.
    pub(crate) fn root_from_bool<L>(
        expr: Option<&BoolExpr<L>>,
        leaf: &dyn Fn(&L) -> R,
        negated: &dyn Fn(&L) -> Option<R>,
    ) -> Self {
        match expr.map(|e| Self::from_bool(e, leaf, negated)) {
            None => Self::empty(),
            Some(row @ Self::Row { .. }) => Self::group(GroupMode::All, vec![row]),
            Some(group) => group,
        }
    }
}

/// Text of `expr` with chosen separators: `,` or `&&` for All of, `||` or `|` for Any of. Parentheses only where
/// the parser would group otherwise (All of binds tighter than Any of), as `BoolExpr`'s own `Display` does.
pub(crate) fn render_bool<L: std::fmt::Display>(
    expr: &BoolExpr<L>,
    and_sep: &str,
    or_sep: &str,
) -> String {
    fn write<L: std::fmt::Display>(
        e: &BoolExpr<L>,
        and_sep: &str,
        or_sep: &str,
        parent: u8,
        out: &mut String,
    ) {
        const OR: u8 = 1;
        const AND: u8 = 2;
        let join = |items: &[BoolExpr<L>], sep: &str, prec: u8, out: &mut String| {
            let wrap = parent > prec;
            if wrap {
                out.push('(');
            }
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                write(item, and_sep, or_sep, prec, out);
            }
            if wrap {
                out.push(')');
            }
        };
        match e {
            BoolExpr::Leaf(l) => out.push_str(&l.to_string()),
            BoolExpr::Not(inner) => {
                out.push('!');
                write(inner, and_sep, or_sep, u8::MAX, out);
            }
            BoolExpr::And(items) => join(items, and_sep, AND, out),
            BoolExpr::Or(items) => join(items, or_sep, OR, out),
        }
    }
    let mut out = String::new();
    write(expr, and_sep, or_sep, 0, &mut out);
    out
}
