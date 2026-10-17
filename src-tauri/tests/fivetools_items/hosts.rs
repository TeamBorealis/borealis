//! Eligible hosts of the item audit: an item whose attunement is restricted ("by a cleric",
//! "by a spellcaster", "by an elf of neutral good alignment") is barred on the level-5 human
//! fighter, so its attunement and value checks run on a level-5 character that qualifies: the
//! named class (a wizard for "a spellcaster"), the named race and an allowed alignment, read
//! from the engine's own requirement for the restriction.

use crate::support::{self, Base, LEVEL, SCORES, WEAK_SCORES};
use borealis_content::{Element, Ruleset};
use borealis_engine::attunement;
use borealis_engine::universe::Universe;
use borealis_engine::{commands, CharacterState, Engine};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// What an eligible host needs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostSpec {
    /// Class element id.
    pub class: String,
    /// Race / species element id (human when the restriction names none).
    pub race: Option<String>,
    /// Alignment element id.
    pub alignment: Option<String>,
}

/// A built host (`None` = could not be built).
type Built = HashMap<(Ruleset, HostSpec), Option<Rc<Base>>>;

/// Hosts built so far.
#[derive(Default)]
pub struct Hosts {
    built: RefCell<Built>,
}

/// Core (PHB / XPHB) elements win over other sources of the same name.
fn named(uni: &Universe, types: &[&str], name: &str) -> Option<String> {
    let mut found: Vec<&str> = types
        .iter()
        .flat_map(|t| uni.by_type(t))
        .filter(|e| !e.edition.is_foreign() && e.el.name.trim().eq_ignore_ascii_case(name))
        .map(|e| e.el.id.as_str())
        .collect();
    found.sort_by_key(|id| {
        (
            !(id.starts_with("ID_5ET_PHB_") || id.starts_with("ID_5ET_XPHB_")),
            id.len(),
        )
    });
    found.first().map(|s| (*s).to_string())
}

/// The `[...]` terms and `ID_ALIGNMENT_*` ids of a requirement, in order.
fn terms(req: &str) -> (Vec<String>, Vec<String>) {
    let mut tests = Vec::new();
    let mut rest = req;
    while let Some(i) = rest.find('[') {
        let Some(j) = rest[i..].find(']') else { break };
        tests.push(rest[i + 1..i + j].to_string());
        rest = &rest[i + j..];
    }
    let alignments = req
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| w.starts_with("ID_ALIGNMENT_"))
        .map(str::to_string)
        .collect();
    (tests, alignments)
}

/// The host an item restricted by `el`'s attunement setter needs, or `None` when the
/// restriction is not enforced or names nothing a host can be built from.
pub fn spec_for(engine: &Engine, rs: Ruleset, el: &Element) -> Option<HostSpec> {
    let uni = engine.universe(rs);
    let req = attunement::restriction(uni, el)?.requirement?;
    let (tests, alignments) = terms(&req);
    let mut class = None;
    let mut race = None;
    for t in &tests {
        let (key, value) = t.rsplit_once(':')?;
        let (key, value) = (key.trim(), value.trim());
        if key.eq_ignore_ascii_case("feature") && value.eq_ignore_ascii_case("spellcasting") {
            class = class.or_else(|| named(uni, &["Class"], "Wizard"));
        } else if key.eq_ignore_ascii_case("feature") {
            race = race.or_else(|| named(uni, &["Race", "Sub Race"], value));
        } else if value.parse::<i64>().is_ok() {
            class = class.or_else(|| named(uni, &["Class"], key));
        }
    }
    Some(HostSpec {
        class: class.unwrap_or_else(|| support::ids::fighter(rs).to_string()),
        race,
        alignment: alignments.into_iter().next(),
    })
}

fn build_state(
    engine: &Engine,
    rs: Ruleset,
    spec: &HostSpec,
    scores: [i32; 6],
) -> Option<CharacterState> {
    let mut state = support::with_class(engine, rs, &spec.class, scores);
    let race = spec.race.as_deref().unwrap_or(support::ids::human(rs));
    state = support::pick(engine, &state, race)?;
    if let Some(a) = &spec.alignment {
        state = support::pick(engine, &state, a)?;
    }
    for _ in 1..LEVEL {
        state = commands::level_up_main(engine, &state, None).ok()?.state;
    }
    (engine.derive(&state).level == LEVEL).then_some(state)
}

impl Hosts {
    /// The settled host of `spec` (built once).
    pub fn get(&self, engine: &Engine, rs: Ruleset, spec: &HostSpec) -> Option<Rc<Base>> {
        let key = (rs, spec.clone());
        if let Some(b) = self.built.borrow().get(&key) {
            return b.clone();
        }
        let base = build_state(engine, rs, spec, SCORES).and_then(|state| {
            let weak = build_state(engine, rs, spec, WEAK_SCORES)?;
            Some(Rc::new(Base::from_states(engine, rs, state, weak)))
        });
        self.built.borrow_mut().insert(key, base.clone());
        base
    }

    /// Number of hosts built (and not).
    pub fn counts(&self) -> (usize, usize) {
        let b = self.built.borrow();
        let ok = b.values().filter(|v| v.is_some()).count();
        (ok, b.len() - ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_of_a_requirement() {
        let (t, a) = terms(
            "([feature:Elf]||[feature:Half-Elf])&&(ID_ALIGNMENT_NEUTRAL_GOOD||!(ID_ALIGNMENT_X))",
        );
        assert_eq!(t, vec!["feature:Elf", "feature:Half-Elf"]);
        assert_eq!(a[0], "ID_ALIGNMENT_NEUTRAL_GOOD");
    }
}
