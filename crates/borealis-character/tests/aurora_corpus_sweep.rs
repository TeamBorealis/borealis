//! Local-only engine sweep over community Aurora element repositories, converted the way the app
//! converts a dropped folder / zip (`migrate_path` -> `write_output` -> pack directory loaded on top
//! of the bundled packs). Gated on `BOREALIS_CORPUS_DIR` (the local corpus root holding `repos/`, or
//! the `repos/` directory itself); it skips when unset. Prints counts and element ids only.
//!
//! ```text
//! BOREALIS_CORPUS_DIR=/path/to/corpus cargo test --release -p borealis-character \
//!     --test aurora_corpus_sweep -- --nocapture
//! ```
//!
//! `BOREALIS_SWEEP_BASE` (default `aurorabuilder__elements`) is converted and swept first.
//! `BOREALIS_SWEEP_STACK` (comma-separated repo folder names) adds runs where each listed repo is
//! loaded on top of the base, as users stack indexes, and only that repo's elements are swept.
//! `BOREALIS_SWEEP_LIMIT` caps the elements swept per kind (quick runs);
//! `BOREALIS_SWEEP_SKIP_BASE=1` sweeps only the stacked repos.
//!
//! Every class takes a 2014 character from level 1 to 20 twice (open selects filled with their
//! first option, then with their last option); every archetype is swept on the class that offers
//! it; races, sub races, race variants and backgrounds are picked on a level-1 fighter; feats are
//! looked for on level 1/4/19 hosts; spells are checked against the class spell lists. The test
//! asserts only that nothing panics: the counts are the audit. Every checked snapshot also runs
//! the settle invariants (`borealis_engine::invariants::check`), reported as `invariant.*`
//! problems.

#[path = "../../../tools/regression-gate/outcome.rs"]
mod outcome;
#[path = "../../../src-tauri/tests/support/prereq.rs"]
mod prereq;

use borealis_content::{ContentStore, ContentStoreBuilder, Element, PackOrigin, Rule, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::state::SlotKey;
use borealis_engine::{invariants, CharacterSnapshot, CharacterState, Choice, Engine, Severity};
use borealis_migrate::{migrate_path, write_output, MigrateOptions};
use prereq::{feat_chain, required_of_type};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Counts = BTreeMap<String, usize>;

const TEST_SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];
/// Scores of the caster host (origins and feats): 13+ in every mental ability, so ability
/// prerequisites like `[int:13]||[wis:13]` and `[cha:13]` are met.
const CASTER_SCORES: [i32; 6] = [12, 12, 14, 15, 15, 15];
const FILL_PASSES: usize = 60;
const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Race Variant",
    "Background",
    "Class",
    "Archetype",
    "Feat",
    "Multiclass",
];
const MAX_LEVEL: u32 = 20;
const SLOW: Duration = Duration::from_secs(30);
const EXAMPLES: usize = 3;
const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const HUMAN: &str = "ID_RACE_HUMAN";
const HUMAN_VARIANT: &str = "ID_RACE_VARIANT_HUMAN_VARIANT";
const WIZARD: &str = "ID_WOTC_PHB_CLASS_WIZARD";
/// Origin of the class and archetype sweeps: a race and a background, so expertise and
/// race-dependent features have something to work with (2014 ids, then 2024 ids).
const ORIGIN_2014: (&str, &str) = ("ID_RACE_HUMAN", "ID_BACKGROUND_ACOLYTE");
const ORIGIN_2024: (&str, &str) = ("ID_2024_SPECIES_HUMAN", "ID_2024_BACKGROUND_ACOLYTE");
const BASE_PACKS: &[&str] = &[
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

/// Ruleset and race (plus sub race) a sweep character starts with; `None` = the default human.
#[derive(Clone, Debug)]
struct Origin {
    rs: Ruleset,
    race: Option<(String, Option<String>)>,
}

impl Origin {
    fn plain(rs: Ruleset) -> Self {
        Self { rs, race: None }
    }
}

/// `BOREALIS_SWEEP_LIST=1` prints every problem per element (`LIST` lines) for triage.
fn listing() -> bool {
    std::env::var("BOREALIS_SWEEP_LIST").is_ok()
}

/// `ID_*` tokens of an expression.
fn ids_in(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| t.starts_with("ID_"))
        .map(str::to_string)
        .collect()
}

#[derive(Clone, Copy)]
enum Pick {
    First,
    Last,
}

/// Strip digits so messages group by cause.
fn cause(message: &str) -> String {
    let mut out = String::new();
    for word in message.split(' ') {
        if !out.is_empty() {
            out.push(' ');
        }
        if word.starts_with("ID_") {
            out.push_str("ID_*");
        } else {
            out.extend(
                word.chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c }),
            );
        }
    }
    out.truncate(120);
    out
}

/// Example label: the swept element, plus the first other id the message names.
fn example(id: &str, message: &str) -> String {
    match message
        .split(' ')
        .find(|w| w.starts_with("ID_") && *w != id)
    {
        Some(w) => format!("{id}<{}>", w.trim_end_matches([':', ',', '.'])),
        None => id.to_string(),
    }
}

#[derive(Default)]
struct Tally {
    tried: BTreeMap<&'static str, usize>,
    failed: BTreeMap<&'static str, usize>,
    causes: BTreeMap<&'static str, Counts>,
    examples: BTreeMap<(&'static str, String), Vec<String>>,
}

impl Tally {
    fn record(&mut self, kind: &'static str, id: &str, mut problems: Vec<String>) {
        problems.sort();
        problems.dedup();
        outcome::record(
            "aurora_sweep",
            &format!("{kind}/{id}"),
            problems.is_empty(),
            &problems.join("; "),
        );
        *self.tried.entry(kind).or_default() += 1;
        if !problems.is_empty() {
            *self.failed.entry(kind).or_default() += 1;
        }
        for p in problems {
            if listing() {
                println!("LIST {kind} {id}: {p}");
            }
            let c = cause(&p);
            *self
                .causes
                .entry(kind)
                .or_default()
                .entry(c.clone())
                .or_default() += 1;
            let ex = self.examples.entry((kind, c)).or_default();
            if ex.len() < EXAMPLES {
                ex.push(example(id, &p));
            }
        }
    }

