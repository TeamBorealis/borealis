//! The statistic engine: named additive groups evaluated as a dependency DAG.
//!
//! Semantics follow `engine.md` section 7: a group is a list of `(source label, value)`
//! contributions whose sum is the statistic. Rules without `bonus` stack; rules sharing a
//! `bonus` label contribute only the largest value (the real maximum, even when negative).
//! Derived groups (ability score/modifier, proficiency halves) are synthesized here.

use crate::diag::{code, DiagSink, Diagnostic, Severity};
use crate::ids::{ABILITIES, SKILLS};
use crate::provenance::{StepKind, TraceStep};
use crate::universe::{UStat, Val};
use std::collections::{HashMap, HashSet};

/// One group: contributions per source label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatGroup {
    /// `(label, value)` in insertion order; equal labels are merged by addition.
    pub contribs: Vec<(String, i32)>,
}

impl StatGroup {
    /// The statistic value.
    #[must_use]
    pub fn sum(&self) -> i32 {
        self.contribs.iter().map(|c| c.1).sum()
    }

    /// Add a contribution, merging with an existing label.
    pub fn add(&mut self, label: &str, value: i32) {
        if let Some(c) = self.contribs.iter_mut().find(|c| c.0 == label) {
            c.1 += value;
        } else {
            self.contribs.push((label.to_string(), value));
        }
    }
}

/// Everything the evaluator needs about the character.
#[derive(Debug, Default)]
pub struct StatInputs<'u> {
    /// Active rules in evaluation order.
    pub rules: Vec<&'u UStat>,
    /// Initial contributions `(group, label, value)`.
    pub seeds: Vec<(String, String, i32)>,
    /// Groups that exist (value 0) even without rules.
    pub builtin: HashSet<String>,
    /// Base ability scores before bonuses.
    pub base_scores: [i32; 6],
    /// Apply the `<ability>:max` cap.
    pub use_ability_max: bool,
    /// Extra merges `(target, source)` (per-class spellcasting DC / attack).
    pub merges: Vec<(String, String)>,
    /// Record per-group provenance steps (opt-in).
    pub trace: bool,
    /// Rules that were considered but are not active, with the reason (only when `trace`).
    pub skipped: Vec<(&'u UStat, String)>,
}

/// Evaluated statistics.
#[derive(Debug, Default)]
pub struct StatSet {
    groups: HashMap<String, StatGroup>,
    traces: HashMap<String, Vec<TraceStep>>,
}

impl StatSet {
    /// Value of a group (0 if absent).
    #[must_use]
    pub fn sum(&self, name: &str) -> i32 {
        self.groups.get(name).map_or(0, StatGroup::sum)
    }

    /// True if the group exists (has a seed, a rule or is built in).
    #[must_use]
    pub fn exists(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }

    /// A group.
    #[must_use]
    pub fn group(&self, name: &str) -> Option<&StatGroup> {
        self.groups.get(name)
    }

    /// All groups.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &StatGroup)> {
        self.groups.iter()
    }

    /// Provenance steps of a group (empty unless the evaluation traced).
    #[must_use]
    pub fn trace_steps(&self, name: &str) -> &[TraceStep] {
        self.traces.get(name).map_or(&[], Vec::as_slice)
    }

    /// Group sums as a plain map.
    #[must_use]
    pub fn sums(&self) -> HashMap<String, i32> {
        self.groups
            .iter()
            .map(|(k, v)| (k.clone(), v.sum()))
            .collect()
    }
}

/// Ability modifier: `floor((score - 10) / 2)` (equals the legacy truncating formula).
#[must_use]
pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

/// Bonus to every ability check (Borealis): merged into each skill's `:misc` (so passives) and
/// into initiative.
pub const ABILITY_CHECKS_GROUP: &str = "ability checks";
/// Bonus to Constitution saves to keep concentration (Borealis), on top of the Constitution
/// save: `CharacterSnapshot::concentration_save`.
pub const CONCENTRATION_SAVE_GROUP: &str = "concentration:save";

