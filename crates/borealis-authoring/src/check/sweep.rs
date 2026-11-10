//! The engine sweep: a test character of the right edition takes the class (or the parent class
//! plus the subclass) and levels 1 to 20, deriving at every level.
//!
//! What it finds is typed ([`SweepProblem`], [`SweepFinding`]) so the homebrew creator can say it in plain words
//! (`check::messages`); [`engine_sweep`] gives the same findings as text ([`SweepIssue`]) for the import pipeline.

use std::collections::HashSet;
use std::fmt;

use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStoreBuilder, Issue, Ruleset, Severity as ContentSeverity};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine, Severity as EngineSeverity, SlotKey};
use serde::{Deserialize, Serialize};

/// Highest level swept.
pub const MAX_LEVEL: u32 = 20;
const TEST_SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];

/// A problem found at a level, as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepIssue {
    /// Level (0 = while loading).
    pub level: u32,
    /// Draft element to blame, if known.
    pub element_id: Option<String>,
    /// What went wrong.
    pub message: String,
}

/// What the sweep can find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepProblem {
    /// The content validator's error on a draft element (the draft doesn't load).
    Store(Issue),
    /// The test character has no class slot (the edition's core content is missing).
    NoClassSlot,
    /// The engine refuses the class (reason from the engine).
    ClassRefused(String),
    /// Levelling up fails (reason from the engine).
    LevelUpFails(String),
    /// Selecting the subclass fails (reason from the engine).
    SubclassRefused(String),
    /// The parent class never offers the subclass.
    SubclassNeverOffered,
    /// An engine error on a draft element while deriving (message from the engine).
    Engine(String),
    /// A selection the engine marks invalid (its debug text).
    BrokenSelection(String),
    /// A select of the draft offers nothing.
    EmptySelect {
        /// The select's name (its label).
        name: String,
        /// The element type it selects.
        ty: String,
    },
    /// A feature the class grants does not appear at its level.
    MissingFeature(String),
    /// A caster class shows no spellcasting.
    NoSpellcasting,
    /// The slot table shrinks at this level.
    SlotsShrink(u32),
    /// The slot table never grows.
    SlotsFlat,
    /// A caster class never gets slots.
    NoSlots,
}

impl fmt::Display for SweepProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(i) => f.write_str(&i.message),
            Self::NoClassSlot => f.write_str("the test character has no class slot"),
            Self::ClassRefused(e) => write!(f, "the engine refuses the class: {e}"),
            Self::LevelUpFails(e) => f.write_str(e),
            Self::SubclassRefused(e) => write!(f, "selecting the subclass fails: {e}"),
            Self::SubclassNeverOffered => f.write_str(
                "the parent class never offers this subclass (check `supports` against the parent's subclass choice)",
            ),
            Self::Engine(m) => f.write_str(m),
            Self::BrokenSelection(d) => write!(f, "broken selection: {d}"),
            Self::EmptySelect { name, ty } => write!(f, "select \"{name}\" ({ty}) offers no options"),
            Self::MissingFeature(id) => write!(f, "feature {id} is missing"),
            Self::NoSpellcasting => f.write_str("no spellcasting appears for the class"),
            Self::SlotsShrink(level) => write!(f, "spell slots shrink at level {level}"),
            Self::SlotsFlat => f.write_str("the spell slot table does not increase over levels 1-20"),
            Self::NoSlots => f.write_str("the class never gets spell slots"),
        }
    }
}

/// One finding of the sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepFinding {
    /// Level (0 = while loading).
    pub level: u32,
    /// Draft element to blame, if known.
    pub element_id: Option<String>,
    /// What went wrong.
    pub problem: SweepProblem,
}

impl SweepFinding {
    /// The finding as text (the import pipeline's [`SweepIssue`]).
    #[must_use]
    pub fn to_issue(&self) -> SweepIssue {
        let message = match &self.problem {
            SweepProblem::LevelUpFails(e) => format!("level-up to {} fails: {e}", self.level),
            SweepProblem::MissingFeature(id) => {
                format!("feature {id} is missing at level {}", self.level)
            }
            other => other.to_string(),
        };
        SweepIssue {
            level: self.level,
            element_id: self.element_id.clone(),
            message,
        }
    }
}

fn issue(level: u32, element_id: Option<&str>, problem: SweepProblem) -> SweepFinding {
    SweepFinding {
        level,
        element_id: element_id.map(str::to_string),
        problem,
    }
}

/// What a sweep looks at: one class, or a parent class plus one subclass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepTarget {
    /// The class the test character takes.
    pub class_id: String,
    /// The subclass it picks when the class offers it.
    pub subclass_id: Option<String>,
    /// (feature id, level) that must appear.
    pub features: Vec<(String, u32)>,
    /// The element problems of the class or subclass itself are blamed on.
    pub root_id: String,
    /// A slot caster: its slot table must grow over levels 1-20.
    pub casts: bool,
}

/// Load `file` (the draft content, pack `pack_name`) on top of `base`, report its store errors, and when there are
/// none sweep every target 1-20. `ids` are the draft's own element ids: only their problems are reported, and a
/// `List` select is filled only when it is theirs.
pub fn engine_sweep(
    base: &ContentStoreBuilder,
    ruleset: Ruleset,
    pack_name: &str,
    file: ContentFile,
    ids: &HashSet<String>,
    targets: &[SweepTarget],
) -> Vec<SweepIssue> {
    sweep_findings(base, ruleset, pack_name, file, ids, targets)
        .iter()
        .map(SweepFinding::to_issue)
        .collect()
}