    fn print(&self) {
        for (kind, n) in &self.tried {
            println!(
                "{kind:>16}: {n} tried, {} with problems",
                self.failed.get(kind).unwrap_or(&0)
            );
        }
        for (kind, counts) in &self.causes {
            println!("--- {kind} problems by cause");
            let mut rows: Vec<_> = counts.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            for (c, v) in rows.into_iter().take(30) {
                let ex = &self.examples[&(*kind, c.clone())];
                println!("{v:>6}  {c}  [{}]", ex.join(" "));
            }
        }
    }
}

/// Lower-cased spellcasting names that receive slot stats from some element of the store.
fn slot_lists(store: &ContentStore) -> HashSet<String> {
    store
        .iter()
        .flat_map(|se| se.element.rules.iter())
        .filter_map(|r| match r {
            Rule::Stat(st) => st
                .name
                .to_lowercase()
                .split_once(":spellcasting:slots")
                .map(|(list, _)| list.to_string()),
            _ => None,
        })
        .collect()
}

struct Sweeper<'a> {
    engine: &'a Engine,
    store: &'a ContentStore,
    ours: &'a HashSet<String>,
    /// Lower-cased spellcasting names some loaded element gives slot stats
    /// (`<name>:spellcasting:slots:<n>`): only these must end with slots.
    slot_lists: HashSet<String>,
    /// Diagnostics on elements that are not part of the build (option lists, global rules),
    /// keyed by element: they show on every character, so they are reported once.
    unattributed: std::cell::RefCell<BTreeMap<String, String>>,
}

