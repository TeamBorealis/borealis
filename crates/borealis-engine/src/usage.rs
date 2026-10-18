//! Limited-use pools read from an element's sheet `usage` text (Aurora XML `<sheet usage="...">`
//! and `<description level="N" usage="...">`).
//!
//! Aurora itself prints the usage text only (CharacterSheetGenerator.cs:925-975): the effective
//! usage is the sheet default, replaced by the usage of every description whose level is at most
//! the element's progression level (its class level, else the character level), in level order;
//! placeholders are resolved by `StatisticsCalculator.ReplaceInline` (engine.md 5.5). Borealis
//! reads that same resolved text as a count and a recharge so Rage, Ki, Channel Divinity and the
//! other pools of Aurora-format content can be tracked. Only these shapes are read:
//!
//! - `<n>/<period>` and `<n>d<size>/<period>` (`3/Long Rest`, `{{proficiency}}/Long Rest`,
//!   `{{healing light:dice count}}d6/Long Rest`, `{{lay on hands:hp pool}} HP/Long Rest`), with
//!   period Short Rest, Long Rest, Short or Long Rest, Long or Short Rest or Day;
//! - `<placeholder> <unit>` (`{{sorcerer:sorcery points}} Sorcery Points`): a pool named by its
//!   unit, or by the element for a hit-point pool (`hp pool`). A literal number before a unit
//!   (`1 ki point`, `2 Sorcery Points`) is a cost, not a pool.
//!
//! Segments split by an em dash are read in order (`1/Turn — 3/Long Rest`). Per-turn, per-round
//! and per-rage limits, day counts (`1/7 days`) and words (`Unlimited`, `Channel Divinity`) are no
//! pools. A count of 0 or less, or an unresolved placeholder, gives no pool.

use crate::graph::{Graph, NodeId};
use crate::snapshot::ResourceSnapshot;
use crate::stats::StatSet;
use borealis_content::expr::{Template, TemplatePart};
use borealis_content::{Element, Rule};

/// Element types whose usage is read (the feature lists of Aurora's sheet).
const FEATURE_TYPES: &[&str] = &[
    "Class Feature",
    "Archetype Feature",
    "Racial Trait",
    "Background Feature",
    "Feat",
];

/// Words of a hit-point pool unit: such a pool is named after its element.
const HP_UNITS: &[&str] = &["hp pool", "hp", "hit points", "hit point pool"];

/// How a pool recharges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recharge {
    /// Short or Long Rest.
    ShortRest,
    /// Long Rest (also `Day`).
    LongRest,
    /// No recharge stated (a Long Rest restores it, as for every resource without one).
    Unstated,
}

/// A pool read from usage text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsagePool {
    /// Maximum uses (or points).
    pub count: i32,
    /// Recharge.
    pub recharge: Recharge,
    /// Pool name from the unit (`Sorcery Points`); `None` = the element's name.
    pub unit_name: Option<String>,
}

fn period(text: &str) -> Option<Recharge> {
    let p = text
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_ascii_lowercase();
    match p.as_str() {
        "short rest" | "short or long rest" | "long or short rest" => Some(Recharge::ShortRest),
        "long rest" | "day" => Some(Recharge::LongRest),
        _ => None,
    }
}

/// A leading count `<n>` or `<n>d<size>` (dice pools count dice), and the rest.
fn leading_count(text: &str) -> Option<(i32, &str)> {
    let t = text.trim_start();
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let n: i32 = t[..digits].parse().ok()?;
    let mut rest = &t[digits..];
    if let Some(r) = rest.strip_prefix(['d', 'D']) {
        let size = r.bytes().take_while(u8::is_ascii_digit).count();
        if size > 0 {
            rest = &r[size..];
        }
    }
    Some((n, rest))
}

