//! Stage 2: every imported element is offered where it belongs, measured on builder hosts
//! ([`Driver`]). Hosts: the L1 builder, one per origin (race / species / background), a 1..=20
//! progression per class and per archetype, and a prerequisite-aware host per feat. At every
//! step the open selects' options count as offered, the automatic fill records the options of
//! every slot it fills, and [`Driver::explore`] tries every option that owns a select.
//! `Scope::Repo` (one community repo stacked on A-core) runs only the hosts the repo's elements
//! climb to ([`plan`]); `Scope::Full` runs every class the builder offers.

use super::app::{
    select_owners, Driver, Explore, Host, Offers, CASTER_SCORES, MAX_LEVEL, TEST_SCORES,
};
use super::plan::{tokens, Offerers, Plan};
use super::prereq;
use crate::{limit, metric, outcome, Installed, HOSTED, NON_OFFERABLE, SUITE};
use borealis_content::{ContentStore, Rule, Ruleset};
use borealis_lib::state::AppState;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Full,
    Repo,
}

/// Classes an archetype's progression is tried on before giving up.
const MAX_ARCHETYPE_CLASSES: usize = 3;

/// Names of the variant-human option the L1 origin-feat host takes (2014 Aurora and 5etools).
const VARIANT_NAMES: [&str; 3] = ["Variant Human", "Human Variant", "Variant"];

/// Hosts a source needs: per edition its fighter/wizard/human ids found by name in the builder.
pub struct Ids {
    pub fighter: Option<String>,
    pub wizard: Option<String>,
    pub human: Option<String>,
    pub variant: Option<String>,
}

pub fn base_ids(d: &Driver<'_>, rs: Ruleset, prefix: &str) -> Ids {
    let base = d.create(rs, TEST_SCORES).expect("create");
    let race_ty = if rs == Ruleset::Dnd2024 {
        "Species"
    } else {
        "Race"
    };
    let mut ids = Ids {
        fighter: d.find_named(&base, "Class", "Fighter", prefix),
        wizard: d.find_named(&base, "Class", "Wizard", prefix),
        human: d.find_named(&base, race_ty, "Human", prefix),
        variant: None,
    };
    let mut h = base;
    if let Some(human) = &ids.human {
        if d.pick(&mut h, human) {
            ids.variant = ["Race Variant", "Sub Race"].iter().find_map(|t| {
                VARIANT_NAMES
                    .iter()
                    .find_map(|n| d.find_named(&h, t, n, prefix))
            });
        }
    }
    d.drop_host(h);
    eprintln!(
        "  host ids {rs:?}: fighter {:?} wizard {:?} human {:?} variant {:?}",
        ids.fighter, ids.wizard, ids.human, ids.variant
    );
    ids
}

/// One step of a host: what is open now, then the alternatives that open selects.
fn survey(d: &Driver<'_>, h: &mut Host, ex: &mut Explore<'_>, label: &str, offers: &mut Offers) {
    d.collect(h, label, offers);
    d.explore(h, ex, label, offers);
}

/// A host of `class` with an origin, levelled 1..=20. `archetype` is picked when offered.
/// Returns the archetypes offered and whether `archetype` was picked.
#[allow(clippy::too_many_arguments)]
fn progression(
    d: &Driver<'_>,
    rs: Ruleset,
    ids: &Ids,
    class: &str,
    archetype: Option<&str>,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
) -> (Vec<String>, bool) {
    let mut archetypes = Vec::new();
    let Some(mut h) = d.create(rs, CASTER_SCORES) else {
        return (archetypes, false);
    };
    let label = format!("{class}/{}", archetype.unwrap_or("-"));
    if !d.pick(&mut h, class) {
        d.drop_host(h);
        return (archetypes, false);
    }
    for race in [&ids.human, &ids.variant].into_iter().flatten() {
        d.pick(&mut h, race);
    }
    let mut picked = archetype.is_none();
    for level in 1..=MAX_LEVEL {
        if level > 1 && !d.level_up(&mut h) {
            break;
        }
        // the improvement select's own options first, then its "Feat" option
        survey(d, &mut h, ex, &label, offers);
        let surveyed = h.gen;
        d.open_feat_selects(&mut h);
        if archetype.is_none() {
            for s in
                h.snap.slots.iter().filter(|s| {
                    s.element_type.eq_ignore_ascii_case("Archetype") && s.chosen.is_none()
                })
            {
                archetypes.extend(d.options(&h, &s.id).into_iter().map(|o| o.0));
            }
        }
        if let (Some(a), false) = (archetype, picked) {
            picked = d.pick(&mut h, a);
        }
        // a second look only when something changed since the first (feat option, archetype)
        if h.gen != surveyed {
            survey(d, &mut h, ex, &label, offers);
        }
        d.fill(&mut h, &label, offers);
        d.collect(&h, &label, offers);
    }
    d.drop_host(h);
    archetypes.sort();
    archetypes.dedup();
    (archetypes, picked)
}