/// Names of the groups that are always present.
#[must_use]
pub fn builtin_groups() -> HashSet<String> {
    let mut s: HashSet<String> = HashSet::new();
    for a in ABILITIES {
        for suffix in [
            "",
            ":score",
            ":score:set",
            ":modifier",
            ":modifier:half",
            ":modifier:half:up",
            ":max",
            ":save",
            ":save:proficiency",
            ":save:misc",
        ] {
            s.insert(format!("{a}{suffix}"));
        }
    }
    for (k, _) in SKILLS {
        for suffix in ["", ":proficiency", ":misc", ":passive"] {
            s.insert(format!("{k}{suffix}"));
        }
    }
    for n in [
        "hp",
        "hp:starting",
        "hp:temp",
        "level",
        "level:half",
        "level:half:up",
        "level:quarter",
        "level:quarter:up",
        "proficiency",
        "proficiency:half",
        "proficiency:half:down",
        "proficiency:half:up",
        "ac",
        "speed",
        "speed:fly",
        "speed:climb",
        "speed:swim",
        "speed:burrow",
        "speed:misc",
        "speed:fly:misc",
        "speed:climb:misc",
        "speed:swim:misc",
        "speed:burrow:misc",
        "innate speed",
        "innate speed:fly",
        "innate speed:climb",
        "innate speed:swim",
        "innate speed:burrow",
        "innate speed:misc",
        "innate speed:fly:misc",
        "innate speed:climb:misc",
        "innate speed:swim:misc",
        "innate speed:burrow:misc",
        "initiative",
        "initiative:misc",
        "attunement:max",
        "spellcasting:attack",
        "spellcasting:dc",
        "spellcasting:known spells",
        "ac:calculation",
        "ac:shield",
        "ac:misc",
        "attunement:current",
        ABILITY_CHECKS_GROUP,
        CONCENTRATION_SAVE_GROUP,
    ] {
        s.insert(n.to_string());
    }
    for n in 1..=9 {
        s.insert(format!("spellcasting:slots:{n}"));
        s.insert(format!("multiclass:spellcasting:slots:{n}"));
    }
    s
}

struct Eval<'a, 'u> {
    inp: &'a StatInputs<'u>,
    by_group: HashMap<&'a str, Vec<&'u UStat>>,
    seeds: HashMap<&'a str, Vec<(&'a str, i32)>>,
    merges: HashMap<String, Vec<String>>,
    done: HashMap<String, StatGroup>,
    traces: HashMap<String, Vec<TraceStep>>,
    visiting: Vec<String>,
    diags: &'a mut DiagSink,
}

/// Ability maximum granted by Epic Boons unless the content says otherwise.
const DEFAULT_BOON_CAP: i32 = 30;
/// Reference depth limit of statistic evaluation. Bundled chains are a few links long; a
/// malformed chain thousands deep would otherwise overflow the stack.
const MAX_STAT_DEPTH: usize = 128;

fn ability_of(name: &str) -> Option<(usize, &str)> {
    for (i, a) in ABILITIES.iter().enumerate() {
        if let Some(rest) = name.strip_prefix(a) {
            if let Some(rest) = rest.strip_prefix(':') {
                return Some((i, rest));
            }
        }
    }
    None
}

impl<'a, 'u> Eval<'a, 'u> {
    fn new(inp: &'a StatInputs<'u>, diags: &'a mut DiagSink) -> Self {
        let mut by_group: HashMap<&str, Vec<&UStat>> = HashMap::new();
        for r in &inp.rules {
            by_group.entry(r.name.as_str()).or_default().push(r);
        }
        let mut seeds: HashMap<&str, Vec<(&str, i32)>> = HashMap::new();
        for (g, l, v) in &inp.seeds {
            seeds.entry(g.as_str()).or_default().push((l.as_str(), *v));
        }
        let mut merges: HashMap<String, Vec<String>> = HashMap::new();
        merges.insert(
            "ac".into(),
            vec![
                "ac:calculation".into(),
                "ac:shield".into(),
                "ac:misc".into(),
            ],
        );
        merges.insert(
            "initiative".into(),
            vec!["initiative:misc".into(), ABILITY_CHECKS_GROUP.into()],
        );
        for (k, _) in SKILLS {
            merges.insert(format!("{k}:misc"), vec![ABILITY_CHECKS_GROUP.into()]);
        }
        for (t, s) in &inp.merges {
            merges.entry(t.clone()).or_default().push(s.clone());
        }
        Self {
            inp,
            by_group,
            seeds,
            merges,
            done: HashMap::new(),
            traces: HashMap::new(),
            visiting: Vec::new(),
            diags,
        }
    }

