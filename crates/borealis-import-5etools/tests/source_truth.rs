//! Source-field truth for origins and feats (acceptance step 3b, specs/acceptance-imports-1.md):
//! for every 5etools race / species, subrace, background and feat of a real `data/` folder
//! (F-full), what the engine computes on a host character against what the entry's fields say.
//! The items version is `src-tauri/tests/fivetools_items.rs`; this follows its pattern (raw
//! entries resolved as the importer does, matched to their elements by name and source, a
//! tally of checked / correct / wrong / not modelled per category, gate `OUTCOME` lines).
//!
//! Checked fields (each against the host before and after the pick, see `support/truth_hosts.rs`):
//! `ability` (fixed increases applied; `choose` / `weighted` forms offered as a choice whose
//! reachable allocations equal the source's; the MPMM `lineage: "VRGR"` rule), `skillProficiencies`,
//! `toolProficiencies`, `languageProficiencies` (granted, or a choice of the right size from the
//! right list), `speed` (every mode), `resist` / `immune` / `conditionImmune` (and resistance
//! choices), `additionalSpells` (`known` / `innate` / `prepared` spells present from their level and
//! not before it, limited uses, spell choices of the right count, named alternatives offered).
//! Not checked: `expanded` (grants no spell, it extends a class list) and the fields the spec does
//! not name (`feats`, `skillToolLanguageProficiencies`, weapon / armor proficiencies).
//!
//! Local-only: `BOREALIS_5ETOOLS_DIR` (skipped otherwise; `BOREALIS_ACCEPT_STRICT=1` fails on a
//! skip). Ids and numbers only. Pass: 0 wrong; "not modelled" only in [`NOT_MODELLED`]; field
//! shapes the audit cannot read only in [`UNREAD_OK`]. Run in release mode:
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis-import-5etools \
//!     --test source_truth -- --nocapture
//! ```

#[path = "../../../tools/regression-gate/outcome.rs"]
mod outcome;
#[path = "../../../src-tauri/tests/support/prereq.rs"]
mod prereq;
mod support;
#[path = "support/truth_hosts.rs"]
mod truth_hosts;
#[path = "support/truth_raw.rs"]
mod truth_raw;