/// A host that meets a feat's prerequisites (shared `prereq::NeedsReader`), levelled until it
/// reaches the needed level with the prior feats placed; true when the feat is offered there.
/// When offered, the host also explores, so the selects the feat opens are measured.
#[allow(clippy::too_many_arguments)]
fn needs_host(
    d: &Driver<'_>,
    rs: Ruleset,
    ids: &Ids,
    reader: &prereq::NeedsReader<'_>,
    feat: &str,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
) -> bool {
    let Some(el) = reader.store.element(feat) else {
        return false;
    };
    let mut needs = prereq::Needs::default();
    reader.needs(rs, el, 0, &mut needs);
    let mut scores = if needs.caster {
        CASTER_SCORES
    } else {
        TEST_SCORES
    };
    for (i, v) in &needs.scores {
        scores[*i] = scores[*i].max(*v);
    }
    let class = needs.class.clone().or_else(|| {
        if needs.caster {
            ids.wizard.clone()
        } else {
            ids.fighter.clone()
        }
    });
    let (Some(class), Some(mut h)) = (class, d.create(rs, scores)) else {
        return false;
    };
    d.pick(&mut h, &class);
    let mut chain: Vec<String> = Vec::new();
    match &needs.race {
        Some(r) => {
            if let Some(parent) = reader
                .store
                .element(r)
                .filter(|e| e.ty == "Sub Race")
                .and_then(|e| e.supports.first())
            {
                chain.push(parent.clone());
            }
            chain.push(r.clone());
        }
        None => chain.extend(
            [ids.human.clone(), ids.variant.clone()]
                .into_iter()
                .flatten(),
        ),
    }
    for r in &chain {
        d.pick(&mut h, r);
    }
    let label = format!("needs:{feat}");
    d.fill(&mut h, &label, offers);
    let mut pending = needs.feats.clone();
    let mut found = false;
    loop {
        d.collect(&h, &label, offers);
        d.open_feat_selects(&mut h);
        while let Some(next) = pending.first().cloned() {
            if !d.pick(&mut h, &next) {
                break;
            }
            pending.remove(0);
            d.fill(&mut h, &label, offers);
            d.open_feat_selects(&mut h);
        }
        d.collect(&h, &label, offers);
        if h.snap
            .slots
            .iter()
            .any(|s| s.chosen.is_none() && d.options(&h, &s.id).iter().any(|(id, _)| id == feat))
        {
            found = true;
            // the feat itself, then the selects it opens
            if d.pick(&mut h, feat) {
                d.collect(&h, &label, offers);
                d.explore(&mut h, ex, &label, offers);
            }
            break;
        }
        if h.snap.level.total >= MAX_LEVEL || !d.level_up(&mut h) {
            break;
        }
        // the improvement's "Feat" option before the fill takes the first option
        d.open_feat_selects(&mut h);
        d.fill(&mut h, &label, offers);
    }
    d.drop_host(h);
    found
}

/// An origin host: a L1 character with `origin` picked (races: then every sub race / variant
/// select explored).
fn origin_host(
    d: &Driver<'_>,
    rs: Ruleset,
    origin: &str,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
) {
    let Some(mut h) = d.create(rs, TEST_SCORES) else {
        return;
    };
    let label = format!("origin:{origin}");
    if d.pick(&mut h, origin) {
        survey(d, &mut h, ex, &label, offers);
        d.fill(&mut h, &label, offers);
        d.collect(&h, &label, offers);
    }
    d.drop_host(h);
}