impl Sweeper<'_> {
    fn base(&self, rs: Ruleset) -> CharacterState {
        let state = commands::new_character(self.engine, rs);
        commands::set_base_scores(self.engine, &state, TEST_SCORES).map_or(state, |o| o.state)
    }

    fn with_class(&self, rs: Ruleset, class: &str) -> Result<CharacterState, String> {
        self.with_class_from(self.base(rs), class)
    }

    /// The level-1 wizard host with high mental scores.
    fn caster(&self) -> Option<CharacterState> {
        let state = commands::new_character(self.engine, Ruleset::Dnd2014);
        let state = commands::set_base_scores(self.engine, &state, CASTER_SCORES)
            .ok()?
            .state;
        self.with_class_from(state, WIZARD).ok()
    }

    fn with_class_from(
        &self,
        state: CharacterState,
        class: &str,
    ) -> Result<CharacterState, String> {
        let snap = self.engine.derive(&state);
        let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).ok_or("no class slot")?;
        commands::select(self.engine, &state, &key, Choice::Element(class.into()))
            .map(|o| o.state)
            .map_err(|e| format!("class refused: {e}"))
    }

    /// `state` with the race (and sub race) picked; `None` when one is not offered.
    fn with_race(
        &self,
        state: &CharacterState,
        race: &str,
        sub: Option<&str>,
    ) -> Option<CharacterState> {
        let mut state = self.pick(state, race)?.ok()?;
        if let Some(sub) = sub {
            state = self.pick(&state, sub)?.ok()?;
        }
        Some(state)
    }

    /// A class at level 1 with a race and a background (the sweep character).
    fn start(&self, o: &Origin, class: &str) -> Result<CharacterState, String> {
        let state = self.with_class(o.rs, class)?;
        let (race, background) = if o.rs == Ruleset::Dnd2024 {
            ORIGIN_2024
        } else {
            ORIGIN_2014
        };
        let state = match &o.race {
            Some((r, sub)) => self
                .with_race(&state, r, sub.as_deref())
                .ok_or_else(|| format!("race {r} not offered"))?,
            None => self.with_race(&state, race, None).unwrap_or(state),
        };
        Ok(match self.pick(&state, background) {
            Some(Ok(s)) => s,
            _ => state,
        })
    }

    fn problems(&self, state: &CharacterState, snap: &CharacterSnapshot, out: &mut Vec<String>) {
        let built: HashSet<&str> = snap
            .elements
            .iter()
            .map(|r| r.element.id.as_str())
            .collect();
        // settle invariants (engine-test-audit-1 WP2); unknown rules of elements this character
        // does not use belong to other sweeps
        for v in invariants::check(self.engine, state, snap) {
            let foreign = v.kind == invariants::kind::UNKNOWN_RULE
                && v.element
                    .as_deref()
                    .is_some_and(|e| !built.contains(e) && !self.ours.contains(e));
            if !foreign {
                out.push(format!("{} {}", v.kind, v.message));
            }
        }
        for d in &snap.diagnostics {
            let Some(el) = d.element.as_ref().filter(|e| self.ours.contains(*e)) else {
                continue;
            };
            if d.severity < Severity::Warning {
                continue;
            }
            let text = format!("diagnostic {:?} {}: {}", d.severity, d.code, d.message);
            if built.contains(el.as_str()) {
                out.push(text);
            } else {
                self.unattributed
                    .borrow_mut()
                    .entry(el.clone())
                    .or_insert(text);
            }
        }
        for inv in &snap.invalid_selections {
            if self.ours.contains(&inv.slot.owner) {
                out.push("broken selection".into());
            }
        }
        for s in &snap.slots {
            if self.ours.contains(&s.owner.id)
                && s.ty != "Archetype"
                && s.options.is_empty()
                && s.list_items.is_empty()
            {
                out.push(format!(
                    "select of type {} offers no options {}",
                    s.ty, s.owner.id
                ));
            }
        }
        let lvl = i32::try_from(snap.level.max(1)).unwrap_or(1);
        if snap.hit_points <= 0 || snap.hit_points > 25 * lvl + 60 {
            out.push("stat out of range: hit points".into());
        }
        if !(5..=30).contains(&snap.armor_class) {
            out.push("stat out of range: armor class".into());
        }
        if !(2..=6).contains(&snap.proficiency_bonus) {
            out.push("stat out of range: proficiency bonus".into());
        }
        if snap
            .abilities
            .iter()
            .any(|a| !(1..=30).contains(&a.score) || a.save.abs() > 25)
        {
            out.push("stat out of range: ability score or save".into());
        }
        for sc in &snap.spellcasting {
            if !(8..=30).contains(&sc.dc) || sc.attack.abs() > 20 || sc.ability.is_empty() {
                out.push("stat out of range: spell DC/attack/ability".into());
            }
        }
    }

    fn fill(&self, mut state: CharacterState, pick: Pick) -> CharacterState {
        let mut refused = HashSet::new();
        for _ in 0..FILL_PASSES {
            let snap = self.engine.derive(&state);
            let open = snap.slots.iter().find(|s| {
                s.chosen.is_none()
                    && s.list_choice.is_none()
                    && !NOT_FILLED.contains(&s.ty.as_str())
                    && !refused.contains(&s.key)
                    && (!s.options.is_empty() || !s.list_items.is_empty())
            });
            let Some(slot) = open else { break };
            let (opt, item) = match pick {
                Pick::First => (slot.options.first(), slot.list_items.first()),
                Pick::Last => (slot.options.last(), slot.list_items.last()),
            };
            let choice = match (opt, item) {
                (Some(o), _) => Choice::Element(o.id.clone()),
                (None, Some((i, _))) => Choice::ListItem(*i),
                (None, None) => break,
            };
            match commands::select(self.engine, &state, &slot.key, choice) {
                Ok(o) => state = o.state,
                Err(_) => {
                    refused.insert(slot.key.clone());
                }
            }
        }
        state
    }

    fn pick(&self, state: &CharacterState, target: &str) -> Option<Result<CharacterState, String>> {
        let snap = self.engine.derive(state);
        let slot = snap
            .slots
            .iter()
            .find(|s| s.chosen.is_none() && s.options.iter().any(|o| o.id == target))?;
        Some(
            commands::select(
                self.engine,
                state,
                &slot.key,
                Choice::Element(target.into()),
            )
            .map(|o| o.state)
            .map_err(|e| format!("select refused: {e}")),
        )
    }

    /// Levels 1-20; returns problems and the archetypes offered (id -> level).
    fn sweep(
        &self,
        class: &Element,
        sub: Option<&str>,
        origin: &Origin,
        pick: Pick,
        spells: &mut Vec<String>,
    ) -> (Vec<String>, BTreeMap<String, u32>) {
        let mut out = Vec::new();
        let mut offered = BTreeMap::new();
        let mut state = match self.start(origin, &class.id) {
            Ok(s) => s,
            Err(e) => return (vec![e], offered),
        };
        let expected: Vec<(&str, u32)> = class
            .rules
            .iter()
            .filter_map(|r| match r {
                // conditional grants depend on choices the sweep does not make, and so do
                // features with element requirements (replaced by another feature)
                Rule::Grant(g)
                    if g.ty.as_deref() == Some("Class Feature")
                        && g.requirements
                            .as_deref()
                            .map_or(true, |r| r.trim().is_empty())
                        && self
                            .store
                            .element(&g.id)
                            .map_or(true, |e| e.requirements.trim().is_empty()) =>
                {
                    Some((g.id.as_str(), g.level.unwrap_or(1)))
                }
                _ => None,
            })
            .collect();
        let mut picked = sub.is_none();
        let mut slots_seen = 0;
        for level in 1..=MAX_LEVEL {
            if level > 1 {
                match commands::level_up_main(self.engine, &state, None) {
                    Ok(o) => state = o.state,
                    Err(e) => {
                        out.push(format!("level-up fails: {e}"));
                        break;
                    }
                }
            }
            let snap = self.engine.derive(&state);
            for s in snap.slots.iter().filter(|s| s.ty == "Archetype") {
                for o in &s.options {
                    offered.entry(o.id.clone()).or_insert(level);
                }
            }
            if let (Some(s), false) = (sub, picked) {
                if let Some(r) = self.pick(&state, s) {
                    match r {
                        Ok(next) => {
                            state = next;
                            picked = true;
                        }
                        Err(e) => out.push(e),
                    }
                }
            }
            state = self.fill(state, pick);
            let snap = self.engine.derive(&state);
            self.problems(&state, &snap, &mut out);
            for (fid, flevel) in &expected {
                if *flevel == level && !snap.features.iter().any(|f| f.id == *fid) {
                    out.push(format!("class feature missing at its level {fid}"));
                }
            }
            let slots: i32 = snap
                .spellcasting
                .iter()
                .map(|s| s.slots.iter().sum::<i32>() + s.pact.map_or(0, |p| p.0 * p.1))
                .sum();
            if slots < slots_seen {
                out.push("spell slots shrink".into());
            }
            slots_seen = slots;
            if snap.level != level {
                out.push("character level does not match".into());
            }
            if level == MAX_LEVEL {
                self.spell_check(class, sub, &snap, &mut out, spells);
            }
        }
        if !picked {
            out.push("archetype not offered by the class that lists it".into());
        }
        (out, offered)
    }

    /// Level-20 spellcasting of a class (or archetype) that declares spellcasting.
    fn spell_check(
        &self,
        class: &Element,
        sub: Option<&str>,
        snap: &CharacterSnapshot,
        out: &mut Vec<String>,
        rows: &mut Vec<String>,
    ) {
        let declares = snap.elements.iter().any(|r| {
            self.ours.contains(&r.element.id)
                && self
                    .store
                    .element(&r.element.id)
                    .is_some_and(|e| e.spellcasting.is_some())
        });
        if !declares {
            return;
        }
        if snap.spellcasting.is_empty() {
            out.push("declares spellcasting but has none at 20".into());
            return;
        }
        for sc in &snap.spellcasting {
            let slots: i32 = sc.slots.iter().sum::<i32>() + sc.pact.map_or(0, |p| p.0 * p.1);
            let choosable: usize = snap
                .slots
                .iter()
                .filter(|s| s.spellcasting.as_deref() == Some(sc.name.as_str()))
                .map(|s| s.options.len())
                .max()
                .unwrap_or(0);
            // Aurora gives slots only through `<name>:spellcasting:slots:<n>` stats; a block the
            // content gives none (Mystic disciplines, cantrip-only subclasses) has none in Aurora too
            if slots == 0 && self.slot_lists.contains(&sc.name.to_lowercase()) {
                out.push(
                    "spellcasting with no slots at 20 although the content declares slots".into(),
                );
            }
            if sc.spells.is_empty() && choosable == 0 {
                out.push("spellcasting offers and knows no spells at 20".into());
            }
            rows.push(format!(
                "{} {} list={:?} ability={} dc={} slots={slots} prepare_limit={:?} known={} choosable={choosable}",
                class.id,
                sub.unwrap_or("-"),
                sc.name,
                sc.ability,
                sc.dc,
                sc.prepare_limit,
                sc.spells.len()
            ));
        }
    }

    /// Archetypes a class offers while levelling 1-20 without other choices (id -> level).
    fn archetypes_of(&self, class: &str, origin: &Origin) -> BTreeMap<String, u32> {
        let mut offered = BTreeMap::new();
        let Ok(mut state) = self.start(origin, class) else {
            return offered;
        };
        for level in 1..=MAX_LEVEL {
            if level > 1 {
                match commands::level_up_main(self.engine, &state, None) {
                    Ok(o) => state = o.state,
                    Err(_) => break,
                }
            }
            for s in self
                .engine
                .derive(&state)
                .slots
                .into_iter()
                .filter(|s| s.ty == "Archetype")
            {
                for o in s.options {
                    offered.entry(o.id).or_insert(level);
                }
            }
        }
        offered
    }
}

