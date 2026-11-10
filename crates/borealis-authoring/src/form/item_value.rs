//! Value shapes of the item fields that are lists (spec 4.9), on the wire and in [`super::FormState::fields`]:
//!
//! | Control | Value |
//! |---|---|
//! | `contents` | list of [`ContentsRow`]: an item pack's `extract.items` |
//! | `packageOptions` | list of [`PackageOption`]: an Equipment Package's `data.options` |
//!
//! TS mirrors: `HomebrewContentsRow`, `HomebrewPackageOption`, `HomebrewPackageItem` (`ui/src/api/types/homebrew.ts`).
//! Unknown parts of a stored row travel in `kept` and are written back unchanged.

use super::value::RefValue;
use super::Lookup;
use borealis_content::extra::Extra;
use borealis_content::{Extract, ExtractItem};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One item of a pack (`extract.items[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentsRow {
    /// The item; `None` until one is picked.
    #[serde(default)]
    pub item: Option<RefValue>,
    /// How many (1 when the pack doesn't say).
    #[serde(default = "one")]
    pub amount: u32,
    /// Unknown attributes of the stored row.
    #[serde(default, skip_serializing_if = "Extra::is_empty")]
    pub kept: Extra,
}

const fn one() -> u32 {
    1
}

impl ContentsRow {
    /// The rows of a stored pack.
    #[must_use]
    pub fn rows(extract: Option<&Extract>, lookup: &dyn Lookup) -> Vec<Self> {
        extract
            .map(|x| {
                x.items
                    .iter()
                    .map(|i| Self {
                        item: Some(RefValue::resolve(&i.id, lookup)),
                        amount: i.amount.unwrap_or(1),
                        kept: i.extra.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The stored item of this row; `None` when no item is picked.
    #[must_use]
    pub fn to_item(&self) -> Option<ExtractItem> {
        let id = self.item.as_ref()?.id.trim();
        (!id.is_empty()).then(|| ExtractItem {
            id: id.to_string(),
            amount: (self.amount != 1).then_some(self.amount),
            extra: self.kept.clone(),
        })
    }
}

/// The coins of a package option (`coins`), each denomination a whole number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coins {
    /// Copper.
    #[serde(default)]
    pub cp: u64,
    /// Silver.
    #[serde(default)]
    pub sp: u64,
    /// Electrum.
    #[serde(default)]
    pub ep: u64,
    /// Gold.
    #[serde(default)]
    pub gp: u64,
    /// Platinum.
    #[serde(default)]
    pub pp: u64,
}

/// Coin keys in the order they are written.
pub(crate) const COIN_KEYS: [&str; 5] = ["cp", "sp", "ep", "gp", "pp"];

impl Coins {
    fn get(&self, key: &str) -> u64 {
        match key {
            "cp" => self.cp,
            "sp" => self.sp,
            "ep" => self.ep,
            "gp" => self.gp,
            "pp" => self.pp,
            _ => 0,
        }
    }

    fn from_value(v: Option<&Value>) -> Self {
        let n = |k: &str| {
            v.and_then(|c| c.get(k))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        Self {
            cp: n("cp"),
            sp: n("sp"),
            ep: n("ep"),
            gp: n("gp"),
            pp: n("pp"),
        }
    }

    /// The stored object: only the denominations with coins.
    fn to_value(&self) -> Value {
        Value::Object(
            COIN_KEYS
                .iter()
                .filter(|k| self.get(k) > 0)
                .map(|k| ((*k).to_string(), Value::from(self.get(k))))
                .collect(),
        )
    }
}

/// One item of a package option (`items[]`): a loaded item, or a generic item named in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageItem {
    /// The item; `None` for a generic item (`name`) or before one is picked.
    #[serde(default)]
    pub item: Option<RefValue>,
    /// A generic item's name (when no item is picked).
    #[serde(default)]
    pub name: String,
    /// How many (1 when the package doesn't say).
    #[serde(default = "one")]
    pub qty: u32,
    /// A remark shown after it ("prayers", "10 sheets").
    #[serde(default)]
    pub note: String,
    /// Unknown keys of the stored item.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub kept: Map<String, Value>,
}

const ITEM_KEYS: [&str; 4] = ["id", "name", "qty", "note"];
const OPTION_KEYS: [&str; 3] = ["id", "items", "coins"];

fn rest(v: &Value, known: &[&str]) -> Map<String, Value> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter(|(k, _)| !known.contains(&k.as_str()))
                .map(|(k, x)| (k.clone(), x.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl PackageItem {
    fn read(v: &Value, lookup: &dyn Lookup) -> Self {
        let id = text(v, "id");
        Self {
            item: (!id.is_empty()).then(|| RefValue::resolve(&id, lookup)),
            name: text(v, "name"),
            qty: v
                .get("qty")
                .and_then(Value::as_u64)
                .and_then(|q| u32::try_from(q).ok())
                .unwrap_or(1),
            note: text(v, "note"),
            kept: rest(v, &ITEM_KEYS),
        }
    }

    /// The stored item; `None` when it names nothing.
    fn write(&self) -> Option<Value> {
        let mut m = Map::new();
        let id = self.item.as_ref().map(|r| r.id.trim()).unwrap_or_default();
        let name = self.name.trim();
        if !id.is_empty() {
            m.insert("id".into(), Value::from(id));
        } else if !name.is_empty() {
            m.insert("name".into(), Value::from(name));
        } else {
            return None;
        }
        if self.qty != 1 {
            m.insert("qty".into(), Value::from(self.qty));
        }
        let note = self.note.trim();
        if !note.is_empty() {
            m.insert("note".into(), Value::from(note));
        }
        m.extend(self.kept.clone());
        Some(Value::Object(m))
    }
}

/// One lettered option of an Equipment Package (`data.options[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageOption {
    /// The letter (`A`, `B`).
    pub label: String,
    /// The items it gives.
    #[serde(default)]
    pub items: Vec<PackageItem>,
    /// The coins it gives.
    #[serde(default)]
    pub coins: Coins,
    /// Unknown keys of the stored option.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub kept: Map<String, Value>,
}

impl PackageOption {
    /// The options of a stored package (`data.options`).
    #[must_use]
    pub fn read_all(v: Option<&Value>, lookup: &dyn Lookup) -> Vec<Self> {
        v.and_then(Value::as_array)
            .map(|opts| {
                opts.iter()
                    .map(|o| Self {
                        label: text(o, "id"),
                        items: o
                            .get("items")
                            .and_then(Value::as_array)
                            .map(|items| {
                                items.iter().map(|i| PackageItem::read(i, lookup)).collect()
                            })
                            .unwrap_or_default(),
                        coins: Coins::from_value(o.get("coins")),
                        kept: rest(o, &OPTION_KEYS),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The stored option.
    #[must_use]
    pub fn write(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), Value::from(self.label.trim()));
        m.insert(
            "items".into(),
            Value::Array(self.items.iter().filter_map(PackageItem::write).collect()),
        );
        m.insert("coins".into(), self.coins.to_value());
        m.extend(self.kept.clone());
        Value::Object(m)
    }
}