/// Select types (lower case) and the support tokens each names (`None` = any element).
fn select_index(store: &ContentStore) -> HashMap<String, Vec<Option<BTreeSet<String>>>> {
    let mut out: HashMap<String, Vec<Option<BTreeSet<String>>>> = HashMap::new();
    for se in store.iter() {
        for r in &se.element.rules {
            if let Rule::Select(s) = r {
                out.entry(s.ty.to_lowercase())
                    .or_default()
                    .push(s.supports.as_deref().map(tokens));
            }
        }
    }
    out
}

/// The hosts the source's elements need (see [`super::plan`]).
fn plan(ins: &Installed, rs: Ruleset, scope: Scope) -> (Plan, BTreeSet<String>) {
    let offerers = Offerers::new(&ins.store);
    let mut p = Plan {
        all_classes: scope == Scope::Full,
        ..Plan::default()
    };
    let mut own_feats = BTreeSet::new();
    for (id, ty, _) in ins.ours.iter().filter(|o| o.2 == rs) {
        match ty.as_str() {
            "Class" => {
                p.classes.insert(id.clone());
            }
            "Race" | "Species" | "Background" => {
                p.origins.insert(id.clone());
            }
            "Feat" => {
                own_feats.insert(id.clone());
            }
            "Archetype" => {
                let mut q = Plan::default();
                offerers.place(id, &mut q);
                let classes: BTreeSet<String> = q
                    .classes
                    .into_iter()
                    .chain(q.archetypes.into_values().flatten())
                    .collect();
                if classes.is_empty() {
                    p.all_classes = true;
                }
                p.archetypes.insert(id.clone(), classes);
            }
            // gear is offered by search_items; sub races, variants and spells climb to a host
            t if HOSTED.contains(&t) && !matches!(t, "Spell" | "Sub Race" | "Race Variant") => {}
            t if NON_OFFERABLE.iter().any(|(n, _)| *n == t) => {}
            _ => {
                if !offerers.place(id, &mut p) {
                    p.all_classes = true;
                }
            }
        }
    }
    (p, own_feats)
}

/// One host of the stage-2 plan.
enum Job {
    Origin(String),
    Class(String),
    /// archetype, classes to try it on (in order)
    Archetype(String, Vec<String>),
    /// a prerequisite-aware feat host (explores the feat's selects when offered)
    Feat(String),
}

/// What one worker found: offers, (archetype, class) pairs offered, options explored, hosts.
type Found = (Offers, Vec<(String, String)>, usize, usize);

/// Shared inputs of the hosts of one edition.
struct Edition<'a> {
    ins: &'a Installed,
    rs: Ruleset,
    ids: &'a Ids,
    reader: &'a prereq::NeedsReader<'a>,
    owners: &'a BTreeSet<String>,
}

fn run_job(d: &Driver<'_>, e: &Edition<'_>, job: &Job, ex: &mut Explore<'_>, f: &mut Found) {
    let offers = &mut f.0;
    match job {
        Job::Origin(o) => origin_host(d, e.rs, o, ex, offers),
        Job::Class(c) => {
            let ours = |a: &String| e.ins.ours.iter().any(|o| &o.0 == a);
            for a in progression(d, e.rs, e.ids, c, None, ex, offers).0 {
                if ours(&a) {
                    f.1.push((a, c.clone()));
                }
            }
        }
        Job::Archetype(a, classes) => {
            for c in classes.iter().take(MAX_ARCHETYPE_CLASSES) {
                if progression(d, e.rs, e.ids, c, Some(a), ex, offers).1 {
                    break;
                }
            }
        }
        Job::Feat(feat) => {
            needs_host(d, e.rs, e.ids, e.reader, feat, ex, offers);
        }
    }
}

