//! Local-only item audit over a real, full 5etools `data/` folder. Enabled by
//! `BOREALIS_5ETOOLS_DIR` (the `data/` directory of a 5etools checkout); skipped otherwise.
//! Prints ids and counts only, never content text (C4/AI14). Run it in release mode:
//!
//! ```text
//! BOREALIS_5ETOOLS_DIR=/path/to/5etools/data cargo test --release -p borealis \
//!     --test fivetools_items -- --nocapture
//! ```
//!
//! All sources are imported as the app does (bundled SRD dirs, fluff on). Every imported item
//! (every element of type Weapon, Armor, Magic Item or Item) is added to a level-5 fighter of
//! its edition, equipped (and so attuned) when it can be, and settled; panics, settle-invariant
//! violations and unread rule fields are counted. Values the engine computes are compared with
//! expectations derived from the 5etools source fields of the same entry (`+N` weapons, armor
//! AC formulas, ability scores, saves, defences, speeds, charges, attunement, spells, masteries,
//! cost, weight...). Equipment packs and item groups must point at real items. Hard
//! assertions: no panics, no dangling pack/group references and, on a full run (no
//! `BOREALIS_5ETOOLS_ITEM_LIMIT`), a ratchet: no wrong values beyond the known ones and no more
//! "not modelled" checks in the converted categories. Value checks are reported per category
//! (checked / correct / wrong / not modelled).

#[path = "fivetools_items/checks.rs"]
mod checks;
#[path = "fivetools_items/hosts.rs"]
mod hosts;
#[path = "fivetools_items/support.rs"]
mod support;

use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use borealis_engine::commands;
use borealis_engine::{invariants, CharacterState, Engine};
use borealis_import_5etools::{import_folder, ImportOptions};
use checks::{verdict, verdict_nm, Check};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;
use support::{books_table, ids, raw_index, Base, Counts, RawItem, Tally, Verdict, ITEM_TYPES};

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}

#[test]
fn five_etools_item_audit() {
    let Ok(dir) = std::env::var("BOREALIS_5ETOOLS_DIR") else {
        eprintln!("five_etools_item_audit: BOREALIS_5ETOOLS_DIR not set, skipping");
        return;
    };
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || run(Path::new(&dir)))
        .expect("spawn");
    handle.join().expect("audit thread");
}

/// Everything one item check needs.
struct Ctx<'a> {
    engine: &'a Engine,
    bases: HashMap<Ruleset, Base>,
    /// Eligible hosts of items with an attunement restriction.
    hosts: hosts::Hosts,
    /// Imported elements by id.
    ours: &'a HashMap<String, (Element, Ruleset)>,
}

/// Counters of the engine pass, by item category.
#[derive(Default)]
struct Pass {
    tried: Counts,
    equipped: Counts,
    carried_only: Counts,
    equip_refused: BTreeMap<String, Vec<String>>,
    panics: Vec<String>,
    /// invariant kind -> item ids
    invariants: BTreeMap<String, Vec<String>>,
    /// item ids whose own rules carry unread fields or kinds
    unread: Vec<String>,
}

