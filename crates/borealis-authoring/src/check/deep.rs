//! The engine checks of a draft (spec 8.1 steps 6-7), run when the user pauses (1.5 s) or opens PROBLEMS: the draft
//! is loaded on top of the loaded packs (its pack without the draft's saved version), then
//!
//! * a test character takes each species, race, feat and item of it (`load_into_engine`'s paths);
//! * for a class, or a subclass with the class that offers it, a test character levels 1 to 20 (the sweep);
//! * choices whose filter matches nothing, and spell lists no spell is on, are found with the engine's view of the
//!   content (implicit tags included); a swept draft gets them from the sweep instead;
//! * balance hints (info) compare spells, feats, magic items, species and creatures with the SRD baselines.
//!
//! None of these block Save: the pack loads either way. Each one is a plain sentence at its form path.

use super::balance::{balance_report, Baselines};
use super::engine::{take_element, CharacterCheck};
use super::live::{pairs, NameBook};
use super::messages::{balance_message, refusal_message, sweep_code, sweep_message, Finding};
use super::problem::{dedupe, Problem, ProblemSeverity};
use super::sweep::{sweep_engine, SweepProblem, SweepTarget};
use crate::form::{
    form_path_for_issue, form_path_for_location, form_to_elements, FormContext, FormState,
};
use borealis_content::expr::SupportsExpr;
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::validate::validate_elements;
use borealis_content::{ContentStore, ContentStoreBuilder, Element, Rule, Ruleset, Severity};
use borealis_engine::eval::{supports_match, NoDynamics};
use borealis_engine::Engine;
use std::collections::HashSet;

/// What the engine checks look at.
pub struct DeepInput<'a> {
    /// Every loaded pack, with the draft's pack holding everything but the draft's saved elements.
    pub base: &'a ContentStoreBuilder,
    /// The edition of the test character.
    pub ruleset: Ruleset,
    /// The name the draft's content is loaded under.
    pub pack_name: &'a str,
    /// The draft's form, with the ids its elements have or would get now.
    pub form: &'a FormState,
    /// The form's elements as a save writes them (a copy's grants re-pointed to its copied features); `None` writes
    /// the form.
    pub written: Option<&'a [Element]>,
    /// The form context over the loaded content.
    pub cx: &'a FormContext<'a>,
}

/// Types a test character takes (the others are only derived).
const TAKEN: [&str; 7] = [
    "Race",
    "Species",
    "Feat",
    "Item",
    "Weapon",
    "Armor",
    "Magic Item",
];

/// Types with a balance model (`balance_report`).
const BALANCED: [&str; 6] = ["Spell", "Feat", "Magic Item", "Race", "Species", "Monster"];

/// The engine checks' problems of a draft. Empty when the draft does not build or does not load (the live pipeline
/// reports why).
#[must_use]
pub fn deep_problems(input: &DeepInput<'_>) -> Vec<Problem> {
    let elements = match input.written {
        Some(written) => written.to_vec(),
        None => match form_to_elements(input.form, input.cx) {
            Ok(elements) => elements,
            Err(_) => return Vec::new(),
        },
    };
    let ids: HashSet<String> = elements.iter().map(|e| e.id.clone()).collect();
    let store = input
        .base
        .clone()
        .add_file(
            input.pack_name,
            PackOrigin::Custom,
            ContentFile::new(elements.clone()),
        )
        .build();
    let own: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if validate_elements(&store, &own)
        .iter()
        .any(|i| i.severity == Severity::Error)
    {
        return Vec::new();
    }
    let engine = Engine::new(&store);
    let names = NameBook::new(input.form, &elements, &store);
    let pairs = pairs(input.form, &elements);
    let targets = sweep_targets(&elements, &store, input.ruleset);
    let mut out = Vec::new();
    for (prefix, form, e) in &pairs {
        let at = prefix.trim_end_matches('.').to_string();
        if TAKEN.contains(&e.ty.as_str()) {
            if let CharacterCheck::Refused(why) = take_element(&engine, input.ruleset, &e.id, &e.ty)
            {
                out.push(Problem::new(
                    ProblemSeverity::Warning,
                    "engine.refused",
                    refusal_message(&e.ty, &why, &names),
                    at.clone(),
                    Some(&e.id),
                ));
            }
        }
        let lint = |location: &str, f: Finding| {
            let path = form_path_for_location(form, e, location, input.cx)
                .map_or_else(|| at.clone(), |p| format!("{prefix}{p}"));
            Problem::new(f.severity(), f.code(), f.message(), path, Some(&e.id))
        };
        if targets.is_empty() {
            for i in empty_selects(e, &engine, input.ruleset) {
                out.push(lint(&format!("rules[{i}]"), Finding::SelectOffersNothing));
            }
        }
        if spell_list_is_empty(e, &engine, input.ruleset) {
            out.push(lint("spellcasting", Finding::SpellListEmpty));
        }
    }
    out.extend(sweep_problems(
        input, &engine, &elements, &ids, &targets, &names,
    ));
    out.extend(balance_problems(input, &store, &elements));
    dedupe(out)
}