fn corpus_repos() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("BOREALIS_CORPUS_DIR").ok()?);
    Some(if dir.join("repos").is_dir() {
        dir.join("repos")
    } else {
        dir
    })
}

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

fn pack_id(repo: &str) -> String {
    let s: String = repo
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("dl-{}", s.trim_matches('-'))
}

/// Convert a repo exactly as the folder/zip import does; returns the pack dir.
fn convert(repo_dir: &Path, repo: &str, tmp: &Path) -> Option<PathBuf> {
    let id = pack_id(repo);
    let opts = MigrateOptions {
        pack_id: Some(id.clone()),
        pack_name: Some(repo.to_string()),
        origin: PackOrigin::Custom,
        ..MigrateOptions::default()
    };
    let t = Instant::now();
    let out = match migrate_path(repo_dir, &opts) {
        Ok(o) => o,
        Err(e) => {
            println!("[{repo}] migrate failed: {e}");
            return None;
        }
    };
    let mut by_code = Counts::new();
    for i in &out.report.issues {
        *by_code
            .entry(format!("{:?} {:?}", i.severity, i.code))
            .or_default() += 1;
    }
    println!(
        "[{repo}] migrated in {:?}: {} elements, {} dropped, {} skipped files, issues {by_code:?}",
        t.elapsed(),
        out.report.totals.elements,
        out.report.totals.dropped_elements,
        out.report.summary.skipped_files
    );
    let dir = tmp.join(&id);
    write_output(&dir, &out).expect("write pack");
    Some(dir)
}

#[test]
fn aurora_corpus_engine_sweep() {
    let Some(repos) = corpus_repos() else {
        eprintln!("SKIPPED: BOREALIS_CORPUS_DIR is not set");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run_all(&repos))
        .expect("spawn");
    handle.join().expect("sweep thread");
}