fn run(data: &Path) {
    let t0 = Instant::now();
    let options = ImportOptions {
        include_fluff: true,
        srd_dirs: vec![
            content_dir().join("srd-5.1"),
            content_dir().join("srd-5.2.1"),
        ],
        ..ImportOptions::default()
    };
    let result = import_folder(data, &options).expect("import");
    let t_import = t0.elapsed();
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        builder = builder.add_pack_dir(&content_dir().join(pack)).unwrap();
    }
    let mut ours: HashMap<String, (Element, Ruleset)> = HashMap::new();
    for pack in &result.packs {
        builder = builder
            .add_pack_dir(&pack.write_to(tmp.path()).unwrap())
            .unwrap();
        for e in pack.elements() {
            let rs = e.ruleset.or(pack.ruleset).unwrap_or(Ruleset::Dnd2014);
            ours.insert(e.id.clone(), (e.clone(), rs));
        }
    }
    let store = builder.build();
    let engine = Engine::new(&store);
    let mut bases = HashMap::new();
    for rs in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        bases.insert(rs, Base::new(&engine, rs));
    }
    let cx = Ctx {
        engine: &engine,
        bases,
        hosts: hosts::Hosts::default(),
        ours: &ours,
    };
    println!(
        "=== 5etools item audit: import {:.1}s, {} packs, {} elements",
        t_import.as_secs_f64(),
        result.packs.len(),
        ours.len()
    );

    // raw 5etools entries (resolved as the importer does) matched to their elements
    let raw = raw_index(data, &ours);
    println!(
        "raw item entries: {} matched to an element, {} without element, {} ambiguous",
        raw.by_id.len(),
        raw.unmatched.len(),
        raw.ambiguous
    );
    for (src, n) in &raw.unmatched_by_source {
        println!("    unmatched raw items {src}: {n}");
    }

    let mut items: Vec<&(Element, Ruleset)> = ours
        .values()
        .filter(|(e, _)| ITEM_TYPES.contains(&e.ty.as_str()))
        .filter(|(e, _)| e.setter_value("category") != Some("Item Groups"))
        .collect();
    items.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    if let Some(limit) = std::env::var("BOREALIS_5ETOOLS_ITEM_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        items.truncate(limit);
    }

    let t1 = Instant::now();
    let mut pass = Pass::default();
    let mut tally = Tally::default();
    for (el, rs) in &items {
        let category = el.setter_value("category").unwrap_or("(none)").to_string();
        support::bump(&mut pass.tried, &category);
        let raw_item = raw.by_id.get(&el.id);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            audit_item(&cx, el, *rs, raw_item, &category, &mut pass, &mut tally)
        }));
        if outcome.is_err() {
            pass.panics.push(el.id.clone());
        }
    }
    let t_items = t1.elapsed();

    let t2 = Instant::now();
    let dangling = packs_and_groups(&cx, &raw, &mut tally);
    starting_equipment(&cx, &mut tally);
    let t_rest = t2.elapsed();

    print_pass(&pass);
    let (hosts_ok, hosts_failed) = cx.hosts.counts();
    println!("eligible attunement hosts: {hosts_ok} built, {hosts_failed} could not be built");
    tally.print();
    let item_errors = result
        .report
        .errors
        .iter()
        .filter(|n| n.kind == "item")
        .count();
    println!(
        "report.errors (unread rules) on items: {item_errors} of {}",
        result.report.errors.len()
    );
    books_table(&result.report, &ours, &tally);
    println!(
        "wall: import {:.1}s, items {:.1}s ({} items), packs/groups/starting equipment {:.1}s, total {:.1}s",
        t_import.as_secs_f64(),
        t_items.as_secs_f64(),
        items.len(),
        t_rest.as_secs_f64(),
        t0.elapsed().as_secs_f64()
    );
    assert!(pass.panics.is_empty(), "panics: {:?}", pass.panics);
    assert!(
        dangling.is_empty(),
        "pack/group contents that are not installed items: {dangling:?}"
    );
    if std::env::var_os("BOREALIS_5ETOOLS_ITEM_LIMIT").is_none() {
        ratchet(&tally);
    }
}

/// Wrong values allowed per category (known, not fixed yet).
const KNOWN_WRONG: &[(&str, usize)] = &[
    // medium armor with `dexterityMax: null` (no Dex cap)
    ("base armor AC (light/medium/heavy dex caps)", 1),
];

/// Most "not modelled" checks allowed in categories that are converted.
const NOT_MODELLED_CEILING: &[(&str, usize)] = &[
    ("+N weapon attack/damage", 0),
    ("+N shield AC", 0),
    ("ability score flat bonus", 0),
    ("weapon mastery (2024)", 0),
    ("weapon proficiency (fighter, simple/martial)", 0),
    ("Weapon Mastery select offers imported weapons", 0),
    ("speed change (modifySpeed)", 0),
    // charges on mundane gear that cannot be equipped
    ("charges/uses", 7),
];

/// The full corpus run may not lose ground: no new wrong values, no regressions in what is
/// converted.
fn ratchet(tally: &Tally) {
    for (cat, n) in tally.wrong_counts() {
        let allowed = KNOWN_WRONG
            .iter()
            .find(|(c, _)| *c == cat)
            .map_or(0, |(_, a)| *a);
        assert!(n <= allowed, "{cat}: {n} wrong (allowed {allowed})");
    }
    for (cat, ceiling) in NOT_MODELLED_CEILING {
        let (_, missing) = tally.misses(cat).unwrap_or((0, 0));
        assert!(
            missing <= *ceiling,
            "{cat}: {missing} not modelled (ceiling {ceiling})"
        );
    }
}

