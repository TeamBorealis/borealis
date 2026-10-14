//! The engine sweep: a test character of the right edition takes the class (or the parent class
//! plus the subclass) and levels 1 to 20, deriving at every level.

use std::collections::HashSet;

use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::Severity as ContentSeverity;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine, Severity as EngineSeverity, SlotKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::homebrew::PackContext;

use super::types::{DraftElement, ElementStatus, ImportPlan, Role};

/// Highest level swept.
pub const MAX_LEVEL: u32 = 20;
const TEST_SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];

/// A problem found at a level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepIssue {
    /// Level (0 = while loading).
    pub level: u32,
    /// Draft element to blame, if known.
    pub element_id: Option<String>,
    /// What went wrong.
    pub message: String,
}

fn issue(level: u32, element_id: Option<&str>, message: impl Into<String>) -> SweepIssue {
    SweepIssue {
        level,
        element_id: element_id.map(str::to_string),
        message: message.into(),
    }
}

/// What a sweep looks at.
struct Target<'a> {
    class_id: String,
    subclass_id: Option<String>,
    /// (feature id, level) that must appear.
    features: Vec<(&'a str, u32)>,
    root_id: &'a str,
    casts: bool,
}

fn targets(plan: &ImportPlan) -> Vec<Target<'_>> {
    let mut out = Vec::new();
    for g in &plan.groups {
        let Some(root_id) = g.root_id.as_deref() else {
            continue;
        };
        let Some(root) = plan.elements.iter().find(|e| e.id == root_id) else {
            continue;
        };
        if !matches!(root.role, Role::Class | Role::Subclass) {
            continue;
        }
        let features: Vec<(&str, u32)> = plan
            .elements
            .iter()
            .filter(|e| e.owner.as_deref() == Some(root_id))
            .filter_map(|e| Some((e.id.as_str(), e.level?)))
            .collect();
        let (class_id, subclass_id) = match (&g.parent, root.role) {
            (Some(p), Role::Subclass) => (p.class_id.clone(), Some(root.id.clone())),
            _ => (root.id.clone(), None),
        };
        // Slot casters only: a points caster gets no slots by design.
        let casts = root.role == Role::Class
            && g.outline
                .as_ref()
                .and_then(|o| o.caster())
                .is_some_and(|c| {
                    super::caster::caster_kind(c) != super::caster::CasterKind::Points
                });
        out.push(Target {
            class_id,
            subclass_id,
            features,
            root_id,
            casts,
        });
    }
    out
}

/// Run the sweep for every class/subclass group of the plan. Standalone elements get a load check
/// only.
pub fn engine_sweep(
    ctx: &PackContext,
    plan: &ImportPlan,
    elements: &[DraftElement],
) -> Vec<SweepIssue> {
    let docs: Vec<Value> = elements
        .iter()
        .filter(|e| {
            e.status != ElementStatus::Pending
                && e.status != ElementStatus::Failed
                && !e.json.is_null()
        })
        .map(|e| e.json.clone())
        .collect();
    let library = ctx.base.clone().build();
    let doc = super::derived::content_doc(&super::derived::derive(plan, &docs, &library));
    let file = match ContentFile::from_json_str(&doc.to_string()) {
        Ok(f) => f,
        Err(e) => return vec![issue(0, None, format!("the draft does not load: {e}"))],
    };
    let ids: HashSet<String> = elements.iter().map(|e| e.id.clone()).collect();
    let store = ctx
        .base
        .clone()
        .add_file("import-draft", PackOrigin::Custom, file)
        .build();
    let mut issues: Vec<SweepIssue> = store
        .validate()
        .into_iter()
        .filter(|i| i.severity == ContentSeverity::Error)
        .filter_map(|i| {
            let id = i.element?;
            ids.contains(&id).then(|| issue(0, Some(&id), i.message))
        })
        .collect();
    if !issues.is_empty() {
        return issues;
    }
    let engine = Engine::new(&store);
    for t in targets(plan) {
        issues.extend(sweep_one(&engine, ctx, &ids, &t));
    }
    // A problem that persists is reported once, at the first level it shows.
    let mut seen = HashSet::new();
    issues.retain(|i| seen.insert((i.element_id.clone(), i.message.clone())));
    issues
}