    fn known(&self, name: &str) -> bool {
        self.by_group.contains_key(name)
            || self.seeds.contains_key(name)
            || self.inp.builtin.contains(name)
            || self.merges.contains_key(name)
            || self.synthetic(name).is_some()
    }

    /// Dependencies of a synthetic group and its kind.
    fn synthetic(&self, name: &str) -> Option<Synth> {
        if let Some((i, rest)) = ability_of(name) {
            return match rest {
                "score" => Some(Synth::Score(i)),
                "modifier" => Some(Synth::Mod(i, 1)),
                "modifier:half" => Some(Synth::Mod(i, 2)),
                "modifier:half:up" => Some(Synth::Mod(i, 3)),
                _ => None,
            };
        }
        match name {
            "proficiency:half" | "proficiency:half:down" => Some(Synth::ProfHalf(false)),
            "proficiency:half:up" => Some(Synth::ProfHalf(true)),
            _ => None,
        }
    }

    fn sum_of(&mut self, name: &str) -> i32 {
        self.eval(name);
        self.done.get(name).map_or(0, StatGroup::sum)
    }

    fn eval(&mut self, name: &str) {
        if self.done.contains_key(name) {
            return;
        }
        if self.visiting.iter().any(|v| v == name) {
            self.diags.push(
                Diagnostic::new(
                    code::STAT_CYCLE,
                    Severity::Error,
                    format!(
                        "statistic '{name}' depends on itself ({})",
                        self.visiting.join(" -> ")
                    ),
                )
                .at(name),
            );
            return;
        }
        if self.visiting.len() >= MAX_STAT_DEPTH {
            self.diags.push(
                Diagnostic::new(
                    code::STAT_CYCLE,
                    Severity::Error,
                    format!(
                        "statistic '{name}' sits more than {MAX_STAT_DEPTH} references deep; \
                         not evaluated"
                    ),
                )
                .at(name),
            );
            return;
        }
        self.visiting.push(name.to_string());
        let (group, steps) = self.compute(name);
        self.visiting.pop();
        self.done.insert(name.to_string(), group);
        if self.inp.trace {
            self.traces.insert(name.to_string(), steps);
        }
    }

    fn resolve(&mut self, v: &Val, owner: &str) -> Option<i32> {
        match v {
            Val::Lit(n) => Some(*n),
            Val::Ref { name, neg } => {
                if !self.known(name) {
                    self.diags.push(
                        Diagnostic::new(code::STAT_UNRESOLVED, Severity::Info, format!("statistic '{owner}' refers to unknown group '{name}'; rule skipped"))
                            .at(owner),
                    );
                    return None;
                }
                let s = self.sum_of(name);
                Some(if *neg { -s } else { s })
            }
        }
    }