use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::{commands, CharacterSnapshot, CharacterState, Choice, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use support::core_pack;
use truth_hosts::{Kit, Picked};
use truth_raw::{Alloc, AllocSet, Entry, Expect, Grant, Kind, SpellRef, Uses, ABILITY_NAMES};

const SUITE: &str = "source_truth";

/// Categories that may report "not modelled", with the reason. Anything else not modelled is a
/// failure.
const NOT_MODELLED: &[(&str, &str)] = &[
    (
        "tool (granted)",
        "a tool the core proficiency set has no element for (`vehicles (space)`, `vehicles (air)`): \
         the importer cannot grant it and notes the gap; adding it is content/core's call",
    ),
    (
        "host",
        "the element is not offered on any test host (a feat whose prerequisites no host meets); \
         offering is acceptance stage 2's check, its fields stay unverified here",
    ),
    (
        "spell alternatives offered (filter-only group)",
        "a named additionalSpells group made only of `choose` filters is not offered: the importer \
         drops a group whose filter matches no spell (PHB Spell Sniper: classes without an \
         attack cantrip); the audit does not evaluate 5etools spell filters",
    ),
];

/// Field shapes the audit does not read, with the reason (prefix match on the shape text).
const UNREAD_OK: &[(&str, &str)] = &[
    (
        "additionalSpells prepared key s",
        "spell-level keys (`s1`..`s9`: once the character can cast spells of that level) need a \
         caster host of the right level; the fighter hosts never reach them",
    ),
    (
        "additionalSpells: unnamed filter-only alternatives",
        "unnamed alternative groups made only of `choose` filters: nothing locates their option \
         (the importer names them `Option <n>`)",
    ),
];

/// Wrong values found by the corpus run, classified (Borealis bugs, owner named). Printed next
/// to the counts; the test still fails while any wrong value remains. Empty since det-5e-5
/// (2026-10-18): the lineage rule, distinct multi-pick increases, `pb` uses, alternative tool
/// arrays and tool category keys are imported; core-less tools are NOT_MODELLED.
const FINDINGS: &[(&str, &str)] = &[];

// ------------------------------------------------------------------------------------ tally

enum Verdict {
    Correct,
    Wrong(String),
    NotModelled(String),
}

fn verdict(ok: bool, why: impl FnOnce() -> String) -> Verdict {
    if ok {
        Verdict::Correct
    } else {
        Verdict::Wrong(why())
    }
}

#[derive(Default)]
struct Cat {
    checked: usize,
    correct: usize,
    wrong: Vec<(String, String)>,
    not_modelled: Vec<(String, String)>,
}

#[derive(Default)]
struct Tally {
    cats: BTreeMap<String, Cat>,
    unread: BTreeMap<String, Vec<String>>,
    panics: Vec<String>,
}

impl Tally {
    fn record(&mut self, cat: &str, id: &str, v: Verdict) {
        let (pass, why) = match &v {
            Verdict::Correct => (true, String::new()),
            Verdict::Wrong(w) => (false, w.clone()),
            Verdict::NotModelled(w) => (false, format!("not modelled: {w}")),
        };
        outcome::record(SUITE, &format!("{cat}/{id}"), pass, &why);
        let c = self.cats.entry(cat.to_string()).or_default();
        c.checked += 1;
        match v {
            Verdict::Correct => c.correct += 1,
            Verdict::Wrong(w) => c.wrong.push((id.to_string(), w)),
            Verdict::NotModelled(w) => c.not_modelled.push((id.to_string(), w)),
        }
    }

    fn print(&self) {
        println!("--- checked / correct / wrong / not modelled  category");
        for (cat, c) in &self.cats {
            println!(
                "{:>6} {:>6} {:>6} {:>6}  {cat}",
                c.checked,
                c.correct,
                c.wrong.len(),
                c.not_modelled.len()
            );
        }
        for (cat, c) in &self.cats {
            let mut why: BTreeMap<String, usize> = BTreeMap::new();
            for (_, w) in c.wrong.iter().chain(&c.not_modelled) {
                let key: String = w
                    .chars()
                    .map(|ch| if ch.is_ascii_digit() { '#' } else { ch })
                    .take(110)
                    .collect();
                *why.entry(key).or_default() += 1;
            }
            if why.is_empty() {
                continue;
            }
            println!("--- {cat}: causes");
            for (w, n) in &why {
                println!("    {n:>5}x {w}");
            }
            let limit = std::env::var("BOREALIS_TRUTH_SHOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12);
            for (id, w) in c.wrong.iter().take(limit) {
                println!("    WRONG {id}: {w}");
            }
        }
        for (shape, ids) in &self.unread {
            println!(
                "--- unread shape {shape}: {} e.g. {:?}",
                ids.len(),
                &ids[..ids.len().min(4)]
            );
        }
        println!("panics: {} {:?}", self.panics.len(), self.panics);
        for (cat, why) in FINDINGS {
            if self.cats.get(*cat).is_some_and(|c| !c.wrong.is_empty()) {
                println!("  FINDING {cat}: {why}");
            }
        }
    }

    /// Failure messages of a full run.
    fn failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (cat, c) in &self.cats {
            if !c.wrong.is_empty() {
                out.push(format!("{cat}: {} wrong", c.wrong.len()));
            }
            if !c.not_modelled.is_empty() && !NOT_MODELLED.iter().any(|(k, _)| k == cat) {
                out.push(format!(
                    "{cat}: {} not modelled (not a listed category)",
                    c.not_modelled.len()
                ));
            }
        }
        for (shape, ids) in &self.unread {
            if !UNREAD_OK.iter().any(|(k, _)| shape.starts_with(k)) {
                out.push(format!("unread shape {shape:?} ({} entries)", ids.len()));
            }
        }
        if !self.panics.is_empty() {
            out.push(format!("{} panics", self.panics.len()));
        }
        out
    }
}

// --------------------------------------------------------------------- engine-side readers

/// New open selects of the pick, grouped by (owner, name, type).
struct Group<'a> {
    ty: &'a str,
    slots: Vec<&'a SlotSnapshot>,
}

impl Group<'_> {
    fn option_names(&self) -> BTreeSet<String> {
        self.slots
            .iter()
            .flat_map(|s| s.options.iter().map(|o| norm(&o.name)))
            .collect()
    }
}

fn norm(s: &str) -> String {
    let l = s.to_lowercase().replace(['\u{2019}', '\u{2018}'], "'");
    let inner = l
        .strip_prefix("tool proficiency (")
        .and_then(|x| x.strip_suffix(')'))
        .map(str::to_string);
    inner.unwrap_or(l).trim().to_string()
}

fn new_groups<'a>(before: &CharacterSnapshot, after: &'a CharacterSnapshot) -> Vec<Group<'a>> {
    let old: BTreeSet<_> = before.slots.iter().map(|s| &s.key).collect();
    let mut by: BTreeMap<(&str, &str, &str), Vec<&SlotSnapshot>> = BTreeMap::new();
    for s in &after.slots {
        if s.chosen.is_none() && s.list_choice.is_none() && !old.contains(&s.key) {
            by.entry((s.owner.id.as_str(), s.name.as_str(), s.ty.as_str()))
                .or_default()
                .push(s);
        }
    }
    by.into_iter()
        .map(|((_, _, ty), slots)| Group { ty, slots })
        .collect()
}

fn ability_of(name: &str) -> Option<usize> {
    let n = name.trim();
    let inner = n
        .strip_prefix("Ability Score Increase (")
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(n);
    ABILITY_NAMES.iter().position(|a| *a == inner)
}

/// `Strength +2, Dexterity +1` (the engine's ability list items).
fn parse_alloc(text: &str) -> Option<Alloc> {
    let mut out = Vec::new();
    for part in text.split(", ") {
        let (name, n) = part.rsplit_once(' ')?;
        out.push((ability_of(name)?, n.trim_start_matches('+').parse().ok()?));
    }
    out.sort_unstable();
    Some(out)
}

