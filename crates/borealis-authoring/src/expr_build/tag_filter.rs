//! The tag filter builder (spec 6.3): select `supports`, spellcasting lists, magic item `weapon`/`armor`, read
//! exactly as `borealis_engine::eval::supports_match` evaluates them.
//!
//! Two modes, as the content format has them: *Specific elements* (`ID_A|ID_B`, single bars: an `ID_` term names
//! one element) and *Tagged* (`Standard||Exotic`: a term names a tag, and an `ID_` term an element or a tag of that
//! name). Rows: a tag, an element, "the class's spell list" (`$(spellcasting:list)`), "a level the class can cast"
//! (`$(spellcasting:slots)`), "the option chosen for [name]" (`$(param:Spell List)`); any other `$(...)` is kept as
//! is (spec 6.5).
//!
//! Canonical text joins All of with `,` (`$(spellcasting:list),(1||2)`), the spelling the bundled filters use; the
//! parser reads `,` and `&&` the same.

use super::tree::render_bool;
use super::{BuilderProblem, Built, ExprLanguage, Node};
use borealis_content::expr::{BoolExpr, Dynamic, SupportsExpr, SupportsLeaf};
use serde::{Deserialize, Serialize};

/// One row of the tag filter builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "row", rename_all = "camelCase")]
pub enum TagRow {
    /// A support tag (`Exotic`, `Wizard`, `3`).
    Tag {
        /// The tag, as written.
        tag: String,
    },
    /// One element (an `ID_` term; picked, never shown).
    Element {
        /// Element id.
        id: String,
    },
    /// The owning class's spell list (`$(spellcasting:list)`).
    ClassList,
    /// A spell level the class has slots for (`$(spellcasting:slots)`).
    CastableLevel,
    /// The value chosen in the element's named choice (`$(param:Spell List)`, a `select_param`).
    Choice {
        /// The choice's name.
        name: String,
    },
    /// A placeholder the engine doesn't know, written back as is (spec 6.5).
    Kept {
        /// The placeholder's text (`$(something)`).
        text: String,
    },
}

/// The tag filter builder's model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagFilter {
    /// *Specific elements* mode (single bars). It has a meaning only when the filter has an Any of group; without one
    /// the text has no bar and reads as *Tagged*.
    pub specific: bool,
    /// The rows.
    pub root: Node<TagRow>,
}

impl TagFilter {
    /// A *Specific elements* filter of these ids.
    #[must_use]
    pub fn elements(ids: &[&str]) -> Self {
        Self {
            specific: true,
            root: Node::group(
                super::GroupMode::Any,
                ids.iter()
                    .map(|id| {
                        Node::row(TagRow::Element {
                            id: (*id).to_string(),
                        })
                    })
                    .collect(),
            ),
        }
    }
}

const PARAM: &str = "param:";

fn leaf(row: &TagRow) -> BoolExpr<SupportsLeaf> {
    BoolExpr::Leaf(match row {
        TagRow::Tag { tag } => SupportsLeaf::Tag(tag.trim().to_string()),
        TagRow::Element { id } => SupportsLeaf::Tag(id.trim().to_string()),
        TagRow::ClassList => SupportsLeaf::Dynamic(Dynamic::SpellcastingList),
        TagRow::CastableLevel => SupportsLeaf::Dynamic(Dynamic::SpellcastingSlots),
        TagRow::Choice { name } => {
            SupportsLeaf::Dynamic(Dynamic::Other(format!("{PARAM}{}", name.trim())))
        }
        TagRow::Kept { text } => {
            let inner = text
                .trim()
                .strip_prefix("$(")
                .and_then(|t| t.strip_suffix(')'))
                .unwrap_or(text);
            SupportsLeaf::Dynamic(Dynamic::Other(inner.trim().to_string()))
        }
    })
}

fn row_of(l: &SupportsLeaf) -> TagRow {
    match l {
        SupportsLeaf::Tag(t) if t.starts_with("ID_") => TagRow::Element { id: t.clone() },
        SupportsLeaf::Tag(t) => TagRow::Tag { tag: t.clone() },
        SupportsLeaf::Dynamic(Dynamic::SpellcastingList) => TagRow::ClassList,
        SupportsLeaf::Dynamic(Dynamic::SpellcastingSlots) => TagRow::CastableLevel,
        SupportsLeaf::Dynamic(Dynamic::Other(o)) => match o.strip_prefix(PARAM) {
            Some(name) if !name.trim().is_empty() => TagRow::Choice {
                name: name.trim().to_string(),
            },
            _ => TagRow::Kept {
                text: format!("$({o})"),
            },
        },
    }
}