/// Runs `jobs` on `apps.len()` workers (job `i` on worker `i % n`, one app each), each with its
/// own driver and explore memo; results come back in worker order, so a run is deterministic.
fn run_jobs(apps: &[&AppState], e: &Edition<'_>, jobs: &[Job]) -> Vec<Found> {
    let n = apps.len().min(jobs.len()).max(1);
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..n)
            .map(|w| {
                let app = apps[w];
                std::thread::Builder::new()
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(sc, move || {
                        borealis_engine::invariants::enable_for_thread();
                        let d = Driver::new(app);
                        let mut ex = Explore {
                            owners: e.owners,
                            memo: BTreeSet::new(),
                            explored: 0,
                        };
                        let mut f: Found = (Offers::default(), Vec::new(), 0, 0);
                        for job in jobs.iter().skip(w).step_by(n) {
                            run_job(&d, e, job, &mut ex, &mut f);
                        }
                        f.2 = ex.explored;
                        f.3 = d.made.get();
                        f
                    })
                    .expect("stage-2 worker")
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    })
}

fn merge(into: &mut Found, parts: Vec<Found>) {
    for (o, subs, n, m) in parts {
        for (id, host) in o.found {
            into.0.add(&id, &host);
        }
        into.1.extend(subs);
        into.2 += n;
        into.3 += m;
    }
}

/// Builds every host the scope needs for one edition (on `apps`, in phases: origins and classes,
/// then archetypes, then feats) and records what they offer.
#[allow(clippy::too_many_arguments)]
fn edition(
    apps: &[&AppState],
    ins: &Installed,
    rs: Ruleset,
    prefix: &str,
    reader: &prereq::NeedsReader<'_>,
    owners: &BTreeSet<String>,
    scope: Scope,
    total: &mut Found,
) {
    let cap = limit();
    let (plan, own_feats) = plan(ins, rs, scope);
    // a repo explores only the options on its elements' climb paths (and its own)
    let scoped: BTreeSet<String>;
    let owners = if scope == Scope::Repo {
        scoped = owners
            .iter()
            .filter(|o| plan.path.contains(*o) || ins.ours.iter().any(|x| &x.0 == *o))
            .cloned()
            .collect();
        &scoped
    } else {
        owners
    };
    let d = Driver::new(apps[0]);
    let ids = base_ids(&d, rs, prefix);
    let mut ex = Explore {
        owners,
        memo: BTreeSet::new(),
        explored: 0,
    };
    let mut base = d.create(rs, TEST_SCORES).expect("create");
    survey(&d, &mut base, &mut ex, "L1", &mut total.0);
    d.items(&base, &mut total.0, "search_items");
    let builder_classes: Vec<String> = base
        .snap
        .slots
        .iter()
        .find(|s| s.element_type.eq_ignore_ascii_case("Class") && s.chosen.is_none())
        .map(|slot| {
            d.options(&base, &slot.id)
                .into_iter()
                .map(|o| o.0)
                .collect()
        })
        .unwrap_or_default();
    d.drop_host(base);
    total.2 += ex.explored;
    total.3 += d.made.get();
    let e = Edition {
        ins,
        rs,
        ids: &ids,
        reader,
        owners,
    };
    let mut classes = plan.classes.clone();
    if plan.all_classes {
        // every class the builder offers (bundled ones too: their lists name our spells)
        classes.extend(builder_classes.iter().cloned());
    }
    let mut jobs: Vec<Job> = plan.origins.iter().cloned().map(Job::Origin).collect();
    jobs.extend(classes.into_iter().take(cap).map(Job::Class));
    let mut found: Found = (Offers::default(), Vec::new(), 0, 0);
    merge(&mut found, run_jobs(apps, &e, &jobs));
    // archetype -> classes whose progression offered it (tried first), then the climbed ones
    let mut subs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut offered_subs = std::mem::take(&mut found.1);
    offered_subs.sort();
    offered_subs.dedup();
    for (a, c) in offered_subs {
        subs.entry(a).or_default().push(c);
    }
    for (a, cls) in &plan.archetypes {
        let tried = subs.entry(a.clone()).or_default();
        for c in cls.iter().filter(|c| builder_classes.contains(c)) {
            if !tried.contains(c) {
                tried.push(c.clone());
            }
        }
    }
    let jobs: Vec<Job> = subs
        .into_iter()
        .take(cap)
        .map(|(a, c)| Job::Archetype(a, c))
        .collect();
    merge(&mut found, run_jobs(apps, &e, &jobs));
    let mut feats: Vec<Job> = Vec::new();
    let openers = plan.feats.iter().filter(|f| !own_feats.contains(*f));
    for feat in own_feats.iter().chain(openers) {
        let opener = plan.feats.contains(feat);
        if (found.0.found.contains_key(feat) || total.0.found.contains_key(feat)) && !opener {
            continue;
        }
        feats.push(Job::Feat(feat.clone()));
    }
    feats.truncate(cap);
    merge(&mut found, run_jobs(apps, &e, &feats));
    merge(total, vec![found]);
}