fn run_all(repos: &Path) {
    let base =
        std::env::var("BOREALIS_SWEEP_BASE").unwrap_or_else(|_| "aurorabuilder__elements".into());
    let limit = std::env::var("BOREALIS_SWEEP_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let tmp = tempfile::tempdir().unwrap();
    let Some(base_dir) = convert(&repos.join(&base), &base, tmp.path()) else {
        panic!("base repo {base} did not convert");
    };
    // BOREALIS_SWEEP_SKIP_BASE=1 sweeps only the stacked repos (the base is still loaded)
    let mut panics = if std::env::var("BOREALIS_SWEEP_SKIP_BASE").is_ok() {
        0
    } else {
        run(
            &base,
            std::slice::from_ref(&base_dir),
            &pack_id(&base),
            limit,
        )
    };
    let stack = std::env::var("BOREALIS_SWEEP_STACK").unwrap_or_default();
    for repo in stack.split(',').map(str::trim).filter(|r| !r.is_empty()) {
        if let Some(dir) = convert(&repos.join(repo), repo, tmp.path()) {
            panics += run(repo, &[base_dir.clone(), dir], &pack_id(repo), limit);
        }
    }
    assert_eq!(panics, 0, "engine panics while sweeping community content");
}

fn load(packs: &[PathBuf], label: &str) -> Option<ContentStore> {
    let mut b = ContentStoreBuilder::new();
    for p in BASE_PACKS {
        b = b
            .add_pack_dir(&content_dir().join(p))
            .expect("bundled pack");
    }
    for dir in packs {
        match b.clone().add_pack_dir(dir) {
            Ok(next) => b = next,
            Err(e) => {
                println!("[{label}] PACK LOAD FAILED (the app would roll the install back): {e}");
                return None;
            }
        }
    }
    Some(b.build())
}

/// Sweep one configuration; returns the number of panics.
#[allow(clippy::too_many_lines)]
fn run(label: &str, packs: &[PathBuf], ours_pack: &str, limit: usize) -> usize {
    let Some(store) = load(packs, label) else {
        return 0;
    };
    let prefix = format!("{ours_pack}/");
    let elements: Vec<Element> = store
        .iter()
        .filter(|s| s.file.starts_with(&prefix))
        .map(|s| s.element.clone())
        .collect();
    let ours: HashSet<String> = elements.iter().map(|e| e.id.clone()).collect();
    let mut types = Counts::new();
    for e in &elements {
        *types.entry(e.ty.clone()).or_default() += 1;
    }
    println!(
        "=== [{label}] {} elements in store, ours {}: {types:?}",
        store.len(),
        ours.len()
    );

    // content-level issues (dangling references etc.) on our elements
    let mut store_issues = Tally::default();
    let mut per_el: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for i in store
        .validate()
        .into_iter()
        .chain(store.diagnostics().iter().cloned())
    {
        if let Some(el) = i.element.as_ref().filter(|e| ours.contains(*e)) {
            if format!("{:?}", i.severity) == "Info" {
                continue;
            }
            per_el
                .entry(el.clone())
                .or_default()
                .push(format!("{:?} {:?}: {}", i.severity, i.code, i.message));
        }
    }
    for (el, p) in per_el {
        store_issues.record("store", &el, p);
    }
    // rules the engine does not understand
    for e in &elements {
        let unknown: Vec<String> = e
            .rules
            .iter()
            .filter_map(|r| match r {
                Rule::Unknown(_) => Some("rule kind unknown".to_string()),
                Rule::Custom(_) => Some("rule kind custom".to_string()),
                _ => None,
            })
            .collect();
        if !unknown.is_empty() {
            store_issues.record("unknown rules", &e.id, unknown);
        }
    }
    store_issues.print();

    let engine = Engine::new(&store);
    let sw = Sweeper {
        engine: &engine,
        store: &store,
        ours: &ours,
        slot_lists: slot_lists(&store),
        unattributed: std::cell::RefCell::default(),
    };
    let of_type = |ty: &str| -> Vec<&Element> {
        elements.iter().filter(|e| e.ty == ty).take(limit).collect()
    };
    let mut t = Tally::default();
    let mut panics = 0usize;
    let mut spells_rows = Vec::new();
    let mut guarded =
        |t: &mut Tally, kind: &'static str, id: &str, f: &mut dyn FnMut() -> Vec<String>| {
            let start = Instant::now();
            let r = catch_unwind(AssertUnwindSafe(f));
            let mut problems = r.unwrap_or_else(|_| {
                panics += 1;
                vec!["PANIC".into()]
            });
            if start.elapsed() > SLOW {
                problems.push("slow: over 30 s".into());
            }
            t.record(kind, id, problems);
        };

    // classes: first-option and last-option passes (Aurora content is 2014)
    let plain14 = Origin::plain(Ruleset::Dnd2014);
    for class in of_type("Class") {
        for (kind, pick) in [("class(first)", Pick::First), ("class(last)", Pick::Last)] {
            guarded(&mut t, kind, &class.id, &mut || {
                sw.sweep(class, None, &plain14, pick, &mut spells_rows).0
            });
        }
    }

    // races and the race each sub race / race variant belongs to (on a bare level-1 fighter)
    let fighter = sw.fill(
        sw.with_class(Ruleset::Dnd2014, FIGHTER).expect("fighter"),
        Pick::First,
    );
    // race (and sub race) offering each sub race / race variant; variants may hang off a sub race
    let mut race_of: HashMap<String, (String, Option<String>)> = HashMap::new();
    for race in store.by_type("Race") {
        let rid = race.element.id.clone();
        let Some(Ok(s)) = sw.pick(&fighter, &rid) else {
            continue;
        };
        for slot in sw.engine.derive(&s).slots {
            if slot.ty != "Sub Race" && slot.ty != "Race Variant" {
                continue;
            }
            for o in slot.options {
                race_of.entry(o.id.clone()).or_insert((rid.clone(), None));
                if slot.ty != "Sub Race" {
                    continue;
                }
                let Some(Ok(sub)) = sw.pick(&s, &o.id) else {
                    continue;
                };
                for v in sw.engine.derive(&sub).slots {
                    if v.ty == "Race Variant" {
                        for vo in v.options {
                            race_of
                                .entry(vo.id)
                                .or_insert((rid.clone(), Some(o.id.clone())));
                        }
                    }
                }
            }
        }
    }
    // races satisfying a requirement: the race itself, a sub race, or one granting the id
    let races_for = |requirements: &str| -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for id in ids_in(requirements) {
            for r in ["Race", "Sub Race", "Race Variant"]
                .iter()
                .flat_map(|ty| store.by_type(ty))
            {
                let e = &r.element;
                let grants = e
                    .rules
                    .iter()
                    .any(|x| matches!(x, Rule::Grant(g) if g.id == id));
                if e.id != id && !grants {
                    continue;
                }
                let pair = if e.ty == "Race" {
                    Some((e.id.clone(), None))
                } else {
                    race_of
                        .get(&e.id)
                        .map(|(p, _)| (p.clone(), Some(e.id.clone())))
                };
                if let Some(p) = pair.filter(|p| !out.contains(p)) {
                    out.push(p);
                }
            }
        }
        out
    };

    // archetype -> the class offering it: 2014 classes, then 2024 classes (2014 subclasses
    // are usable there), then 2014 classes with a race the archetype requires
    let mut parent: HashMap<String, (String, Origin)> = HashMap::new();
    let find_parents =
        |parent: &mut HashMap<String, (String, Origin)>, origin: &Origin, only: Option<&str>| {
            for class in store.by_type("Class") {
                let is_2024 = class.element.ruleset == Some(Ruleset::Dnd2024);
                if is_2024 != (origin.rs == Ruleset::Dnd2024) {
                    continue;
                }
                let id = class.element.id.clone();
                let offered = catch_unwind(AssertUnwindSafe(|| sw.archetypes_of(&id, origin)))
                    .unwrap_or_default();
                for (a, _) in offered {
                    if only.map_or(true, |o| o == a) {
                        parent
                            .entry(a)
                            .or_insert_with(|| (id.clone(), origin.clone()));
                    }
                }
                if only.is_some_and(|o| parent.contains_key(o)) {
                    return;
                }
            }
        };
    find_parents(&mut parent, &plain14, None);
    if of_type("Archetype")
        .iter()
        .any(|a| !parent.contains_key(&a.id))
    {
        find_parents(&mut parent, &Origin::plain(Ruleset::Dnd2024), None);
    }
    for sub in of_type("Archetype") {
        if parent.contains_key(&sub.id) {
            continue;
        }
        for race in races_for(&sub.requirements) {
            let origin = Origin {
                rs: Ruleset::Dnd2014,
                race: Some(race),
            };
            find_parents(&mut parent, &origin, Some(&sub.id));
            if parent.contains_key(&sub.id) {
                break;
            }
        }
    }
    for sub in of_type("Archetype") {
        let Some((cid, origin)) = parent.get(&sub.id).cloned() else {
            t.record(
                "archetype",
                &sub.id,
                vec!["no class offers this archetype".into()],
            );
            continue;
        };
        let class = store.element(&cid).unwrap().clone();
        for (kind, pick) in [
            ("archetype(first)", Pick::First),
            ("archetype(last)", Pick::Last),
        ] {
            guarded(&mut t, kind, &sub.id, &mut || {
                sw.sweep(&class, Some(&sub.id), &origin, pick, &mut spells_rows)
                    .0
            });
        }
    }

    // origins on a level-1 wizard with high mental scores (a fighter is already proficient with
    // every weapon and armor, which empties "choose a martial weapon" selects), then on the
    // fighter; an element whose requirements name a race is retried on a host of that race
    let caster = sw.caster().map(|c| sw.fill(c, Pick::First));
    let origin_hosts: Vec<&CharacterState> = caster.iter().chain([&fighter]).collect();
    let race_host = |race: &(String, Option<String>)| {
        let base = sw.caster()?;
        Some(sw.fill(
            sw.with_race(&base, &race.0, race.1.as_deref())?,
            Pick::First,
        ))
    };
    for (ty, kind) in [
        ("Race", "race"),
        ("Sub Race", "sub race"),
        ("Race Variant", "race variant"),
        ("Background", "background"),
    ] {
        for el in of_type(ty) {
            guarded(&mut t, kind, &el.id, &mut || {
                // start states: each host with the parent race (and sub race) picked
                let mut starts: Vec<CharacterState> = origin_hosts
                    .iter()
                    .filter_map(|h| match race_of.get(&el.id) {
                        Some((race, sub)) => sw.with_race(h, race, sub.as_deref()),
                        None => Some((*h).clone()),
                    })
                    .collect();
                if starts.is_empty() {
                    return vec![format!("{ty}: no race offers it")];
                }
                if ty == "Background" && starts.iter().all(|s| sw.pick(s, &el.id).is_none()) {
                    starts.extend(races_for(&el.requirements).iter().filter_map(&race_host));
                }
                // an origin works when it works on one host (a host may already have the
                // proficiencies or languages a select offers)
                let runs: Vec<Vec<String>> = starts
                    .iter()
                    .filter(|s| sw.pick(s, &el.id).is_some())
                    .map(|state| {
                        let mut problems = Vec::new();
                        for pick in [Pick::First, Pick::Last] {
                            match sw.pick(state, &el.id) {
                                Some(Ok(s)) => {
                                    let s = sw.fill(s, pick);
                                    sw.problems(&s, &sw.engine.derive(&s), &mut problems);
                                }
                                Some(Err(e)) => problems.push(e),
                                None => {}
                            }
                        }
                        problems
                    })
                    .collect();
                let problems = runs.into_iter().min_by_key(Vec::len).unwrap_or_else(|| {
                    vec![if el.requirements.trim().is_empty() {
                        format!("{ty} is not offered")
                    } else {
                        format!("{ty} with requirements is not offered")
                    }]
                });
                if !problems.is_empty() && listing() {
                    println!("LIST {kind} {} req={:?}", el.id, el.requirements);
                }
                problems
            });
        }
    }

    // feats on level 1 (variant human), 4 and 19 wizards with high mental scores (they satisfy
    // "can cast a spell", `[type:spell]`, and mental ability prerequisites), then fighters;
    // feats naming a race are also looked for on wizards of that race
    let mut hosts = sw.caster().map_or_else(Vec::new, |c| feat_hosts(&sw, &c));
    hosts.extend(feat_hosts(&sw, &fighter));
    let mut race_hosts: HashMap<(String, Option<String>), Vec<CharacterState>> = HashMap::new();
    let mut unoffered = Counts::new();
    let mut unoffered_ex: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for el in of_type("Feat") {
        let mut found = true;
        let mut extra = Vec::new();
        let mut deeper = Vec::new();
        if hosts.iter().all(|h| sw.pick(h, &el.id).is_none()) {
            for race in races_for(&el.requirements) {
                let hs = race_hosts.entry(race.clone()).or_insert_with(|| {
                    race_host(&race).map_or_else(Vec::new, |h| feat_hosts(&sw, &h))
                });
                extra.extend(hs.iter().cloned());
            }
            // prerequisites the default hosts lack: a class the feat names, the feats it chains on
            let mut class_hosts: Vec<CharacterState> = sw
                .with_class(Ruleset::Dnd2014, FIGHTER)
                .into_iter()
                .collect();
            class_hosts.extend(sw.caster());
            for class in required_of_type(sw.store, &el.requirements, "Class") {
                if let Ok(h) = sw.with_class(Ruleset::Dnd2014, &class) {
                    extra.extend(feat_hosts(&sw, &h));
                    class_hosts.push(h);
                }
            }
            // an archetype feature the feat needs: the archetype's class, levelled until granted
            for feature in required_of_type(sw.store, &el.requirements, "Archetype Feature") {
                let Some((arch, level)) = granted_by(sw.store, "Archetype", &feature) else {
                    continue;
                };
                if let Some(h) = parent
                    .get(&arch)
                    .and_then(|(class, _)| archetype_host(&sw, class, &arch, level))
                {
                    class_hosts.push(h);
                }
            }
            let chain = feat_chain(sw.store, &el.id);
            extra.extend(
                class_hosts
                    .iter()
                    .filter_map(|h| chain_host(&sw, h, &chain)),
            );
            let again = with_next_copies(sw.store, &chain, &el.id);
            if again != chain {
                deeper.extend(
                    class_hosts
                        .iter()
                        .filter_map(|h| chain_host(&sw, h, &again)),
                );
            }
        }
        guarded(&mut t, "feat", &el.id, &mut || {
            let own_empty = format!("offers no options {}", el.id);
            let empty = |p: &[String]| p.iter().any(|x| x.ends_with(&own_empty));
            let attempt = |h: &CharacterState| {
                sw.pick(h, &el.id).map(|r| match r {
                    Err(e) => vec![e],
                    Ok(s) => {
                        let s = sw.fill(s, Pick::First);
                        let mut problems = Vec::new();
                        sw.problems(&s, &sw.engine.derive(&s), &mut problems);
                        problems
                    }
                })
            };
            match hosts.iter().chain(&extra).find_map(attempt) {
                None => {
                    found = false;
                    Vec::new()
                }
                // the feat's own select offers nothing on this host because its options need
                // picks of an earlier feat that the requirements do not name (the third Script
                // Sage mastery needs a third hieroglyph, from Script Carver taken again): a
                // host that also took the next copy of each feat of the chain
                Some(p) if empty(&p) => deeper
                    .iter()
                    .find_map(attempt)
                    .filter(|q| !empty(q))
                    .unwrap_or(p),
                Some(p) => p,
            }
        });
        outcome::record(
            "aurora_sweep",
            &format!("feat_offered/{}", el.id),
            found,
            "not offered on the hosts",
        );
        if !found {
            let why = if el.requirements.trim().is_empty() {
                "feat not offered: no requirements"
            } else {
                "feat not offered: has requirements"
            };
            if listing() {
                println!("LIST unoffered-feat {} req={:?}", el.id, el.requirements);
            }
            *unoffered.entry(why.into()).or_default() += 1;
            let ex = unoffered_ex.entry(why.into()).or_default();
            if ex.len() < EXAMPLES {
                ex.push(el.id.clone());
            }
        }
    }

    // gear
    for ty in ["Weapon", "Armor", "Magic Item"] {
        for el in of_type(ty) {
            guarded(&mut t, "item", &el.id, &mut || {
                let mut problems = Vec::new();
                match commands::add_item(sw.engine, &fighter, &el.id, 1) {
                    Err(e) => problems.push(format!("add refused: {e}")),
                    Ok(o) => {
                        let uid = o.state.inventory.items.last().map(|i| i.uid.clone());
                        if let (Some(uid), true) = (uid, ty != "Magic Item") {
                            match commands::equip(sw.engine, &o.state, &uid, None) {
                                Err(e) => problems.push(format!("equip refused: {e}")),
                                Ok(eq) => {
                                    sw.problems(
                                        &eq.state,
                                        &sw.engine.derive(&eq.state),
                                        &mut problems,
                                    );
                                }
                            }
                        } else {
                            sw.problems(&o.state, &sw.engine.derive(&o.state), &mut problems);
                        }
                    }
                }
                problems
            });
        }
    }

    // spells: each spell's supports must name some spell list
    let lists: BTreeSet<String> = store
        .iter()
        .filter_map(|s| s.element.spellcasting.as_ref()?.name.clone())
        .collect();
    let spells = of_type("Spell");
    let mut no_list = Vec::new();
    for s in &spells {
        if !s.supports.iter().any(|x| lists.contains(x)) {
            no_list.push(s.id.clone());
        }
    }
    let mut list_sizes = Counts::new();
    for l in &lists {
        let n = store
            .by_type("Spell")
            .filter(|s| s.element.supports.contains(l))
            .count();
        list_sizes.insert(l.clone(), n);
    }

    println!("=== [{label}] engine sweep");
    t.print();
    let mut global = Tally::default();
    for (el, text) in sw.unattributed.borrow().iter() {
        global.record("unattributed", el, vec![text.clone()]);
    }
    println!("--- diagnostics on our elements outside the build (once per element)");
    global.print();
    println!("--- feats not offered on the level 1/4/19 fighter and wizard hosts");
    for (k, v) in &unoffered {
        println!("{v:>6}  {k}  [{}]", unoffered_ex[k].join(" "));
    }
    println!(
        "--- spells: {} of ours, {} name no known spell list [{}]",
        spells.len(),
        no_list.len(),
        no_list
            .iter()
            .take(EXAMPLES)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    );
    let empty: Vec<_> = list_sizes
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(k, _)| k.clone())
        .collect();
    println!(
        "--- spell lists {} total, {} with zero spells: {empty:?}",
        list_sizes.len(),
        empty.len()
    );
    println!("--- level-20 spellcasting (first-option passes)");
    spells_rows.sort();
    spells_rows.dedup();
    for r in spells_rows.iter().filter(|r| r.contains(" - ")).take(80) {
        println!("  {r}");
    }
    println!("=== [{label}] panics: {panics}");
    panics
}

