//! The "while wearing/holding" builder (spec 6.3): stat `equipped`, read exactly as
//! `borealis_engine::eval::equipped_holds` evaluates it.
//!
//! Rows: armor (none, any, light, medium, heavy, a specific item), shield (none, any, a specific item), main hand and
//! off hand (none, anything, a weapon, a specific item; main hand also "a versatile weapon in both hands"), attuned
//! to an item, wearing an item. Tests under keys the engine doesn't read are kept as is (spec 6.5).

use super::{BuilderProblem, Built, ExprLanguage, Node};
use borealis_content::expr::{BoolExpr, EquippedExpr, EquippedTest};
use serde::{Deserialize, Serialize};

/// Where an equipment test looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    /// Body armor (`armor`).
    Armor,
    /// A shield in the off hand (`shield`).
    Shield,
    /// The main hand (`main`, also written `primary`).
    MainHand,
    /// The off hand, a shield excluded (`offhand`, also written `secondary`).
    OffHand,
}

impl Slot {
    fn key(self) -> &'static str {
        match self {
            Self::Armor => "armor",
            Self::Shield => "shield",
            Self::MainHand => "main",
            Self::OffHand => "offhand",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "armor" => Some(Self::Armor),
            "shield" => Some(Self::Shield),
            "main" | "primary" => Some(Self::MainHand),
            "offhand" | "secondary" => Some(Self::OffHand),
            _ => None,
        }
    }

    /// The words this slot gives a meaning (the rest name an item).
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Armor => &["", "any", "none", "light", "medium", "heavy"],
            Self::Shield => &["", "any", "none"],
            Self::MainHand => &["", "any", "none", "weapon", "versatile"],
            Self::OffHand => &["", "any", "none", "weapon"],
        }
    }
}

/// What a slot holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SlotTest {
    /// Something (`any`).
    Any,
    /// Nothing (`none`).
    None,
    /// Light armor.
    Light,
    /// Medium armor.
    Medium,
    /// Heavy armor.
    Heavy,
    /// A weapon (hands).
    Weapon,
    /// A versatile weapon held in both hands (main hand).
    Versatile,
    /// A specific item, by id or name (lower case, as the engine compares).
    Item {
        /// The item id or name.
        item: String,
    },
}

impl SlotTest {
    fn word(&self) -> &str {
        match self {
            Self::Any => "any",
            Self::None => "none",
            Self::Light => "light",
            Self::Medium => "medium",
            Self::Heavy => "heavy",
            Self::Weapon => "weapon",
            Self::Versatile => "versatile",
            Self::Item { item } => item,
        }
    }

    fn of(slot: Slot, value: &str) -> Self {
        let keyword = slot.keywords().contains(&value);
        match value {
            "" | "any" => Self::Any,
            "none" => Self::None,
            "light" if keyword => Self::Light,
            "medium" if keyword => Self::Medium,
            "heavy" if keyword => Self::Heavy,
            "weapon" if keyword => Self::Weapon,
            "versatile" if keyword => Self::Versatile,
            item => Self::Item {
                item: item.to_string(),
            },
        }
    }
}

/// One row of the equipped builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "row", rename_all = "camelCase")]
pub enum EquippedRow {
    /// What a slot holds (`[armor:heavy]`, `[main:weapon]`).
    Slot {
        /// The slot.
        slot: Slot,
        /// The test.
        test: SlotTest,
    },
    /// Attuned to an item (`[attunement:<id>]`).
    Attuned {
        /// Item id (lower case).
        item: String,
    },
    /// Wearing or holding an item (`[item:<id>]`).
    Wearing {
        /// Item id (lower case).
        item: String,
    },
    /// A test under a key the engine doesn't read, written back as is.
    Kept {
        /// The test's text.
        text: String,
    },
}

/// The equipped builder's model.
pub type Equipped = Node<EquippedRow>;

fn test_leaf(key: &str, value: &str) -> BoolExpr<EquippedTest> {
    BoolExpr::Leaf(EquippedTest {
        key: key.to_string(),
        value: value.trim().to_lowercase(),
    })
}