fn print_pass(pass: &Pass) {
    println!("--- engine pass by item category: tried / equipped / carried only");
    for (cat, n) in &pass.tried {
        println!(
            "{n:>7} {:>7} {:>7}  {cat}",
            pass.equipped.get(cat).unwrap_or(&0),
            pass.carried_only.get(cat).unwrap_or(&0)
        );
    }
    println!("panics: {} {:?}", pass.panics.len(), pass.panics);
    for (cause, ids) in &pass.equip_refused {
        println!(
            "equip refused ({cause}): {} e.g. {:?}",
            ids.len(),
            first(ids)
        );
    }
    for (kind, ids) in &pass.invariants {
        println!(
            "invariant {kind}: {} items e.g. {:?}",
            ids.len(),
            first(ids)
        );
    }
    println!(
        "items with unread rule fields/kinds: {} e.g. {:?}",
        pass.unread.len(),
        first(&pass.unread)
    );
}

fn first(ids: &[String]) -> Vec<&str> {
    ids.iter().take(8).map(String::as_str).collect()
}

/// Add, equip and settle one item; then run every value check that applies to it. An item
/// whose attunement restriction bars the fighter is checked as barred there and then audited
/// on an eligible host (`hosts`).
fn audit_item(
    cx: &Ctx<'_>,
    el: &Element,
    rs: Ruleset,
    raw: Option<&RawItem>,
    category: &str,
    pass: &mut Pass,
    tally: &mut Tally,
) {
    let fighter = &cx.bases[&rs];
    let Equipped {
        state,
        uid,
        warnings,
        refused,
    } = equip_on(cx, &fighter.state, el);
    match refused {
        None => support::bump(&mut pass.equipped, category),
        Some(cause) => {
            if matches!(el.ty.as_str(), "Weapon" | "Armor") {
                pass.equip_refused
                    .entry(cause)
                    .or_default()
                    .push(el.id.clone());
            }
            support::bump(&mut pass.carried_only, category);
        }
    }
    let snap = cx.engine.derive(&state);
    let line = snap.inventory.items.iter().find(|l| l.uid == uid);
    let fighter_attuned = line.is_some_and(|l| l.attuned);
    let fighter_proficient = snap
        .attacks
        .iter()
        .find(|a| a.uid == uid)
        .map(|a| a.proficient);
    let host = if line.is_some_and(|l| l.attunement_barred) {
        barred_and_host(cx, el, rs, fighter_attuned, &warnings, tally)
    } else {
        None
    };
    let (base, state, uid, snap): (&Base, _, _, _) = match &host {
        Some((h, st, u)) => (h, st.clone(), u.clone(), cx.engine.derive(st)),
        None => (fighter, state, uid, snap),
    };
    for v in invariants::check(cx.engine, &state, &snap) {
        let text = v.to_string();
        if base.violations.contains(&text) {
            continue;
        }
        if v.kind == invariants::kind::UNKNOWN_RULE && v.element.as_deref() == Some(&el.id) {
            pass.unread.push(el.id.clone());
        }
        pass.invariants
            .entry(v.kind.to_string())
            .or_default()
            .push(el.id.clone());
    }
    let Some(raw) = raw else { return };
    let ck = Check {
        cx,
        rs,
        base,
        el,
        raw,
        snap: &snap,
        state: &state,
        uid: &uid,
        fighter_attuned,
        fighter_proficient,
    };
    ck.all(tally);
}

/// An item added to a character and, when it can be, equipped.
struct Equipped {
    state: CharacterState,
    uid: String,
    warnings: Vec<String>,
    /// Why equipping was refused (the item is only carried).
    refused: Option<String>,
}

/// Add `el` to `state` and equip it.
fn equip_on(cx: &Ctx<'_>, state: &CharacterState, el: &Element) -> Equipped {
    let added = commands::add_item(cx.engine, state, &el.id, 1).expect("add item");
    let uid = added.state.inventory.items.last().unwrap().uid.clone();
    match commands::equip(cx.engine, &added.state, &uid, None) {
        Ok(o) => Equipped {
            state: o.state,
            uid,
            warnings: o.warnings,
            refused: None,
        },
        Err(e) => Equipped {
            state: added.state,
            uid,
            warnings: Vec::new(),
            refused: Some(e.to_string()),
        },
    }
}