struct Cx<'a> {
    kit: &'a Kit<'a>,
    engine: &'a Engine,
    /// dialect -> its language (a dialect is granted as that language)
    dialects: HashMap<String, String>,
}

impl Cx<'_> {
    /// The allocations every new ability choice of the pick can reach.
    fn ability_sets(&self, after: &CharacterState, groups: &[Group<'_>]) -> Vec<AllocSet> {
        let mut out = Vec::new();
        for g in groups {
            if g.ty == "Ability Score Improvement" {
                let pool: Vec<(usize, String)> = g.slots[0]
                    .options
                    .iter()
                    .filter_map(|o| ability_of(&o.name).map(|a| (a, o.id.clone())))
                    .collect();
                let n = g.slots.len();
                let dup = n >= 2
                    && pool
                        .first()
                        .is_some_and(|(_, id)| self.takes_twice(after, g, id));
                let idx: Vec<usize> = pool.iter().map(|(a, _)| *a).collect();
                out.push(if dup {
                    with_repeats(&idx, n)
                } else {
                    truth_raw::choose_set(&idx, n, 1)
                });
                continue;
            }
            if let Some(set) = self.option_allocs(after, g) {
                out.push(set);
                continue;
            }
            for s in &g.slots {
                let items: Option<AllocSet> = (!s.list_items.is_empty())
                    .then(|| {
                        s.list_items
                            .iter()
                            .map(|(_, t)| parse_alloc(t))
                            .collect::<Option<AllocSet>>()
                    })
                    .flatten();
                if let Some(set) = items {
                    out.push(set);
                }
            }
        }
        out
    }

    /// A one-pick select whose every option is named after an ability (`Resilient (Strength)`):
    /// each option chosen in turn, its allocation read from the scores it changes.
    fn option_allocs(&self, after: &CharacterState, g: &Group<'_>) -> Option<AllocSet> {
        let [slot] = g.slots.as_slice() else {
            return None;
        };
        let by_name = |n: &str| {
            let inner = n.rsplit_once('(')?.1.strip_suffix(')')?;
            ABILITY_NAMES.iter().position(|a| *a == inner)
        };
        if slot.options.is_empty() || !slot.options.iter().all(|o| by_name(&o.name).is_some()) {
            return None;
        }
        let base = self.engine.derive(after);
        let mut set = AllocSet::new();
        for o in &slot.options {
            let next =
                commands::select(self.engine, after, &slot.key, Choice::Element(o.id.clone()))
                    .ok()?;
            let snap = self.engine.derive(&next.state);
            let alloc: Alloc = (0..6)
                .filter_map(|i| {
                    let d = snap.abilities[i].score - base.abilities[i].score;
                    (d != 0).then_some((i, d))
                })
                .collect();
            set.insert(alloc);
        }
        Some(set)
    }

    /// The first two slots of `g` both accept option `id`.
    fn takes_twice(&self, after: &CharacterState, g: &Group<'_>, id: &str) -> bool {
        let Ok(one) = commands::select(
            self.engine,
            after,
            &g.slots[0].key,
            Choice::Element(id.into()),
        ) else {
            return false;
        };
        commands::select(
            self.engine,
            &one.state,
            &g.slots[1].key,
            Choice::Element(id.into()),
        )
        .is_ok()
    }
}

/// `n` picks of the pool, the same ability allowed more than once.
fn with_repeats(pool: &[usize], n: usize) -> AllocSet {
    fn rec(pool: &[usize], n: usize, start: usize, cur: &mut Vec<usize>, out: &mut AllocSet) {
        if cur.len() == n {
            let mut m: BTreeMap<usize, i32> = BTreeMap::new();
            for a in cur.iter() {
                *m.entry(*a).or_default() += 1;
            }
            out.insert(m.into_iter().collect());
            return;
        }
        for i in start..pool.len() {
            cur.push(pool[i]);
            rec(pool, n, i, cur, out);
            cur.pop();
        }
    }
    let mut out = AllocSet::new();
    rec(pool, n, 0, &mut Vec::new(), &mut out);
    out
}

