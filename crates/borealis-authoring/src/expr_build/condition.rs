//! The condition builder (spec 6.3): element and rule `requirements`, read exactly as `borealis_engine::eval`
//! evaluates them.
//!
//! Rows: *Has [element]*, *Doesn't have [element]*, *[Ability] score at least N* (`[str:13]`), *Character level at
//! least N* (`[character:4]`), *Level at least N* in the owning class (`[level:3]`), *[Class] level at least N*
//! (`[warlock:2]`), *Has a feature named [name]* (`[feature:...]`), *Can cast spells of level N*
//! (`[spellcasting:N]`), *Has an element of type [type]* (`[type:spell]`) and *[Statistic] at least N*
//! (`[innate speed:fly:1]`), grouped by All of / Any of / None of. Tests with a text value under any other key never
//! hold in the engine; they are kept as is ([`ConditionRow::Kept`]).

use super::{Ability, BuilderProblem, Built, ExprLanguage, GroupMode, Node};
use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr, StatTest, TestValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One row of the condition builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "row", rename_all = "camelCase")]
pub enum ConditionRow {
    /// The character has the element (`ID_X`).
    Has {
        /// Element id (picked, never shown).
        id: String,
    },
    /// The character doesn't have the element (`!ID_X`); "first class only" is this row with the class's multiclass id.
    Lacks {
        /// Element id.
        id: String,
    },
    /// An ability score of at least `min` (`[str:13]`).
    Ability {
        /// The ability.
        ability: Ability,
        /// The lowest score that passes.
        min: i64,
    },
    /// Character level at least `min` (`[character:4]`).
    CharacterLevel {
        /// The lowest level that passes.
        min: i64,
    },
    /// Level at least `min` in the class the rule belongs to; the character level outside a class (`[level:3]`).
    Level {
        /// The lowest level that passes.
        min: i64,
    },
    /// Levels in a class (`[warlock:2]`).
    ClassLevel {
        /// The class name, lower case.
        class: String,
        /// The lowest level that passes.
        min: i64,
    },
    /// Some element of the character has this name (`[feature:Fighting Style]`).
    Feature {
        /// The name, as written.
        name: String,
    },
    /// A class spell slot of this level or higher (`[spellcasting:1]`).
    Spellcasting {
        /// The spell level (1-9).
        level: i64,
    },
    /// Some element of the character has this type (`[type:spell]`).
    Type {
        /// The element type.
        value: String,
    },
    /// A statistic of at least `min` (`[innate speed:fly:1]`).
    Statistic {
        /// The statistic name, lower case.
        stat: String,
        /// The lowest value that passes.
        min: i64,
    },
    /// A test the builder cannot represent, written back as is (spec 6.5).
    Kept {
        /// The test's text (`[ranger:archetype:hunter]`).
        text: String,
    },
}

/// The condition builder's model.
pub type Condition = Node<ConditionRow>;

/// What reading a condition needs: the class names loaded content defines, so `[warlock:2]` reads as a class
/// level rather than a statistic (the engine looks up class levels before statistics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConditionContext {
    classes: BTreeSet<String>,
}

impl ConditionContext {
    /// A context knowing these class names (any case).
    #[must_use]
    pub fn new<I: IntoIterator<Item = S>, S: AsRef<str>>(classes: I) -> Self {
        Self {
            classes: classes
                .into_iter()
                .map(|c| c.as_ref().trim().to_lowercase())
                .collect(),
        }
    }

    fn is_class(&self, key: &str) -> bool {
        self.classes.contains(key)
    }
}

const FEATURE: &str = "feature";
const FEATURE_PREFIX: &str = "feature:";
/// Keys the engine reads before class levels and statistics.
const RESERVED_KEYS: [&str; 5] = ["character", "level", "spellcasting", "feature", "type"];

fn test(key: impl Into<String>, value: TestValue) -> BoolExpr<RequirementLeaf> {
    BoolExpr::Leaf(RequirementLeaf::Test(StatTest {
        key: key.into(),
        value,
    }))
}

/// A test value as the content parser reads it: an integer, else text.
fn value_of(text: &str) -> TestValue {
    let t = text.trim();
    t.parse::<i64>()
        .map_or_else(|_| TestValue::Text(t.to_string()), TestValue::Number)
}