fn sweep_one(
    engine: &Engine,
    ctx: &PackContext,
    ids: &HashSet<String>,
    t: &Target,
) -> Vec<SweepIssue> {
    let mut out = Vec::new();
    let root = Some(t.root_id);
    let mut state = commands::new_character(engine, ctx.ruleset);
    let snap = engine.derive(&state);
    let Some(key) = slot_key(&snap, "ID_LEVEL_1", "Class", 1) else {
        return vec![issue(0, root, "the test character has no class slot")];
    };
    state = match commands::select(engine, &state, &key, Choice::Element(t.class_id.clone())) {
        Ok(o) => o.state,
        Err(e) => return vec![issue(1, root, format!("the engine refuses the class: {e}"))],
    };
    if let Ok(o) = commands::set_base_scores(engine, &state, TEST_SCORES) {
        state = o.state;
    }
    let mut totals: Vec<(u32, i32)> = Vec::new();
    let mut picked_subclass = t.subclass_id.is_none();
    for level in 1..=MAX_LEVEL {
        if level > 1 {
            match commands::level_up_main(engine, &state, None) {
                Ok(o) => state = o.state,
                Err(e) => {
                    out.push(issue(
                        level,
                        root,
                        format!("level-up to {level} fails: {e}"),
                    ));
                    break;
                }
            }
        }
        let mut snap = engine.derive(&state);
        if let Some(sub) = &t.subclass_id {
            if !picked_subclass {
                let slot = snap
                    .slots
                    .iter()
                    .find(|s| s.chosen.is_none() && s.options.iter().any(|o| &o.id == sub));
                if let Some(slot) = slot {
                    let key = slot.key.clone();
                    match commands::select(engine, &state, &key, Choice::Element(sub.clone())) {
                        Ok(o) => {
                            state = o.state;
                            snap = engine.derive(&state);
                            picked_subclass = true;
                        }
                        Err(e) => out.push(issue(
                            level,
                            Some(sub),
                            format!("selecting the subclass fails: {e}"),
                        )),
                    }
                }
            }
        }
        let filled_later = offered_after_picks(engine, &state, &snap, ids);
        collect_level(&snap, &filled_later, level, ids, t, &mut out, &mut totals);
    }
    if !picked_subclass {
        if let Some(sub) = &t.subclass_id {
            out.push(issue(
                MAX_LEVEL,
                Some(sub),
                "the parent class never offers this subclass (check `supports` against the parent's subclass choice)",
            ));
        }
    }
    check_slot_table(t, &totals, &mut out);
    out
}

/// Rounds of picks per level: a pick can open another select (a feat with a choice of its own).
const FILL_ROUNDS: usize = 4;
/// Options tried per select before the sweep leaves it open.
const FILL_TRIES: usize = 8;

/// Picks of the character as a whole (and the class and subclass, made apart): the sweep
/// looks at the class alone and leaves them open.
const NOT_FILLED: [&str; 7] = [
    "Archetype",
    "Class",
    "Race",
    "Sub Race",
    "Background",
    "Alignment",
    "Deity",
];

/// An open, required select of the class's own picks the test character can fill; a `List`
/// only when it is one of ours.
fn fillable(s: &borealis_engine::snapshot::SlotSnapshot, ids: &HashSet<String>) -> bool {
    s.chosen.is_none()
        && s.list_choice.is_none()
        && s.requires_selection
        && s.rejected.is_none()
        && !NOT_FILLED.contains(&s.ty.as_str())
        && (s.ty != "List" || ids.contains(&s.owner.id))
        && (!s.options.is_empty() || !s.list_items.is_empty())
}

