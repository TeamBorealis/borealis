//! Differential character comparison shared by the corpus differentials (acceptance step 3d):
//! `borealis-import-5etools/tests/diff_corpus.rs` (F-full vs bundled SRD) and
//! `src-tauri/tests/diff_corpus.rs` (A-core vs bundled SRD, A-core vs F-full).
//!
//! The build, fill and normalisation are those of `diff_bundled.rs` (same scores, check levels,
//! name-ordered choices and normalised outputs), generalised so that each side of a pair can
//! use its own engine (A-core replaces bundled ids, so it needs a store of its own). `diff_bundled.rs`
//! keeps its private copy; [`LOSSY`] repeats its accepted list verbatim and
//! [`lossy_matches_diff_bundled`] fails when the two drift apart. Content-level only
//! (`borealis_content`, `borealis_engine`), included with `#[path]` by both crates.
#![allow(dead_code)]

use borealis_content::{ContentStore, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::SlotSnapshot;
use borealis_engine::state::SlotKey;
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Base scores STR..CHA (odd values so that +1 increases change modifiers).
pub const SCORES: [i32; 6] = [15, 13, 14, 12, 10, 8];
pub const CHECK_LEVELS: [u32; 5] = [1, 3, 5, 11, 20];
pub const FILL_PASSES: usize = 120;
/// Stack of the test threads (deep settles on the full packs).
pub const STACK: usize = 64 << 20;
/// Slot types filled only with the element the pair names.
pub const NAMED_TYPES: [&str; 5] = ["Race", "Sub Race", "Class", "Archetype", "Background"];

/// An accepted difference: pair label substring, normalised key prefix, check levels (empty: all)
/// and the reason.
pub type Accepted = (&'static str, &'static str, &'static [u32], &'static str);

/// `diff_bundled.rs`'s LOSSY list, verbatim (checked by [`lossy_matches_diff_bundled`]).
pub const LOSSY: &[Accepted] = &[
    (
        "Dnd2014",
        "select.Language",
        &[],
        "5etools encodes the SRD 5.1 Acolyte's 'two of your choice' as anyStandard (bundled: \
         Standard||Exotic); the High Elf extra-language count follows from the Acolyte picks",
    ),
    (
        "Dnd2014 Fighter",
        "select.Class Feature",
        &[],
        "the fixtures hold 1 SRD-flagged 2014 Fighting Style optional feature, the bundled \
         pack 6: Fighting Style option counts differ",
    ),
    (
        "",
        "select.Archetype",
        &[],
        "an imported subclass is also offered by the bundled class of the same name; an \
         imported class offers only imported subclasses",
    ),
];

/// Every LOSSY entry of this copy appears, label, key and reason, in `diff_bundled.rs`.
pub fn lossy_matches_diff_bundled(diff_bundled_source: &str) -> Vec<String> {
    let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    // the source spells long reasons with `\` line continuations
    let source = flat(&diff_bundled_source.replace("\\\n", " "));
    let lossy_block = source
        .split("const LOSSY")
        .nth(1)
        .and_then(|rest| rest.split("const KNOWN_GAPS").next())
        .unwrap_or_default()
        .to_string();
    let mut problems = Vec::new();
    for (label, key, _, why) in LOSSY {
        for part in [*label, *key, &flat(why)[..]] {
            if !lossy_block.contains(part) {
                problems.push(format!(
                    "{label:?} {key:?}: {part:?} not in diff_bundled.rs LOSSY"
                ));
            }
        }
    }
    // one level list (`&[]`, `&[20]`) per entry
    let entries = lossy_block
        .match_indices("&[")
        .filter(|(i, _)| {
            lossy_block[i + 2..]
                .chars()
                .next()
                .is_some_and(|c| c == ']' || c.is_ascii_digit())
        })
        .count();
    if entries != LOSSY.len() {
        problems.push(format!(
            "diff_bundled.rs LOSSY has {entries} entries, this copy {}",
            LOSSY.len()
        ));
    }
    problems
}

/// One side of a pair: the elements a character is built from.
#[derive(Clone, Debug)]
pub struct Side {
    pub race: String,
    pub subrace: Option<String>,
    pub background: String,
    pub class: String,
    pub subclass: String,
}

impl Side {
    pub fn wanted(&self) -> Vec<&str> {
        let mut v = vec![
            self.race.as_str(),
            self.background.as_str(),
            self.class.as_str(),
            self.subclass.as_str(),
        ];
        v.extend(self.subrace.as_deref());
        v
    }
}

/// A pair of characters that must come out alike; `left` is the reference side (bundled SRD,
/// or A-core in the three-way check).
pub struct Pair {
    pub label: String,
    pub ruleset: Ruleset,
    pub left: Side,
    pub right: Side,
}

/// Option names a side prefers (the reference store's names): with a corpus store, an option
/// only the corpus has (sorting before the SRD's) is taken only when no preferred one is offered,
/// so both sides make the same choices. `None`: no preference (as `diff_bundled.rs`).
pub type Prefer<'a> = Option<&'a BTreeSet<String>>;

/// The names of the elements a store loaded from the given packs (the [`Prefer`] set of one
/// edition: `core` + `srd-5.1`, or `core` + `srd-5.2.1`).
pub fn names(store: &ContentStore, packs: &[&str]) -> BTreeSet<String> {
    store
        .iter()
        .filter(|se| packs.iter().any(|p| se.file.starts_with(&format!("{p}/"))))
        .map(|se| se.element.name.clone())
        .collect()
}

/// The bundled packs of an edition's [`Prefer`] set.
pub fn edition_packs(rs: Ruleset) -> [&'static str; 2] {
    if rs == Ruleset::Dnd2024 {
        ["core", "srd-5.2.1"]
    } else {
        ["core", "srd-5.1"]
    }
}

/// The choice for an open slot: the named element when offered, otherwise the option (or
/// list item) with the smallest name, preferred names first. Slots of the [`NAMED_TYPES`] take
/// only named elements.
pub fn choice_for(slot: &SlotSnapshot, wanted: &[&str], prefer: Prefer<'_>) -> Option<Choice> {
    if let Some(o) = slot
        .options
        .iter()
        .find(|o| wanted.contains(&o.id.as_str()))
    {
        return Some(Choice::Element(o.id.clone()));
    }
    if NAMED_TYPES.contains(&slot.ty.as_str()) {
        return None;
    }
    let rank = |name: &str| prefer.is_some_and(|p| !p.contains(name));
    if let Some(o) = slot
        .options
        .iter()
        .min_by(|a, b| (rank(&a.name), &a.name, &a.id).cmp(&(rank(&b.name), &b.name, &b.id)))
    {
        return Some(Choice::Element(o.id.clone()));
    }
    slot.list_items
        .iter()
        .min_by(|a, b| a.1.cmp(&b.1))
        .map(|(id, _)| Choice::ListItem(*id))
}

/// Fill the open slots: every pass applies the choices of one snapshot in slot order; a slot
/// whose choice is refused in a pass that changed nothing is given up.
pub fn fill(
    e: &Engine,
    mut s: CharacterState,
    wanted: &[&str],
    prefer: Prefer<'_>,
) -> CharacterState {
    let mut refused: HashSet<SlotKey> = HashSet::new();
    for _ in 0..FILL_PASSES {
        let snap = e.derive(&s);
        let open: Vec<(SlotKey, Choice)> = snap
            .slots
            .iter()
            .filter(|x| x.chosen.is_none() && x.list_choice.is_none() && !refused.contains(&x.key))
            .filter_map(|x| choice_for(x, wanted, prefer).map(|c| (x.key.clone(), c)))
            .collect();
        if open.is_empty() {
            return s;
        }
        let mut changed = false;
        let mut failed = Vec::new();
        for (key, choice) in open {
            match commands::select(e, &s, &key, choice) {
                Ok(o) => {
                    s = o.state;
                    changed = true;
                }
                Err(_) => failed.push(key),
            }
        }
        if !changed {
            refused.extend(failed);
        }
    }
    panic!("fill did not settle in {FILL_PASSES} passes");
}

/// Normalised, id-free outputs of a snapshot (as `diff_bundled.rs`).
pub fn normalise(snap: &CharacterSnapshot) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("level".into(), snap.level.to_string());
    m.insert("proficiency".into(), snap.proficiency_bonus.to_string());
    m.insert("hp".into(), snap.hit_points.to_string());
    m.insert("ac".into(), snap.armor_class.to_string());
    m.insert("initiative".into(), snap.initiative.to_string());
    m.insert("speed".into(), format!("{:?}", snap.speed));
    for a in &snap.abilities {
        m.insert(format!("ability.{}", a.name), a.score.to_string());
        m.insert(
            format!("save.{}", a.name),
            format!("{} proficient={}", a.save, a.save_proficient),
        );
    }
    for s in &snap.skills {
        m.insert(
            format!("skill.{}", s.name),
            format!(
                "{} proficient={} expertise={}",
                s.total, s.proficient, s.expertise
            ),
        );
    }
    let mut casting: Vec<String> = snap
        .spellcasting
        .iter()
        .filter(|c| c.slots != [0; 9] || c.pact.is_some() || c.prepare_limit.is_some())
        .map(|c| {
            format!(
                "{} slots={:?} pact={:?} prepare_limit={:?}",
                c.ability, c.slots, c.pact, c.prepare_limit
            )
        })
        .collect();
    casting.sort();
    m.insert("casting".into(), format!("{casting:?}"));
    let mut spell_choices: BTreeMap<String, usize> = BTreeMap::new();
    let mut selects: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for s in &snap.slots {
        if s.ty == "Spell" {
            let by = s.spellcasting.clone().unwrap_or_default();
            *spell_choices.entry(by).or_default() += 1;
        } else {
            let n = if s.list_items.is_empty() {
                s.options.len()
            } else {
                s.list_items.len()
            };
            let ty = if s.ty == "Grants" {
                "List"
            } else {
                s.ty.as_str()
            };
            selects.entry(ty.to_string()).or_default().push(n);
        }
    }
    let mut per_block: Vec<usize> = spell_choices.into_values().collect();
    per_block.sort_unstable();
    m.insert("spell_choices".into(), format!("{per_block:?}"));
    for (ty, mut v) in selects {
        v.sort_unstable();
        m.insert(format!("select.{ty}"), format!("{v:?}"));
    }
    m
}

