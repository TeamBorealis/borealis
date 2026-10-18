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
}

/// What one worker found: offers, (archetype, class) pairs offered, options explored, hosts.
type Found = (Offers, Vec<(String, String)>, usize, usize);

/// Shared inputs of the hosts of one edition.
struct Edition<'a> {
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
        Job::Option(id) => {
            option_host(d, e, id, ex, offers);
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
    let offerers = Offerers::new(&ins.store);
    let selects = select_index(&ins.store);
    let e = Edition {
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
    merge(&mut found, run_jobs(apps, &e, &options));
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
    let offering = selects.offering(id, ty, el);
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
fn record(src: &str, ins: &Installed, offers: &Offers) {
    let selects = select_index(&ins.store);
    let mut rows: BTreeMap<(String, &str), [i64; 2]> = BTreeMap::new();
    let mut seen_types: BTreeSet<&str> = BTreeSet::new();
    for (id, ty, _) in &ins.ours {
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
    // needed: earlier copies, then what the requirements name (and what those name), as
    // any-of groups
    let mut groups: Vec<Vec<String>> = earlier_copies(store, target)
        .into_iter()
        .map(|c| vec![c])
        .collect();
    let mut queue = vec![target.to_string()];
    for _ in 0..prereq::MAX_PRIOR_FEATS {
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
    classes
        .into_iter()
        .take(OPTION_HOST_CLASSES)
        .any(|(class, archetype)| {
            option_host_on(
                d,
                e,
                target,
                ex,
                offers,
                (&class, archetype.as_deref()),
                scores,
                race.as_deref(),
                &roots,
                &groups,
            )
        })
}

/// Host configurations an option host tries (needs class, climbed classes, archetypes, fallback).
const OPTION_HOST_CLASSES: usize = 4;

/// One configuration of [`option_host`].
#[allow(clippy::too_many_arguments)]
fn option_host_on(
    d: &Driver<'_>,
    e: &Edition<'_>,
    target: &str,
    ex: &mut Explore<'_>,
    offers: &mut Offers,
    (class, archetype): (&str, Option<&str>),
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
        eprintln!("TRACE {target}: class {class} {took} archetype {archetype:?} groups {groups:?}");
    }
    for r in race
        .into_iter()
        .chain(roots.origins.iter().map(String::as_str))
    {
        d.pick(&mut h, r);
    }
    let mut want: Vec<Vec<String>> = archetype.map(|a| vec![a.to_string()]).into_iter().collect();
    // a sub race or race variant on the climb path is picked like any needed id
    let sub_origins: Vec<String> = roots
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
    if !sub_origins.is_empty() {
        want.push(sub_origins);
    }
    want.extend(groups.iter().cloned());
    let offered = |d: &Driver<'_>, h: &Host| {
        h.snap
            .slots
            .iter()
            .any(|s| s.chosen.is_none() && d.options(h, &s.id).iter().any(|(id, _)| id == target))
    };
    let mut found = false;
    loop {
        d.collect(&h, &label, offers);
        d.open_feat_selects(&mut h);
        // pick what is wanted wherever it is offered, until nothing more can be picked
        let mut progress = true;
        while progress {
            progress = false;
            for g in want.clone() {
                if g.iter().any(|w| d.pick(&mut h, w)) {
                    want.retain(|x| *x != g);
                    progress = true;
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
        }
        if offered(d, &h) {
            found = true;
            if d.pick(&mut h, target) {
                d.collect(&h, &label, offers);
                d.explore(&mut h, ex, &label, offers);
            }
            break;
        }
        d.fill(&mut h, &label, offers);
        if h.snap.level.total >= MAX_LEVEL || !d.level_up(&mut h) {
            break;
        }
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