/// One usage segment, already resolved. `dynamic` = its count came from a placeholder.
fn parse_segment(seg: &str, dynamic: bool) -> Option<UsagePool> {
    let (count, rest) = leading_count(seg)?;
    if count <= 0 {
        return None;
    }
    let rest = rest.trim();
    if let Some((unit, per)) = rest.split_once('/') {
        let unit = unit.trim().to_ascii_lowercase();
        if !unit.is_empty() && !HP_UNITS.contains(&unit.as_str()) {
            return None;
        }
        return Some(UsagePool {
            count,
            recharge: period(per)?,
            unit_name: None,
        });
    }
    if !dynamic || rest.is_empty() || !rest.starts_with(|c: char| c.is_alphabetic()) {
        return None;
    }
    let lower = rest.to_ascii_lowercase();
    Some(UsagePool {
        count,
        recharge: Recharge::Unstated,
        unit_name: (!HP_UNITS.contains(&lower.as_str())).then(|| rest.to_string()),
    })
}

/// Read a usage text whose placeholders are already resolved. `dynamic[i]` tells whether the
/// count of em-dash segment `i` came from a placeholder.
#[must_use]
pub fn parse_usage(resolved: &str, dynamic: &[bool]) -> Option<UsagePool> {
    resolved
        .split(['—', '–'])
        .enumerate()
        .find_map(|(i, seg)| parse_segment(seg, dynamic.get(i).copied().unwrap_or(false)))
}

/// Resolve placeholders like `ReplaceInline`; `None` when one is unknown. Also tells, per
/// em-dash segment, whether the segment starts with a placeholder.
fn resolve(usage: &str, stats: &StatSet) -> Option<(String, Vec<bool>)> {
    let mut out = String::new();
    let mut dynamic = vec![false];
    let mut at_start = true;
    for part in Template::parse(usage).parts() {
        match part {
            TemplatePart::Text(t) => {
                for ch in t.chars() {
                    if ch == '—' || ch == '–' {
                        dynamic.push(false);
                        at_start = true;
                    } else if !ch.is_whitespace() {
                        at_start = false;
                    }
                    out.push(ch);
                }
            }
            TemplatePart::Placeholder(ph) => {
                let key = ph.stat_key();
                if !stats.exists(&key) {
                    return None;
                }
                if at_start {
                    if let Some(d) = dynamic.last_mut() {
                        *d = true;
                    }
                }
                at_start = false;
                out.push_str(&stats.sum(&key).to_string());
            }
        }
    }
    Some((out, dynamic))
}

/// The usage text in effect at `level` (Aurora's order: sheet default, then each description
/// with a usage attribute and a level at most `level`, ascending; ElementParser.cs:176-200).
#[must_use]
pub fn effective_usage(el: &Element, level: u32) -> Option<&str> {
    let sheet = el.sheet.as_ref()?;
    let mut descs: Vec<_> = sheet.descriptions.iter().collect();
    descs.sort_by_key(|d| d.effective_level());
    let mut usage = sheet.usage.as_deref();
    for d in descs {
        if d.effective_level() <= level {
            // a description's `usage` attribute replaces the text even when empty (HasUsage)
            if let Some(u) = d.usage.as_deref() {
                usage = Some(u);
            }
        }
    }
    usage.filter(|u| !u.trim().is_empty())
}

fn has_resource_rule(el: &Element) -> bool {
    el.rules
        .iter()
        .any(|r| matches!(r, Rule::Custom(c) if c.kind == "resource"))
}