fn show(set: &AllocSet) -> String {
    let mut v: Vec<String> = set
        .iter()
        .take(4)
        .map(|a| {
            a.iter()
                .map(|(i, n)| format!("{}{n:+}", &ABILITY_NAMES[*i][..3]))
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    if set.len() > 4 {
        v.push(format!("... ({})", set.len()));
    }
    v.join(" ")
}

/// Spell names the character has (spellcasting lists and free casts), lower case.
fn spell_names(s: &CharacterSnapshot) -> BTreeSet<String> {
    s.spellcasting
        .iter()
        .flat_map(|c| c.spells.iter().map(|x| x.name.to_lowercase()))
        .chain(s.free_casts.iter().map(|f| f.name.to_lowercase()))
        .collect()
}

fn open_spell_slots(before: &CharacterSnapshot, after: &CharacterSnapshot) -> usize {
    new_groups(before, after)
        .iter()
        .filter(|g| g.ty == "Spell")
        .map(|g| g.slots.len())
        .sum()
}

// ----------------------------------------------------------------------------------- checks

struct Audit<'a> {
    cx: Cx<'a>,
    t: Tally,
}

impl Audit<'_> {
    fn entry(&mut self, e: &Entry, picked: &Picked) {
        let id = e.id.as_str();
        let (b, a) = (
            self.cx.engine.derive(&picked.before),
            self.cx.engine.derive(&picked.after),
        );
        let groups = new_groups(&b, &a);
        let x = &e.expect;
        self.abilities(id, x, &b, &a, &picked.after, &groups);
        self.proficiencies(id, x, &b, &a, &groups);
        self.speed(id, e, x, &a);
        self.defences(id, x, &a, &groups);
        self.spells(id, e, x, picked);
    }

    fn abilities(
        &mut self,
        id: &str,
        x: &Expect,
        b: &CharacterSnapshot,
        a: &CharacterSnapshot,
        after: &CharacterState,
        groups: &[Group<'_>],
    ) {
        let delta: Vec<i32> = (0..6)
            .map(|i| a.abilities[i].score - b.abilities[i].score)
            .collect();
        if x.fixed != [0; 6] || delta.iter().any(|d| *d != 0) {
            self.t.record(
                "ability (fixed)",
                id,
                verdict(delta == x.fixed, || {
                    format!("applied {delta:?}, source {:?}", x.fixed)
                }),
            );
        }
        let mut engine = self.cx.ability_sets(after, groups);
        for want in &x.ability_choices {
            let hit = engine.iter().position(|s| s == want);
            let v = match hit {
                Some(i) => {
                    engine.remove(i);
                    Verdict::Correct
                }
                None => Verdict::Wrong(format!(
                    "source {} ; offered {}",
                    show(want),
                    engine.iter().map(show).collect::<Vec<_>>().join(" | ")
                )),
            };
            self.t.record("ability (choice)", id, v);
        }
        for extra in engine {
            self.t.record(
                "ability (choice)",
                id,
                Verdict::Wrong(format!("choice the source does not give: {}", show(&extra))),
            );
        }
    }

    fn proficiencies(
        &mut self,
        id: &str,
        x: &Expect,
        b: &CharacterSnapshot,
        a: &CharacterSnapshot,
        groups: &[Group<'_>],
    ) {
        let skill_names: BTreeSet<String> =
            a.skills.iter().map(|s| s.name.to_lowercase()).collect();
        for s in &x.skills {
            let got = a
                .skills
                .iter()
                .any(|k| k.name.to_lowercase() == *s && k.proficient);
            self.t.record(
                "skill (granted)",
                id,
                verdict(got, || format!("{s} not proficient")),
            );
        }
        let held = |ty: &str| -> BTreeSet<String> {
            a.elements
                .iter()
                .filter(|r| r.element.ty == ty)
                .map(|r| norm(&r.element.name))
                .collect()
        };
        let (tools, langs) = (held("Proficiency"), held("Language"));
        for s in &x.tools {
            let v = if !tools.contains(s)
                && borealis_import_5etools::core_ids::tool_prof_id(s).is_none()
            {
                Verdict::NotModelled(format!("{s} has no core proficiency element"))
            } else {
                verdict(tools.contains(s), || format!("{s} not granted"))
            };
            self.t.record("tool (granted)", id, v);
        }
        for s in &x.languages {
            let got =
                langs.contains(s) || self.cx.dialects.get(s).is_some_and(|p| langs.contains(p));
            self.t.record(
                "language (granted)",
                id,
                verdict(got, || format!("{s} not granted")),
            );
        }
        let kind_of = |g: &Group<'_>| -> Option<Kind> {
            match g.ty {
                "Language" => Some(Kind::Language),
                "Proficiency" => {
                    let names = g.option_names();
                    if names.is_empty() {
                        None
                    } else if names.iter().all(|n| skill_names.contains(n)) {
                        Some(Kind::Skill)
                    } else {
                        Some(Kind::Tool)
                    }
                }
                _ => None,
            }
        };
        self.alternatives(id, x, b, a, groups);
        let mut used = vec![false; groups.len()];
        for c in &x.choices {
            let cat = match c.kind {
                Kind::Skill => "skill (choice)",
                Kind::Tool => "tool (choice)",
                Kind::Language => "language (choice)",
            };
            let fits = |g: &Group<'_>| {
                let k = kind_of(g);
                let kind_ok = k == Some(c.kind)
                    || (k.is_none() && g.ty == "Proficiency" && c.kind != Kind::Language);
                let from_ok = c
                    .from
                    .as_ref()
                    .map_or(true, |f| g.option_names().iter().all(|n| f.contains(n)));
                kind_ok && from_ok
            };
            let exact = groups
                .iter()
                .enumerate()
                .position(|(i, g)| !used[i] && fits(g) && g.slots.len() == c.count);
            let v = if let Some(i) = exact {
                used[i] = true;
                Verdict::Correct
            } else if let Some(g) = groups
                .iter()
                .enumerate()
                .find(|(i, g)| !used[*i] && fits(g))
            {
                Verdict::Wrong(format!(
                    "offered {} picks, source {}",
                    g.1.slots.len(),
                    c.count
                ))
            } else {
                Verdict::Wrong(format!("choice of {} not offered", c.count))
            };
            self.t.record(cat, id, v);
        }
    }

    /// "One of these arrays": the pick grants the named items of at most one alternative, or
    /// offers a choice.
    fn alternatives(
        &mut self,
        id: &str,
        x: &Expect,
        b: &CharacterSnapshot,
        a: &CharacterSnapshot,
        groups: &[Group<'_>],
    ) {
        let held = |s: &CharacterSnapshot, kind: Kind| -> BTreeSet<String> {
            match kind {
                Kind::Skill => s
                    .skills
                    .iter()
                    .filter(|k| k.proficient)
                    .map(|k| k.name.to_lowercase())
                    .collect(),
                Kind::Tool => s
                    .elements
                    .iter()
                    .filter(|r| r.element.ty == "Proficiency")
                    .map(|r| norm(&r.element.name))
                    .collect(),
                Kind::Language => s
                    .elements
                    .iter()
                    .filter(|r| r.element.ty == "Language")
                    .map(|r| norm(&r.element.name))
                    .collect(),
            }
        };
        for (kind, alts) in &x.alternatives {
            let named: BTreeSet<&String> = alts.iter().flatten().collect();
            let before = held(b, *kind);
            let new: BTreeSet<String> = held(a, *kind)
                .into_iter()
                .filter(|n| !before.contains(n) && named.contains(n))
                .collect();
            let one = alts.iter().any(|alt| new.iter().all(|n| alt.contains(n)));
            let offered = new.is_empty() && !groups.is_empty();
            let v = if one && (!new.is_empty() || offered) {
                Verdict::Correct
            } else if !one {
                Verdict::Wrong(format!(
                    "grants {} items of {} alternative arrays at once",
                    new.len(),
                    alts.len()
                ))
            } else {
                Verdict::Wrong(format!(
                    "none of {} alternatives granted or offered",
                    alts.len()
                ))
            };
            self.t.record("proficiency (alternative arrays)", id, v);
        }
    }

    fn speed(&mut self, id: &str, e: &Entry, x: &Expect, a: &CharacterSnapshot) {
        let Some(sp) = x.speed else { return };
        if !matches!(e.kind, "species" | "subrace") {
            return;
        }
        let got = [
            a.speed.walk,
            a.speed.fly,
            a.speed.climb,
            a.speed.swim,
            a.speed.burrow,
        ];
        for (i, want) in sp.iter().enumerate() {
            if let Some(w) = want {
                let mode = truth_raw::SPEED_MODES[i];
                self.t.record(
                    "speed",
                    id,
                    verdict(got[i] == *w, || format!("{mode} {} source {w}", got[i])),
                );
            }
        }
    }

    fn defences(&mut self, id: &str, x: &Expect, a: &CharacterSnapshot, groups: &[Group<'_>]) {
        let d = &a.defenses;
        for (cat, want, list) in [
            ("damage resistance", &x.resist, &d.resistances),
            ("damage immunity", &x.immune, &d.immunities),
            (
                "condition immunity",
                &x.condition_immune,
                &d.condition_immunities,
            ),
        ] {
            for w in want {
                let got = list.iter().any(|e| e.active && e.name.to_lowercase() == *w);
                self.t
                    .record(cat, id, verdict(got, || format!("{w} missing")));
            }
        }
        for (cat, from, count) in &x.defence_choices {
            let offered = groups.iter().any(|g| {
                g.slots.len() == *count && {
                    let names = g.option_names();
                    !names.is_empty()
                        && names
                            .iter()
                            .all(|n| from.iter().any(|f| n.contains(f.as_str())))
                }
            });
            self.t.record(
                &format!("{cat} (choice)"),
                id,
                verdict(offered, || {
                    format!("choice of {} types not offered", from.len())
                }),
            );
        }
    }

    fn spells(&mut self, id: &str, e: &Entry, x: &Expect, picked: &Picked) {
        let progression = matches!(e.kind, "species" | "subrace");
        // the pick's own choices (spellcasting ability, lineage, ...) made, spell picks left open
        let filled = self.cx.kit.fill_skipping(picked.after.clone(), &["Spell"]);
        for set in &x.spell_sets {
            if set.iter().all(|g| g.grants.is_empty()) {
                continue;
            }
            if let [group] = set.as_slice() {
                self.grants(id, &picked.before, &filled, &group.grants, progression);
            } else {
                self.alternative_spells(id, set, picked, progression);
            }
        }
    }

    /// Several groups of one source: each must be an option of a new select, and choosing it
    /// must give its spells.
    fn alternative_spells(
        &mut self,
        id: &str,
        set: &[truth_raw::SpellGroup],
        picked: &Picked,
        progression: bool,
    ) {
        let a = self.cx.engine.derive(&picked.after);
        let b = self.cx.engine.derive(&picked.before);
        let groups = new_groups(&b, &a);
        for g in set {
            // a named group by its name; an unnamed one by its first spells (its option's name)
            let named: BTreeSet<String> = g
                .grants
                .iter()
                .filter_map(|x| match &x.spell {
                    SpellRef::Named(n) => Some(n.clone()),
                    SpellRef::Choose(_) => None,
                })
                .collect();
            let (needles, exact) = match g.name.as_deref() {
                Some(n) => (vec![n.to_lowercase()], false),
                None => (named.iter().cloned().collect::<Vec<_>>(), true),
            };
            if needles.is_empty() {
                self.t
                    .unread
                    .entry("additionalSpells: unnamed filter-only alternatives".into())
                    .or_default()
                    .push(id.into());
                continue;
            }
            let joined = needles.join(" / ");
            let _ = &joined;
            let option = groups.iter().find_map(|gr| {
                gr.slots[0]
                    .options
                    .iter()
                    .find(|o| {
                        let on = o.name.to_lowercase();
                        if exact {
                            // the importer names an unnamed group after its first three spells
                            let tail = on.rsplit_once(": ").map_or(on.as_str(), |x| x.1);
                            let parts: BTreeSet<&str> = tail.split(" / ").collect();
                            parts.len() == named.len().min(3)
                                && parts.iter().all(|p| named.contains(*p))
                        } else {
                            on.contains(&joined)
                        }
                    })
                    .map(|o| (gr.slots[0].key.clone(), o.id.clone()))
            });
            let Some((key, oid)) = option else {
                let filters_only = g
                    .grants
                    .iter()
                    .all(|x| matches!(x.spell, SpellRef::Choose(_)));
                if filters_only {
                    self.t.record(
                        "spell alternatives offered (filter-only group)",
                        id,
                        Verdict::NotModelled("filter-only group not offered".into()),
                    );
                } else {
                    self.t.record(
                        "spell alternatives offered",
                        id,
                        Verdict::Wrong("named alternative not offered".into()),
                    );
                }
                continue;
            };
            match commands::select(self.cx.engine, &picked.after, &key, Choice::Element(oid)) {
                Ok(o) => {
                    self.t
                        .record("spell alternatives offered", id, Verdict::Correct);
                    let filled = self.cx.kit.fill_skipping(o.state, &["Spell"]);
                    self.grants(id, &picked.after, &filled, &g.grants, progression);
                }
                Err(err) => self.t.record(
                    "spell alternatives offered",
                    id,
                    Verdict::Wrong(format!("refused: {err}")),
                ),
            }
        }
    }

    /// The grants of one group on `state` (picked from `before`); species also level up to
    /// each grant's level and check the level below.
    fn grants(
        &mut self,
        id: &str,
        before: &CharacterState,
        state: &CharacterState,
        grants: &[Grant],
        progression: bool,
    ) {
        let level = self.cx.engine.derive(state).level;
        self.at_level(id, before, state, grants, level);
        if !progression {
            return;
        }
        let levels: BTreeSet<u32> = grants
            .iter()
            .map(|g| g.level)
            .filter(|l| *l > level && *l <= 20)
            .collect();
        let mut s = state.clone();
        let mut cur = level;
        for target in levels {
            while cur < target - 1 {
                let Some(n) = self.cx.kit.level_up(&s) else {
                    return;
                };
                s = self.cx.kit.fill_skipping(n, &["Spell"]);
                cur += 1;
            }
            let below = spell_names(&self.cx.engine.derive(&s));
            let Some(n) = self.cx.kit.level_up(&s) else {
                return;
            };
            s = self.cx.kit.fill_skipping(n, &["Spell"]);
            cur += 1;
            for g in grants.iter().filter(|g| g.level == target) {
                if let SpellRef::Named(name) = &g.spell {
                    let early = below.contains(name)
                        && !grants
                            .iter()
                            .any(|o| o.level < target && o.spell == g.spell);
                    self.t.record(
                        "spell (not before its level)",
                        id,
                        verdict(!early, || format!("{name} before level {target}")),
                    );
                }
            }
            let snap = self.cx.engine.derive(&s);
            self.named(
                id,
                &snap,
                grants.iter().filter(|g| g.level == target),
                target,
            );
        }
    }

    fn at_level(
        &mut self,
        id: &str,
        before: &CharacterState,
        state: &CharacterState,
        grants: &[Grant],
        level: u32,
    ) {
        let (b, a) = (self.cx.engine.derive(before), self.cx.engine.derive(state));
        self.named(id, &a, grants.iter().filter(|g| g.level <= level), level);
        let want: usize = grants
            .iter()
            .filter(|g| g.level <= level)
            .map(|g| match g.spell {
                SpellRef::Choose(n) => n,
                SpellRef::Named(_) => 0,
            })
            .sum();
        if want > 0 {
            let got = open_spell_slots(&b, &a);
            self.t.record(
                "spell (choice)",
                id,
                verdict(got == want, || format!("{got} spell picks, source {want}")),
            );
        }
    }

    fn named<'g>(
        &mut self,
        id: &str,
        a: &CharacterSnapshot,
        grants: impl Iterator<Item = &'g Grant>,
        level: u32,
    ) {
        let names = spell_names(a);
        for g in grants {
            let SpellRef::Named(name) = &g.spell else {
                continue;
            };
            let present = names.contains(name);
            self.t.record(
                "spell (granted)",
                id,
                verdict(present, || format!("{name} missing at level {level}")),
            );
            let n = match g.uses {
                Uses::Daily(n) | Uses::Rest(n) => n,
                Uses::DailyPb => a.proficiency_bonus,
                _ => continue,
            };
            if !present {
                continue;
            }
            let cast = a.free_casts.iter().find(|f| f.name.to_lowercase() == *name);
            self.t.record(
                "innate uses",
                id,
                verdict(cast.is_some_and(|f| f.uses == Some(n)), || {
                    format!(
                        "{name}: {:?} uses ({:?}), source {n}",
                        cast.map(|f| f.uses),
                        cast.map(|f| f.uses_rule.as_str())
                    )
                }),
            );
        }
    }
}

// -------------------------------------------------------------------------------------- run

fn run(data: &Path) {
    let t0 = std::time::Instant::now();
    let content = core_pack().join("..");
    let options = ImportOptions {
        include_fluff: true,
        srd_dirs: vec![content.join("srd-5.1"), content.join("srd-5.2.1")],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("import");
    let tmp = tempfile::tempdir().expect("tmp");
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder
            .add_pack_dir(&content.join(pack))
            .expect("bundled pack");
    }
    let mut ours: HashMap<String, (Element, Ruleset)> = HashMap::new();
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp.path()).expect("write"))
            .expect("pack loads");
        for e in pack.elements() {
            let rs = e.ruleset.or(pack.ruleset).unwrap_or(Ruleset::Dnd2014);
            ours.insert(e.id.clone(), (e.clone(), rs));
        }
    }
    let store = builder.build();
    let engine = Engine::new(&store);
    let editions: HashMap<String, Ruleset> =
        ours.iter().map(|(k, (_, rs))| (k.clone(), *rs)).collect();
    let kit = Kit {
        engine: &engine,
        store: &store,
        ours: &editions,
    };
    let matched = truth_raw::load(data, &ours);
    println!(
        "=== source truth: {} entries matched; unmatched {:?}; ambiguous {:?}",
        matched.entries.len(),
        matched.unmatched,
        matched.ambiguous
    );
    let limit: usize = std::env::var("BOREALIS_TRUTH_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let hosts: HashMap<Ruleset, Vec<CharacterState>> = [Ruleset::Dnd2014, Ruleset::Dnd2024]
        .into_iter()
        .map(|rs| (rs, kit.feat_hosts(rs)))
        .collect();
    let dialects = truth_raw::dialects(data);
    let mut audit = Audit {
        cx: Cx {
            kit: &kit,
            engine: &engine,
            dialects,
        },
        t: Tally::default(),
    };
    for e in matched.entries.iter().take(limit) {
        let el = &ours[&e.id].0;
        for u in &e.expect.unread {
            audit
                .t
                .unread
                .entry(u.clone())
                .or_default()
                .push(e.id.clone());
        }
        let picked = if e.kind == "feat" {
            kit.feat(e.ruleset, el, &hosts[&e.ruleset])
        } else {
            kit.origin(e.ruleset, &e.id, e.parent.as_deref())
        };
        let picked = match picked {
            Ok(p) => p,
            Err(why) => {
                audit.t.record("host", &e.id, Verdict::NotModelled(why));
                continue;
            }
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| audit.entry(e, &picked)));
        if outcome.is_err() {
            audit.t.panics.push(e.id.clone());
        }
    }
    audit.t.print();
    println!("wall {:.1}s", t0.elapsed().as_secs_f64());
    let fails = audit.t.failures();
    assert!(fails.is_empty(), "{fails:#?}");
}