/// The test character makes the picks a player would (the first allowed option of each open
/// required select, never one already picked), so a select whose options depend on another pick
/// (expertise over the skills picked as proficiencies) is judged after that pick, as the
/// engine's own invariants judge it. A select that offers nothing stays open and is flagged.
fn fill_open_picks(
    engine: &Engine,
    state: CharacterState,
    ids: &HashSet<String>,
) -> CharacterState {
    let mut state = state;
    for _ in 0..FILL_ROUNDS {
        let snap = engine.derive(&state);
        let mut taken: HashSet<String> = snap
            .slots
            .iter()
            .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
            .collect();
        let mut changed = false;
        for s in snap.slots.iter().filter(|s| fillable(s, ids)) {
            let choices = s
                .options
                .iter()
                .filter(|o| !taken.contains(&o.id))
                .map(|o| Choice::Element(o.id.clone()))
                .chain(s.list_items.iter().map(|(i, _)| Choice::ListItem(*i)))
                .take(FILL_TRIES);
            for c in choices {
                let picked = match &c {
                    Choice::Element(id) => Some(id.clone()),
                    Choice::ListItem(_) => None,
                };
                if let Ok(o) = commands::select(engine, &state, &s.key, c) {
                    state = o.state;
                    taken.extend(picked);
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    state
}

/// A select of ours that offers nothing (before any pick of the test character).
fn is_empty_select(s: &borealis_engine::snapshot::SlotSnapshot, ids: &HashSet<String>) -> bool {
    ids.contains(&s.owner.id)
        && s.ty != "Archetype"
        && s.options.is_empty()
        && s.list_items.is_empty()
}

/// The empty selects of `snap` that offer something (or are filled) once the test character
/// has made the picks a player makes first: expertise over the skills picked as proficiencies.
/// The sweep's own character stays unfilled; the picks are made on a copy, only when needed.
fn offered_after_picks(
    engine: &Engine,
    state: &CharacterState,
    snap: &borealis_engine::CharacterSnapshot,
    ids: &HashSet<String>,
) -> HashSet<SlotKey> {
    let empty: Vec<&SlotKey> = snap
        .slots
        .iter()
        .filter(|s| is_empty_select(s, ids))
        .map(|s| &s.key)
        .collect();
    if empty.is_empty() {
        return HashSet::new();
    }
    let filled = engine.derive(&fill_open_picks(engine, state.clone(), ids));
    filled
        .slots
        .iter()
        .filter(|s| empty.contains(&&s.key) && (s.chosen.is_some() || !is_empty_select(s, ids)))
        .map(|s| s.key.clone())
        .collect()
}

fn collect_level(
    snap: &borealis_engine::CharacterSnapshot,
    filled_later: &HashSet<SlotKey>,
    level: u32,
    ids: &HashSet<String>,
    t: &Target,
    out: &mut Vec<SweepIssue>,
    totals: &mut Vec<(u32, i32)>,
) {
    for d in &snap.diagnostics {
        let ours = d.element.as_ref().is_some_and(|e| ids.contains(e));
        if d.severity == EngineSeverity::Error && ours {
            out.push(issue(level, d.element.as_deref(), d.message.clone()));
        }
    }
    for inv in &snap.invalid_selections {
        out.push(issue(
            level,
            Some(t.root_id),
            format!("broken selection: {inv:?}"),
        ));
    }
    for s in snap
        .slots
        .iter()
        .filter(|s| is_empty_select(s, ids) && !filled_later.contains(&s.key))
    {
        out.push(issue(
            level,
            Some(&s.owner.id),
            format!("select \"{}\" ({}) offers no options", s.name, s.ty),
        ));
    }
    for (fid, flevel) in &t.features {
        if *flevel == level && !snap.features.iter().any(|f| f.id == *fid) {
            out.push(issue(
                level,
                Some(fid),
                format!("feature {fid} is missing at level {level}"),
            ));
        }
    }
    if t.casts {
        let total: i32 = snap
            .spellcasting
            .iter()
            .map(|s| s.slots.iter().sum::<i32>() + s.pact.map_or(0, |p| p.0 * p.1))
            .sum();
        if snap.spellcasting.is_empty() && level >= 3 {
            out.push(issue(
                level,
                Some(t.root_id),
                "no spellcasting appears for the class",
            ));
        }
        totals.push((level, total));
    }
}

fn check_slot_table(t: &Target, totals: &[(u32, i32)], out: &mut Vec<SweepIssue>) {
    if !t.casts {
        return;
    }
    for w in totals.windows(2) {
        if w[1].1 < w[0].1 {
            out.push(issue(
                w[1].0,
                Some(t.root_id),
                format!("spell slots shrink at level {}", w[1].0),
            ));
        }
    }
    let first = totals.iter().find(|x| x.1 > 0);
    let last = totals.last();
    if let (Some(f), Some(l)) = (first, last) {
        if l.1 <= f.1 {
            out.push(issue(
                l.0,
                Some(t.root_id),
                "the spell slot table does not increase over levels 1-20",
            ));
        }
    } else if !totals.is_empty() {
        out.push(issue(
            MAX_LEVEL,
            Some(t.root_id),
            "the class never gets spell slots",
        ));
    }
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
