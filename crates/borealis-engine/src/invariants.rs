//! Settle invariants (engine-test-audit-1 WP2): properties every derived snapshot must have,
//! whatever the character. Tests run them after every derive of their helpers; the gated corpus
//! sweeps collect them as problems.
//!
//! Two entry points:
//! - [`check`] returns the [`Violation`]s of one snapshot (sweeps report them);
//! - [`enable_for_thread`] makes every full [`Engine::derive`] on the calling thread panic when
//!   an invariant fails (test helpers call it once per test). Derives made by the checks
//!   themselves are not checked again.
//!
//! The checks re-derive the character (idempotence; the same evaluation feeds the option-list
//! oracle, which re-runs the option filters per candidate) and prune twice (the first from the
//! second derive's slots, the second only when the first dropped something), so a check costs
//! about one derive. Nothing runs unless a caller asks for it.

use crate::derive::Evaluation;
use crate::diag::code;
use crate::engine::Engine;
use crate::snapshot::{CharacterSnapshot, SpellcastingSnapshot};
use crate::state::CharacterState;
use borealis_content::Rule;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Violation codes (stable, one per invariant).
pub mod kind {
    /// A custom rule kind or field the engine ignores (`content.unknown_rule_*` diagnostics).
    pub const UNKNOWN_RULE: &str = "invariant.unknown_rule";
    /// Requirements and statistics did not settle (`eval.not_converged`).
    pub const NOT_CONVERGED: &str = "invariant.not_converged";
    /// Character level outside 1..=20, or class levels that do not sum to it.
    pub const LEVEL: &str = "invariant.level";
    /// A required select offers nothing, nothing else is left to pick and no diagnostic explains
    /// why.
    pub const EMPTY_SELECT: &str = "invariant.empty_select";
    /// A pick that the slot's option list does not offer.
    pub const PICK_NOT_OFFERED: &str = "invariant.pick_not_offered";
    /// Spell slots negative or growing with the slot level.
    pub const SLOTS: &str = "invariant.slots";
    /// Pact Magic slots with no slot level, or a pact caster that also has ordinary slots.
    pub const PACT: &str = "invariant.pact";
    /// A caster with spell slots and no way to learn, prepare or be granted a levelled spell.
    pub const NO_SPELL_ACQUISITION: &str = "invariant.no_spell_acquisition";
    /// A second derive or a second prune differs from the first.
    pub const NOT_IDEMPOTENT: &str = "invariant.not_idempotent";
    /// The option list differs from the per-candidate requirement check.
    pub const OPTION_ORACLE: &str = "invariant.option_oracle";
    /// Option filters ran with statistic sums other than the settled ones.
    pub const STALE_OPTION_STATS: &str = "invariant.stale_option_stats";
    /// An element grants or selects itself (directly or through others), or a grant chain nests
    /// past the engine's depth limit (`content.grant_cycle`). The engine cuts the chain, so the
    /// character still builds, but the content is malformed.
    pub const GRANT_CYCLE: &str = "invariant.grant_cycle";
}

/// One failed invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// One of the [`kind`] codes.
    pub kind: &'static str,
    /// The element (or spellcasting name) concerned, if any.
    pub element: Option<String>,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.element {
            Some(e) => write!(f, "{} [{e}]: {}", self.kind, self.message),
            None => write!(f, "{}: {}", self.kind, self.message),
        }
    }
}