#[test]
fn five_etools_origin_and_feat_truth() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        assert!(
            std::env::var_os("BOREALIS_ACCEPT_STRICT").is_none(),
            "BOREALIS_5ETOOLS_DIR not set (strict mode)"
        );
        return eprintln!("source_truth: BOREALIS_5ETOOLS_DIR not set, skipping");
    };
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn")
        .join()
        .expect("audit thread");
}

// ----------------------------------------------------- the reading, on invented entries

fn read(v: serde_json::Value) -> Expect {
    Expect::read(v.as_object().expect("object"))
}

#[test]
fn reads_fixed_and_chosen_increases() {
    let e = read(
        serde_json::json!({"ability": [{"dex": 2, "choose": {"from": ["str", "con"], "count": 1}}]}),
    );
    assert_eq!(e.fixed, [0, 2, 0, 0, 0, 0]);
    assert_eq!(
        e.ability_choices,
        vec![[vec![(0, 1)], vec![(2, 1)]]
            .into_iter()
            .collect::<AllocSet>()]
    );
    let two = read(serde_json::json!({"ability": [{"choose": {"count": 2}}]}));
    assert_eq!(
        two.ability_choices[0].len(),
        15,
        "two different scores of six"
    );
}

#[test]
fn reads_weighted_alternatives_and_the_hidden_rule() {
    let bg = read(serde_json::json!({"ability": [
        {"choose": {"weighted": {"from": ["str", "dex", "con"], "weights": [2, 1]}}},
        {"choose": {"weighted": {"from": ["str", "dex", "con"], "weights": [1, 1, 1]}}}]}));
    assert_eq!(bg.ability_choices[0].len(), 7);
    let alt = read(serde_json::json!({"ability": [{"str": 1}, {"dex": 1}]}));
    assert_eq!(alt.fixed, [0; 6]);
    assert_eq!(alt.ability_choices[0].len(), 2);
    let asi = read(serde_json::json!({"ability": [{"choose": {"amount": 2}, "hidden": true}]}));
    assert_eq!(
        asi.ability_choices[0].len(),
        6 + 15,
        "+2 to one or +1 to two"
    );
    let lineage = read(serde_json::json!({"lineage": "VRGR"}));
    assert_eq!(lineage.ability_choices[0].len(), 30 + 20);
}