fn sweep_problems(
    input: &DeepInput<'_>,
    engine: &Engine,
    elements: &[Element],
    ids: &HashSet<String>,
    targets: &[SweepTarget],
    names: &NameBook<'_>,
) -> Vec<Problem> {
    let root = elements.first();
    let mut out = Vec::new();
    if root.is_some_and(|r| r.ty == "Archetype") && targets.is_empty() {
        let f = super::sweep::SweepFinding {
            level: 0,
            element_id: root.map(|r| r.id.clone()),
            problem: SweepProblem::SubclassNeverOffered,
        };
        out.push(Problem::new(
            ProblemSeverity::Warning,
            sweep_code(&f.problem),
            sweep_message(&f, "Archetype", names),
            "fields.tags",
            root.map(|r| r.id.as_str()),
        ));
    }
    for f in sweep_engine(engine, input.ruleset, ids, targets) {
        let id = f.element_id.clone().or_else(|| root.map(|r| r.id.clone()));
        let el = id
            .as_deref()
            .and_then(|id| elements.iter().find(|e| e.id == id));
        let location = match (&f.problem, el) {
            (SweepProblem::EmptySelect { name, .. }, Some(e)) => select_rule(e, name),
            _ => None,
        };
        let path = id
            .as_deref()
            .and_then(|id| {
                form_path_for_issue(input.form, elements, id, location.as_deref(), input.cx)
                    .or_else(|| form_path_for_issue(input.form, elements, id, None, input.cx))
            })
            .unwrap_or_default();
        out.push(Problem::new(
            ProblemSeverity::Warning,
            sweep_code(&f.problem),
            sweep_message(&f, el.map_or("Class", |e| e.ty.as_str()), names),
            path,
            id.as_deref(),
        ));
    }
    out
}

/// `rules[i]` of the select called `name`.
fn select_rule(e: &Element, name: &str) -> Option<String> {
    e.rules
        .iter()
        .position(|r| matches!(r, Rule::Select(s) if s.name == name))
        .map(|i| format!("rules[{i}]"))
}

fn balance_problems(
    input: &DeepInput<'_>,
    store: &ContentStore,
    elements: &[Element],
) -> Vec<Problem> {
    let graded: Vec<&Element> = elements
        .iter()
        .filter(|e| BALANCED.contains(&e.ty.as_str()))
        .collect();
    if graded.is_empty() {
        return Vec::new();
    }
    let own: std::collections::HashSet<&str> = elements.iter().map(|e| e.id.as_str()).collect();
    let others = store
        .iter()
        .map(|s| &s.element)
        .filter(|e| !own.contains(e.id.as_str()));
    let baselines = Baselines::from_elements(others, input.ruleset);
    let mut out = Vec::new();
    for e in graded {
        let path =
            form_path_for_issue(input.form, elements, &e.id, None, input.cx).unwrap_or_default();
        for flag in balance_report(&baselines, e).flags {
            if let Some(message) = balance_message(&flag) {
                out.push(Problem::new(
                    ProblemSeverity::Info,
                    format!("balance.{}", flag.metric),
                    message,
                    path.clone(),
                    Some(&e.id),
                ));
            }
        }
    }
    out
}