/// Normalised outputs of one snapshot (key -> value).
pub type Normalised = BTreeMap<String, String>;

/// The normalised outputs at [`CHECK_LEVELS`] of a character built from `side`, or why it
/// could not be built.
pub fn build(
    e: &Engine,
    rs: Ruleset,
    side: &Side,
    prefer: Prefer<'_>,
) -> Result<Vec<(u32, Normalised)>, String> {
    let wanted = side.wanted();
    let s = commands::new_character(e, rs);
    let mut s = commands::set_base_scores(e, &s, SCORES)
        .map_err(|err| format!("scores: {err}"))?
        .state;
    for (name, id) in [
        ("Race", side.race.as_str()),
        ("Background", side.background.as_str()),
        ("Class", side.class.as_str()),
    ] {
        let key = slot_key(&e.derive(&s), "ID_LEVEL_1", name, 1).ok_or(format!("{name} slot"))?;
        s = commands::select(e, &s, &key, Choice::Element(id.into()))
            .map_err(|err| format!("{id}: {err}"))?
            .state;
    }
    let mut out = Vec::new();
    for level in 1..=20 {
        if level > 1 {
            s = commands::level_up_main(e, &s, None)
                .map_err(|err| format!("level up to {level}: {err}"))?
                .state;
        }
        s = fill(e, s, &wanted, prefer);
        if CHECK_LEVELS.contains(&level) {
            let snap = e.derive(&s);
            for id in wanted
                .iter()
                .filter(|id| level >= 3 || **id != side.subclass)
            {
                if !snap.elements.iter().any(|r| r.element.id == *id) {
                    return Err(format!("{id} not on the character at level {level}"));
                }
            }
            out.push((level, normalise(&snap)));
        }
    }
    Ok(out)
}

