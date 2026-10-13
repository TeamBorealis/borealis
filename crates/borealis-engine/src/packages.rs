//! Starting-equipment packages (`equipment_package`, ENGINE-NEEDS): the choices offered by the
//! character's class and background, and the command that turns a choice into inventory.

use crate::graph::Graph;
use crate::rule_view::live_rules;
use crate::state::{CharacterState, Coins};
use crate::universe::Universe;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// One item of a package option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageItem {
    /// Item element id (absent when the SRD names a generic item).
    pub id: Option<String>,
    /// Display name.
    pub name: String,
    /// Quantity.
    pub qty: u32,
    /// Remark (`prayers`, `10 sheets`).
    pub note: Option<String>,
}

/// One lettered option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageOption {
    /// Label (`A`, `B`, `C`).
    pub id: String,
    /// Items it gives.
    pub items: Vec<PackageItem>,
    /// Coins it gives.
    pub coins: Coins,
}

/// A package the character may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentPackageSnapshot {
    /// Package element id.
    pub package: String,
    /// Display name.
    pub name: String,
    /// Class or background that offers it.
    pub owner: String,
    /// Options.
    pub options: Vec<PackageOption>,
    /// The option already taken.
    pub chosen: Option<String>,
}

fn coins_of(v: Option<&Value>) -> Coins {
    let n = |k: &str| {
        v.and_then(|c| c.get(k))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Coins {
        cp: n("cp"),
        sp: n("sp"),
        ep: n("ep"),
        gp: n("gp"),
        pp: n("pp"),
    }
}

fn options_of(
    uni: &Universe,
    data: &std::collections::BTreeMap<String, Value>,
) -> Vec<PackageOption> {
    let Some(opts) = data.get("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    opts.iter()
        .filter_map(|o| {
            let items = o
                .get("items")?
                .as_array()?
                .iter()
                .map(|it| {
                    let id = it.get("id").and_then(Value::as_str).map(str::to_string);
                    let name = id
                        .as_deref()
                        .and_then(|i| uni.get(i))
                        .map(|u| u.el.name.clone())
                        .or_else(|| it.get("name").and_then(Value::as_str).map(str::to_string))
                        .unwrap_or_default();
                    PackageItem {
                        id,
                        name,
                        qty: it
                            .get("qty")
                            .and_then(Value::as_u64)
                            .and_then(|q| u32::try_from(q).ok())
                            .unwrap_or(1),
                        note: it.get("note").and_then(Value::as_str).map(str::to_string),
                    }
                })
                .collect();
            Some(PackageOption {
                id: o.get("id")?.as_str()?.to_string(),
                items,
                coins: coins_of(o.get("coins")),
            })
        })
        .collect()
}

/// The packages offered by the live class and background.
#[must_use]
pub fn packages(
    graph: &Graph<'_>,
    uni: &Universe,
    chosen: &BTreeMap<String, String>,
) -> Vec<EquipmentPackageSnapshot> {
    let mut out: Vec<EquipmentPackageSnapshot> = Vec::new();
    for l in live_rules(graph, &["equipment_package"]) {
        let Some(pid) = l.str("ref") else { continue };
        // `!ID_2024_MULTICLASS_X`: only the first class gives starting equipment
        if let Some(blocker) = l
            .str("requirements")
            .and_then(|r| r.trim().strip_prefix('!'))
        {
            if graph.has(blocker) {
                continue;
            }
        }
        if out.iter().any(|p| p.package == pid) {
            continue;
        }
        let Some(pkg) = uni.get(pid) else { continue };
        out.push(EquipmentPackageSnapshot {
            package: pid.to_string(),
            name: l.str("name").unwrap_or(&pkg.el.name).to_string(),
            owner: l.el.name.clone(),
            options: options_of(uni, &pkg.el.data),
            chosen: chosen.get(pid).cloned(),
        });
    }
    out
}

/// Add the coins of an option to the purse.
#[must_use]
pub fn add_coins(state: &CharacterState, add: Coins) -> Coins {
    let c = state.inventory.coins;
    Coins {
        cp: c.cp + add.cp,
        sp: c.sp + add.sp,
        ep: c.ep + add.ep,
        gp: c.gp + add.gp,
        pp: c.pp + add.pp,
    }
}
