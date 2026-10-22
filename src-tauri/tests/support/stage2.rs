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
use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr};
use borealis_content::{ContentStore, Rule, Ruleset};
use borealis_lib::state::AppState;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[path = "stage2_items.rs"]
mod items;

#[path = "stage2_defects.rs"]
mod defects;

#[path = "stage2_climb.rs"]
mod climb;
use climb::{
    attunement_configs, climb_items, counter_configs, grant_level_configs,
    grant_requirement_groups, kept_slots, list_owner_configs, multiclass_entries, negated_ids,
    offering_owners, on_climb, rank_configs, spell_list_hosts, stat_groups, MULTICLASS_TYPE,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Full,
    Repo,
}

/// Option hosts per edition (each levels up to 20 at most).
const MAX_OPTION_HOSTS: usize = 2000;

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

/// A classed L1 host of `origin` when one of the origin's own selects can never offer anything
/// (`defects::supports_unfillable`), with the character options that select requires turned
/// on. With a class and every other select filled nothing is left to pick, so the settle
/// invariants judge that empty select and its content-verdict row is recorded whatever other
/// hosts happen to take this origin (the class-less origin host always has the class open).
fn empty_select_host(d: &Driver<'_>, e: &Edition<'_>, origin: &str, offers: &mut Offers) {
    let store = &e.ins.store;
    let Some(el) = store.element(origin) else {
        return;
    };
    let mut ids = Vec::new();
    let mut unfillable = false;
    for r in &el.rules {
        let Rule::Select(s) = r else { continue };
        if !defects::supports_unfillable(store, &s.ty, s.supports.as_deref()) {
            continue;
        }
        unfillable = true;
        let req = s.requirements.as_deref().unwrap_or("");
        if let Ok(Some(expr)) = RequirementsExpr::parse(req) {
            positive_ids(expr.root(), &mut ids);
        }
    }
    let (Some(class), true) = (e.ids.fighter.as_deref(), unfillable) else {
        return;
    };
    let Some(mut h) = d.create(e.rs, TEST_SCORES) else {
        return;
    };
    // the character options the select requires: the core's options live in the engine's
    // universe, not in the installed store
    let hub = d.app.hub().ok();
    let options: Vec<String> = ids
        .into_iter()
        .filter(|id| {
            hub.as_ref()
                .and_then(|h| h.element(id))
                .is_some_and(|x| x.ty == items::OPTION_TYPE)
        })
        .collect();
    let label = format!("origin-select:{origin}");
    d.enable_options(&mut h, &options);
    // the class's own picks first (its skill list, before an origin's open skill selects take
    // the first skills), then the origin
    let classed = d.pick(&mut h, class);
    if classed {
        d.fill(&mut h, &label, offers);
    }
    // a class select that is already empty before the origin (the 2024 fighter's skill select in
    // a setup whose skills it cannot name) would be judged in place of the origin's: no check
    let class_empty =
        h.snap.slots.iter().any(|s| {
            s.id.starts_with(class) && s.chosen.is_none() && d.options(&h, &s.id).is_empty()
        });
    if class_empty {
        d.drop_host(h);
        return;
    }
    if classed && d.pick(&mut h, origin) {
        // the fill leaves origin and feat slots to the hosts; here their first option is taken
        // too, so nothing is left to pick
        for _ in 0..MAX_ORIGIN_PICKS {
            d.fill(&mut h, &label, offers);
            let open = h.snap.slots.iter().find_map(|s| {
                let first = d.options(&h, &s.id).into_iter().next();
                first.filter(|_| s.chosen.is_none()).map(|o| o.0)
            });
            match open {
                Some(id) if d.pick(&mut h, &id) => {}
                _ => break,
            }
        }
        d.collect(&h, &label, offers);
    }
    if std::env::var("BOREALIS_ACCEPT_TRACE")
        .is_ok_and(|t| t.split(',').any(|x| x.trim() == origin))
    {
        let open: Vec<(&str, usize, bool)> = h
            .snap
            .slots
            .iter()
            .filter(|s| s.id.starts_with(origin) || s.chosen.is_none())
            .map(|s| {
                (
                    s.id.as_str(),
                    d.options(&h, &s.id).len(),
                    s.chosen.is_some(),
                )
            })
            .collect();
        eprintln!("TRACE {origin}: empty-select host options {options:?} slots {open:?}");
    }
    d.drop_host(h);
}

/// Origin, sub race and feat slots an [`empty_select_host`] fills with their first option.
const MAX_ORIGIN_PICKS: usize = 8;

/// The ids a requirements expression needs present (leaves outside any negation).
fn positive_ids(expr: &BoolExpr<RequirementLeaf>, out: &mut Vec<String>) {
    match expr {
        BoolExpr::Leaf(RequirementLeaf::Id(id)) => out.push(id.clone()),
        BoolExpr::And(items) | BoolExpr::Or(items) => {
            items.iter().for_each(|x| positive_ids(x, out));
        }
        _ => {}
    }
}

/// Every select by type (lower case): its owner, the support tokens it names (`None` = any
/// element) and its requirements; plus who grants each element unconditionally.
/// One select: owner id, support tokens (`None` = any element), requirements.
type SelectRow = (String, Option<BTreeSet<String>>, String);

struct Selects {
    by_type: HashMap<String, Vec<SelectRow>>,
    granted_by: HashMap<String, BTreeSet<String>>,
}

fn select_index(store: &ContentStore) -> Selects {
    let mut by_type: HashMap<String, Vec<SelectRow>> = HashMap::new();
    let mut granted_by: HashMap<String, BTreeSet<String>> = HashMap::new();
    for se in store.iter() {
        for r in &se.element.rules {
            match r {
                Rule::Select(s) => by_type.entry(s.ty.to_lowercase()).or_default().push((
                    se.element.id.clone(),
                    s.supports.as_deref().map(tokens),
                    s.requirements.clone().unwrap_or_default(),
                )),
                // only an unconditional grant means "always held" (a grant behind `!option` is
                // not, unless that option is not in the store at all)
                Rule::Grant(g) if always_true(store, g.requirements.as_deref().unwrap_or("")) => {
                    granted_by
                        .entry(g.id.clone())
                        .or_default()
                        .insert(se.element.id.clone());
                }
                _ => {}
            }
        }
    }
    Selects {
        by_type,
        granted_by,
    }
}