    fn compute(&mut self, name: &str) -> (StatGroup, Vec<TraceStep>) {
        let trace = self.inp.trace;
        let mut steps: Vec<TraceStep> = Vec::new();
        let mut g = StatGroup::default();
        if let Some(seeds) = self.seeds.get(name) {
            for (l, v) in seeds.clone() {
                g.add(l, v);
                if trace {
                    steps.push(seed_step(name, l, v));
                }
            }
        }
        if let Some(s) = self.synthetic(name) {
            let v = self.compute_synth(s, name, &mut steps);
            g.add("Final", v);
        }
        let rules: Vec<&UStat> = self.by_group.get(name).cloned().unwrap_or_default();
        // bonus label -> (label, value, step index) candidates
        type Cand = (String, i32, Option<usize>);
        let mut partitions: Vec<(String, Vec<Cand>)> = Vec::new();
        for r in rules {
            let Some(mut v) = self.resolve(&r.value, name) else {
                if trace {
                    let mut st = rule_step(r, 0, false);
                    st.reason = Some("refers to an unknown statistic; rule skipped".into());
                    steps.push(st);
                }
                continue;
            };
            let mut note = None;
            if let Some(m) = &r.maximum {
                if let Some(mv) = self.resolve(m, name) {
                    if mv < v {
                        note = Some(format!("capped by maximum {mv} (rule gives {v})"));
                    }
                    v = v.min(mv);
                }
            }
            let idx = trace.then(|| {
                let mut st = rule_step(r, v, true);
                st.reason = note;
                if let Val::Ref { name: n, .. } = &r.value {
                    st.inputs = self.traces.get(n).cloned().unwrap_or_default();
                }
                steps.push(st);
                steps.len() - 1
            });
            match &r.bonus {
                None => g.add(&r.label, v),
                Some(b) => match partitions.iter_mut().find(|p| &p.0 == b) {
                    Some(p) => p.1.push((r.label.clone(), v, idx)),
                    None => partitions.push((b.clone(), vec![(r.label.clone(), v, idx)])),
                },
            }
        }
        if trace {
            let skipped: Vec<(&UStat, String)> = self
                .inp
                .skipped
                .iter()
                .filter(|(r, _)| r.name == name)
                .map(|(r, why)| (*r, why.clone()))
                .collect();
            for (r, why) in skipped {
                let v = match &r.value {
                    Val::Lit(n) => *n,
                    Val::Ref { name: n, neg } if self.known(n) => {
                        let s = self.sum_of(n);
                        if *neg {
                            -s
                        } else {
                            s
                        }
                    }
                    Val::Ref { .. } => 0,
                };
                let mut st = rule_step(r, v, false);
                st.reason = Some(why);
                steps.push(st);
            }
        }
        for (bonus, items) in partitions {
            if let [(l, v, _)] = items.as_slice() {
                g.add(l, *v);
                continue;
            }
            let best = items.iter().map(|i| i.1).max().unwrap_or(0);
            if best < 0 {
                self.diags.push(
                    Diagnostic::new(code::LEGACY_BONUS_FLOOR, Severity::Info, format!("all stacking candidates of '{name}' are negative; legacy files may use 0 here, using {best}"))
                        .at(name),
                );
            }
            let labels: Vec<&str> = items
                .iter()
                .filter(|i| i.1 == best)
                .map(|i| i.0.as_str())
                .collect();
            g.add(&labels.join(" | "), best);
            if trace {
                let winner = items.iter().find(|i| i.1 == best).map(|i| i.0.clone());
                let mut seen_winner = false;
                for (label, v, idx) in &items {
                    let Some(i) = idx else { continue };
                    if *v == best && !seen_winner {
                        seen_winner = true;
                        continue;
                    }
                    let st = &mut steps[*i];
                    st.included = false;
                    st.reason = Some(if *v == best {
                        format!(
                            "bonus '{bonus}': ties with {} at {best}; counted once",
                            winner.as_deref().unwrap_or("another rule")
                        )
                    } else {
                        format!(
                            "bonus '{bonus}': only the highest value counts ({best} from {}), {label} gives {v}",
                            winner.as_deref().unwrap_or("another rule")
                        )
                    });
                }
            }
        }
        if let Some(sources) = self.merges.get(name).cloned() {
            for src in sources {
                self.eval(&src);
                let Some(sg) = self.done.get(&src).cloned() else {
                    continue;
                };
                let total = sg.sum();
                if trace {
                    if let Some(sub) = self.traces.get(&src) {
                        steps.extend(sub.iter().cloned());
                    }
                }
                if total == 0 {
                    continue;
                }
                if total < 0 {
                    self.diags.push(
                        Diagnostic::new(code::LEGACY_NONPOSITIVE_MERGE, Severity::Info, format!("'{src}' totals {total}; merged into '{name}' (legacy files may drop non-positive groups)"))
                            .at(name),
                    );
                    if src == "initiative:misc" {
                        self.diags.push(
                            Diagnostic::new(
                                code::LEGACY_INITIATIVE_MISC,
                                Severity::Info,
                                "initiative:misc is merged",
                            )
                            .at(name),
                        );
                    }
                } else if src == "initiative:misc" {
                    self.diags.push(
                        Diagnostic::new(
                            code::LEGACY_INITIATIVE_MISC,
                            Severity::Info,
                            "initiative:misc is merged",
                        )
                        .at(name),
                    );
                }
                for (l, v) in sg.contribs {
                    g.add(&l, v);
                }
            }
        }
        (g, steps)
    }