fn violation(kind: &'static str, element: Option<&str>, message: String) -> Violation {
    Violation {
        kind,
        element: element.map(str::to_string),
        message,
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static CHECKING: Cell<bool> = const { Cell::new(false) };
    static ALLOWED: RefCell<Vec<(&'static str, String)>> = const { RefCell::new(Vec::new()) };
    static PASSED: RefCell<HashSet<PassedKey>> = RefCell::new(HashSet::new());
    static RUN: Cell<usize> = const { Cell::new(0) };
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static RECORDED: RefCell<Vec<Violation>> = const { RefCell::new(Vec::new()) };
}

/// Make the derive hook of this thread record violations instead of panicking (a corpus sweep of
/// community content, where one defective element must not drop the whole setup's results).
/// [`take_recorded`] returns them; [`disable_for_thread`] turns this off.
pub fn record_for_thread() {
    RECORD.with(|r| r.set(true));
}

/// The distinct violations the hook recorded on this thread since the last call (kind and
/// element; the first message of each).
#[must_use]
pub fn take_recorded() -> Vec<Violation> {
    RECORDED.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// Does the derive hook of this thread record violations ([`record_for_thread`])?
#[must_use]
pub fn recording_for_thread() -> bool {
    RECORD.with(Cell::get)
}

/// Add violations recorded elsewhere (a worker thread's [`take_recorded`]) to this thread's.
pub fn record_violations(found: Vec<Violation>) {
    record(found);
}

fn record(found: Vec<Violation>) {
    RECORDED.with(|r| {
        let mut r = r.borrow_mut();
        for v in found {
            if !r.iter().any(|o| o.kind == v.kind && o.element == v.element) {
                r.push(v);
            }
        }
    });
}

/// Let the derive hook of this thread accept violations of `kind` concerning `element` (a
/// content limit the test knows about; say why at the call site).
pub fn allow_for_thread(kind: &'static str, element: impl Into<String>) {
    ALLOWED.with(|a| a.borrow_mut().push((kind, element.into())));
}

fn allowed(v: &Violation) -> bool {
    ALLOWED.with(|a| {
        a.borrow()
            .iter()
            .any(|(k, e)| *k == v.kind && v.element.as_deref() == Some(e.as_str()))
    })
}

/// Check every later full derive ([`Engine::derive`], or `derive_with(.., true)`) on this thread
/// and panic on a violation. Test helpers call this; production code never does.
pub fn enable_for_thread() {
    ENABLED.with(|e| e.set(true));
}

/// Stop checking derives on this thread (and forget its allowed violations).
pub fn disable_for_thread() {
    ENABLED.with(|e| e.set(false));
    RECORD.with(|r| r.set(false));
    ALLOWED.with(|a| a.borrow_mut().clear());
    PASSED.with(|p| p.borrow_mut().clear());
}

/// A state the hook already checked without any violation: (engine serial, hash and length of
/// the state's JSON). A derive is deterministic (the checks prove it: idempotence), so the same
/// state on the same engine passes again; checking it again only costs a few derives.
type PassedKey = (u64, u64, usize);

/// Most passed states a thread remembers (then it starts over): a long corpus run derives
/// millions of states.
const PASSED_CAP: usize = 100_000;

fn passed_key(engine: &Engine, state: &CharacterState) -> Option<PassedKey> {
    use std::hash::{Hash, Hasher};
    let json = serde_json::to_vec(state).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut h);
    Some((engine.serial(), h.finish(), json.len()))
}

/// Is checking enabled on this thread?
#[must_use]
pub fn enabled_for_thread() -> bool {
    ENABLED.with(Cell::get)
}

/// How many derives the hook of this thread has checked (a state that already passed is not
/// checked again, see [`after_derive`]).
#[must_use]
pub fn checks_run_on_thread() -> usize {
    RUN.with(Cell::get)
}

/// Runs `f` with the derive hook off, so the checks' own derives are not checked again.
fn quiet<R>(f: impl FnOnce() -> R) -> R {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            CHECKING.with(|c| c.set(self.0));
        }
    }
    let _reset = Reset(CHECKING.with(|c| c.replace(true)));
    f()
}