impl Selects {
    /// The selects (owner, requirements) that can offer `id`.
    fn offering<'s>(
        &'s self,
        id: &str,
        ty: &str,
        el: Option<&borealis_content::Element>,
    ) -> Vec<&'s SelectRow> {
        self.by_type
            .get(&ty.to_lowercase())
            .map(|v| {
                v.iter()
                    .filter(|(_, tokens, _)| match tokens {
                        None => true,
                        Some(t) => {
                            t.contains(&id.to_lowercase())
                                || el.is_some_and(|e| {
                                    e.supports.iter().any(|s| t.contains(&s.to_lowercase()))
                                })
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
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
    /// a prerequisite-aware host for an option still not offered (see [`option_host`])
    Option(String),
    /// an element only an item's select offers, and the items to hold (see [`items::item_host`])
    Item(String, Vec<String>),
}

/// What one worker found: offers, (archetype, class) pairs offered, options explored, hosts.
type Found = (Offers, Vec<(String, String)>, usize, usize);

/// Shared inputs of the hosts of one edition.
struct Edition<'a> {
    src: &'a str,
    offerers: &'a Offerers<'a>,
    selects: &'a Selects,
    builder_classes: &'a [String],
    ins: &'a Installed,
    rs: Ruleset,
    ids: &'a Ids,
    reader: &'a prereq::NeedsReader<'a>,
    owners: &'a BTreeSet<String>,
}

fn run_job(d: &Driver<'_>, e: &Edition<'_>, job: &Job, ex: &mut Explore<'_>, f: &mut Found) {
    let offers = &mut f.0;
    match job {
        Job::Origin(o) => {
            origin_host(d, e.rs, o, ex, offers);
            empty_select_host(d, e, o, offers);
        }
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
        Job::Option(id) => {
            option_host(d, e, id, ex, offers);
        }
        Job::Item(id, held) => {
            items::item_host(d, e, id, held, ex, offers);
        }
    }
}

/// Runs `jobs` on `apps.len()` workers (job `i` on worker `i % n`, one app each), each with its
/// own driver and explore memo; results come back in worker order, so a run is deterministic.
fn run_jobs(apps: &[&AppState], e: &Edition<'_>, phase: &str, jobs: &[Job]) -> Vec<Found> {
    let n = apps.len().min(jobs.len()).max(1);
    let t0 = std::time::Instant::now();
    let edition = format!("{:?}", e.rs);
    super::progress(&format!(
        "{} s2 {edition} {phase}: {} hosts on {n} workers",
        e.src,
        jobs.len()
    ));
    // workers record settle-invariant violations like their caller (a-comm), and hand them back
    let recording = borealis_engine::invariants::recording_for_thread();
    let found: Vec<(Found, Vec<_>)> = std::thread::scope(|sc| {
        let handles: Vec<_> = (0..n)
            .map(|w| {
                let app = apps[w];
                std::thread::Builder::new()
                    .stack_size(WORKER_STACK)
                    .spawn_scoped(sc, move || {
                        borealis_engine::invariants::enable_for_thread();
                        if recording {
                            borealis_engine::invariants::record_for_thread();
                        }
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
                        (f, borealis_engine::invariants::take_recorded())
                    })
                    .expect("stage-2 worker")
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    });
    super::progress(&format!(
        "{} s2 {edition} {phase}: done in {} s",
        e.src,
        t0.elapsed().as_secs()
    ));
    found
        .into_iter()
        .map(|(f, recorded)| {
            borealis_engine::invariants::record_violations(recorded);
            f
        })
        .collect()
}

fn merge(into: &mut Found, parts: Vec<Found>) {
    for (o, subs, n, m) in parts {
        into.0.merge(o);
        into.1.extend(subs);
        into.2 += n;
        into.3 += m;
    }
}

/// Builds every host the scope needs for one edition (on `apps`, in phases: origins and classes,
/// then archetypes, then feats) and records what they offer.
#[allow(clippy::too_many_arguments)]
fn edition(
    src: &str,
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
    // the items this edition's item search lists: the only ones an item host may hold
    let mut searchable = Offers::default();
    d.items(&base, &mut searchable, "search_items");
    let searchable: BTreeSet<String> = searchable.found.into_keys().collect();
    for id in &searchable {
        total.0.add(id, "search_items");
    }
    if scope == Scope::Full {
        // the generated proxy items get rows of their own, never offer rows (IM13 D5)
        super::proxy_rows::record(src, apps[0], rs);
    }
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
    let offerers = Offerers::new(&ins.store);
    let selects = select_index(&ins.store);
    let e = Edition {
        src,
        offerers: &offerers,
        selects: &selects,
        builder_classes: &builder_classes,
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
    merge(&mut found, run_jobs(apps, &e, "origins+classes", &jobs));
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
    merge(&mut found, run_jobs(apps, &e, "archetypes", &jobs));
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
    merge(&mut found, run_jobs(apps, &e, "feats", &feats));
    // every offerable option still not offered gets a host built for its prerequisites
    let options: Vec<Job> = ins
        .ours
        .iter()
        .filter(|o| {
            o.2 == rs && !matches!(o.1.as_str(), "Item" | "Weapon" | "Armor" | "Magic Item")
        })
        .filter(|o| !found.0.found.contains_key(&o.0) && !total.0.found.contains_key(&o.0))
        .filter(|o| {
            let c = category(&selects, &o.0, &o.1, ins.store.element(&o.0));
            c == "host" || c == "select"
        })
        .take(cap.min(MAX_OPTION_HOSTS))
        .map(|o| Job::Option(o.0.clone()))
        .collect();
    merge(&mut found, run_jobs(apps, &e, "options", &options));
    // what still is not offered and only an item's select offers gets an item host (also what
    // another item host offered: each one is picked once, to see that it applies)
    let by_item = |id: &str| {
        let label = found.0.found.get(id).or_else(|| total.0.found.get(id));
        label.map_or(true, |l| l.starts_with(items::ITEM_LABEL))
            && !found.0.applied.contains_key(id)
            && !total.0.applied.contains_key(id)
    };
    let held: Vec<Job> = ins
        .ours
        .iter()
        .filter(|o| o.2 == rs && !items::is_item_type(&o.1) && by_item(&o.0))
        .filter_map(|o| {
            let el = ins.store.element(&o.0)?;
            (category(&selects, &o.0, &o.1, Some(el)) == "select")
                .then(|| items::host_items(&e, &o.0, el, &searchable))
                .filter(|v| !v.is_empty())
                .map(|v| Job::Item(o.0.clone(), v))
        })
        .take(cap.min(MAX_OPTION_HOSTS))
        .collect();
    merge(&mut found, run_jobs(apps, &e, "items", &held));
    merge(total, vec![found]);
}

/// The content verdict of a settle-invariant violation a community setup recorded, or `None`
/// (see `defects::invariant_verdict`).
pub fn invariant_verdict(
    store: &ContentStore,
    v: &borealis_engine::invariants::Violation,
) -> Option<&'static str> {
    defects::invariant_verdict(store, v)
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
        edition(
            src, apps, ins, rs, prefix, &reader, &owners, scope, &mut total,
        );
    }
    metric(src, "s2", "*", "hosts_created", total.3 as i64);
    metric(src, "s2", "*", "options_explored", total.2 as i64);
    metric(src, "s2", "*", "workers", apps.len() as i64);
    record(src, apps[0], ins, &total.0, scope);
    metric(src, "s2", "*", "wall_s", t0.elapsed().as_secs() as i64);
}

/// Stack of a stage-2 worker (deep content recursion, as `run_big`).
const WORKER_STACK: usize = 64 << 20;

/// "host" (own host type), "select" (some select offers it) or the non-offerable reason.
/// An element every offering select's owner also grants is "Grants": a select never offers
/// what the character already has (Four Elements' Elemental Attunement).
fn category<'a>(
    selects: &Selects,
    id: &str,
    ty: &'a str,
    el: Option<&borealis_content::Element>,
) -> &'a str {
    if el.is_some_and(|e| legacy_of(e).is_some()) {
        return "Legacy";
    }
    if HOSTED.contains(&ty) {
        return "host";
    }
    if let Some((_, why)) = NON_OFFERABLE.iter().find(|(t, _)| *t == ty) {
        return why;
    }
    // an element's own select never offers it (it is held while its select is open): the
    // trait holding a 5etools race's version select names itself as the support (det-5e-6 F14)
    let offering: Vec<_> = selects
        .offering(id, ty, el)
        .into_iter()
        .filter(|(owner, _, _)| owner != id)
        .collect();
    let granters = selects.granted_by.get(id);
    let self_granted = !offering.is_empty()
        && offering
            .iter()
            .all(|(owner, _, _)| granters.is_some_and(|g| g.contains(owner)));
    if offering.is_empty() || self_granted {
        "Grants"
    } else {
        "select"
    }
}

/// A former version of a 5etools race whose `_versions` became options of the replaced trait
/// (det-5e-6, F14): kept under its old id for saved characters, requiring the race it is a
/// version of, so no new character is offered it. Returns that base race id.
fn legacy_of(el: &borealis_content::Element) -> Option<&str> {
    let base = el.data.get("version_of")?.as_str()?;
    (HOSTED.contains(&el.ty.as_str()) && el.requirements.trim() == base).then_some(base)
}

/// Offered / offerable per type, one outcome per offerable element. A legacy race version
/// ([`legacy_of`]) gets its own outcome: passes when it is not offered and its base race is.
fn record(src: &str, app: &AppState, ins: &Installed, offers: &Offers, scope: Scope) {
    let selects = select_index(&ins.store);
    // a community repo's rows: a content defect (see [`defects`]) asserts that verdict
    let hub = app.hub().ok();
    let defined = |id: &str| hub.as_ref().map_or(true, |h| h.element(id).is_some());
    let defects = (scope == Scope::Repo).then(|| defects::Defects::new(&ins.store, &defined));
    let mut defect_rows = 0;
    let mut rows: BTreeMap<(String, &str), [i64; 2]> = BTreeMap::new();
    let mut seen_types: BTreeSet<&str> = BTreeSet::new();
    for (id, ty, rs) in &ins.ours {
        let el = ins.store.element(id);
        seen_types.insert(ty);
        if let Some(base) = el.and_then(legacy_of) {
            let (offered, base_offered) = (
                offers.found.contains_key(id),
                offers.found.contains_key(base),
            );
            let why = match (offered, base_offered) {
                (false, true) => "legacy version: kept, not offered; its base race is offered",
                (true, _) => "legacy version still offered for new characters",
                (false, false) => "legacy version: its base race is not offered either",
            };
            let r = rows.entry((ty.clone(), "legacy")).or_default();
            r[0] += 1;
            r[1] += i64::from(!offered && base_offered);
            outcome::record(
                SUITE,
                &format!("{src}/s2/{ty}/{id}"),
                why.starts_with("legacy version: kept"),
                why,
            );
            continue;
        }
        let category = category(&selects, id, ty, el);
        if category != "host" && category != "select" {
            rows.entry((ty.clone(), "nonoffer")).or_default()[0] += 1;
            continue;
        }
        let ok = offers.reached(id).is_some();
        let r = rows.entry((ty.clone(), "offerable")).or_default();
        r[0] += 1;
        if ok {
            r[1] += 1;
        }
        let defect = defects
            .as_ref()
            .filter(|_| !ok)
            .and_then(|d| d.of(&ins.store, id, *rs, &defined));
        defect_rows += i64::from(defect.is_some());
        let by = offers
            .reached(id)
            .map_or("not offered by any host", String::as_str);
        let row = format!("{src}/s2/{ty}/{id}");
        outcome::record(SUITE, &row, ok || defect.is_some(), defect.unwrap_or(by));
    }
    if defects.is_some() {
        metric(src, "s2", "*", "content_defects", defect_rows);
    }
    for ((ty, kind), [n, offered]) in &rows {
        if *kind == "nonoffer" {
            metric(src, "s2", ty, "non_offerable", *n);
        } else if *kind == "legacy" {
            metric(src, "s2", ty, "legacy_versions", *n);
            metric(src, "s2", ty, "legacy_versions_ok", *offered);
        } else {
            metric(src, "s2", ty, "offerable", *n);
            metric(src, "s2", ty, "offered", *offered);
        }
    }
    metric(src, "s2", "*", "types", seen_types.len() as i64);
    metric(src, "s2", "*", "unmapped_types", 0);
    items::record_extra(src, ins, offers);
}

/// What a requirements expression asks for, as groups of alternatives (any one id of a group
/// satisfies it: `a||b` is one group, `a,b` two); negations and tests are left to `Needs`.
fn required_groups(
    store: &ContentStore,
    expr: &BoolExpr<RequirementLeaf>,
    out: &mut Vec<Vec<String>>,
) {
    match expr {
        BoolExpr::Not(_) | BoolExpr::Leaf(RequirementLeaf::Test(_)) => {}
        BoolExpr::Leaf(RequirementLeaf::Id(id)) => {
            if store.element(id).is_some() {
                out.push(vec![id.clone()]);
            }
        }
        BoolExpr::And(items) => items.iter().for_each(|e| required_groups(store, e, out)),
        BoolExpr::Or(items) => {
            let mut any: Vec<String> = Vec::new();
            for e in items {
                let mut inner = Vec::new();
                required_groups(store, e, &mut inner);
                // an alternative that is itself one id (or one group) joins the any-of group
                if let [g] = inner.as_slice() {
                    any.extend(g.iter().cloned());
                }
            }
            if !any.is_empty() {
                out.push(any);
            }
        }
    }
}

/// The earlier copies of a repeatable option (`X_5` needs `X`, `X_2` .. `X_4`), oldest first.
fn earlier_copies(store: &ContentStore, id: &str) -> Vec<String> {
    let Some((base, n)) = id.rsplit_once('_') else {
        return Vec::new();
    };
    let Ok(n) = n.parse::<u32>() else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    if store.element(base).is_some() {
        out.push(base.to_string());
    }
    for k in 1..n {
        let c = format!("{base}_{k}");
        if store.element(&c).is_some() {
            out.push(c);
        }
    }
    out
}

/// What an option needs, as any-of groups: earlier copies of a repeatable option, what its
/// requirements name (and what those name), then what the selects that offer it require.
fn needed_groups(
    e: &Edition<'_>,
    target: &str,
    el: &borealis_content::Element,
    depth: usize,
) -> Vec<Vec<String>> {
    let store = &e.ins.store;
    let mut groups: Vec<Vec<String>> = earlier_copies(store, target)
        .into_iter()
        .map(|c| vec![c])
        .collect();
    let mut queue = vec![target.to_string()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for id in &queue {
            let Some(x) = store.element(id) else { continue };
            let Ok(Some(expr)) = RequirementsExpr::parse(&x.requirements) else {
                continue;
            };
            let mut found = Vec::new();
            required_groups(store, expr.root(), &mut found);
            for g in found {
                if !groups.contains(&g) && !g.iter().any(|i| i == target) {
                    next.extend(g.iter().cloned());
                    groups.insert(0, g);
                }
            }
        }
        queue = next;
    }
    // what the selects that offer it require (a character option such as Tasha's customized
    // proficiencies, a feature)
    for (_, _, req) in e.selects.offering(target, &el.ty, Some(el)) {
        if let Ok(Some(expr)) = RequirementsExpr::parse(req) {
            let mut found = Vec::new();
            required_groups(store, expr.root(), &mut found);
            for g in found {
                if !groups.contains(&g) {
                    groups.push(g);
                }
            }
        }
    }
    groups
}

/// A host built for one option still not offered: its requirements read (scores, class, level,
/// race via `prereq::Needs`; every element id it names, the element ids those name, earlier
/// copies of a repeatable option), the class/archetype/origin its select climbs to, then levels
/// 1..=20 picking each needed id wherever the builder offers it. True when the option is offered.
fn option_host(
    d: &Driver<'_>,
    e: &Edition<'_>,
    target: &str,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
) -> bool {
    let store = &e.ins.store;
    let Some(el) = store.element(target) else {
        return false;
    };
    let mut needs = prereq::Needs::default();
    e.reader.needs(e.rs, el, 0, &mut needs);
    let groups = needed_groups(e, target, el, prereq::MAX_PRIOR_FEATS);
    // hosts the option (and each needed id) climb to; each candidate class is tried in turn
    let mut roots = Plan::default();
    e.offerers.place(target, &mut roots);
    for g in &groups {
        for id in g {
            e.offerers.place(id, &mut roots);
        }
    }
    let mut classes: Vec<(String, Option<String>)> = Vec::new();
    let mut add = |c: String, a: Option<String>| {
        if !classes.iter().any(|x| x.0 == c && x.1 == a) {
            classes.push((c, a));
        }
    };
    if let Some(c) = needs.class.clone() {
        add(c, None);
    }
    // a repeatable copy needs one slot per earlier copy: the fighter has the most improvement
    // slots in both editions (2014: 4, 6, 8, 12, 14, 16, 19; 2024 the same with the Epic Boon)
    if !needs.caster && !earlier_copies(store, target).is_empty() {
        if let Some(f) = &e.ids.fighter {
            add(f.clone(), None);
        }
    }
    let usable = |c: &String| e.builder_classes.contains(c);
    // the option's own select owners first (the needed ids climb to many unrelated classes)
    let mut own = Plan::default();
    e.offerers.place(target, &mut own);
    if std::env::var("BOREALIS_ACCEPT_TRACE")
        .is_ok_and(|t| t.split(',').any(|x| x.trim() == target))
    {
        eprintln!(
            "TRACE {target}: own classes {:?} archetypes {:?} roots archetypes {:?}",
            own.classes, own.archetypes, roots.archetypes
        );
    }
    for c in own.classes.iter().filter(|c| usable(c)) {
        add(c.clone(), None);
    }
    for (a, cs) in &own.archetypes {
        for c in cs.iter().filter(|c| usable(c)) {
            add(c.clone(), Some(a.clone()));
        }
    }
    for c in roots.classes.iter().filter(|c| usable(c)) {
        add(c.clone(), None);
    }
    for (a, cs) in &roots.archetypes {
        for c in cs.iter().filter(|c| usable(c)) {
            add(c.clone(), Some(a.clone()));
        }
    }
    let fallback = if needs.caster {
        &e.ids.wizard
    } else {
        &e.ids.fighter
    };
    if let Some(c) = fallback {
        add(c.clone(), None);
    }
    let mut scores = if needs.caster {
        CASTER_SCORES
    } else {
        TEST_SCORES
    };
    for (i, v) in &needs.scores {
        scores[*i] = scores[*i].max(*v);
    }
    let race = needs.race.clone().or_else(|| e.ids.human.clone());
    let mut every = classes.clone();
    classes.truncate(OPTION_HOST_CLASSES);
    // last, for an option a character option gates, the other fallback: a select never offers
    // what a proficiency the class already has covers (a martial class is not offered the DMG
    // firearms, which are martial ranged weapons)
    let gated = !items::split_options(store, &groups).0.is_empty();
    let other = if !gated {
        &None
    } else if needs.caster {
        &e.ids.fighter
    } else {
        &e.ids.wizard
    };
    if let Some(c) = other {
        if !classes.iter().any(|x| &x.0 == c && x.1.is_none()) {
            classes.push((c.clone(), None));
        }
    }
    // first the needed ids only; then, where that offers nothing, the same configurations also
    // picking the elements on the climb paths (each pass is deterministic, and the first one is
    // exactly the older host, so what it offered stays offered)
    let run = |(climb, mc, keep): (bool, bool, bool),
               configs: &[(String, Option<String>)],
               roots: &Plan,
               groups: &[Vec<String>],
               ex: &mut Explore<'_>,
               offers: &mut Offers| {
        configs.iter().any(|(class, archetype)| {
            option_host_on(
                d,
                e,
                target,
                ex,
                offers,
                (class, archetype.as_deref(), climb, mc, keep),
                scores,
                race.as_deref(),
                roots,
                groups,
            )
        })
    };
    if run((false, false, false), &classes, &roots, &groups, ex, offers)
        || offers.climbed.contains_key(target)
    {
        return true;
    }
    // the climb pass ranks every configuration by the needed ids it reaches (the archetype a
    // needed archetype feature belongs to, the class of a needed class feature)
    every.extend(classes.iter().cloned());
    let ranked = rank_configs(e, target, &groups, every);
    // a spell a subclass's (or feature's) own spell list names: that host first
    let (mut classes, mut roots) = spell_list_hosts(e, target, ranked, roots);
    // a needed spell that a subclass's own list adds to a class whose select offers the option
    // (an invocation that requires a spell only one patron's expanded list holds): that
    // configuration first
    let own_classes: BTreeSet<String> = {
        let mut own = Plan::default();
        e.offerers.place_precise(target, &mut own);
        own.classes
    };
    let mut listed: Vec<(String, Option<String>)> = Vec::new();
    for id in groups.iter().flatten() {
        if let Some(spell) = store.element(id).filter(|x| x.ty == "Spell") {
            let mut scratch = Plan::default();
            for c in list_owner_configs(e, spell, &mut scratch) {
                if c.1.is_some() && own_classes.contains(&c.0) && !listed.contains(&c) {
                    listed.push(c);
                }
            }
        }
    }
    if !listed.is_empty() {
        classes.retain(|c| !listed.contains(c));
        classes.splice(0..0, listed);
    }
    // the climb pass records apart, with its own explore memo: the hosts after it (options,
    // items) are planned exactly as before it existed
    let mut memo = Explore {
        owners: ex.owners,
        memo: ex.memo.clone(),
        explored: 0,
    };
    let mut pass = Offers::default();
    // the climb pass follows a requirement chain to its start (the 10th copy of an upgrade
    // that requires the 9th, which requires the 8th, ...), then takes the elements that set a
    // counter a requirement tests (`[smith:armor:1]` set by an earlier upgrade pick)
    let mut deep = needed_groups(e, target, el, MAX_CHAIN);
    for g in groups {
        if !deep.contains(&g) {
            deep.push(g);
        }
    }
    // what a grant on the climb paths asks for (an item grants a trait whose select's choice
    // decides which further trait it grants; a feat grants a feature to one race only): the
    // user makes that choice, and takes that race when nothing else decides it
    let granting = grant_requirement_groups(store, &roots.path);
    if roots.origins.is_empty() {
        for id in granting.iter().flatten() {
            let mut p = Plan::default();
            e.offerers.place(id, &mut p);
            roots.origins.extend(p.origins);
        }
    }
    // when one element alone owns the selects that offer the option, what that owner requires
    // is needed too (a feat for one race whose select offers the feature)
    let owners: BTreeSet<&str> = e
        .selects
        .offering(target, &el.ty, Some(el))
        .into_iter()
        .map(|(o, _, _)| o.as_str())
        .collect();
    let mut owned = Vec::new();
    if let [owner] = owners.into_iter().collect::<Vec<_>>().as_slice() {
        if let Some(x) = store.element(owner) {
            if let Ok(Some(expr)) = RequirementsExpr::parse(&x.requirements) {
                required_groups(store, expr.root(), &mut owned);
            }
        }
    }
    for g in granting.into_iter().chain(owned) {
        if !deep.contains(&g) && !g.iter().any(|i| i == target) {
            deep.push(g);
        }
    }
    let mut groups = deep;
    let counters = stat_groups(store, target, &groups);
    // the classes whose features set such a counter (a monk's ki for a feat option testing
    // `[ki:points:1]`) are tried after the ranked ones
    groups.extend(counters.iter().cloned());
    // and the classes a carried item's attunement restriction names (the robe whose select
    // offers the option attunes only to a sorcerer, warlock, or wizard)
    let carried = climb_items(store, target, &roots, &groups);
    for c in counter_configs(e, &counters)
        .into_iter()
        .chain(attunement_configs(e, &carried))
        .chain(grant_level_configs(e, &roots.path))
    {
        if !classes.contains(&c) {
            classes.push(c);
        }
    }
    // then, where the climb pass offers nothing, the multiclass pass: the same configurations
    // also multiclassing as a user does (a class's own multiclass node; another class leading
    // the levels), apart so that what the climb pass offered stays offered
    let found = run(
        (true, false, false),
        &classes,
        &roots,
        &groups,
        &mut memo,
        &mut pass,
    ) || {
        // only the configurations the multiclass pass changes (a multiclass the climb pass
        // does not take), at most MAX_MULTICLASS_CONFIGS: the pass is a retry, not a sweep
        let path: Vec<Vec<String>> = roots
            .path
            .iter()
            .filter(|p| on_climb(store, p))
            .map(|p| vec![p.clone()])
            .collect();
        let changed: Vec<(String, Option<String>)> = classes
            .iter()
            .filter(|(c, _)| {
                multiclass_entries(e, c, &groups, true) != multiclass_entries(e, c, &groups, false)
                    || !multiclass_entries(e, c, &path, true).is_empty()
            })
            .take(MAX_MULTICLASS_CONFIGS)
            .cloned()
            .collect();
        // and last, where that offers nothing either, the same configurations planned as a
        // user who wants the option plans: multiclass into the class whose select offers it,
        // and keep that select for it (see `option_host_on`)
        !changed.is_empty()
            && (run(
                (true, true, false),
                &changed,
                &roots,
                &groups,
                &mut memo,
                &mut pass,
            ) || run(
                (true, true, true),
                &changed,
                &roots,
                &groups,
                &mut memo,
                &mut pass,
            ))
    };
    ex.explored += memo.explored;
    offers.keep_climbed(pass);
    found
}

/// Host configurations an option host tries (needs class, climbed classes, archetypes, fallback),
/// before the other fallback class.
const OPTION_HOST_CLASSES: usize = 4;

/// How deep the climb pass follows a requirement chain (an upgrade line of ten copies, each
/// requiring the one before).
const MAX_CHAIN: usize = 12;

/// Configurations the multiclass pass of [`option_host`] retries at most.
const MAX_MULTICLASS_CONFIGS: usize = 4;

/// The ability score multiclass prerequisites ask for (PHB p.163; homebrew follows it).
const MULTICLASS_SCORE: i32 = 13;

/// One configuration of [`option_host`].
#[allow(clippy::too_many_arguments)]
fn option_host_on(
    d: &Driver<'_>,
    e: &Edition<'_>,
    target: &str,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
    (class, archetype, climb, mc, keep_offering): (&str, Option<&str>, bool, bool, bool),
    scores: [i32; 6],
    race: Option<&str>,
    roots: &Plan,
    groups: &[Vec<String>],
) -> bool {
    let Some(mut h) = d.create(e.rs, scores) else {
        return false;
    };
    let label = format!("option:{target}");
    let trace = std::env::var("BOREALIS_ACCEPT_TRACE")
        .is_ok_and(|t| t.split(',').any(|x| x.trim() == target));
    let took = d.pick(&mut h, class);
    if trace {
        eprintln!(
            "TRACE {target}: class {class} {took} archetype {archetype:?} climb {climb} groups {groups:?}"
        );
    }
    // the climb pass takes the origins the paths reach before the fallback race (a sub race's
    // trait needs the sub race's own race, not the human host)
    let origins = roots.origins.iter().map(String::as_str);
    // a race or background the option's requirements name comes before the path origins
    let needed_races = groups.iter().flatten().filter(|id| {
        e.ins
            .store
            .element(id)
            .is_some_and(|x| matches!(x.ty.as_str(), "Race" | "Species" | "Background"))
    });
    let mut own = Plan::default();
    e.offerers.place(target, &mut own);
    // then a race an element on the paths requires (a class variant for elves only)
    let mut path_races: Vec<String> = Vec::new();
    if climb {
        for p in roots.path.iter().filter(|p| on_climb(&e.ins.store, p)) {
            let Some(x) = e.ins.store.element(p) else {
                continue;
            };
            if let Ok(Some(expr)) = RequirementsExpr::parse(&x.requirements) {
                let mut found = Vec::new();
                required_groups(&e.ins.store, expr.root(), &mut found);
                let races = found.into_iter().flatten().filter(|id| {
                    e.ins
                        .store
                        .element(id)
                        .is_some_and(|x| matches!(x.ty.as_str(), "Race" | "Species"))
                });
                path_races.extend(races);
            }
        }
    }
    // and the race of a sub race the option's requirements name (a feat for one subrace)
    let mut sub_parents: Vec<String> = Vec::new();
    if climb {
        let subs = groups.iter().flatten().filter(|id| {
            e.ins
                .store
                .element(id)
                .is_some_and(|x| matches!(x.ty.as_str(), "Sub Race" | "Race Variant"))
        });
        for id in subs {
            let mut p = Plan::default();
            e.offerers.place_precise(id, &mut p);
            let races = p.origins.into_iter().filter(|o| {
                e.ins
                    .store
                    .element(o)
                    .is_some_and(|x| matches!(x.ty.as_str(), "Race" | "Species"))
            });
            sub_parents.extend(races);
        }
    }
    // an option only origins reach (no class, subclass or feat on its own path: a racial trait
    // of one sub race's trait select) has its own origins next, so the race is the one whose
    // sub race offers it, not the first race on the path of something it needs
    let origin_only = own.classes.is_empty() && own.archetypes.is_empty() && own.feats.is_empty();
    let own_origins = own.origins.iter().filter(|_| climb && origin_only);
    let order: Vec<&str> = if climb {
        needed_races
            .map(String::as_str)
            .chain(sub_parents.iter().map(String::as_str))
            .chain(own_origins.map(String::as_str))
            .chain(path_races.iter().map(String::as_str))
            .chain(origins)
            .chain(race)
            .collect()
    } else {
        race.into_iter().chain(origins).collect()
    };
    // the climb pass leaves alone what the option's requirements negate (a race variant)
    let avoid = if climb {
        // and what the selects offering it negate (a variant select closed to one variant)
        let select_reqs: Vec<&str> = e
            .ins
            .store
            .element(target)
            .map(|el| e.selects.offering(target, &el.ty, Some(el)))
            .unwrap_or_default()
            .into_iter()
            .map(|(_, _, r)| r.as_str())
            .collect();
        negated_ids(&e.ins.store, target, groups, &select_reqs)
    } else {
        Vec::new()
    };
    if trace && climb {
        eprintln!("TRACE {target}: avoid {avoid:?} order {order:?}");
    }
    for r in order.into_iter().filter(|r| !avoid.iter().any(|a| a == r)) {
        d.pick(&mut h, r);
    }
    let mut want: Vec<Vec<String>> = archetype.map(|a| vec![a.to_string()]).into_iter().collect();
    // a sub race or race variant on the climb path is picked like any needed id
    let mut sub_origins: Vec<String> = roots
        .path
        .iter()
        .filter(|p| {
            e.ins
                .store
                .element(p)
                .is_some_and(|x| matches!(x.ty.as_str(), "Sub Race" | "Race Variant"))
        })
        .cloned()
        .collect();
    // the option's own path first: a user picks the sub race whose trait's select offers the
    // option, not the first sub race on the path of something it needs (a tool proficiency
    // that several sub races of the same race grant)
    sub_origins.sort_by_key(|p| !own.path.contains(p));
    if !sub_origins.is_empty() {
        want.push(sub_origins);
    }
    // a needed character option (an `Option` element such as the DMG firearms) is turned on in
    // the options dialog, not picked in a slot
    let (options, rest) = items::split_options(&e.ins.store, groups);
    d.enable_options(&mut h, &options);
    want.extend(rest);
    // the climb pass also meets what the elements on the paths require (the character option
    // a homebrew multiclass entry needs), after the option's own needs
    if climb {
        let mut path_groups: Vec<Vec<String>> = Vec::new();
        for p in roots.path.iter().filter(|p| on_climb(&e.ins.store, p)) {
            let Some(x) = e.ins.store.element(p) else {
                continue;
            };
            if let Ok(Some(expr)) = RequirementsExpr::parse(&x.requirements) {
                required_groups(&e.ins.store, expr.root(), &mut path_groups);
            }
        }
        path_groups.retain(|g| !g.iter().any(|i| i == target) && !want.contains(g));
        let (options, rest) = items::split_options(&e.ins.store, &path_groups);
        d.enable_options(&mut h, &options);
        want.extend(rest);
    }
    // a feat whose select offers the option is taken where a feat is offered (Weapon Master)
    if !own.feats.is_empty() {
        want.push(own.feats.iter().cloned().collect());
    }
    // then every element a select puts on the climb paths of the option and of what it needs
    // (the racial trait whose select offers the option, the feat whose select offers a needed
    // option): a user picks the chain down to the option, so the host does too
    let path = roots.path.iter().filter(|_| climb);
    for p in path.filter(|p| *p != target && on_climb(&e.ins.store, p)) {
        if !want.iter().any(|g| g.contains(p)) {
            want.push(vec![p.clone()]);
        }
    }
    // an item on a path, or one that grants a needed id (a "feat unlocker" item granting the
    // id a feat's requirement names), is carried, as a user adds it on the equipment screen
    if climb {
        let carried = climb_items(&e.ins.store, target, roots, groups);
        for item in &carried {
            let held = d.hold_item(&mut h, item);
            if trace {
                eprintln!("TRACE {target}: item {item} held {held}");
            }
        }
        // a later item of the same slot took a needed item off (the armor whose stat a
        // requirement tests, then a path item's armor): a user wears the needed one
        for g in groups {
            if let Some(item) = g.iter().find(|i| carried.contains(i)) {
                let worn = d.equip_again(&mut h, item);
                if trace {
                    eprintln!("TRACE {target}: item {item} equipped {worn}");
                }
            }
        }
    }
    let offered = |d: &Driver<'_>, h: &Host| {
        h.snap
            .slots
            .iter()
            .any(|s| s.chosen.is_none() && d.options(h, &s.id).iter().any(|(id, _)| id == target))
    };
    // a Multiclass element (the option or one on its paths: homebrew "auxiliary levels") is
    // offered by the select of a new multiclass level: the climb pass takes its second level so
    let is_mc = |id: &str| {
        e.ins
            .store
            .element(id)
            .is_some_and(|x| x.ty == MULTICLASS_TYPE)
    };
    // a needed id only another class reaches (a class's own grant that a pact boon requires):
    // the climb pass multiclasses into that class
    if climb {
        let entries = multiclass_entries(e, class, groups, mc);
        if !entries.is_empty() && !want.contains(&entries) {
            want.push(entries);
        }
    }
    // an element on the paths that only another class reaches (the bard's Magical Secrets,
    // whose select offers a spell that requires the Elementalist class: a multiclass never
    // registers the class id, so that class stays the main one): the climb pass multiclasses
    // into the other class and levels it, as a user does
    let mut lead = if climb && mc {
        multiclass_entries(e, class, &want, true)
    } else {
        Vec::new()
    };
    // the last pass plans for the option: the class whose own select (or subclass's select)
    // offers it leads first, as a user multiclasses into the class that offers it, not into the
    // first class some path reaches
    if keep_offering && lead.len() > 1 {
        let offering = multiclass_entries(e, class, &[vec![target.to_string()]], true);
        lead.sort_by_key(|m| !offering.contains(m));
    }
    // and it keeps the selects that can offer the option (by type and supports) for it: the
    // elements on the paths are not picked there, nor is the select filled, so it offers the
    // option once its requirements hold (a blank subclass feature select that every subclass
    // feature fits, and the only select that offers the option)
    let offer_owners = if keep_offering {
        offering_owners(e, target)
    } else {
        BTreeSet::new()
    };
    let needed: BTreeSet<&str> = groups.iter().flatten().map(String::as_str).collect();
    let mut kept_before_lead: Option<BTreeSet<String>> = None;
    // the wanted groups the lead class serves (a spell only its select offers): once they are
    // picked, the levels go back to the main class (whose select offers the option)
    let lead_for: Vec<Vec<String>> = if lead.is_empty() {
        Vec::new()
    } else {
        want.iter()
            .filter(|g| !multiclass_entries(e, class, std::slice::from_ref(*g), true).is_empty())
            .cloned()
            .collect()
    };
    if !lead.is_empty() && !want.contains(&lead) {
        want.push(lead.clone());
    }
    let mut multiclass =
        climb && (is_mc(target) || want.iter().flatten().any(|w| is_mc(w)) || !lead.is_empty());
    for g in &mut want {
        g.retain(|i| !avoid.contains(i));
    }
    want.retain(|g| !g.is_empty());
    let mut level_slot = String::from("main");
    let mut found = false;
    loop {
        d.collect(&h, &label, offers);
        d.open_feat_selects(&mut h);
        // pick what is wanted wherever it is offered, until nothing more can be picked
        // and stop once the option is offered: a user does not fill the select that offers it
        // with other picks first (a points budget the other path traits would spend)
        let mut progress = true;
        while progress && !offered(d, &h) {
            progress = false;
            for g in want.clone() {
                let leave = if g.iter().any(|w| needed.contains(w.as_str())) {
                    BTreeSet::new()
                } else {
                    kept_slots(&h, &offer_owners)
                };
                if g.iter().any(|w| d.pick_leaving(&mut h, w, &leave)) {
                    want.retain(|x| *x != g);
                    progress = true;
                    if offered(d, &h) {
                        break;
                    }
                }
            }
        }
        d.collect(&h, &label, offers);
        if trace {
            let chosen: Vec<&str> = h
                .snap
                .slots
                .iter()
                .filter_map(|s| s.chosen.as_ref().map(|c| c.id.as_str()))
                .collect();
            eprintln!(
                "TRACE {target}: level {} want {want:?} offered {} chosen {chosen:?}",
                h.snap.level.total,
                offered(d, &h)
            );
            // the open slots, with how many options each has and the wanted ids among them
            let wanted: BTreeSet<&str> = want.iter().flatten().map(String::as_str).collect();
            let open: Vec<(String, usize, Vec<String>)> = h
                .snap
                .slots
                .iter()
                .filter(|s| s.chosen.is_none())
                .map(|s| {
                    let o = d.options(&h, &s.id);
                    let hit = o
                        .iter()
                        .filter(|(id, _)| wanted.contains(id.as_str()))
                        .map(|(id, _)| id.clone())
                        .collect();
                    (s.id.clone(), o.len(), hit)
                })
                .collect();
            eprintln!("TRACE {target}: open {open:?}");
        }
        if offered(d, &h) {
            found = true;
            if d.pick(&mut h, target) {
                d.collect(&h, &label, offers);
                d.explore(&mut h, ex, &label, offers);
            }
            break;
        }
        let leave = kept_slots(&h, &offer_owners);
        d.fill_leaving(&mut h, &label, offers, &avoid, &leave);
        if h.snap.level.total >= MAX_LEVEL {
            break;
        }
        if std::mem::take(&mut multiclass) {
            // a user taking a multiclass level first meets its prerequisites (13 in the
            // multiclass ability; a homebrew class may ask for two, `[dex:13],[cha:13]`)
            if mc {
                d.set_scores(&mut h, scores.map(|s| s.max(MULTICLASS_SCORE)));
            }
            let added = d.add_multiclass(&mut h);
            if trace {
                let open: Vec<(String, Vec<String>)> = h
                    .snap
                    .slots
                    .iter()
                    .filter(|s| s.chosen.is_none())
                    .map(|s| {
                        let o = d.options(&h, &s.id).into_iter().map(|(id, _)| id);
                        (s.id.clone(), o.take(12).collect())
                    })
                    .collect();
                eprintln!("TRACE {target}: multiclass added {added} open {open:?}");
            }
            if added {
                if !lead.is_empty() {
                    level_slot = format!("multi-{}", h.snap.level.total);
                    kept_before_lead = Some(kept_slots(&h, &offer_owners));
                }
                continue;
            }
        }
        // planning for the option, the lead class has served once a select that can offer the
        // option is on the character that was not there before it (the subclass feature)
        let kept_new = kept_before_lead
            .as_ref()
            .is_some_and(|before| !kept_slots(&h, &offer_owners).is_subset(before));
        let served =
            kept_new || (!lead_for.is_empty() && lead_for.iter().all(|g| !want.contains(g)));
        if served && level_slot != "main" {
            level_slot = String::from("main");
        }
        if !d.level_up_slot(&mut h, &level_slot) {
            break;
        }
    }
    // every slot filled: a choice made earlier can be changed to the option. Aurora evaluates a
    // select's requirements against the character as it is now (`[level:warlock:2]` reads the
    // current class level, ExpressionInterpreter.cs:42-63), and so does the builder: the 10th
    // copy of PHB24 Lessons of the First Ones takes the Warlock 1 invocation slot at level 18
    let keep: Vec<&str> = groups.iter().flatten().map(String::as_str).collect();
    if !found && d.rechoose(&mut h, target, &keep) {
        found = true;
        d.collect(&h, &label, offers);
        d.explore(&mut h, ex, &label, offers);
    }
    // an option the host's character holds without a pick reaches it through a grant (a
    // subclass feature grants a feature that the class's select also lists, and a select never
    // offers what the character already has): it is on the character, which is what the row asks
    if !found && climb && d.holds(&h, target) {
        found = true;
        offers.add(target, &format!("held:{label}"));
    }
    d.drop_host(h);
    found
}

/// A requirements string that holds on every character of this store: empty, or only negations
/// of ids the store does not have (`!ID_WOTC_TCOE_OPTION_CUSTOMIZED_PROFICIENCY` without Tasha's).
fn always_true(store: &ContentStore, requirements: &str) -> bool {
    fn holds(store: &ContentStore, e: &BoolExpr<RequirementLeaf>) -> bool {
        match e {
            BoolExpr::Not(inner) => match inner.as_ref() {
                BoolExpr::Leaf(RequirementLeaf::Id(id)) => store.element(id).is_none(),
                _ => false,
            },
            BoolExpr::And(items) => items.iter().all(|x| holds(store, x)),
            BoolExpr::Or(items) => items.iter().any(|x| holds(store, x)),
            BoolExpr::Leaf(_) => false,
        }
    }
    if requirements.trim().is_empty() {
        return true;
    }
    matches!(RequirementsExpr::parse(requirements), Ok(Some(expr)) if holds(store, expr.root()))
}