/// One difference between the two sides of a pair.
#[derive(Clone, Debug)]
pub struct Diff {
    pub label: String,
    pub level: u32,
    pub key: String,
    pub left: String,
    pub right: String,
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} L{} {}: left {} / right {}",
            self.label, self.level, self.key, self.left, self.right
        )
    }
}

/// Every difference of a pair (nothing accepted here), or why a side could not be built.
pub fn differences(
    left: &Engine,
    right: &Engine,
    p: &Pair,
    prefer: Prefer<'_>,
) -> Result<Vec<Diff>, String> {
    let l = build(left, p.ruleset, &p.left, prefer).map_err(|e| format!("left side: {e}"))?;
    let r = build(right, p.ruleset, &p.right, prefer).map_err(|e| format!("right side: {e}"))?;
    let mut out = Vec::new();
    for ((level, a), (_, b)) in l.iter().zip(&r) {
        let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for k in keys {
            let (av, bv) = (a.get(k), b.get(k));
            if av == bv {
                continue;
            }
            out.push(Diff {
                label: p.label.clone(),
                level: *level,
                key: k.clone(),
                left: av.cloned().unwrap_or_else(|| "-".into()),
                right: bv.cloned().unwrap_or_else(|| "-".into()),
            });
        }
    }
    Ok(out)
}