#[test]
fn reads_proficiencies_speed_and_defences() {
    let e = read(serde_json::json!({
        "skillProficiencies": [{"perception": true, "choose": {"from": ["arcana", "history"]}}],
        "toolProficiencies": [{"musical instrument": true, "smith's tools": true}],
        "languageProficiencies": [{"common": true, "other": true, "anyStandard": 2}],
        "speed": {"walk": 30, "fly": true, "swim": {"number": 20, "condition": "x"}},
        "resist": ["fire", {"choose": {"from": ["cold", "acid"]}}],
        "conditionImmune": ["charmed"]
    }));
    assert!(e.skills.contains("perception") && e.tools.contains("smith's tools"));
    assert!(e.languages.contains("common"));
    let mut kinds: Vec<(Kind, usize, bool)> = e
        .choices
        .iter()
        .map(|c| (c.kind, c.count, c.from.is_some()))
        .collect();
    kinds.sort();
    assert_eq!(
        kinds,
        vec![
            (Kind::Skill, 1, true),
            (Kind::Tool, 1, false),
            (Kind::Language, 1, false),
            (Kind::Language, 2, false)
        ]
    );
    assert_eq!(e.speed, Some([Some(30), Some(30), None, Some(20), None]));
    assert!(e.resist.contains("fire") && e.condition_immune.contains("charmed"));
    assert_eq!(e.defence_choices.len(), 1);
    assert!(e.unread.is_empty(), "{:?}", e.unread);
}