/// [`engine_sweep`], typed.
pub fn sweep_findings(
    base: &ContentStoreBuilder,
    ruleset: Ruleset,
    pack_name: &str,
    file: ContentFile,
    ids: &HashSet<String>,
    targets: &[SweepTarget],
) -> Vec<SweepFinding> {
    let store = base
        .clone()
        .add_file(pack_name, PackOrigin::Custom, file)
        .build();
    let issues: Vec<SweepFinding> = store
        .validate()
        .into_iter()
        .filter(|i| i.severity == ContentSeverity::Error)
        .filter_map(|i| {
            let id = i.element.clone()?;
            ids.contains(&id)
                .then(|| issue(0, Some(&id), SweepProblem::Store(i)))
        })
        .collect();
    if !issues.is_empty() {
        return issues;
    }
    sweep_engine(&Engine::new(&store), ruleset, ids, targets)
}

/// The level sweep of `targets` on an engine that holds the draft already (its store loads without errors; also
/// Try it's private engine, spec 9): every target swept 1-20.
#[must_use]
pub fn sweep_engine(
    engine: &Engine,
    ruleset: Ruleset,
    ids: &HashSet<String>,
    targets: &[SweepTarget],
) -> Vec<SweepFinding> {
    let mut issues = Vec::new();
    for t in targets {
        issues.extend(sweep_one(engine, ruleset, ids, t));
    }
    // A problem that persists is reported once, at the first level it shows.
    let mut seen = HashSet::new();
    issues.retain(|i: &SweepFinding| seen.insert((i.element_id.clone(), i.to_issue().message)));
    issues
}

fn sweep_one(
    engine: &Engine,
    ruleset: Ruleset,
    ids: &HashSet<String>,
    t: &SweepTarget,
) -> Vec<SweepFinding> {
    let mut out = Vec::new();
    let root = Some(t.root_id.as_str());
    let mut state = commands::new_character(engine, ruleset);
    let snap = engine.derive(&state);
    let Some(key) = slot_key(&snap, "ID_LEVEL_1", "Class", 1) else {
        return vec![issue(0, root, SweepProblem::NoClassSlot)];
    };
    state = match commands::select(engine, &state, &key, Choice::Element(t.class_id.clone())) {
        Ok(o) => o.state,
        Err(e) => return vec![issue(1, root, SweepProblem::ClassRefused(e.to_string()))],
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
                        SweepProblem::LevelUpFails(e.to_string()),
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
                            SweepProblem::SubclassRefused(e.to_string()),
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
                SweepProblem::SubclassNeverOffered,
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
/// looks at the class alone and leaves them open. The background is picked (on the copy that
/// judges empty selects only): its skills are what a player's second Expertise draws on (the
/// SRD Bard's at level 9, after the class's three skills went to the first).
const NOT_FILLED: [&str; 6] = [
    "Archetype",
    "Class",
    "Race",
    "Sub Race",
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
    t: &SweepTarget,
    out: &mut Vec<SweepFinding>,
    totals: &mut Vec<(u32, i32)>,
) {
    for d in &snap.diagnostics {
        let ours = d.element.as_ref().is_some_and(|e| ids.contains(e));
        if d.severity == EngineSeverity::Error && ours {
            out.push(issue(
                level,
                d.element.as_deref(),
                SweepProblem::Engine(d.message.clone()),
            ));
        }
    }
    for inv in &snap.invalid_selections {
        out.push(issue(
            level,
            Some(t.root_id.as_str()),
            SweepProblem::BrokenSelection(format!("{inv:?}")),
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
            SweepProblem::EmptySelect {
                name: s.name.clone(),
                ty: s.ty.clone(),
            },
        ));
    }
    for (fid, flevel) in &t.features {
        if *flevel == level && !snap.features.iter().any(|f| &f.id == fid) {
            out.push(issue(
                level,
                Some(fid),
                SweepProblem::MissingFeature(fid.clone()),
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
                Some(t.root_id.as_str()),
                SweepProblem::NoSpellcasting,
            ));
        }
        totals.push((level, total));
    }
}

fn check_slot_table(t: &SweepTarget, totals: &[(u32, i32)], out: &mut Vec<SweepFinding>) {
    if !t.casts {
        return;
    }
    for w in totals.windows(2) {
        if w[1].1 < w[0].1 {
            out.push(issue(
                w[1].0,
                Some(t.root_id.as_str()),
                SweepProblem::SlotsShrink(w[1].0),
            ));
        }
    }
    let first = totals.iter().find(|x| x.1 > 0);
    let last = totals.last();
    if let (Some(f), Some(l)) = (first, last) {
        if l.1 <= f.1 {
            out.push(issue(
                l.0,
                Some(t.root_id.as_str()),
                SweepProblem::SlotsFlat,
            ));
        }
    } else if !totals.is_empty() {
        out.push(issue(
            MAX_LEVEL,
            Some(t.root_id.as_str()),
            SweepProblem::NoSlots,
        ));
    }
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