/// Index of the entry of `list` that covers a difference, if any.
pub fn covered(list: &[Accepted], d: &Diff) -> Option<usize> {
    list.iter().position(|(l, k, levels, _)| {
        d.label.contains(l)
            && d.key.starts_with(k)
            && (levels.is_empty() || levels.contains(&d.level))
    })
}

/// The option counts of a `select.*` value (`[2, 5]`), sorted.
pub fn counts(v: &str) -> Option<Vec<usize>> {
    if v == "-" {
        return Some(Vec::new());
    }
    let inner = v.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner.split(',').map(|x| x.trim().parse().ok()).collect()
}

/// `right` offers at least what `left` does on a select of the same shape: the same number of
/// selects, each (sorted) with at least as many options.
pub fn dominates(left: &str, right: &str) -> bool {
    match (counts(left), counts(right)) {
        (Some(l), Some(r)) => l.len() == r.len() && l.iter().zip(&r).all(|(a, b)| b >= a),
        _ => false,
    }
}

/// Run `f` over every pair on `threads` worker threads (big stacks), keeping pair order.
pub fn par_map<T: Send>(pairs: &[Pair], threads: usize, f: &(dyn Fn(&Pair) -> T + Sync)) -> Vec<T> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<T>>> =
        pairs.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.max(1))
            .map(|_| {
                std::thread::Builder::new()
                    .stack_size(STACK)
                    .spawn_scoped(scope, || loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let Some(p) = pairs.get(i) else { break };
                        let out = f(p);
                        *slots[i].lock().expect("slot") = Some(out);
                    })
                    .expect("spawn")
            })
            .collect();
        for w in workers {
            w.join().expect("pair worker");
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().expect("slot").expect("every pair ran"))
        .collect()
}

/// Worker threads for the pair builds (`BOREALIS_DIFF_THREADS`, default 4).
pub fn threads() -> usize {
    std::env::var("BOREALIS_DIFF_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4)
}

/// The element of `ty` named `name` whose id starts with `prefix`, if exactly one exists.
pub fn by_name(store: &ContentStore, ty: &str, name: &str, prefix: &str) -> Result<String, String> {
    let hits: Vec<&str> = store
        .iter()
        .map(|se| &se.element)
        .filter(|e| e.ty == ty && e.name == name && e.id.starts_with(prefix))
        .map(|e| e.id.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).to_string()),
        [] => Err(format!("no {ty} named {name:?} with id prefix {prefix}")),
        many => Err(format!(
            "{} {ty} named {name:?} with prefix {prefix}",
            many.len()
        )),
    }
}