/// The expression one row means.
fn leaf(row: &ConditionRow) -> BoolExpr<RequirementLeaf> {
    match row {
        ConditionRow::Has { id } => BoolExpr::Leaf(RequirementLeaf::Id(id.clone())),
        ConditionRow::Lacks { id } => {
            BoolExpr::Not(Box::new(BoolExpr::Leaf(RequirementLeaf::Id(id.clone()))))
        }
        ConditionRow::Ability { ability, min } => test(ability.short(), TestValue::Number(*min)),
        ConditionRow::CharacterLevel { min } => test("character", TestValue::Number(*min)),
        ConditionRow::Level { min } => test("level", TestValue::Number(*min)),
        ConditionRow::ClassLevel { class, min } => {
            test(class.trim().to_lowercase(), TestValue::Number(*min))
        }
        ConditionRow::Statistic { stat, min } => {
            test(stat.trim().to_lowercase(), TestValue::Number(*min))
        }
        ConditionRow::Spellcasting { level } => test("spellcasting", TestValue::Number(*level)),
        ConditionRow::Type { value } => test("type", TestValue::Text(value.trim().to_string())),
        ConditionRow::Feature { name } => {
            // the parser splits at the last `:`; the engine joins `feature:<head>` and the value back
            let name = name.trim();
            match name.rsplit_once(':') {
                Some((head, tail)) => {
                    test(format!("{FEATURE_PREFIX}{}", head.trim()), value_of(tail))
                }
                None => test(FEATURE, value_of(name)),
            }
        }
        ConditionRow::Kept { text } => match RequirementsExpr::parse(text) {
            Ok(Some(e)) => e.root().clone(),
            // never produced by `parse`; written back as text it stays unreadable, as it was
            _ => test(text.clone(), TestValue::Text(String::new())),
        },
    }
}

/// The row a parsed test is, in the order the engine reads tests (`eval.rs` `ReqEnv::test`).
fn row_of(l: &RequirementLeaf, cx: &ConditionContext) -> ConditionRow {
    let t = match l {
        RequirementLeaf::Id(id) => return ConditionRow::Has { id: id.clone() },
        RequirementLeaf::Test(t) => t,
    };
    let key = t.key.trim();
    let lower = key.to_lowercase();
    match &t.value {
        TestValue::Number(n) if lower == FEATURE => ConditionRow::Feature {
            name: n.to_string(),
        },
        TestValue::Number(n) => {
            let min = *n;
            if lower == "spellcasting" {
                ConditionRow::Spellcasting { level: min }
            } else if let Some(ability) = Ability::from_key(&lower) {
                ConditionRow::Ability { ability, min }
            } else if lower == "character" {
                ConditionRow::CharacterLevel { min }
            } else if lower == "level" {
                ConditionRow::Level { min }
            } else if cx.is_class(&lower) {
                ConditionRow::ClassLevel { class: lower, min }
            } else {
                ConditionRow::Statistic { stat: lower, min }
            }
        }
        TestValue::Text(v) if lower == FEATURE => ConditionRow::Feature { name: v.clone() },
        TestValue::Text(v) if lower.starts_with(FEATURE_PREFIX) => ConditionRow::Feature {
            name: format!("{}:{v}", &key[FEATURE_PREFIX.len()..]),
        },
        TestValue::Text(v) if lower == "type" => ConditionRow::Type { value: v.clone() },
        TestValue::Text(_) => ConditionRow::Kept {
            text: l.to_string(),
        },
    }
}

fn negated_row(l: &RequirementLeaf) -> Option<ConditionRow> {
    match l {
        RequirementLeaf::Id(id) => Some(ConditionRow::Lacks { id: id.clone() }),
        RequirementLeaf::Test(_) => None,
    }
}

/// The condition builder.
pub struct ConditionLanguage;

impl ConditionLanguage {
    /// The expression tree a condition means (`None`: no condition).
    #[must_use]
    pub fn to_bool(model: &Condition) -> Option<BoolExpr<RequirementLeaf>> {
        model.to_bool(&leaf)
    }

    /// The model of a parsed tree.
    #[must_use]
    pub fn from_bool(expr: Option<&BoolExpr<RequirementLeaf>>, cx: &ConditionContext) -> Condition {
        Node::root_from_bool(expr, &|l| row_of(l, cx), &negated_row)
    }

