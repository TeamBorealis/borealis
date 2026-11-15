//! Spec 18.2 re-creation (WB-27a), the creator's core guarantee: every bundled element can be made again **only from
//! the editable form fields**, and what the engine makes of it is the same.
//!
//! For every element of every bundled pack (`core`, `srd-5.1`, `srd-5.2.1` and both monster packs; the rules-text
//! packs hold documents, no elements), its form is read as the creator opens it (a parent with its children:
//! `element_tree_to_form` over its pack, every element not a child of another being a root), and then made new
//! ([`support::recreated`]): the stored text of every field and block forgotten, the preserved bucket of the form and
//! of every block emptied (so the 2024 mirror keys no field writes are dropped, and the mirrors a field writes are no
//! longer written, since the element "never had them"), only the ids kept. The forms are written
//! (`form_to_elements`), and the re-created elements replace the originals in a second library.
//!
//! Then the same test characters ([`probe`]) are derived in both libraries and their snapshots (stats, features,
//! choices offered, slots, spells, attacks, inventory: the whole `CharacterSnapshot`) must be equal:
//!
//! * every class 1-20 and every subclass on its class (the WB-07 ladder, picks made at every level);
//! * every re-created element that differs from its original and that those ladders do not apply is taken on its own:
//!   an origin or feat on a level 1 or 4 fighter, an option at the ladder level that offers it, an item added and
//!   equipped; what no character takes is checked by a plain derive (the `load_into_engine` path).
//!
//! Elements that cannot be re-created would be listed with their reason in [`EXCEPTIONS`] (T6: an asserted, reviewed
//! list; checked both ways: a listed element that passes fails the test too). The target is an empty list for
//! `srd-5.2.1`. The counts are printed (`--nocapture`) and written to `target/recreation.txt`.
//!
//! The snapshots are compared as JSON in a canonical form ([`probe::canonical`]): what only orders a list or labels a
//! spell choice, white space in text, and a save DC's formula text (its value is compared) are left out.
//!
//! About 30 s in a test build on 10 cores (the workspace's test profile builds the engine, the content store and the
//! dependencies at opt-level 1; at 0 it took 4.5 minutes), 15 s in release
//! (`cargo test --release -p borealis-authoring --test recreation -- --nocapture`). Debugging aids:
//! `BOREALIS_RECREATION_DUMP=<id>,<id>` prints elements before and after (`*`: every differing one, to
//! `target/recreation-dump.jsonl`), `BOREALIS_RECREATION_CLIP=<n>` widens the differences printed.

mod support;

#[path = "recreation/probe.rs"]
mod probe;

use borealis_authoring::check::sweep_targets;
use borealis_authoring::form::{
    element_tree_to_form, form_to_elements, CatalogBlockCodec, FormContext,
};
use borealis_authoring::pack::parent_of;
use borealis_content::{ContentStore, ContentStoreBuilder, Element, ParsedPack, Ruleset};
use borealis_engine::Engine;
use probe::{Probe, Rung};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use support::{bundled_packs, recreated, PackDir};

/// Elements whose re-creation is not engine-equivalent, with the reason (id, pack, why). Reviewed.
const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
    "ID_2024_PROFILE_TABLES",
    "srd-5.2.1",
    "the 2024 rules tables (proficiency bonus, point buy, standard arrays, languages, multiclass slots) the engine \
     reads from its `data`: a Core element, engine plumbing the creator does not author (spec 3.1 \"Not authored\"); \
     the generic form opens it and keeps the tables in the preserved bucket",
    ),
];

const ITEM_TYPES: [&str; 4] = ["Item", "Weapon", "Armor", "Magic Item"];

/// One bundled element with where it lives.
struct Entry {
    pack: String,
    ruleset: Option<Ruleset>,
    element: Element,
}