fn feat_hosts(sw: &Sweeper<'_>, class_host: &CharacterState) -> Vec<CharacterState> {
    let mut state = class_host.clone();
    if let Some(Ok(s)) = sw.pick(&state, HUMAN) {
        state = s;
    }
    if let Some(Ok(s)) = sw.pick(&state, HUMAN_VARIANT) {
        state = s;
    }
    let mut state = sw.fill(state, Pick::First);
    let mut out = vec![state.clone()];
    for target in [4, 19] {
        while sw.engine.derive(&state).level < target {
            match commands::level_up_main(sw.engine, &state, None) {
                Ok(o) => state = o.state,
                Err(_) => return out,
            }
        }
        let mut host = state.clone();
        if let Some((key, id)) = feat_option(&sw.engine.derive(&state)) {
            if let Ok(o) = commands::select(sw.engine, &host, &key, Choice::Element(id)) {
                host = o.state;
            }
        }
        out.push(host);
    }
    out
}

/// The open "Feat" option of an ability score improvement select, if any.
fn feat_option(snap: &CharacterSnapshot) -> Option<(SlotKey, String)> {
    snap.slots.iter().find_map(|s| {
        let o = s
            .options
            .iter()
            .find(|o| prereq::is_feat_option_aurora(&o.name))?;
        (s.chosen.is_none() && s.ty == "Class Feature").then(|| (s.key.clone(), o.id.clone()))
    })
}