    fn compute_synth(&mut self, s: Synth, name: &str, steps: &mut Vec<TraceStep>) -> i32 {
        let trace = self.inp.trace;
        match s {
            Synth::Score(i) => {
                let a = ABILITIES[i];
                let add = self.sum_of(a);
                let ov = self.sum_of(&format!("{a}:score:set"));
                let max_name = format!("{a}:max");
                let max_sum = self.sum_of(&max_name);
                let has_max = self
                    .done
                    .get(&max_name)
                    .is_some_and(|g| !g.contribs.is_empty());
                let max = if has_max { max_sum } else { 20 };
                let total = self.inp.base_scores[i] + add;
                let (v, adjust) = if ov > total {
                    (
                        ov,
                        Some(("Score override", format!("set to {ov} by {a}:score:set"))),
                    )
                } else if self.inp.use_ability_max && total > max {
                    (
                        max,
                        Some((
                            "Ability maximum",
                            format!("capped at {max} (total was {total})"),
                        )),
                    )
                } else {
                    (total, None)
                };
                // Epic Boon style increases (`<a>:boon`, cap `<a>:boon:cap`, default 30) come after
                // the ordinary maximum so they never revive increases lost to it.
                let boon = self.sum_of(&format!("{a}:boon"));
                let boon_cap = {
                    let cap_name = format!("{a}:boon:cap");
                    let cap = self.sum_of(&cap_name);
                    if self
                        .done
                        .get(&cap_name)
                        .is_some_and(|g| !g.contribs.is_empty())
                    {
                        cap
                    } else {
                        DEFAULT_BOON_CAP
                    }
                };
                let boosted = if boon == 0 {
                    v
                } else if self.inp.use_ability_max {
                    (v + boon).min(boon_cap.max(v))
                } else {
                    v + boon
                };
                if trace {
                    steps.push(seed_step(name, "Base score", self.inp.base_scores[i]));
                    steps.extend(self.traces.get(a).cloned().unwrap_or_default());
                    if let Some((label, why)) = adjust {
                        let mut st = derived_step(name, label, v - total);
                        st.reason = Some(why);
                        steps.push(st);
                    }
                    if boon != 0 {
                        steps.extend(
                            self.traces
                                .get(&format!("{a}:boon"))
                                .cloned()
                                .unwrap_or_default(),
                        );
                        if boosted != v + boon {
                            let mut st = derived_step(name, "Boon maximum", boosted - (v + boon));
                            st.reason = Some(format!("capped at {boon_cap}"));
                            steps.push(st);
                        }
                    }
                }
                boosted
            }
            Synth::Mod(i, kind) => {
                let score = self.sum_of(&format!("{}:score", ABILITIES[i]));
                let m = modifier(score);
                let (v, what) = match kind {
                    1 => (m, "modifier"),
                    2 => (m / 2, "half the modifier, rounded down"),
                    _ => (m / 2 + m % 2, "half the modifier, rounded up"),
                };
                if trace {
                    let mut st = derived_step(name, "Ability modifier table", v);
                    st.reason = Some(format!("{what} of {} score {score}", ABILITIES[i]));
                    steps.push(st);
                }
                v
            }
            Synth::ProfHalf(up) => {
                let p = self.sum_of("proficiency");
                let v = if up { p / 2 + p % 2 } else { p / 2 };
                if trace {
                    let mut st = derived_step(name, "Proficiency bonus", v);
                    st.reason = Some(format!(
                        "half of proficiency {p}, rounded {}",
                        if up { "up" } else { "down" }
                    ));
                    steps.push(st);
                }
                v
            }
        }
    }
}