/// Every bundled element, with its effective edition (its own, its file's, its pack's only one).
fn entries(packs: &[PackDir]) -> Vec<Entry> {
    let mut out = Vec::new();
    for p in packs {
        let pack_ruleset = (p.manifest.rulesets.len() == 1).then(|| p.manifest.rulesets[0]);
        for (_, f) in &p.files {
            for e in &f.elements {
                let ruleset = e
                    .ruleset
                    .or(f.ruleset)
                    .or(pack_ruleset)
                    .filter(|r| *r != Ruleset::Any);
                out.push(Entry {
                    pack: p.id().to_string(),
                    ruleset,
                    element: e.clone(),
                });
            }
        }
    }
    out
}

/// The editions a test character of `e` is made in (both for an edition-less element).
fn rulesets(e: &Entry) -> Vec<Ruleset> {
    e.ruleset
        .map_or_else(|| vec![Ruleset::Dnd2014, Ruleset::Dnd2024], |r| vec![r])
}

/// What re-creating every element gave: the new elements by id, and the forms that did not write.
struct Remade {
    made: HashMap<String, Element>,
    problems: Vec<String>,
}

/// Re-create every element of `packs`: each root with its children, as the creator opens it.
fn remake(packs: &[PackDir], cx: &FormContext<'_>) -> Remade {
    let mut made = HashMap::new();
    let mut problems = Vec::new();
    for p in packs {
        let els = p.elements();
        for root in els.iter().filter(|e| parent_of(e, &els).is_none()) {
            let form = element_tree_to_form(root, &els, cx);
            match form_to_elements(&recreated(&form), cx) {
                Ok(out) => {
                    for e in out {
                        made.entry(e.id.clone()).or_insert(e);
                    }
                }
                Err(ps) => problems.push(format!(
                    "{} {}: the form does not write: {ps:?}",
                    p.id(),
                    root.id
                )),
            }
        }
        for e in &els {
            if !made.contains_key(&e.id) {
                problems.push(format!("{} {}: no form re-creates it", p.id(), e.id));
            }
        }
    }
    Remade { made, problems }
}

/// The bundled library with `made` in place of the originals (same packs, files and order).
fn library(packs: &[PackDir], made: &HashMap<String, Element>) -> ContentStore {
    let mut b = ContentStoreBuilder::new();
    for p in packs {
        let files = p
            .files
            .iter()
            .map(|(path, f)| {
                let mut f = f.clone();
                f.elements = f
                    .elements
                    .iter()
                    .map(|e| made.get(&e.id).cloned().unwrap_or_else(|| e.clone()))
                    .collect();
                (path.clone(), f)
            })
            .collect();
        b = b.add_parsed_pack(ParsedPack::from_parts(p.manifest.clone(), files));
    }
    b.build()
}

/// Every id a snapshot names (applied or offered).
fn mentioned_ids(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::String(s) if s.starts_with("ID_") => {
            out.insert(s.clone());
        }
        Value::Object(m) => m.values().for_each(|x| mentioned_ids(x, out)),
        Value::Array(items) => items.iter().for_each(|x| mentioned_ids(x, out)),
        _ => {}
    }
}