/// Level up until an ability score improvement offers "Feat" and choose it (the feat slot stays
/// open). `None` when level 20 offers none.
fn open_feat_slot(sw: &Sweeper<'_>, mut state: CharacterState) -> Option<CharacterState> {
    loop {
        let snap = sw.engine.derive(&state);
        if let Some((key, id)) = feat_option(&snap) {
            return commands::select(sw.engine, &state, &key, Choice::Element(id))
                .ok()
                .map(|o| o.state);
        }
        if snap.level >= MAX_LEVEL {
            return None;
        }
        state = commands::level_up_main(sw.engine, &state, None).ok()?.state;
    }
}

/// A variant human on `class_host` who took every feat of `chain` in order (the level-1 feat,
/// then the Feat option of later improvements), with one more feat slot open after the last.
fn chain_host(
    sw: &Sweeper<'_>,
    class_host: &CharacterState,
    chain: &[String],
) -> Option<CharacterState> {
    let mut state = class_host.clone();
    for race in [HUMAN, HUMAN_VARIANT] {
        if let Some(Ok(s)) = sw.pick(&state, race) {
            state = s;
        }
    }
    let mut next = 0;
    loop {
        while let Some(Ok(s)) = chain.get(next).and_then(|id| sw.pick(&state, id)) {
            state = s;
            next += 1;
        }
        state = open_feat_slot(sw, sw.fill(state, Pick::First))?;
        if next == chain.len() {
            return Some(state);
        }
    }
}