fn seed_step(group: &str, label: &str, value: i32) -> TraceStep {
    TraceStep {
        group: group.to_string(),
        kind: StepKind::Seed,
        label: label.to_string(),
        element_id: None,
        element_name: None,
        rule_kind: "seed".into(),
        value,
        raw_value: None,
        bonus: None,
        condition: None,
        included: true,
        reason: None,
        inputs: Vec::new(),
    }
}

fn derived_step(group: &str, label: &str, value: i32) -> TraceStep {
    TraceStep {
        kind: StepKind::Derived,
        rule_kind: "derived".into(),
        ..seed_step(group, label, value)
    }
}

fn rule_step(r: &UStat, value: i32, included: bool) -> TraceStep {
    TraceStep {
        group: r.name.clone(),
        kind: StepKind::Rule,
        label: r.label.clone(),
        element_id: Some(r.owner_id.clone()),
        element_name: Some(r.owner_name.clone()),
        rule_kind: "stat".into(),
        value,
        raw_value: Some(r.value_text.clone()),
        bonus: r.bonus.clone(),
        condition: r.condition_text(),
        included,
        reason: None,
        inputs: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy)]
enum Synth {
    Score(usize),
    Mod(usize, u8),
    ProfHalf(bool),
}

/// Evaluate every group.
#[must_use]
pub fn evaluate_all(inp: &StatInputs, diags: &mut DiagSink) -> StatSet {
    let mut ev = Eval::new(inp, diags);
    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |n: &str| {
        if seen.insert(n.to_string()) {
            names.push(n.to_string());
        }
    };
    for (g, _, _) in &inp.seeds {
        push(g);
    }
    for r in &inp.rules {
        push(&r.name);
    }
    for n in &inp.builtin {
        push(n);
    }
    for (t, s) in &inp.merges {
        push(t);
        push(s);
    }
    for n in ["ac", "initiative"] {
        push(n);
    }
    for n in &names {
        ev.eval(n);
    }
    simulate_legacy_queue(inp, ev.diags);
    StatSet {
        groups: ev.done,
        traces: ev.traces,
    }
}

/// Evaluate only the ability groups (used for the ability/requirement convergence loop).
#[must_use]
pub fn evaluate_abilities(inp: &StatInputs) -> [i32; 6] {
    let mut sink = DiagSink::default();
    let mut ev = Eval::new(inp, &mut sink);
    let mut out = [0; 6];
    for (i, a) in ABILITIES.iter().enumerate() {
        out[i] = ev.sum_of(&format!("{a}:score"));
    }
    out
}

/// Names of groups that exist at a level: used for `$(spellcasting:slots)`.
#[must_use]
pub fn rule_group_names<'u>(rules: &[&'u UStat]) -> HashSet<&'u str> {
    rules.iter().map(|r| r.name.as_str()).collect()
}