fn has_any(e: &BoolExpr<SupportsLeaf>) -> bool {
    match e {
        BoolExpr::Leaf(_) => false,
        BoolExpr::Or(_) => true,
        BoolExpr::Not(inner) => has_any(inner),
        BoolExpr::And(items) => items.iter().any(has_any),
    }
}

/// The tag filter builder.
pub struct TagFilterLanguage;

impl TagFilterLanguage {
    /// The expression tree and the Specific elements flag the engine reads from the filter's text (`None`: no filter).
    #[must_use]
    pub fn to_bool(model: &TagFilter) -> Option<(BoolExpr<SupportsLeaf>, bool)> {
        let e = model.root.to_bool(&leaf)?;
        let specific = model.specific && has_any(&e);
        Some((e, specific))
    }

    fn from_expr(e: Option<&SupportsExpr>) -> TagFilter {
        TagFilter {
            specific: e.is_some_and(SupportsExpr::is_id_range_mode),
            root: Node::root_from_bool(e.map(SupportsExpr::root), &row_of, &|_| None),
        }
    }

    fn normal(model: &TagFilter) -> TagFilter {
        let parts = Self::to_bool(model);
        TagFilter {
            specific: parts.as_ref().is_some_and(|(_, s)| *s),
            root: Node::root_from_bool(parts.as_ref().map(|(e, _)| e), &row_of, &|_| None),
        }
    }
}

impl ExprLanguage for TagFilterLanguage {
    type Model = TagFilter;
    type Context = ();

    fn parse(text: &str, (): &()) -> Built<TagFilter> {
        match SupportsExpr::parse(text) {
            Ok(e) => Built::Model {
                model: Self::from_expr(e.as_ref()),
            },
            Err(e) => Built::Kept {
                text: text.to_string(),
                reason: format!("Borealis can't read this filter ({}).", e.kind),
            },
        }
    }

    fn render(model: &TagFilter, (): &()) -> String {
        Self::to_bool(model).map_or_else(String::new, |(e, specific)| {
            render_bool(&e, ",", if specific { "|" } else { "||" })
        })
    }

    fn means_same(model: &TagFilter, text: &str, (): &()) -> bool {
        SupportsExpr::parse(text).is_ok_and(|e| Self::from_expr(e.as_ref()) == Self::normal(model))
    }

    fn check(model: &TagFilter, (): &()) -> Vec<BuilderProblem> {
        model
            .root
            .rows()
            .into_iter()
            .filter_map(|(path, row)| row_problem(row).map(|m| BuilderProblem::new(path, m)))
            .collect()
    }
}

/// The row's text read alone gives back exactly the row (no operator, placeholder or stray space inside it).
fn reads_back(row: &TagRow) -> bool {
    let text = leaf(row).to_string();
    SupportsExpr::parse(&text)
        .ok()
        .flatten()
        .is_some_and(|e| e.root() == &leaf(row) && row_of_root(&e).as_ref() == Some(row))
}

fn row_of_root(e: &SupportsExpr) -> Option<TagRow> {
    match e.root() {
        BoolExpr::Leaf(l) => Some(row_of(l)),
        _ => None,
    }
}

fn row_problem(row: &TagRow) -> Option<String> {
    let blank = match row {
        TagRow::Tag { tag: s } | TagRow::Element { id: s } | TagRow::Choice { name: s } => {
            s.trim().is_empty() || s.trim() != s
        }
        TagRow::ClassList | TagRow::CastableLevel | TagRow::Kept { .. } => false,
    };
    if blank {
        return Some(match row {
            TagRow::Tag { .. } => "Pick a tag.".to_string(),
            TagRow::Element { .. } => "Pick the element again.".to_string(),
            _ => "Pick the choice again.".to_string(),
        });
    }
    if reads_back(row) {
        return None;
    }
    Some(match row {
        TagRow::Tag { tag } if tag.starts_with("ID_") => {
            "A tag can't start with ID_; pick the element instead.".to_string()
        }
        TagRow::Tag { .. } => {
            "A tag can use letters, digits, spaces and . / - _ : ' only.".to_string()
        }
        TagRow::Element { .. } => "Pick the element again.".to_string(),
        _ => "A choice name can't contain ( ) , & | or !.".to_string(),
    })
}