/// The fighter is barred by `el`'s attunement restriction: check that equipping left it
/// unattuned with a warning, then equip it on an eligible host, which must attune it. Returns
/// the host and its state when one could be built.
fn barred_and_host(
    cx: &Ctx<'_>,
    el: &Element,
    rs: Ruleset,
    fighter_attuned: bool,
    warnings: &[String],
    tally: &mut Tally,
) -> Option<(std::rc::Rc<Base>, CharacterState, String)> {
    let warned = warnings.iter().any(|w| w.contains("requires attunement"));
    tally.record(
        "attunement barred on an ineligible character",
        &el.id,
        verdict(!fighter_attuned && warned, || {
            format!("attuned {fighter_attuned} warned {warned}")
        }),
    );
    let host = hosts::spec_for(cx.engine, rs, el).and_then(|s| cx.hosts.get(cx.engine, rs, &s));
    let Some(host) = host else {
        tally.record(
            "attunement on an eligible host",
            &el.id,
            Verdict::NotModelled,
        );
        return None;
    };
    let Equipped {
        state,
        uid,
        refused: None,
        ..
    } = equip_on(cx, &host.state, el)
    else {
        tally.record(
            "attunement on an eligible host",
            &el.id,
            Verdict::Wrong("cannot equip".into()),
        );
        return None;
    };
    let snap = cx.engine.derive(&state);
    let line = snap.inventory.items.iter().find(|l| l.uid == uid);
    let ok = line.is_some_and(|l| l.attuned && !l.attunement_barred);
    tally.record(
        "attunement on an eligible host",
        &el.id,
        verdict(ok, || "still barred".into()),
    );
    Some((host, state, uid))
}

/// Packs and item groups resolve to installed items with the source's quantities; opening a
/// pack (as the app's `extract_pack` does) adds them. Generic `+N` variants adorn a base item.
/// Returns dangling ids.
fn packs_and_groups(cx: &Ctx<'_>, raw: &support::RawIndex, t: &mut Tally) -> Vec<String> {
    let mut dangling = Vec::new();
    let mut ids_sorted: Vec<&String> = cx.ours.keys().collect();
    ids_sorted.sort();
    for id in ids_sorted {
        let (el, rs) = &cx.ours[id];
        let Some(x) = el.extract.as_ref().filter(|x| !x.items.is_empty()) else {
            continue;
        };
        let group = el.setter_value("category") == Some("Item Groups");
        let cat = if group {
            "item group members resolve"
        } else {
            "pack contents resolve"
        };
        let missing: Vec<&str> = x
            .items
            .iter()
            .filter(|i| !cx.engine.has_element(*rs, &i.id))
            .map(|i| i.id.as_str())
            .collect();
        dangling.extend(missing.iter().map(|m| format!("{id} -> {m}")));
        t.record(
            cat,
            id,
            verdict(missing.is_empty(), || format!("missing {missing:?}")),
        );
        if group {
            continue;
        }
        if let Some(r) = raw.by_id.get(id) {
            let want = r.pack_quantities();
            let got: Vec<u32> = x.items.iter().map(|i| i.amount.unwrap_or(1)).collect();
            t.record(
                "pack quantities",
                id,
                verdict(got == want, || format!("{got:?} want {want:?}")),
            );
        }
        let base = &cx.bases[rs];
        let mut state = base.state.clone();
        for part in &x.items {
            if let Ok(o) = commands::add_item(cx.engine, &state, &part.id, part.amount.unwrap_or(1))
            {
                state = o.state;
            }
        }
        let snap = cx.engine.derive(&state);
        let added = snap.inventory.items.len() - base.snap.inventory.items.len();
        t.record(
            "pack opens into its items",
            id,
            verdict(added == x.items.len() - missing.len(), || {
                format!("{added} lines")
            }),
        );
    }
    generic_variants(cx, raw, t);
    reference_checks(cx, t);
    dangling
}