/// Report statistic groups whose chained references legacy files may compute differently
/// (`legacy.stat_chain_depth`). Informational only.
fn simulate_legacy_queue(inp: &StatInputs, diags: &mut DiagSink) {
    let mut order: Vec<&str> = Vec::new();
    let mut refs: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in &inp.rules {
        // literal, bonus-free rules are evaluated first for legacy compatibility; only the rest
        // takes part in the queue.
        let plain = matches!(r.value, Val::Lit(_)) && r.bonus.is_none() && r.maximum.is_none();
        if plain {
            continue;
        }
        if !refs.contains_key(r.name.as_str()) {
            order.push(r.name.as_str());
        }
        let e = refs.entry(r.name.as_str()).or_default();
        if let Val::Ref { name, .. } = &r.value {
            e.push(name.as_str());
        }
        if let Some(Val::Ref { name, .. }) = &r.maximum {
            e.push(name.as_str());
        }
    }
    let mut pending: HashSet<&str> = order.iter().copied().collect();
    let mut queue: std::collections::VecDeque<(&str, bool)> =
        order.iter().map(|g| (*g, false)).collect();
    let mut deferred: Vec<&str> = Vec::new();
    while let Some((g, retried)) = queue.pop_front() {
        let ready = refs[g].iter().all(|r| !pending.contains(r) || *r == g);
        if ready {
            pending.remove(g);
        } else if !retried {
            queue.push_back((g, true));
        } else {
            deferred.push(g);
        }
    }
    // a second, forcing call resolves what the first deferred; only report chains that stay stuck
    // behind other deferred groups
    let stuck: Vec<&str> = deferred
        .iter()
        .copied()
        .filter(|g| refs[g].iter().any(|r| deferred.contains(r) && r != g))
        .collect();
    if !stuck.is_empty() {
        diags.push(Diagnostic::new(
            code::LEGACY_STAT_CHAIN_DEPTH,
            Severity::Info,
            format!(
                "legacy files may compute these statistics differently: {}",
                stuck.join(", ")
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universe::{Cond, Val};

    fn rule(name: &str, value: &str) -> UStat {
        UStat {
            name: name.into(),
            value: Val::parse(value),
            maximum: None,
            bonus: None,
            level: 1,
            label: name.into(),
            inline: false,
            req: Cond::None,
            equipped: Cond::None,
            owner_id: "ID_T".into(),
            owner_name: "Test".into(),
            value_text: value.into(),
            req_text: None,
            equipped_text: None,
        }
    }

    fn run(rules: &[UStat], base: [i32; 6]) -> (StatSet, Vec<Diagnostic>) {
        let inp = StatInputs {
            rules: rules.iter().collect(),
            seeds: vec![],
            builtin: builtin_groups(),
            base_scores: base,
            use_ability_max: true,
            merges: vec![],
            trace: false,
            skipped: vec![],
        };
        let mut d = DiagSink::default();
        let s = evaluate_all(&inp, &mut d);
        (s, d.into_vec())
    }

    #[test]
    fn tv30_stat_rule_combination_with_e4() {
        let mut rs = vec![rule("g", "2"), rule("g", "3")];
        for (v, b) in [
            ("4", "x"),
            ("6", "x"),
            ("1", "z"),
            ("-2", "z"),
            ("-5", "w"),
            ("-2", "v"),
            ("-3", "v"),
        ] {
            let mut r = rule("g", v);
            r.bonus = Some(b.into());
            rs.push(r);
        }
        let (s, d) = run(&rs, [10; 6]);
        // legacy value was 7 (the 'v' pair floored at 0); the real max -2 is used
        assert_eq!(s.sum("g"), 5 + 6 + 1 - 5 - 2);
        assert!(d.iter().any(|x| x.code == code::LEGACY_BONUS_FLOOR));
    }

    #[test]
    fn tv31_maximum_and_negated_reference() {
        let mut a = rule("d", "dexterity:modifier");
        a.maximum = Some(Val::Lit(2));
        let mut cap = rule("ac:armored:dexterity:cap", "2");
        cap.bonus = Some("base".into());
        let mut b = rule("d", "dexterity:modifier");
        b.maximum = Some(Val::parse("ac:armored:dexterity:cap"));
        let mut cap2 = rule("ac:armored:dexterity:cap", "3");
        cap2.bonus = Some("base".into());
        let strength = rule("dexterity", "8"); // 18 -> +4
        let count = rule("warlock:spellcasting:slots:count", "2");
        let neg = rule("w", "-warlock:spellcasting:slots:count");
        let (s, _) = run(
            &[
                a.clone(),
                cap.clone(),
                strength.clone(),
                count.clone(),
                neg.clone(),
            ],
            [10, 10, 10, 10, 10, 10],
        );
        assert_eq!(s.sum("d"), 2);
        assert_eq!(s.sum("w"), -2);
        let (s, _) = run(&[b, cap, cap2, strength], [10; 6]);
        assert_eq!(
            s.sum("d"),
            3,
            "cap group takes the winner of the base partition"
        );
    }

    #[test]
    fn tv32_dependency_order_and_cycle() {
        // consumer listed before its producers
        let rs = vec![
            rule("ac:calculation", "ac:armored"),
            rule("ac:armored", "ac:armored:armor"),
            rule("ac:armored:armor", "14"),
        ];
        let (s, _) = run(&rs, [10; 6]);
        assert_eq!(s.sum("ac:calculation"), 14);
        assert_eq!(s.sum("ac"), 14);
        let cyc = vec![rule("x", "y"), rule("y", "x")];
        let (_, d) = run(&cyc, [10; 6]);
        assert!(d.iter().any(|x| x.code == code::STAT_CYCLE));
    }

    #[test]
    fn tv33_merges_include_non_positive_totals() {
        let calc = rule("ac:calculation", "12");
        let curse = rule("ac:misc", "-1");
        let (s, d) = run(&[calc.clone(), curse], [10; 6]);
        assert_eq!(s.sum("ac"), 11, "The cursed ring counts");
        assert!(d.iter().any(|x| x.code == code::LEGACY_NONPOSITIVE_MERGE));
        let (s, _) = run(
            &[calc, rule("ac:misc", "2"), rule("ac:misc", "-1")],
            [10; 6],
        );
        assert_eq!(s.sum("ac"), 13);
    }

    #[test]
    fn initiative_misc_is_merged() {
        let (s, _) = run(
            &[rule("initiative", "2"), rule("initiative:misc", "3")],
            [10; 6],
        );
        assert_eq!(s.sum("initiative"), 5, "initiative:misc is merged");
    }

    #[test]
    fn ability_cap_override_and_modifiers() {
        let mut max = rule("strength:max", "20");
        max.label = "Base".into();
        let rs = vec![max.clone(), rule("strength", "3")];
        let (s, _) = run(&rs, [19, 10, 10, 10, 10, 10]);
        assert_eq!(s.sum("strength:score"), 20, "22 capped at 20");
        assert_eq!(s.sum("strength:modifier"), 5);
        let mut set = rule("strength:score:set", "25");
        set.bonus = Some("base".into());
        let mut set2 = rule("strength:score:set", "19");
        set2.bonus = Some("base".into());
        let (s, _) = run(&[max, set, set2], [16, 10, 10, 10, 10, 10]);
        assert_eq!(
            s.sum("strength:score"),
            25,
            "override ignores the cap, max of overrides"
        );
        assert_eq!(s.sum("strength:modifier"), 7);
    }

    #[test]
    fn tv21_modifier_table_and_halves() {
        let table = [
            (1, -5),
            (3, -4),
            (8, -1),
            (9, -1),
            (10, 0),
            (15, 2),
            (20, 5),
            (24, 7),
            (30, 10),
        ];
        for (s, m) in table {
            assert_eq!(modifier(s), m);
        }
        // half / half-up of modifiers: +3 -> 1/2, -1 -> 0/-1, -3 -> -1/-2
        for (m, half, up) in [
            (3, 1, 2),
            (2, 1, 1),
            (1, 0, 1),
            (0, 0, 0),
            (-1, 0, -1),
            (-2, -1, -1),
            (-3, -1, -2),
        ] {
            assert_eq!(m / 2, half);
            assert_eq!(m / 2 + m % 2, up);
        }
        let (s, _) = run(&[rule("proficiency", "3")], [10; 6]);
        assert_eq!(
            (s.sum("proficiency:half"), s.sum("proficiency:half:up")),
            (1, 2)
        );
    }

    #[test]
    fn unresolved_reference_is_skipped_with_note() {
        let (s, d) = run(&[rule("a", "nosuch:group"), rule("a", "2")], [10; 6]);
        assert_eq!(s.sum("a"), 2);
        assert!(d.iter().any(|x| x.code == code::STAT_UNRESOLVED));
    }
}