/// The derive hook: checks a full snapshot when the thread asked for it.
pub(crate) fn after_derive(engine: &Engine, state: &CharacterState, snap: &CharacterSnapshot) {
    if !enabled_for_thread() || CHECKING.with(Cell::get) {
        return;
    }
    // the builder re-derives an unchanged state for every option page and slot it lists
    let key = passed_key(engine, state);
    if key.is_some_and(|k| PASSED.with(|p| p.borrow().contains(&k))) {
        return;
    }
    RUN.with(|r| r.set(r.get() + 1));
    let mut found = quiet(|| check_with(engine, state, snap, true));
    found.retain(|v| !allowed(v));
    let recording = RECORD.with(Cell::get);
    if recording && !found.is_empty() {
        // recorded once: the same state is not judged again
        record(std::mem::take(&mut found));
    }
    if let (true, Some(k)) = (found.is_empty(), key) {
        // the allowed list only grows until disable_for_thread, which forgets the passes too:
        // a state that passed stays passing
        PASSED.with(|p| {
            let mut p = p.borrow_mut();
            if p.len() >= PASSED_CAP {
                p.clear();
            }
            p.insert(k);
        });
    }
    assert!(
        found.is_empty(),
        "settle invariants failed ({:?} level {}):\n{}",
        state.ruleset,
        snap.level,
        found
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Panic when `snap` (a full [`Engine::derive`] of `state`) breaks an invariant.
///
/// # Panics
/// Lists every violation.
pub fn assert_holds(engine: &Engine, state: &CharacterState, snap: &CharacterSnapshot) {
    let found = check(engine, state, snap);
    assert!(
        found.is_empty(),
        "settle invariants failed:\n{}",
        found
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The violations of `snap`, which must be a full [`Engine::derive`] of `state` (option lists
/// included when the engine settings include them).
#[must_use]
pub fn check(engine: &Engine, state: &CharacterState, snap: &CharacterSnapshot) -> Vec<Violation> {
    let with_options = engine.settings().include_options;
    quiet(|| check_with(engine, state, snap, with_options))
}

fn check_with(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    with_options: bool,
) -> Vec<Violation> {
    let mut out = Vec::new();
    diagnostics(snap, &mut out);
    levels(snap, &mut out);
    if with_options {
        selects(snap, &mut out);
    }
    for sc in &snap.spellcasting {
        slot_table(&sc.name, &sc.slots, &mut out);
        pact(sc, &mut out);
        acquisition(engine, state, snap, sc, with_options, &mut out);
    }
    if let Some(m) = &snap.multiclass_slots {
        slot_table("multiclass", &m.slots, &mut out);
    }
    // one evaluation serves the second derive and the oracle (a derive is a pure function of
    // the state; the oracle reads the evaluation before the snapshot consumes it)
    let ev = Evaluation::run(
        engine.universe(state.ruleset),
        state,
        engine.settings(),
        false,
    );
    let mut listed = Vec::new();
    if with_options {
        oracle(engine, state, snap, &ev, &mut listed);
    }
    let again = ev.snapshot(state, engine.settings(), with_options, false);
    idempotence(engine, state, snap, &again, &mut out);
    out.extend(listed);
    out
}

fn diagnostics(snap: &CharacterSnapshot, out: &mut Vec<Violation>) {
    for d in &snap.diagnostics {
        let kind = match d.code.as_str() {
            code::CONTENT_UNKNOWN_RULE_KIND | code::CONTENT_UNKNOWN_RULE_FIELD => {
                kind::UNKNOWN_RULE
            }
            code::EVAL_NOT_CONVERGED => kind::NOT_CONVERGED,
            code::CONTENT_GRANT_CYCLE => kind::GRANT_CYCLE,
            _ => continue,
        };
        out.push(violation(kind, d.element.as_deref(), d.message.clone()));
    }
}

fn levels(snap: &CharacterSnapshot, out: &mut Vec<Violation>) {
    if !(1..=20).contains(&snap.level) {
        out.push(violation(
            kind::LEVEL,
            None,
            format!("character level {} is outside 1..=20", snap.level),
        ));
    }
    let classed: Vec<_> = snap.classes.iter().filter(|c| c.class.is_some()).collect();
    if classed.is_empty() {
        return;
    }
    // a multiclass level whose class is not chosen yet still counts
    let sum: u32 = snap.classes.iter().map(|c| c.level).sum();
    if sum != snap.level || classed.iter().any(|c| c.level == 0) {
        let parts: Vec<String> = snap
            .classes
            .iter()
            .map(|c| {
                format!(
                    "{}={}",
                    c.class.as_ref().map_or("?", |r| r.id.as_str()),
                    c.level
                )
            })
            .collect();
        out.push(violation(
            kind::LEVEL,
            None,
            format!(
                "class levels {} do not sum to character level {}",
                parts.join(" "),
                snap.level
            ),
        ));
    }
}

fn selects(snap: &CharacterSnapshot, out: &mut Vec<Violation>) {
    let explained: BTreeSet<&str> = snap
        .diagnostics
        .iter()
        .filter_map(|d| d.element.as_deref())
        .collect();
    let offered =
        |s: &&crate::snapshot::SlotSnapshot| !s.options.is_empty() || !s.list_items.is_empty();
    // an open pick elsewhere (skills before Expertise, a feat's list before its spells) may
    // still fill an empty select: only a character with nothing else to pick is judged
    let pending = snap
        .slots
        .iter()
        .any(|s| s.requires_selection && s.rejected.is_none() && offered(&s));
    for s in &snap.slots {
        if !pending
            && s.requires_selection
            && s.rejected.is_none()
            && !offered(&s)
            && !explained.contains(s.owner.id.as_str())
        {
            out.push(violation(
                kind::EMPTY_SELECT,
                Some(&s.owner.id),
                format!("required select {} ({}) offers nothing", s.name, s.ty),
            ));
        }
        if let Some(c) = &s.chosen {
            if !s.options.is_empty() && !s.options.iter().any(|o| o.id == c.id) {
                out.push(violation(
                    kind::PICK_NOT_OFFERED,
                    Some(&s.owner.id),
                    format!("{} holds {} but does not offer it", s.name, c.id),
                ));
            }
        }
    }
}

fn slot_table(name: &str, slots: &[i32; 9], out: &mut Vec<Violation>) {
    if slots.iter().any(|n| *n < 0) {
        out.push(violation(
            kind::SLOTS,
            Some(name),
            format!("negative spell slots {slots:?}"),
        ));
    }
    // content may grant slots of some levels only (a species' single 2nd-level slot, a
    // capstone's 7th-9th level slots): the levels it grants must not grow, the empty ones
    // between them say nothing
    let granted: Vec<i32> = slots.iter().copied().filter(|n| *n != 0).collect();
    if granted.windows(2).any(|w| w[1] > w[0]) {
        out.push(violation(
            kind::SLOTS,
            Some(name),
            format!("spell slots grow with the slot level {slots:?}"),
        ));
    }
}

fn pact(sc: &SpellcastingSnapshot, out: &mut Vec<Violation>) {
    let Some((count, level)) = sc.pact else {
        return;
    };
    if count > 0 && !(1..=9).contains(&level) {
        out.push(violation(
            kind::PACT,
            Some(&sc.name),
            format!("{count} pact slots of level {level}"),
        ));
    }
    if sc.slots.iter().any(|n| *n != 0) {
        out.push(violation(
            kind::PACT,
            Some(&sc.name),
            format!("pact caster also has ordinary slots {:?}", sc.slots),
        ));
    }
}

/// A caster that can cast levelled spells has a way to get one: the whole class list to
/// prepare from, a levelled spell it already knows (granted, always prepared, in the spellbook)
/// or a spell choice slot that offers (or holds) a levelled spell.
fn acquisition(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    sc: &SpellcastingSnapshot,
    with_options: bool,
    out: &mut Vec<Violation>,
) {
    let casts = sc.slots.iter().any(|n| *n > 0) || sc.pact.is_some_and(|p| p.0 > 0);
    if !casts || sc.prepare_from_list {
        return;
    }
    let knows = sc.spells.iter().any(|s| s.level > 0)
        || sc
            .spellbook
            .as_ref()
            .is_some_and(|b| b.spells.iter().any(|s| s.level > 0));
    if knows {
        return;
    }
    let uni = engine.universe(state.ruleset);
    let levelled = |id: &str| uni.get(id).and_then(|e| e.spell_level()).unwrap_or(0) > 0;
    let slot = snap.slots.iter().any(|s| {
        s.ty == "Spell"
            && (s.spellcasting.as_deref() == Some(sc.name.as_str()) || s.owner.name == sc.name)
            && (s.chosen.as_ref().is_some_and(|c| levelled(&c.id))
                || s.options.iter().any(|o| o.level.unwrap_or(0) > 0)
                || (!with_options && !s.name.to_lowercase().contains("cantrip")))
    });
    if !slot {
        out.push(violation(
            kind::NO_SPELL_ACQUISITION,
            Some(&sc.name),
            format!(
                "slots {:?} pact {:?} but no levelled spell can be learned, prepared or granted",
                sc.slots, sc.pact
            ),
        ));
    }
}

fn idempotence(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    again: &CharacterSnapshot,
    out: &mut Vec<Violation>,
) {
    if again != snap {
        out.push(violation(
            kind::NOT_IDEMPOTENT,
            None,
            format!("a second derive differs: {}", first_difference(snap, again)),
        ));
    }
    // the second derive lists the state's active slots (as the derive a prune makes), and a
    // prune that dropped nothing gives the same state again
    let once = crate::commands::prune_derived(state, again);
    let twice = if once == *state {
        once.clone()
    } else {
        crate::commands::prune(engine, &once)
    };
    if once != twice {
        out.push(violation(
            kind::NOT_IDEMPOTENT,
            None,
            format!(
                "a second prune drops more selections ({} -> {})",
                once.selections.len(),
                twice.selections.len()
            ),
        ));
    }
}

/// The first top-level snapshot field that differs (for the message).
fn first_difference(a: &CharacterSnapshot, b: &CharacterSnapshot) -> String {
    let (Ok(serde_json::Value::Object(a)), Ok(serde_json::Value::Object(b))) =
        (serde_json::to_value(a), serde_json::to_value(b))
    else {
        return "snapshots do not serialise".into();
    };
    a.iter()
        .find(|(k, v)| b.get(*k) != Some(*v))
        .map_or_else(|| "same JSON, different values".into(), |(k, _)| k.clone())
}

/// Open slots whose option list the oracle re-checks per derive, in tree order. A user (and a
/// fill loop) picks the first open slot next, so over a build every slot is checked once it is
/// first in line; checking all of them each time made level-20 caster tests minutes long.
const ORACLE_OPEN_SLOTS: usize = 2;

/// Option lists of the first open slots equal the per-candidate requirement check (a fresh spell
/// token context for each candidate), and the option filters saw the settled statistic sums.
fn oracle(
    engine: &Engine,
    state: &CharacterState,
    snap: &CharacterSnapshot,
    ev: &Evaluation<'_>,
    out: &mut Vec<Violation>,
) {
    let uni = engine.universe(state.ruleset);
    let g = &ev.graph;
    let settled = ev.stats.sums();
    if g.stat_sums != settled {
        let differs: Vec<&String> = settled
            .iter()
            .filter(|(k, v)| g.stat_sums.get(*k) != Some(*v))
            .map(|(k, _)| k)
            .take(5)
            .collect();
        out.push(violation(
            kind::STALE_OPTION_STATS,
            None,
            format!("option filters read statistic sums other than the settled ones: {differs:?}"),
        ));
    }
    // open slots in tree order, empty lists first (the most suspicious, and rare)
    let empty: BTreeSet<&crate::state::SlotKey> = snap
        .slots
        .iter()
        .filter(|s| s.options.is_empty())
        .map(|s| &s.key)
        .collect();
    let mut open = Vec::new();
    for id in g.preorder() {
        for slot in &g.nodes[id].slots {
            if let Rule::Select(rule) = &g.nodes[id].e.el.rules[slot.rule] {
                // open slots only: an open slot is the list a user picks from
                if !rule.is_list() && slot.chosen.is_none() {
                    open.push((id, slot, rule));
                }
            }
        }
    }
    open.sort_by_key(|(_, slot, _)| !empty.contains(&slot.key));
    let mut slow: BTreeMap<&crate::state::SlotKey, BTreeSet<&str>> = BTreeMap::new();
    for (id, slot, rule) in open.into_iter().take(ORACLE_OPEN_SLOTS) {
        let mut allowed: BTreeSet<&str> = uni
            .select_candidates(&rule.ty, true)
            .filter(|c| g.choice_allowed(id, slot.rule, c, false))
            .map(|c| c.el.id.as_str())
            .collect();
        for alt in crate::custom_rules::alternatives_of(rule) {
            if let Some(c) = uni.get(alt) {
                if g.choice_allowed(id, slot.rule, c, false) {
                    allowed.insert(c.el.id.as_str());
                }
            }
        }
        slow.insert(&slot.key, allowed);
    }
    let name = |id: &str| uni.get(id).map(|e| e.el.name.to_lowercase());
    for s in &snap.slots {
        let Some(expected) = slow.get(&s.key) else {
            continue;
        };
        let listed: BTreeSet<&str> = s.options.iter().map(|o| o.id.as_str()).collect();
        let listed_names: BTreeSet<String> =
            s.options.iter().map(|o| o.name.to_lowercase()).collect();
        let extra: Vec<&&str> = listed.difference(expected).take(5).collect();
        // a Companion select drops a typed monster named like an offered companion
        let missing: Vec<&&str> = expected
            .difference(&listed)
            .filter(|id| s.ty != "Companion" || name(id).is_none_or(|n| !listed_names.contains(&n)))
            .take(5)
            .collect();
        if !extra.is_empty() || !missing.is_empty() {
            out.push(violation(
                kind::OPTION_ORACLE,
                Some(&s.owner.id),
                format!(
                    "{} options differ from the per-candidate check: extra {extra:?}, missing {missing:?}",
                    s.name
                ),
            ));
        }
    }
}