/// The ids a snapshot applies (everything but the options a choice merely lists).
fn applied_ids(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::String(s) if s.starts_with("ID_") => {
            out.insert(s.clone());
        }
        Value::Object(m) => {
            for (k, x) in m {
                if k != "options" {
                    applied_ids(x, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|x| applied_ids(x, out)),
        _ => {}
    }
}

/// Every ladder: each class, and each subclass on the class that offers it.
fn ladders(entries: &[Entry], store: &ContentStore) -> Vec<Probe> {
    let mut out = Vec::new();
    for e in entries {
        for ruleset in rulesets(e) {
            match e.element.ty.as_str() {
                "Class" => out.push(Probe::Ladder {
                    ruleset,
                    class: e.element.id.clone(),
                    subclass: None,
                }),
                "Archetype" => {
                    for t in sweep_targets(std::slice::from_ref(&e.element), store, ruleset) {
                        out.push(Probe::Ladder {
                            ruleset,
                            class: t.class_id,
                            subclass: t.subclass_id,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// `f` over `items` on every core, results in the order of `items`. Each worker takes the next item when it is free
/// (a ladder 1-20 costs many times a plain probe, so fixed chunks left most cores idle behind the slowest one).
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            return out;
                        };
                        out.push((i, f(item)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// A library and what the ladders and hosts made in it, so probes can start from their states.
struct World {
    engine: Engine,
    rungs: HashMap<Probe, Vec<Rung>>,
    hosts: HashMap<
        Ruleset,
        Vec<(
            u32,
            borealis_engine::CharacterState,
            borealis_engine::CharacterSnapshot,
        )>,
    >,
}

impl World {
    fn new(store: &ContentStore, ladders: &[Probe]) -> Self {
        let engine = Engine::new(store);
        let rungs = parallel(ladders, |l| match l {
            Probe::Ladder {
                ruleset,
                class,
                subclass,
            } => probe::ladder(&engine, *ruleset, class, subclass.as_deref()),
            _ => Vec::new(),
        })
        .into_iter()
        .zip(ladders)
        .map(|(r, l)| (l.clone(), r))
        .collect();
        let hosts = [Ruleset::Dnd2014, Ruleset::Dnd2024]
            .into_iter()
            .map(|r| (r, probe::hosts(&engine, r)))
            .collect();
        Self {
            engine,
            rungs,
            hosts,
        }
    }

    /// The origins the level-1 hosts offer (a species, race or background each), as the probes that take them.
    fn origins(&self) -> Vec<Probe> {
        let mut out = Vec::new();
        for ruleset in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
            for (level, _, snap) in self.hosts[&ruleset].iter().filter(|(l, _, _)| *l == 1) {
                for s in snap
                    .slots
                    .iter()
                    .filter(|s| s.chosen.is_none() && probe::ORIGIN_TYPES.contains(&s.ty.as_str()))
                {
                    out.extend(s.options.iter().map(|o| Probe::Take {
                        ruleset,
                        level: *level,
                        id: o.id.clone(),
                    }));
                }
            }
        }
        out.sort_by_key(Probe::label);
        out.dedup();
        out
    }

    /// The one-level ladders of `origins` (a host that took each), so their choices are probed like a ladder's.
    fn with_origins(mut self, origins: &[Probe]) -> Self {
        let made = parallel(origins, |p| {
            let Probe::Take { ruleset, level, id } = p else {
                return Vec::new();
            };
            self.hosts[ruleset]
                .iter()
                .filter(|(l, _, _)| l == level)
                .find_map(|(_, state, snap)| {
                    let key = probe::offering(snap, id)?;
                    probe::origin_rung(&self.engine, state, &key, id)
                })
                .into_iter()
                .collect()
        });
        self.rungs.extend(origins.iter().cloned().zip(made));
        self
    }

    /// The snapshots of `p` in this library.
    fn run(&self, p: &Probe) -> Vec<Value> {
        let e = &self.engine;
        match p {
            Probe::Ladder { .. } => self.rungs[p]
                .iter()
                .map(|r| probe::json(&r.snapshot))
                .collect(),
            Probe::Option {
                ladder,
                level,
                option,
            } => self
                .rungs
                .get(ladder.as_ref())
                .into_iter()
                .flatten()
                .find(|r| r.level == *level)
                .and_then(|r| probe::take_at(e, &r.before, r.open.get(option)?, option))
                .iter()
                .map(probe::json)
                .collect(),
            Probe::Take { ruleset, level, id } => self.hosts[ruleset]
                .iter()
                .filter(|(l, _, _)| l == level)
                .filter_map(|(_, state, snap)| {
                    let key = probe::offering(snap, id)?;
                    probe::take_at(e, state, &key, id)
                })
                .map(|s| probe::json(&s))
                .collect(),
            Probe::Item { ruleset, id } => probe::item(e, *ruleset, id)
                .iter()
                .map(probe::json)
                .collect(),
            Probe::Blank { ruleset } => vec![probe::json(&probe::blank(e, *ruleset))],
        }
    }

    /// The probe that takes `id` (a differing element no ladder applies): an item, an origin or feat of a host, an
    /// option of a ladder, else a plain derive.
    fn probe_for(&self, e: &Entry, ruleset: Ruleset) -> Probe {
        let id = &e.element.id;
        if ITEM_TYPES.contains(&e.element.ty.as_str()) {
            return Probe::Item {
                ruleset,
                id: id.clone(),
            };
        }
        for (level, _, snap) in &self.hosts[&ruleset] {
            if probe::offering(snap, id).is_some() {
                return Probe::Take {
                    ruleset,
                    level: *level,
                    id: id.clone(),
                };
            }
        }
        let mut ladders: Vec<&Probe> = self.rungs.keys().collect();
        ladders.sort_by_key(|p| p.label());
        for l in ladders {
            let in_edition = match l {
                Probe::Ladder { ruleset: r, .. } | Probe::Take { ruleset: r, .. } => *r == ruleset,
                _ => false,
            };
            if !in_edition {
                continue;
            }
            for r in &self.rungs[l] {
                if r.open.contains_key(id) {
                    return Probe::Option {
                        ladder: Box::new(l.clone()),
                        level: r.level,
                        option: id.clone(),
                    };
                }
            }
        }
        Probe::Blank { ruleset }
    }
}

/// Where two canonical JSON values differ: the first path; a list of objects (a multiset) names what only one side
/// has.
fn first_difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(v), Some(w)) => first_difference(v, w, &format!("{path}.{k}")),
                (v, w) => Some(format!("{path}.{k}: {v:?} vs {w:?}")),
            })
        }
        (Value::Array(x), Value::Array(y)) if x.iter().chain(y).all(Value::is_object) => {
            let only_a: Vec<&Value> = x.iter().filter(|v| !y.contains(v)).collect();
            let only_b: Vec<&Value> = y.iter().filter(|v| !x.contains(v)).collect();
            let name = |v: &Value| names(&[v]);
            let paired = only_a.iter().find_map(|v| {
                let w = only_b.iter().find(|w| name(w) == name(v))?;
                first_difference(v, w, &format!("{path}[{}]", name(v)))
            });
            match (only_a.as_slice(), only_b.as_slice()) {
                ([], []) => None,
                ([v], [w]) => first_difference(v, w, &format!("{path}[*]")),
                _ if paired.is_some() => paired,
                _ => Some(format!(
                    "{path}: only before [{}]; only after [{}]",
                    clip(&names(&only_a)),
                    clip(&names(&only_b))
                )),
            }
        }
        (Value::Array(x), Value::Array(y)) => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (v, w))| first_difference(v, w, &format!("{path}[{i}]")))
            .or_else(|| {
                (x.len() != y.len()).then(|| format!("{path}: {} items vs {}", x.len(), y.len()))
            }),
        _ if a == b => None,
        _ => Some(format!(
            "{path}: {} vs {}",
            clip(&a.to_string()),
            clip(&b.to_string())
        )),
    }
}

/// Short names of listed objects: an element's id, an id, a name, else the JSON.
fn names(items: &[&Value]) -> String {
    items
        .iter()
        .map(|v| {
            let named = [&v["element"]["id"], &v["id"], &v["name"]]
                .into_iter()
                .find_map(|x| x.as_str().map(str::to_string));
            let extra = v
                .get("parent")
                .and_then(Value::as_str)
                .map(|p| format!(" (from {p})"));
            named.map_or_else(|| v.to_string(), |n| n + &extra.unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_text<T: serde::Serialize>(x: &T) -> String {
    serde_json::to_string(x).unwrap_or_default()
}

fn clip(s: &str) -> String {
    let limit = std::env::var("BOREALIS_RECREATION_CLIP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    s.chars().take(limit).collect()
}

/// The other way round (T6): each listed element, re-created alone, must still differ for some test character that
/// takes it (else it passes and must leave the list), and must be in the pack it is listed for.
fn check_exceptions(
    packs: &[PackDir],
    entries: &[Entry],
    remade: &Remade,
    before: &World,
    showing: &HashMap<&str, Vec<Probe>>,
    origins: &[Probe],
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (id, pack, _) in EXCEPTIONS {
        let Some(e) = entries.iter().find(|e| e.element.id == *id) else {
            wrong.push(format!("{id}: no bundled element has this id"));
            continue;
        };
        if e.pack != *pack {
            wrong.push(format!("{id}: listed for {pack}, it is in {}", e.pack));
        }
        let Some(r) = remade.made.get(*id).filter(|r| **r != e.element) else {
            wrong.push(format!("{id}: re-created identically"));
            continue;
        };
        let mut probes: Vec<Probe> = showing.get(*id).cloned().unwrap_or_default();
        for ruleset in rulesets(e) {
            probes.push(Probe::Blank { ruleset });
            probes.push(before.probe_for(e, ruleset));
        }
        let needed: Vec<Probe> = probes
            .iter()
            .filter_map(|p| match p {
                Probe::Ladder { .. } => Some(p.clone()),
                Probe::Option { ladder, .. } => Some(ladder.as_ref().clone()),
                _ => None,
            })
            .collect();
        let alone = World::new(
            &library(packs, &HashMap::from([((*id).to_string(), r.clone())])),
            &needed,
        )
        .with_origins(origins);
        if probes.iter().all(|p| before.run(p) == alone.run(p)) {
            wrong.push(format!(
                "{id}: engine-equivalent now (remove it from the list)"
            ));
        }
    }
    wrong
}

#[test]
fn every_bundled_element_recreated_from_its_form_is_engine_equivalent() {
    let packs = bundled_packs();
    let entries = entries(&packs);
    let original = library(&packs, &HashMap::new());
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&original, &codec);
    let remade = remake(&packs, &cx);
    let listed: HashSet<&str> = EXCEPTIONS.iter().map(|(id, _, _)| *id).collect();
    let made_main: HashMap<String, Element> = remade
        .made
        .iter()
        .filter(|(id, _)| !listed.contains(id.as_str()))
        .map(|(id, e)| (id.clone(), e.clone()))
        .collect();
    let again = library(&packs, &made_main);

    let differing: HashSet<&str> = entries
        .iter()
        .filter(|e| !listed.contains(e.element.id.as_str()))
        .filter(|e| remade.made.get(&e.element.id) != Some(&e.element))
        .map(|e| e.element.id.as_str())
        .collect();

    // debugging aid: BOREALIS_RECREATION_DUMP=<id>,<id> prints those elements before and after
    // (`*`: every element that differs, one JSON line each, to target/recreation-dump.jsonl)
    if std::env::var("BOREALIS_RECREATION_DUMP").as_deref() == Ok("*") {
        let lines: Vec<String> = entries
            .iter()
            .filter(|e| differing.contains(e.element.id.as_str()))
            .map(|e| json_text(&(&e.element, remade.made.get(&e.element.id))))
            .collect();
        let _ = std::fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/recreation-dump.jsonl"),
            lines.join("\n"),
        );
    }
    for id in std::env::var("BOREALIS_RECREATION_DUMP")
        .unwrap_or_default()
        .split(',')
        .filter(|x| !x.is_empty() && *x != "*")
    {
        let was = entries
            .iter()
            .find(|e| e.element.id == id)
            .map(|e| &e.element);
        println!(
            "DUMP {id}\n  was {}\n  now {}",
            json_text(&was),
            json_text(&remade.made.get(id))
        );
    }
    let started = std::time::Instant::now();
    let ladders = ladders(&entries, &original);
    // the two libraries' ladders are made side by side (each on every core), so no core waits for a long ladder
    let (before, after) = std::thread::scope(|s| {
        let after = s.spawn(|| World::new(&again, &ladders));
        let before = World::new(&original, &ladders);
        (
            before,
            after.join().expect("the re-created library's ladders"),
        )
    });
    let origins = before.origins();
    let before = before.with_origins(&origins);
    let after = after.with_origins(&origins);
    println!(
        "ladders: {} and {} origins in {:?}",
        ladders.len(),
        origins.len(),
        started.elapsed()
    );

    // what the ladders and hosts apply already
    let mut covered = HashSet::new();
    for rungs in before.rungs.values() {
        for r in rungs {
            applied_ids(&probe::json(&r.snapshot), &mut covered);
        }
    }
    let mut probes: Vec<Probe> = ladders.iter().chain(&origins).cloned().collect();
    let mut seen: HashSet<Probe> = probes.iter().cloned().collect();
    let mut how: BTreeMap<String, usize> = BTreeMap::new();
    let mut plain: BTreeSet<&str> = BTreeSet::new();
    for e in entries
        .iter()
        .filter(|e| differing.contains(e.element.id.as_str()))
    {
        if covered.contains(&e.element.id) {
            *how.entry("applied by a ladder".into()).or_default() += 1;
            continue;
        }
        for ruleset in rulesets(e) {
            let p = before.probe_for(e, ruleset);
            let kind = match &p {
                Probe::Item { .. } => "item added and equipped",
                Probe::Take { .. } => "taken by a host fighter",
                Probe::Option { .. } => "taken at a ladder's choice",
                Probe::Blank { .. } => "plain derive",
                Probe::Ladder { .. } => "ladder",
            };
            *how.entry(kind.into()).or_default() += 1;
            if matches!(p, Probe::Blank { .. }) {
                *how.entry(format!("plain derive: {}", e.element.ty))
                    .or_default() += 1;
                plain.insert(e.element.id.as_str());
            }
            if seen.insert(p.clone()) {
                probes.push(p);
            }
        }
    }

    // run every probe in both libraries
    let outcomes = parallel(&probes, |p| {
        let (a, b) = (before.run(p), after.run(p));
        let mut mentioned = HashSet::new();
        a.iter().for_each(|v| mentioned_ids(v, &mut mentioned));
        let difference = (a != b).then(|| {
            a.iter()
                .zip(&b)
                .find_map(|(x, y)| first_difference(x, y, ""))
                .unwrap_or_else(|| format!("{} snapshots vs {}", a.len(), b.len()))
        });
        (difference, mentioned)
    });
    let failing: BTreeMap<String, String> = probes
        .iter()
        .zip(&outcomes)
        .filter_map(|(p, (d, _))| Some((p.label(), d.clone()?)))
        .collect();
    // per listed element, the probes whose characters show it (taken, granted or offered)
    let showing: HashMap<&str, Vec<Probe>> = EXCEPTIONS
        .iter()
        .map(|(id, _, _)| {
            let ps = probes
                .iter()
                .zip(&outcomes)
                .filter(|(_, (_, ids))| ids.contains(*id))
                .map(|(p, _)| p.clone())
                .collect();
            (*id, ps)
        })
        .collect();

    println!("probes: {} in {:?}", probes.len(), started.elapsed());
    let listed_wrongly = check_exceptions(&packs, &entries, &remade, &before, &showing, &origins);
    let mut report = String::new();
    let _ = writeln!(
        report,
        "re-creation: {} elements, {} re-created identically, {} differ, {} probes, {} failing, {} form problems",
        entries.len(),
        entries.len() - differing.len(),
        differing.len(),
        probes.len(),
        failing.len(),
        remade.problems.len()
    );
    for (k, n) in &how {
        let _ = writeln!(report, "  differing elements {k}: {n}");
    }
    for p in &remade.problems {
        let _ = writeln!(report, "  PROBLEM {p}");
    }
    for (p, d) in &failing {
        let _ = writeln!(report, "  FAIL {p}\n      {d}");
    }
    let _ = writeln!(
        report,
        "  plain derive only: {}",
        plain.iter().copied().collect::<Vec<_>>().join(", ")
    );
    let _ = writeln!(report, "  exceptions: {} listed", EXCEPTIONS.len());
    for (id, pack, why) in EXCEPTIONS {
        let _ = writeln!(report, "    {pack} {id}: {why}");
    }
    for m in &listed_wrongly {
        let _ = writeln!(report, "  WRONGLY LISTED {m}");
    }
    println!("{report}");
    let _ = std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/recreation.txt"),
        &report,
    );
    assert!(
        remade.problems.is_empty() && failing.is_empty() && listed_wrongly.is_empty(),
        "see target/recreation.txt"
    );
}