/// The next copy of a repeatable element (`X` -> `X_2`, `X_2` -> `X_3`): same type and name.
fn next_copy(store: &ContentStore, id: &str) -> Option<String> {
    let (base, n) = match id.rsplit_once('_') {
        Some((b, d)) if !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()) => {
            (b, d.parse::<u32>().ok()?)
        }
        _ => (id, 1),
    };
    let next = format!("{base}_{}", n + 1);
    let (a, b) = (store.element(id)?, store.element(&next)?);
    (a.ty == b.ty && a.name == b.name).then_some(next)
}

/// `chain` with the next copy of each of its feats taken right after it (not `target` itself).
fn with_next_copies(store: &ContentStore, chain: &[String], target: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in chain {
        out.push(id.clone());
        if let Some(next) = next_copy(store, id) {
            if next != target && !chain.contains(&next) && !out.contains(&next) {
                out.push(next);
            }
        }
    }
    out
}

/// The first element of type `ty` that grants `id`, with the grant's level.
fn granted_by(store: &ContentStore, ty: &str, id: &str) -> Option<(String, u32)> {
    store.by_type(ty).find_map(|se| {
        se.element.rules.iter().find_map(|r| match r {
            Rule::Grant(g) if g.id == id => Some((se.element.id.clone(), g.level.unwrap_or(1))),
            _ => None,
        })
    })
}

/// A variant human `class` who took `archetype` and reached `level` (filling every other
/// choice with its first option).
fn archetype_host(
    sw: &Sweeper<'_>,
    class: &str,
    archetype: &str,
    level: u32,
) -> Option<CharacterState> {
    let mut state = sw.with_class(Ruleset::Dnd2014, class).ok()?;
    for race in [HUMAN, HUMAN_VARIANT] {
        if let Some(Ok(s)) = sw.pick(&state, race) {
            state = s;
        }
    }
    loop {
        if let Some(Ok(s)) = sw.pick(&state, archetype) {
            state = s;
        }
        state = sw.fill(state, Pick::First);
        let snap = sw.engine.derive(&state);
        let has = snap.elements.iter().any(|r| r.element.id == archetype);
        if has && snap.level >= level {
            return Some(state);
        }
        if snap.level >= MAX_LEVEL {
            return None;
        }
        state = commands::level_up_main(sw.engine, &state, None).ok()?.state;
    }
}