    /// The "first class only" condition: doesn't have the class's multiclass element (spec 4.5).
    #[must_use]
    pub fn first_class_only(multiclass_id: &str) -> Condition {
        Node::group(
            GroupMode::All,
            vec![Node::row(ConditionRow::Lacks {
                id: multiclass_id.to_string(),
            })],
        )
    }
}

impl ExprLanguage for ConditionLanguage {
    type Model = Condition;
    type Context = ConditionContext;

    fn parse(text: &str, cx: &ConditionContext) -> Built<Condition> {
        match RequirementsExpr::parse(text) {
            Ok(e) => Built::Model {
                model: Self::from_bool(e.as_ref().map(RequirementsExpr::root), cx),
            },
            Err(e) => Built::Kept {
                text: text.to_string(),
                reason: format!("Borealis can't read this condition ({}).", e.kind),
            },
        }
    }

    fn render(model: &Condition, _cx: &ConditionContext) -> String {
        Self::to_bool(model).map_or_else(String::new, |e| e.to_string())
    }

    fn means_same(model: &Condition, text: &str, cx: &ConditionContext) -> bool {
        let Ok(parsed) = RequirementsExpr::parse(text) else {
            return false;
        };
        let normal = Self::from_bool(Self::to_bool(model).as_ref(), cx);
        Self::from_bool(parsed.as_ref().map(RequirementsExpr::root), cx) == normal
    }

    fn check(model: &Condition, _cx: &ConditionContext) -> Vec<BuilderProblem> {
        model
            .rows()
            .into_iter()
            .filter_map(|(path, row)| row_problem(row).map(|m| BuilderProblem::new(path, m)))
            .collect()
    }
}

/// Text inside `[...]`: no `]`, no `=` (it would become the separator), no operator the sanitizer rewrites.
fn bracket_safe(text: &str) -> bool {
    !text.contains([']', '=', '[']) && !text.contains([',', '&', '|'])
}

fn key_problem(key: &str, what: &str) -> Option<String> {
    let k = key.trim().to_lowercase();
    if k.is_empty() {
        return Some(format!("Pick a {what}."));
    }
    if !bracket_safe(&k) {
        return Some(format!("A {what} name can't contain [ ] = , & or |."));
    }
    if RESERVED_KEYS.contains(&k.as_str()) || Ability::from_key(&k).is_some() {
        return Some(format!(
            "\"{}\" can't be used as a {what} name here: Borealis reads it as something else.",
            key.trim()
        ));
    }
    None
}

fn row_problem(row: &ConditionRow) -> Option<String> {
    match row {
        ConditionRow::Has { id } | ConditionRow::Lacks { id } => {
            let ok = id.len() > "ID_".len() + 1
                && id.starts_with("ID_")
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            (!ok).then(|| "Pick the element again.".to_string())
        }
        ConditionRow::ClassLevel { class, .. } => key_problem(class, "class"),
        ConditionRow::Statistic { stat, .. } => key_problem(stat, "statistic"),
        ConditionRow::Spellcasting { level } => {
            (!(1..=9).contains(level)).then(|| "Choose a spell level from 1 to 9.".to_string())
        }
        ConditionRow::Feature { name } => feature_problem(name),
        ConditionRow::Type { value } => {
            let v = value.trim();
            if v.is_empty() {
                Some("Pick an element type.".to_string())
            } else if !bracket_safe(v) || v.contains(':') {
                Some("An element type can't contain [ ] = : , & or |.".to_string())
            } else {
                None
            }
        }
        ConditionRow::Ability { .. }
        | ConditionRow::CharacterLevel { .. }
        | ConditionRow::Level { .. }
        | ConditionRow::Kept { .. } => None,
    }
}

fn feature_problem(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        return Some("Give the feature's name.".to_string());
    }
    if !bracket_safe(n) {
        return Some("A feature name can't contain [ ] = , & or |.".to_string());
    }
    // `[feature:Foo: 3]` reads as the statistic "feature:foo" at least 3
    if n.rsplit_once(':')
        .is_some_and(|(_, tail)| tail.trim().parse::<i64>().is_ok() || tail.trim().is_empty())
    {
        return Some(
            "A feature name can't end with a colon followed by a number or nothing.".to_string(),
        );
    }
    None
}