fn leaf(row: &EquippedRow) -> BoolExpr<EquippedTest> {
    match row {
        EquippedRow::Slot { slot, test } => test_leaf(slot.key(), test.word()),
        EquippedRow::Attuned { item } => test_leaf("attunement", item),
        EquippedRow::Wearing { item } => test_leaf("item", item),
        EquippedRow::Kept { text } => match EquippedExpr::parse(text) {
            Ok(Some(e)) => e.root().clone(),
            _ => test_leaf(text, ""),
        },
    }
}

fn row_of(t: &EquippedTest) -> EquippedRow {
    if let Some(slot) = Slot::from_key(&t.key) {
        return EquippedRow::Slot {
            slot,
            test: SlotTest::of(slot, &t.value),
        };
    }
    match t.key.as_str() {
        "attunement" | "attuned" => EquippedRow::Attuned {
            item: t.value.clone(),
        },
        "item" => EquippedRow::Wearing {
            item: t.value.clone(),
        },
        _ => EquippedRow::Kept {
            text: t.to_string(),
        },
    }
}

/// The equipped builder.
pub struct EquippedLanguage;

impl EquippedLanguage {
    /// The expression tree the model means (`None`: always).
    #[must_use]
    pub fn to_bool(model: &Equipped) -> Option<BoolExpr<EquippedTest>> {
        model.to_bool(&leaf)
    }

    fn from_bool(e: Option<&BoolExpr<EquippedTest>>) -> Equipped {
        Node::root_from_bool(e, &row_of, &|_| None)
    }
}

impl ExprLanguage for EquippedLanguage {
    type Model = Equipped;
    type Context = ();

    fn parse(text: &str, (): &()) -> Built<Equipped> {
        match EquippedExpr::parse(text) {
            Ok(e) => Built::Model {
                model: Self::from_bool(e.as_ref().map(EquippedExpr::root)),
            },
            Err(e) => Built::Kept {
                text: text.to_string(),
                reason: format!("Borealis can't read this equipment test ({}).", e.kind),
            },
        }
    }

    fn render(model: &Equipped, (): &()) -> String {
        Self::to_bool(model).map_or_else(String::new, |e| e.to_string())
    }

    fn means_same(model: &Equipped, text: &str, (): &()) -> bool {
        EquippedExpr::parse(text).is_ok_and(|e| {
            Self::from_bool(e.as_ref().map(EquippedExpr::root))
                == Self::from_bool(Self::to_bool(model).as_ref())
        })
    }

    fn check(model: &Equipped, (): &()) -> Vec<BuilderProblem> {
        model
            .rows()
            .into_iter()
            .filter_map(|(path, row)| row_problem(row).map(|m| BuilderProblem::new(path, m)))
            .collect()
    }
}

fn item_problem(item: &str) -> Option<String> {
    let i = item.trim();
    if i.is_empty() {
        Some("Pick the item.".to_string())
    } else if i.contains([']', '[', ',', '&', '|']) {
        Some("Pick the item again.".to_string())
    } else {
        None
    }
}

fn row_problem(row: &EquippedRow) -> Option<String> {
    match row {
        EquippedRow::Slot { slot, test } => {
            if let SlotTest::Item { item } = test {
                let v = item.trim().to_lowercase();
                if slot.keywords().contains(&v.as_str()) {
                    return Some(format!(
                        "\"{}\" reads as a kind of equipment here; pick the item.",
                        item.trim()
                    ));
                }
                return item_problem(item);
            }
            let ok = slot.keywords().contains(&test.word());
            (!ok).then(|| match slot {
                Slot::Armor => "Armor can be none, any, light, medium, heavy or an item.".to_string(),
                Slot::Shield => "A shield can be none, any or an item.".to_string(),
                Slot::MainHand => {
                    "The main hand can hold nothing, anything, a weapon, a versatile weapon or an item.".to_string()
                }
                Slot::OffHand => {
                    "The off hand can hold nothing, anything, a weapon or an item.".to_string()
                }
            })
        }
        EquippedRow::Attuned { item } | EquippedRow::Wearing { item } => item_problem(item),
        EquippedRow::Kept { .. } => None,
    }
}