/// A tag filter the engine can judge alone: parsed, not a list of ids (those are references), no dynamic parts.
fn static_filter(text: &str) -> Option<SupportsExpr> {
    SupportsExpr::parse(text)
        .ok()
        .flatten()
        .filter(|e| !e.is_id_range_mode() && !e.is_dynamic())
}

/// Does anything of type `ty` the engine knows match `expr`?
fn matches_any(engine: &Engine, ruleset: Ruleset, ty: &str, expr: &SupportsExpr) -> bool {
    engine
        .universe(ruleset)
        .by_type(ty)
        .any(|u| supports_match(expr, u, &NoDynamics))
}

/// The rules of `e` that are choices from a tag filter nothing matches.
fn empty_selects(e: &Element, engine: &Engine, ruleset: Ruleset) -> Vec<usize> {
    e.rules
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match r {
            Rule::Select(s) if s.ty != "List" => {
                let expr = static_filter(s.supports.as_deref()?)?;
                (!matches_any(engine, ruleset, &s.ty, &expr)).then_some(i)
            }
            _ => None,
        })
        .collect()
}

fn spell_list_is_empty(e: &Element, engine: &Engine, ruleset: Ruleset) -> bool {
    e.spellcasting
        .as_ref()
        .filter(|s| s.extend != Some(true))
        .and_then(|s| s.list.as_ref())
        .and_then(|l| static_filter(&l.supports))
        .is_some_and(|expr| !matches_any(engine, ruleset, "Spell", &expr))
}

/// What to sweep for a draft: its class, or its subclass with the loaded class that offers it (by the tags of that
/// class's subclass choice). Features are the draft's children granted at a level without a condition.
#[must_use]
pub fn sweep_targets(
    elements: &[Element],
    store: &ContentStore,
    ruleset: Ruleset,
) -> Vec<SweepTarget> {
    let Some(root) = elements.first() else {
        return Vec::new();
    };
    let own: HashSet<&str> = elements.iter().map(|e| e.id.as_str()).collect();
    let features: Vec<(String, u32)> = root
        .rules
        .iter()
        .filter_map(|r| match r {
            Rule::Grant(g) if own.contains(g.id.as_str()) && g.requirements.is_none() => {
                Some((g.id.clone(), g.level?))
            }
            _ => None,
        })
        .collect();
    let (class_id, subclass_id) = match root.ty.as_str() {
        "Class" => (root.id.clone(), None),
        "Archetype" => match parent_class(root, store, ruleset) {
            Some(parent) => (parent, Some(root.id.clone())),
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let casts = root.ty == "Class"
        && root
            .spellcasting
            .as_ref()
            .is_some_and(|s| s.extend != Some(true) && s.list.is_some());
    vec![SweepTarget {
        class_id,
        subclass_id,
        features,
        root_id: root.id.clone(),
        casts,
    }]
}

/// The loaded class whose subclass choice offers `sub` (one of the choice's tags is on the subclass), preferring the
/// draft's edition. The choice is on the class or on a feature the class grants (the SRD's "Fighter Subclass").
fn parent_class(sub: &Element, store: &ContentStore, ruleset: Ruleset) -> Option<String> {
    let offers = |c: &Element| {
        c.rules.iter().any(|r| match r {
            Rule::Select(s) if s.ty == "Archetype" => s
                .supports
                .as_deref()
                .and_then(|t| SupportsExpr::parse(t).ok().flatten())
                .is_some_and(|e| e.tags().iter().any(|t| sub.supports.iter().any(|s| s == t))),
            _ => false,
        })
    };
    let classes: Vec<&Element> = store.by_type("Class").map(|s| &s.element).collect();
    let mut parents: Vec<&Element> = Vec::new();
    for owner in store.iter().map(|s| &s.element).filter(|e| offers(e)) {
        if owner.ty == "Class" {
            parents.push(owner);
        }
        parents.extend(classes.iter().copied().filter(|c| {
            c.rules
                .iter()
                .any(|r| matches!(r, Rule::Grant(g) if g.id == owner.id))
        }));
    }
    parents
        .iter()
        .find(|c| c.ruleset == Some(ruleset))
        .or_else(|| parents.first())
        .map(|c| c.id.clone())
}

#[cfg(test)]
#[path = "deep_tests.rs"]
mod tests;
