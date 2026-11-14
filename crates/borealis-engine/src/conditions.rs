//! Conditions a character can have switched on (HC1, item HtCaM; spec 4.10 "effects as rule blocks for toggled
//! conditions").
//!
//! The conditions a character can switch on are the `Condition` elements the loaded content defines (the SRD's,
//! imported ones, homebrew ones), not the engine's helper conditions (`Internal`/`Core` sources, `_INTERNAL_` ids:
//! Resistance (Fire), which features grant), not one the creator marks unlisted (`data.listed` false) and not
//! Exhaustion, which has its own levels ([`crate::play_rules`]).
//! A switched-on condition is a root of the character's graph ([`crate::graph::Graph::build`]): its rules apply like
//! those of any element the character has, and requirements naming it hold. The switched-on ids are saved in
//! [`crate::state::PlayState::conditions`]; none is on unless the user switches it on, so a character that never
//! switches one on computes as before.

use crate::graph::Graph;
use crate::snapshot::ConditionSnapshot;
use crate::universe::{UElem, Universe};
use borealis_content::{is_unrestrictable_source, Element, Rule};

/// The element type of a condition.
pub const CONDITION_TYPE: &str = "Condition";
/// Exhaustion is counted in levels, not switched (`PlayState::exhaustion`).
const EXHAUSTION_NAME: &str = "Exhaustion";
const EXHAUSTION_RULE: &str = "exhaustion";
/// The id marker of the engine's helper elements (`ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE`).
const INTERNAL_MARKER: &str = "_INTERNAL_";
/// The creator's mark of a condition not listed with the conditions ("Listed as a condition" off): `data.listed`
/// false. A tag can't tell it, since the SRD 5.1 conditions carry none (IpY2E).
pub const LISTED_KEY: &str = "listed";

/// Is this condition one of the engine's helpers (`Internal`/`Core` source, an `_INTERNAL_` id: Resistance (Fire),
/// which features grant), never listed with the conditions?
#[must_use]
pub fn is_helper(el: &Element) -> bool {
    is_unrestrictable_source(&el.source) || el.id.contains(INTERNAL_MARKER)
}

/// Does the creator mark this condition as not listed with the conditions ([`LISTED_KEY`])?
#[must_use]
pub fn is_marked_unlisted(el: &Element) -> bool {
    el.data.get(LISTED_KEY).and_then(serde_json::Value::as_bool) == Some(false)
}

/// Can a character switch this element on as a condition?
#[must_use]
pub fn is_switchable(u: &UElem) -> bool {
    let el = &u.el;
    el.ty == CONDITION_TYPE
        && u.edition.is_accepted()
        && !u.superseded
        && !is_helper(el)
        && !is_marked_unlisted(el)
        && !el.name.trim().eq_ignore_ascii_case(EXHAUSTION_NAME)
        && !el
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Custom(c) if c.kind == EXHAUSTION_RULE))
}

/// The switchable conditions, by name: this edition's (and edition-neutral) ones, then another edition's accepted ones
/// whose name is not taken yet.
#[must_use]
pub fn switchable(uni: &Universe) -> Vec<&UElem> {
    let all: Vec<&UElem> = uni
        .by_type(CONDITION_TYPE)
        .filter(|u| is_switchable(u))
        .collect();
    let mut out: Vec<&UElem> = Vec::new();
    for u in all
        .iter()
        .filter(|u| !u.edition.is_foreign())
        .chain(all.iter().filter(|u| u.edition.is_foreign()))
    {
        let taken = out
            .iter()
            .any(|o| o.el.name.trim().eq_ignore_ascii_case(u.el.name.trim()));
        if !taken {
            out.push(u);
        }
    }
    out.sort_by_key(|u| u.el.name.trim().to_lowercase());
    out
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// One line per rule of a condition, as the Play tab lists a state's effects: "Speed -10", the granted element's
/// name, else the rule's kind.
fn effects(u: &UElem, uni: &Universe) -> Vec<String> {
    u.el.rules
        .iter()
        .map(|r| match r {
            Rule::Stat(s) => {
                let v = s.value.trim();
                let shown = v
                    .parse::<i32>()
                    .map_or_else(|_| v.to_string(), |n| format!("{n:+}"));
                format!("{} {shown}", capitalized(s.name.trim()))
            }
            Rule::Grant(g) => uni
                .get(&g.id)
                .map_or_else(|| g.id.clone(), |t| t.el.name.clone()),
            Rule::Select(s) => s.name.clone(),
            Rule::Custom(c) => capitalized(&c.kind.replace('_', " ")),
            Rule::Unknown(_) => String::new(),
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/// The snapshot's list of switchable conditions with the character's switched-on ones marked.
#[must_use]
pub fn snapshots(
    graph: &Graph<'_>,
    on: &std::collections::BTreeSet<String>,
) -> Vec<ConditionSnapshot> {
    switchable(graph.uni)
        .into_iter()
        .map(|u| ConditionSnapshot {
            id: u.el.id.clone(),
            name: u.el.name.trim().to_string(),
            on: on.contains(&u.el.id),
            description: u.el.description.clone(),
            effects: effects(u, graph.uni),
            source: u.el.source.clone(),
        })
        .collect()
}