/// Debug view of a side at `level` (`BOREALIS_DIFF_PROBE`): slots, features and spellcasting.
pub fn probe(e: &Engine, rs: Ruleset, side: &Side, level: u32, prefer: Prefer<'_>) -> Vec<String> {
    let wanted = side.wanted();
    let mut out = Vec::new();
    let s = commands::new_character(e, rs);
    let Ok(o) = commands::set_base_scores(e, &s, SCORES) else {
        return vec!["scores refused".into()];
    };
    let mut s = o.state;
    for (name, id) in [
        ("Race", side.race.as_str()),
        ("Background", side.background.as_str()),
        ("Class", side.class.as_str()),
    ] {
        let Some(key) = slot_key(&e.derive(&s), "ID_LEVEL_1", name, 1) else {
            return vec![format!("no {name} slot")];
        };
        match commands::select(e, &s, &key, Choice::Element(id.into())) {
            Ok(o) => s = o.state,
            Err(err) => return vec![format!("{id}: {err}")],
        }
    }
    for l in 1..=level {
        if l > 1 {
            match commands::level_up_main(e, &s, None) {
                Ok(o) => s = o.state,
                Err(err) => return vec![format!("level up: {err}")],
            }
        }
        s = fill(e, s, &wanted, prefer);
    }
    let snap = e.derive(&s);
    for sl in &snap.slots {
        let opts: Vec<&str> = sl.options.iter().take(6).map(|o| o.name.as_str()).collect();
        let items: Vec<&str> = sl.list_items.iter().take(4).map(|i| i.1.as_str()).collect();
        out.push(format!(
            "slot ty={:?} name={:?} owner={} #{} chosen={:?} list={:?} options={} {:?} items={:?}",
            sl.ty,
            sl.name,
            sl.owner.id,
            sl.number,
            sl.chosen.as_ref().map(|c| c.id.as_str()),
            sl.list_choice,
            sl.options.len().max(sl.list_items.len()),
            opts,
            items
        ));
    }
    for f in &snap.features {
        out.push(format!("feature {} L{} {:?}", f.id, f.level, f.name));
    }
    for c in &snap.spellcasting {
        out.push(format!(
            "casting {:?} {} slots={:?} prepare_limit={:?} spells={}",
            c.name,
            c.ability,
            c.slots,
            c.prepare_limit,
            c.spells.len()
        ));
    }
    for (k, v) in &snap.stats {
        if k.starts_with("ac")
            || k.contains("armor")
            || k.contains("hp")
            || k.contains("hit")
            || k.contains("saving")
            || k.contains(":save")
        {
            out.push(format!("stat {k} = {} {:?}", v.value, v.sources));
        }
    }
    out
}

/// Who owns a classified difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// Borealis bug in the 5etools importer (`borealis-import-5etools`).
    Importer,
    /// Borealis bug in the Aurora importer (`borealis-migrate`).
    Migrate,
    /// Borealis bug in `borealis-engine`.
    Engine,
    /// Borealis bug in a bundled SRD pack (`content/srd-5.1`, `content/srd-5.2.1`).
    Bundled,
    /// The books (or the source's data encoding) genuinely differ: accepted.
    Source,
}

/// A classified difference: finding id, pair label substring, normalised key prefixes, check
/// levels (empty: all), owner and reason (repro / cited source field).
pub type Finding = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [u32],
    Owner,
    &'static str,
);

/// Index of the finding of `list` that covers a difference, if any.
pub fn classify(list: &[Finding], d: &Diff) -> Option<usize> {
    list.iter().position(|(_, l, keys, levels, _, _)| {
        d.label.contains(l)
            && keys.iter().any(|k| d.key.starts_with(k))
            && (levels.is_empty() || levels.contains(&d.level))
    })
}

/// Outcome of comparing every pair against the accepted and classified lists.
#[derive(Default)]
pub struct Verdicts {
    /// accepted entry -> differences it covered
    pub accepted: BTreeMap<String, usize>,
    /// finding index -> differences it covered
    pub classified: BTreeMap<usize, usize>,
    pub unclassified: Vec<String>,
    pub build_errors: Vec<String>,
}

impl Verdicts {
    /// Print the summary; returns the failure messages (unclassified, Borealis bugs observed,
    /// stale findings).
    /// `stale_check`: an entry whose label applies to a pair but that nothing matched fails.
    pub fn report(
        &self,
        findings: &[Finding],
        labels: &[String],
        stale_check: bool,
    ) -> Vec<String> {
        println!("accepted differences by entry: {:?}", self.accepted);
        let mut fails = Vec::new();
        for (i, (id, l, keys, _, owner, why)) in findings.iter().enumerate() {
            let applies = labels.iter().any(|x| x.contains(l));
            match self.classified.get(&i) {
                Some(n) => {
                    println!("  FINDING {id} {owner:?} ({n} differences, {l:?} {keys:?}): {why}");
                    if *owner != Owner::Source {
                        fails.push(format!("{id}: {owner:?} bug observed ({n} differences)"));
                    }
                }
                None if applies && stale_check => {
                    println!("  STALE {id} {l:?} {keys:?}");
                    fails.push(format!("{id}: stale (no longer observed)"));
                }
                None => {}
            }
        }
        for e in &self.build_errors {
            println!("  BUILD {e}");
            fails.push(format!("build: {e}"));
        }
        for u in &self.unclassified {
            println!("  UNCLASSIFIED {u}");
        }
        if !self.unclassified.is_empty() {
            fails.push(format!(
                "{} unclassified differences",
                self.unclassified.len()
            ));
        }
        fails
    }
}
