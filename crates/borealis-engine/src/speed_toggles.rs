//! Speed multipliers of switched-on toggles (Boots of Speed: walking speed doubled; Eagle
//! Whistle: a flying speed of twice the walking speed).
//!
//! A `toggle` effect `{"kind": "speed_multiply", "speed": "walk", "factor": 2}` multiplies that
//! speed while the toggle is on. With `"of": "walk"` the target speed becomes at least `factor`
//! times the `of` speed instead.

use crate::graph::Graph;
use crate::play_rules::worn_blocker;
use crate::rule_view::live_rules;
use crate::snapshot::Speeds;
use serde_json::Value;

/// Effect kind inside a `toggle` rule.
pub const SPEED_MULTIPLY: &str = "speed_multiply";

/// One active multiplier.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Multiplier {
    speed: String,
    factor: i32,
    of: Option<String>,
}

fn slot<'s>(s: &'s mut Speeds, mode: &str) -> Option<&'s mut i32> {
    Some(match mode {
        "walk" => &mut s.walk,
        "fly" => &mut s.fly,
        "climb" => &mut s.climb,
        "swim" => &mut s.swim,
        "burrow" => &mut s.burrow,
        _ => return None,
    })
}

fn read(s: &Speeds, mode: &str) -> Option<i32> {
    let mut copy = s.clone();
    slot(&mut copy, mode).map(|v| *v)
}

fn multipliers(graph: &Graph<'_>) -> Vec<Multiplier> {
    let mut out = Vec::new();
    for l in live_rules(graph, &["toggle"]) {
        let Some(tid) = l.str("id") else { continue };
        if !graph.runtime.toggles.contains(tid)
            || worn_blocker(graph, l.str("requires_not_wearing")).is_some()
        {
            continue;
        }
        for eff in
            l.c.fields
                .get("effects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        {
            if eff.get("kind").and_then(Value::as_str) != Some(SPEED_MULTIPLY) {
                continue;
            }
            let text = |k: &str| {
                eff.get(k)
                    .and_then(Value::as_str)
                    .map(|v| v.trim().to_lowercase())
            };
            let factor = eff
                .get("factor")
                .and_then(Value::as_i64)
                .and_then(|f| i32::try_from(f).ok());
            if let (Some(speed), Some(factor)) = (text("speed"), factor) {
                out.push(Multiplier {
                    speed,
                    factor,
                    of: text("of"),
                });
            }
        }
    }
    out
}

/// `base` with the multipliers of the switched-on toggles applied (plain multipliers first,
/// then the `of` ones, which read the multiplied speeds).
#[must_use]
pub fn apply(graph: &Graph<'_>, base: Speeds) -> Speeds {
    let mut s = base;
    let all = multipliers(graph);
    for m in all.iter().filter(|m| m.of.is_none()) {
        if let Some(v) = slot(&mut s, &m.speed) {
            *v = v.saturating_mul(m.factor);
        }
    }
    for m in all.iter().filter(|m| m.of.is_some()) {
        let Some(from) = m.of.as_deref().and_then(|o| read(&s, o)) else {
            continue;
        };
        if let Some(v) = slot(&mut s, &m.speed) {
            *v = (*v).max(from.saturating_mul(m.factor));
        }
    }
    s
}