/// The pool of one live node, as a resource snapshot.
fn node_pool(graph: &Graph<'_>, stats: &StatSet, id: NodeId) -> Option<ResourceSnapshot> {
    let node = &graph.nodes[id];
    let el = &node.e.el;
    if !FEATURE_TYPES.contains(&el.ty.as_str()) || has_resource_rule(el) {
        return None;
    }
    if !el.sheet.as_ref().map_or(true, |s| s.is_displayed()) {
        return None;
    }
    let usage = effective_usage(el, graph.mgr_level(node.mgr))?;
    let (text, dynamic) = resolve(usage, stats)?;
    let pool = parse_usage(&text, &dynamic)?;
    let recharge: Vec<String> = match pool.recharge {
        Recharge::ShortRest => vec!["short_rest".into(), "long_rest".into()],
        Recharge::LongRest => vec!["long_rest".into()],
        Recharge::Unstated => Vec::new(),
    };
    let rid = format!("usage:{}", el.id);
    let used = i32::try_from(graph.runtime.used_of(&rid)).unwrap_or(i32::MAX);
    Some(ResourceSnapshot {
        name: pool.unit_name.unwrap_or_else(|| el.name.clone()),
        max: pool.count,
        current: (pool.count - used).max(0),
        source: el.name.clone(),
        short_rest_regain: (pool.recharge == Recharge::ShortRest).then_some(pool.count),
        long_rest_regain_all: true,
        recharge,
        other_regain: Vec::new(),
        rolled: None,
        id: rid,
    })
}

/// Pools from the sheet usage of live feature nodes that declare no `resource` rule, skipping
/// any whose name an explicit resource in `explicit` already has.
#[must_use]
pub fn usage_resources(
    graph: &Graph<'_>,
    stats: &StatSet,
    explicit: &[ResourceSnapshot],
) -> Vec<ResourceSnapshot> {
    let mut out: Vec<ResourceSnapshot> = Vec::new();
    for id in graph.preorder() {
        let Some(r) = node_pool(graph, stats, id) else {
            continue;
        };
        let taken = |x: &ResourceSnapshot| x.id == r.id || x.name.eq_ignore_ascii_case(&r.name);
        if explicit.iter().any(taken) || out.iter().any(taken) {
            continue;
        }
        out.push(r);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(text: &str, dynamic: &[bool]) -> Option<(i32, Recharge, Option<String>)> {
        parse_usage(text, dynamic).map(|p| (p.count, p.recharge, p.unit_name))
    }

    #[test]
    fn counts_with_a_period_are_pools() {
        assert_eq!(
            pool("3/Long Rest", &[true]),
            Some((3, Recharge::LongRest, None))
        );
        assert_eq!(
            pool("1/Short Rest", &[]),
            Some((1, Recharge::ShortRest, None))
        );
        assert_eq!(
            pool("2/Short or Long Rest", &[]),
            Some((2, Recharge::ShortRest, None))
        );
        assert_eq!(
            pool("1/Long or Short Rest", &[]),
            Some((1, Recharge::ShortRest, None))
        );
        assert_eq!(pool("1/Day", &[]), Some((1, Recharge::LongRest, None)));
        assert_eq!(
            pool("4/Long Rest.", &[true]),
            Some((4, Recharge::LongRest, None))
        );
        assert_eq!(
            pool("3d6/Long Rest", &[true]),
            Some((3, Recharge::LongRest, None))
        );
        assert_eq!(
            pool("50 HP/Long Rest", &[true]),
            Some((50, Recharge::LongRest, None))
        );
    }

    #[test]
    fn limits_and_words_are_no_pools() {
        for t in [
            "1/Turn",
            "1/Round",
            "1/Rage",
            "1/7 days",
            "Unlimited",
            "Channel Divinity",
            "Superiority Die",
            "0/Long Rest",
            "1 ki point",
            "2 Sorcery Points",
            "5 ft/Long Rest",
        ] {
            assert_eq!(pool(t, &[false, false]), None, "{t}");
        }
    }

    #[test]
    fn placeholder_units_are_named_pools() {
        assert_eq!(
            pool("17 Sorcery Points", &[true]),
            Some((17, Recharge::Unstated, Some("Sorcery Points".into())))
        );
        assert_eq!(
            pool("100 hp pool", &[true]),
            Some((100, Recharge::Unstated, None))
        );
    }

    #[test]
    fn em_dash_segments_are_read_in_order() {
        assert_eq!(
            pool("1/Turn — 2/Long Rest", &[false, true]),
            Some((2, Recharge::LongRest, None))
        );
        assert_eq!(
            pool("5 Focus Points — 1/Long Rest", &[false, false]),
            Some((1, Recharge::LongRest, None))
        );
    }
}