/// Generic magic variants (`+1 Weapon`, `+2 Armor`) used as adorners of a base item.
fn generic_variants(cx: &Ctx<'_>, raw: &support::RawIndex, t: &mut Tally) {
    for (id, kind) in &raw.generic {
        let Some((el, rs)) = cx.ours.get(id) else {
            continue;
        };
        let Some(r) = raw.by_id.get(id) else { continue };
        let base = &cx.bases[rs];
        let (host, stat_n) = match kind.as_str() {
            "weapon" => (ids::longsword(*rs), r.int("bonusWeapon")),
            "armor" => (ids::breastplate(*rs), r.int("bonusAc")),
            "shield" => (ids::shield(*rs), r.int("bonusAc")),
            _ => continue,
        };
        let Some(n) = stat_n else { continue };
        let plain = support::equipped(cx.engine, &base.state, host, None);
        let adorned = support::equipped(cx.engine, &base.state, host, Some(&el.id));
        let (Some(p), Some(a)) = (plain, adorned) else {
            t.record(
                "generic +N variant as adorner",
                id,
                Verdict::Wrong("cannot equip".into()),
            );
            continue;
        };
        let (before, after) = if kind == "weapon" {
            (p.attacks[0].attack, a.attacks[0].attack)
        } else {
            (p.armor_class, a.armor_class)
        };
        t.record(
            "generic +N variant as adorner",
            id,
            verdict_nm(after == before + n, after == before, || {
                format!("{before} -> {after} want +{n}")
            }),
        );
    }
}

/// Starting equipment of imported classes and backgrounds: offered as a package (with coins).
fn starting_equipment(cx: &Ctx<'_>, t: &mut Tally) {
    let mut seen = HashSet::new();
    let mut ids_sorted: Vec<&String> = cx.ours.keys().collect();
    ids_sorted.sort();
    for id in ids_sorted {
        let (el, rs) = &cx.ours[id];
        if !matches!(el.ty.as_str(), "Class" | "Background")
            || !el.data.contains_key("starting_equipment")
        {
            continue;
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(snap) = support::level_one_with(cx.engine, *rs, el) else {
            t.record(
                "starting equipment package + coins (not pickable)",
                id,
                Verdict::Wrong("not pickable".into()),
            );
            continue;
        };
        let offered = snap.equipment_packages.iter().any(|p| p.owner == el.name);
        let cat = format!(
            "starting equipment package + coins ({})",
            el.ty.to_lowercase()
        );
        t.record(
            &cat,
            id,
            if offered {
                Verdict::Correct
            } else {
                Verdict::NotModelled
            },
        );
    }
}

/// Bundled-content references: the SRD 5.1 `Shield, +1` adorner on the SRD shield (the same
/// `ac:armored:enhancement` convention the importer uses), and whether the 2024 Weapon Mastery
/// select offers any imported weapon.
fn reference_checks(cx: &Ctx<'_>, t: &mut Tally) {
    let base = &cx.bases[&Ruleset::Dnd2014];
    let shield = "ID_WOTC_GEAR_SHIELD";
    let plus_one = "ID_WOTC_DMG_MAGIC_ITEM_SHIELD_1";
    if cx.engine.has_element(Ruleset::Dnd2014, plus_one) {
        let plain = support::equipped(cx.engine, &base.state, shield, None);
        let magic = support::equipped(cx.engine, &base.state, shield, Some(plus_one));
        if let (Some(p), Some(m)) = (plain, magic) {
            t.record(
                "reference: bundled SRD Shield +1 (no body armor)",
                plus_one,
                verdict_nm(
                    m.armor_class == p.armor_class + 1,
                    m.armor_class == p.armor_class,
                    || format!("{} -> {}", p.armor_class, m.armor_class),
                ),
            );
        }
    }
    let modern = &cx.bases[&Ruleset::Dnd2024];
    let mastery = modern
        .snap
        .slots
        .iter()
        .find(|s| s.name.contains("Mastery") || s.ty.contains("Mastery"));
    if let Some(s) = mastery {
        let imported = s
            .options
            .iter()
            .filter(|o| cx.ours.get(&o.id).is_some_and(|(e, _)| e.ty == "Weapon"))
            .count();
        let id = format!("{}#{}", s.owner.id, s.name);
        t.record(
            "Weapon Mastery select offers imported weapons",
            &id,
            if imported > 0 {
                Verdict::Correct
            } else {
                Verdict::Wrong(format!(
                    "{} options, {imported} imported weapons",
                    s.options.len()
                ))
            },
        );
    }
}