/// Stage 2 over every edition of the source; each edition's hosts run on `apps` (apps holding
/// the same installed content, one worker each: one app's builder commands serialize).
pub fn stage2(
    src: &str,
    apps: &[&AppState],
    ins: &Installed,
    prefix: &str,
    class_id: fn(Ruleset, &str) -> String,
    scope: Scope,
) {
    let t0 = std::time::Instant::now();
    let editions: BTreeSet<Ruleset> = ins.ours.iter().map(|o| o.2).collect();
    let ours_rs: HashMap<String, Ruleset> = ins.ours.iter().map(|o| (o.0.clone(), o.2)).collect();
    let owners = select_owners(&ins.store);
    let reader = prereq::NeedsReader {
        store: &ins.store,
        ours: &ours_rs,
        class_id,
    };
    let mut total: Found = (Offers::default(), Vec::new(), 0, 0);
    for rs in editions {
        edition(apps, ins, rs, prefix, &reader, &owners, scope, &mut total);
    }
    metric(src, "s2", "*", "hosts_created", total.3 as i64);
    metric(src, "s2", "*", "options_explored", total.2 as i64);
    metric(src, "s2", "*", "workers", apps.len() as i64);
    record(src, ins, &total.0);
    metric(src, "s2", "*", "wall_s", t0.elapsed().as_secs() as i64);
}

/// Stack of a stage-2 worker (deep content recursion, as `run_big`).
const WORKER_STACK: usize = 64 << 20;

/// Offered / offerable per type, one outcome per offerable element.
fn record(src: &str, ins: &Installed, offers: &Offers) {
    let selects = select_index(&ins.store);
    let mut rows: BTreeMap<(String, &str), [i64; 2]> = BTreeMap::new();
    let mut seen_types: BTreeSet<&str> = BTreeSet::new();
    for (id, ty, _) in &ins.ours {
        let el = ins.store.element(id);
        let category = if HOSTED.contains(&ty.as_str()) {
            "host"
        } else if let Some((_, why)) = NON_OFFERABLE.iter().find(|(t, _)| t == ty) {
            why
        } else if selects.get(&ty.to_lowercase()).is_some_and(|v| {
            v.iter().any(|tokens| match tokens {
                None => true,
                Some(t) => {
                    t.contains(&id.to_lowercase())
                        || el.is_some_and(|e| {
                            e.supports.iter().any(|s| t.contains(&s.to_lowercase()))
                        })
                }
            })
        }) {
            "select"
        } else {
            "Grants"
        };
        seen_types.insert(ty);
        if category != "host" && category != "select" {
            rows.entry((ty.clone(), "nonoffer")).or_default()[0] += 1;
            continue;
        }
        let ok = offers.found.contains_key(id);
        let r = rows.entry((ty.clone(), "offerable")).or_default();
        r[0] += 1;
        if ok {
            r[1] += 1;
        }
        let by = offers
            .found
            .get(id)
            .map_or("not offered by any host", String::as_str);
        outcome::record(SUITE, &format!("{src}/s2/{ty}/{id}"), ok, by);
    }
    for ((ty, kind), [n, offered]) in &rows {
        if *kind == "nonoffer" {
            metric(src, "s2", ty, "non_offerable", *n);
        } else {
            metric(src, "s2", ty, "offerable", *n);
            metric(src, "s2", ty, "offered", *offered);
        }
    }
    metric(src, "s2", "*", "types", seen_types.len() as i64);
    metric(src, "s2", "*", "unmapped_types", 0);
}
