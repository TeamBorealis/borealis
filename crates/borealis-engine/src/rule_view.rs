//! Shared helpers for reading live custom rules (2024).
use crate::eval::requirement_holds;
use crate::graph::{Graph, NodeId};
use crate::universe::Cond;
use borealis_content::expr::RequirementsExpr;
use borealis_content::rules::CustomRule;
use borealis_content::{Element, Rule};
use serde_json::Value;

/// A custom rule of a live element whose `level` gate is met.
pub struct Live<'u> {
    /// The node that carries the rule.
    pub node: NodeId,
    /// The element.
    pub el: &'u Element,
    /// The rule.
    pub c: &'u CustomRule,
}

impl Live<'_> {
    /// A string field.
    #[must_use]
    pub fn str(&self, k: &str) -> Option<&str> {
        self.c.fields.get(k).and_then(Value::as_str)
    }

    /// A list of strings (a single string counts as a list of one).
    #[must_use]
    pub fn list(&self, k: &str) -> Vec<String> {
        match self.c.fields.get(k) {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Does a rule with this optional `level` apply to the node (class level inside class content,
/// character level elsewhere)?
#[must_use]
pub fn level_met(graph: &Graph<'_>, node: NodeId, level: Option<u64>) -> bool {
    level.map_or(true, |l| {
        u64::from(graph.mgr_level(graph.nodes[node].mgr)) >= l
    })
}

/// Is a custom rule of `node` live: its `level` gate met and its `requirements` expression (if
/// any) true? Every reader of gated kinds (`KNOWN_KINDS` with `gated`) goes through this.
#[must_use]
pub fn rule_live(graph: &Graph<'_>, node: NodeId, c: &CustomRule) -> bool {
    let level = c.fields.get("level").and_then(Value::as_u64);
    level_met(graph, node, level) && requirements_met(graph, c)
}

/// [`rule_live`] for a rule of a carried item that is no graph node (gear without an equip
/// slot): its `level` gate reads the character level.
#[must_use]
pub fn carried_rule_live(graph: &Graph<'_>, c: &CustomRule) -> bool {
    let level = c.fields.get("level").and_then(Value::as_u64);
    level.map_or(true, |l| u64::from(graph.total_level()) >= l) && requirements_met(graph, c)
}

fn requirements_met(graph: &Graph<'_>, c: &CustomRule) -> bool {
    match c.fields.get("requirements").and_then(Value::as_str) {
        Some(t) if !t.trim().is_empty() => {
            let cond = Cond::from_parse(RequirementsExpr::parse(t), |e| e);
            requirement_holds(&cond, &graph.env())
        }
        _ => true,
    }
}

/// Live custom rules of the given kinds, in registration order.
#[must_use]
pub fn live_rules<'u>(graph: &Graph<'u>, kinds: &[&str]) -> Vec<Live<'u>> {
    let mut out = Vec::new();
    for id in graph.preorder() {
        let e: &'u crate::universe::UElem = graph.nodes[id].e;
        for r in &e.el.rules {
            let Rule::Custom(c) = r else { continue };
            if !kinds.contains(&c.kind.as_str()) {
                continue;
            }
            if rule_live(graph, id, c) {
                out.push(Live {
                    node: id,
                    el: &e.el,
                    c,
                });
            }
        }
    }
    out
}

/// Capitalise the first letter.
#[must_use]
pub fn cap(s: &str) -> String {
    let mut ch = s.chars();
    ch.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + ch.as_str()
    })
}

/// Evaluate a small value expression: a number, `proficiency`, `proficiency:half`,
/// `<ability>:modifier`.
#[must_use]
pub fn eval_value(expr: &str, mods: &[i32; 6], prof: i32) -> Option<i32> {
    let e = expr.trim().to_lowercase();
    if let Ok(n) = e.parse::<i32>() {
        return Some(n);
    }
    match e.as_str() {
        "proficiency" => return Some(prof),
        "proficiency:half" => return Some(prof / 2),
        _ => {}
    }
    let ab = e.strip_suffix(":modifier")?;
    crate::ids::ability_index(ab).map(|i| mods[i])
}