#[test]
fn reads_additional_spells_by_level_and_uses() {
    let e = read(serde_json::json!({"additionalSpells": [{
        "known": {"1": ["thaumaturgy#c"], "_": [{"choose": "level=0|class=Wizard", "count": 2}]},
        "innate": {"3": {"daily": {"1": ["hellish rebuke|phb"]}}, "5": {"daily": {"1e": ["darkness"]}}}
    }]}));
    let g = &e.spell_sets[0][0].grants;
    assert!(g
        .iter()
        .any(|x| x.level == 1 && x.spell == SpellRef::Named("thaumaturgy".into())));
    assert!(g
        .iter()
        .any(|x| x.level == 1 && x.spell == SpellRef::Choose(2)));
    assert!(g.iter().any(|x| x.level == 3 && x.uses == Uses::Daily(1)));
    assert!(g
        .iter()
        .any(|x| x.level == 5 && x.spell == SpellRef::Named("darkness".into())));
}

#[test]
fn subrace_merges_onto_its_race_unless_replaced() {
    let race = read(
        serde_json::json!({"ability": [{"con": 2}], "speed": 25, "resist": ["poison"],
        "languageProficiencies": [{"common": true, "dwarvish": true}]}),
    );
    let sub_raw = serde_json::json!({"ability": [{"wis": 1}], "speed": 30, "resist": null});
    let sub = read(sub_raw.clone());
    let m = Expect::merged(
        &race,
        &sub,
        &truth_raw::replaced(sub_raw.as_object().unwrap()),
    );
    assert_eq!(m.fixed, [0, 0, 2, 0, 1, 0]);
    assert_eq!(m.speed, Some([Some(30), None, None, None, None]));
    assert!(m.resist.is_empty(), "resist: null drops the race's");
    assert!(m.languages.contains("dwarvish"));
}

#[test]
fn engine_allocation_text_parses() {
    assert_eq!(
        parse_alloc("Strength +2, Dexterity +1"),
        Some(vec![(0, 2), (1, 1)])
    );
    assert_eq!(parse_alloc("Wisdom"), None);
    assert_eq!(with_repeats(&[0, 1], 2).len(), 3);
}
