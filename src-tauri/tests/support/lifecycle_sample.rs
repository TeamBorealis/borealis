//! Stage 4 character sample and measurements (specs/acceptance-imports-1.md, stage 4): every
//! class of a source at levels 5 and 11, plus characters holding the source's races, sub races,
//! variants, species, backgrounds and feats, all built with the builder commands through
//! [`Driver`] (the stage-2 host builder). A measurement opens every character as the app does
//! (`load_character`) and records its snapshot digest, the options its open slots offer, the
//! content it lists as not installed and the values the update checks read.

use super::app::{Driver, Host, Offers, CASTER_SCORES, TEST_SCORES};
use super::plan::{Offerers, Plan};
use borealis_content::Ruleset;
use borealis_lib::content_dto::PackKind;
use borealis_lib::dto::CharacterSnapshot;
use borealis_lib::state::AppState;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Levels every class character is saved at (one character per level).
pub const CLASS_LEVELS: [u32; 2] = [5, 11];
/// Origin and feat types sampled one character per element.
pub const ORIGIN_TYPES: &[&str] = &[
    "Race",
    "Sub Race",
    "Race Variant",
    "Species",
    "Background",
    "Feat",
];
/// Slot types the sample chooses itself (preferring the source's elements); the rest is the
/// builder fill's first option.
const CHOSEN_TYPES: &[&str] = &[
    "Race",
    "Sub Race",
    "Race Variant",
    "Species",
    "Background",
    "Archetype",
    "Feat",
];
/// Passes of [`settle`] per level.
const SETTLE_PASSES: usize = 40;
/// Highest level an origin or feat character is levelled to while its element is not offered.
const ORIGIN_MAX_LEVEL: u32 = 8;

/// The elements a source installed: id -> (type, edition).
#[derive(Default, Clone)]
pub struct Ours {
    pub packs: Vec<String>,
    pub ids: BTreeMap<String, (String, Ruleset)>,
}

impl Ours {
    pub fn of_type<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = (&'a String, Ruleset)> + 'a {
        self.ids
            .iter()
            .filter(move |(_, (t, _))| t == ty)
            .map(|(id, (_, rs))| (id, *rs))
    }
}

/// Non-bundled pack ids installed now.
pub fn user_packs(app: &AppState) -> BTreeSet<String> {
    app.list_packs()
        .expect("list packs")
        .into_iter()
        .filter(|p| p.kind != PackKind::Bundled)
        .map(|p| p.id)
        .collect()
}

/// The elements of `packs` in the app's live store.
pub fn ours(app: &AppState, packs: &[String]) -> Ours {
    let pack_rs: BTreeMap<String, Option<Ruleset>> = app
        .list_packs()
        .expect("list packs")
        .into_iter()
        .filter(|p| packs.contains(&p.id))
        .map(|p| {
            let rs = (p.rulesets.len() == 1).then(|| p.rulesets[0]);
            (p.id, rs)
        })
        .collect();
    let hub = app.hub().expect("hub");
    let mut ids = BTreeMap::new();
    for se in hub.store.iter() {
        let Some((_, rs)) = pack_rs
            .iter()
            .find(|(p, _)| se.file.starts_with(&format!("{p}/")))
        else {
            continue;
        };
        let rs = se.element.ruleset.or(*rs).unwrap_or(Ruleset::Dnd2014);
        ids.insert(se.element.id.clone(), (se.element.ty.clone(), rs));
    }
    Ours {
        packs: packs.to_vec(),
        ids,
    }
}

/// One saved character of the sample.
#[derive(Clone, Debug)]
pub struct Member {
    pub id: String,
    pub label: String,
}

#[derive(Default)]
pub struct Sample {
    pub members: Vec<Member>,
    /// sampled ids no host could place (counted, not a lifecycle check)
    pub unplaced: Vec<String>,
}

/// Slot types compare without case and spaces (the snapshot says `subrace`, Aurora `Sub Race`).
fn norm(t: &str) -> String {
    t.to_lowercase().replace(' ', "")
}

/// Choose the open slots of [`CHOSEN_TYPES`] first (a preferred id when offered, else the
/// option at `seed`), then let the builder fill take the first option of the rest; repeated
/// until nothing changes. Deterministic.
fn settle(d: &Driver<'_>, h: &mut Host, seed: usize, prefs: &[String], feats: bool) {
    let chosen_types: Vec<String> = CHOSEN_TYPES.iter().map(|t| norm(t)).collect();
    let mut refused = BTreeSet::new();
    let mut sink = Offers::default();
    for _ in 0..SETTLE_PASSES {
        // the improvement's "Feat" option before the fill takes its first option
        if feats {
            d.open_feat_selects(h);
        }
        let open = h.snap.slots.iter().find(|s| {
            s.chosen.is_none()
                && !refused.contains(&s.id)
                && chosen_types.contains(&norm(&s.element_type))
        });
        let Some(slot) = open.map(|s| s.id.clone()) else {
            let gen = h.gen;
            d.fill(h, "", &mut sink);
            if h.gen == gen {
                return;
            }
            continue;
        };
        let opts: Vec<String> = d.options(h, &slot).into_iter().map(|o| o.0).collect();
        let choice = prefs
            .iter()
            .find(|p| opts.contains(p))
            .cloned()
            .or_else(|| (!opts.is_empty()).then(|| opts[seed % opts.len()].clone()));
        let picked = choice.is_some_and(|c| d.pick(h, &c));
        if !picked {
            refused.insert(slot);
        }
    }
}

fn chosen(snap: &CharacterSnapshot) -> BTreeSet<String> {
    snap.slots
        .iter()
        .filter_map(|s| s.chosen.as_ref().map(|c| c.id.clone()))
        .collect()
}

fn save(d: &Driver<'_>, h: &Host) -> bool {
    d.app.save_character(&h.id).is_ok() && d.app.close_character(&h.id, true).is_ok()
}

/// The source's preferred ids for a character of `rs`, rotated by `seed` (spreads coverage).
fn prefs_for(ours: &Ours, rs: Ruleset, seed: usize) -> Vec<String> {
    let mut v: Vec<String> = ours
        .ids
        .iter()
        .filter(|(_, (t, r))| *r == rs && CHOSEN_TYPES.contains(&t.as_str()))
        .map(|(id, _)| id.clone())
        .collect();
    if !v.is_empty() {
        let k = seed % v.len();
        v.rotate_left(k);
    }
    v
}

/// Class ids of the sample per edition: the source's own classes, else the builder's Fighter
/// and Wizard of every edition the source has content for.
fn sample_classes(d: &Driver<'_>, ours: &Ours) -> Vec<(String, Ruleset)> {
    let own: Vec<(String, Ruleset)> = ours
        .of_type("Class")
        .map(|(id, rs)| (id.clone(), rs))
        .collect();
    if !own.is_empty() {
        return own;
    }
    let editions: BTreeSet<Ruleset> = ours.ids.values().map(|(_, rs)| *rs).collect();
    let mut out = Vec::new();
    for rs in editions {
        let Some(h) = d.create(rs, TEST_SCORES) else {
            continue;
        };
        for name in ["Fighter", "Wizard"] {
            if let Some(id) = d.find_named(&h, "Class", name, "") {
                out.push((id, rs));
            }
        }
        d.drop_host(h);
    }
    out
}

/// A class character saved at every level of [`CLASS_LEVELS`] (a copy per level).
fn class_members(
    d: &Driver<'_>,
    ours: &Ours,
    class: &str,
    rs: Ruleset,
    seed: usize,
    out: &mut Sample,
) {
    let Some(mut h) = d.create(rs, CASTER_SCORES) else {
        out.unplaced.push(class.to_string());
        return;
    };
    if !d.pick(&mut h, class) {
        d.drop_host(h);
        out.unplaced.push(class.to_string());
        return;
    }
    let prefs = prefs_for(ours, rs, seed);
    let feats = seed % 2 == 1;
    settle(d, &mut h, seed, &prefs, feats);
    for (n, level) in CLASS_LEVELS.iter().enumerate() {
        while h.snap.level.total < *level && d.level_up(&mut h) {
            settle(d, &mut h, seed, &prefs, feats);
        }
        if h.snap.level.total != *level {
            out.unplaced.push(format!("{class}@{level}"));
            break;
        }
        if n + 1 < CLASS_LEVELS.len() {
            // keep this level as its own character, go on with a copy
            if d.app.save_character(&h.id).is_err() {
                break;
            }
            out.members.push(Member {
                id: h.id.clone(),
                label: format!("{class}@{level}"),
            });
            let name = format!("Lifecycle {seed} L{}", CLASS_LEVELS[n + 1]);
            let Ok(snap) = d.app.save_character_as(&h.id, Some(&name), None) else {
                break;
            };
            h = Host {
                id: snap.id.clone(),
                snap,
                gen: h.gen + 1,
            };
        } else if save(d, &h) {
            out.members.push(Member {
                id: h.id.clone(),
                label: format!("{class}@{level}"),
            });
        }
    }
    let _ = d.app.close_character(&h.id, true);
}

/// A character holding `target` (an origin or feat): its class and origins come from the
/// elements that offer it ([`Offerers::place`]); levelled until it is offered.
fn origin_member(
    d: &Driver<'_>,
    offerers: &Offerers<'_>,
    ours: &Ours,
    classes: &[(String, Ruleset)],
    target: &str,
    rs: Ruleset,
    seed: usize,
) -> Option<Member> {
    let mut plan = Plan::default();
    offerers.place(target, &mut plan);
    let mut h = d.create(rs, CASTER_SCORES)?;
    // the classes that offer it, the sample's classes of this edition, then the builder's
    // Fighter (an improvement at level 4 offers general feats): the first the builder accepts
    let fighter = d.find_named(&h, "Class", "Fighter", "");
    let mut candidates: Vec<&String> = plan
        .classes
        .iter()
        .chain(classes.iter().filter(|(_, r)| *r == rs).map(|c| &c.0))
        .collect();
    let feat = ours.ids.get(target).is_some_and(|(t, _)| t == "Feat");
    if let Some(f) = fighter.as_ref() {
        // a general feat (no select of the source names it) goes on the Fighter first
        let position = if feat && plan.classes.is_empty() {
            0
        } else {
            candidates.len()
        };
        candidates.insert(position, f);
    }
    if !candidates.iter().any(|c| d.pick(&mut h, c)) {
        d.drop_host(h);
        return None;
    }
    let mut prefs = vec![target.to_string()];
    prefs.extend(plan.origins.iter().cloned());
    prefs.extend(prefs_for(ours, rs, seed));
    loop {
        settle(d, &mut h, seed, &prefs, feat);
        if chosen(&h.snap).contains(target) {
            if save(d, &h) {
                return Some(Member {
                    id: h.id.clone(),
                    label: format!("holds:{target}"),
                });
            }
            break;
        }
        if h.snap.level.total >= ORIGIN_MAX_LEVEL || !d.level_up(&mut h) {
            break;
        }
    }
    if std::env::var_os("BOREALIS_LIFECYCLE_DEBUG").is_some() {
        let open: Vec<String> = h
            .snap
            .slots
            .iter()
            .map(|s| format!("{}={:?}", s.element_type, s.chosen.as_ref().map(|c| &c.id)))
            .collect();
        eprintln!(
            "  not placed {target}: plan {:?} chosen {:?} open {open:?}",
            plan.classes,
            chosen(&h.snap)
        );
    }
    d.drop_host(h);
    None
}

/// The stage-4 sample of a source; `per_type` caps the origin/feat characters per type.
pub fn build(app: &AppState, ours: &Ours, per_type: usize) -> Sample {
    let d = Driver::new(app);
    let mut out = Sample::default();
    let classes = sample_classes(&d, ours);
    for (seed, (class, rs)) in classes.iter().enumerate() {
        class_members(&d, ours, class, *rs, seed, &mut out);
    }
    let hub = app.hub().expect("hub");
    let offerers = Offerers::new(&hub.store);
    let mut seed = classes.len();
    for ty in ORIGIN_TYPES {
        for (id, rs) in ours.of_type(ty).take(per_type) {
            seed += 1;
            match origin_member(&d, &offerers, ours, &classes, id, rs, seed) {
                Some(m) => out.members.push(m),
                None => out.unplaced.push(id.clone()),
            }
        }
    }
    out
}

// ---------------------------------------------------------------- measurements

/// One character as the app opens it.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub opened: bool,
    pub panicked: bool,
    pub snapshot: String,
    pub offered: String,
    pub chosen: BTreeSet<String>,
    /// ids the snapshot's notices name as not installed
    pub listed_missing: BTreeSet<String>,
    pub value: serde_json::Value,
}

/// Every sample character, plus the L1 builder's offers per edition.
#[derive(Clone, Debug, Default)]
pub struct Measure {
    pub chars: BTreeMap<String, Seen>,
    pub builder: BTreeMap<String, String>,
}

fn sha(text: &str) -> String {
    let d = Sha256::digest(text.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Digest of a snapshot without the session-only fields (`dirty`, the display path).
pub fn snapshot_digest(snap: &CharacterSnapshot) -> (String, serde_json::Value) {
    let mut v = serde_json::to_value(snap).expect("snapshot json");
    if let Some(m) = v.as_object_mut() {
        m.remove("dirty");
        m.remove("displayPath");
    }
    (sha(&v.to_string()), v)
}

/// Options of every open slot, in slot order.
fn offered_digest(d: &Driver<'_>, h: &Host) -> String {
    let mut text = String::new();
    for s in h.snap.slots.iter().filter(|s| s.chosen.is_none()) {
        let ids: Vec<String> = d.options(h, &s.id).into_iter().map(|o| o.0).collect();
        text.push_str(&format!("{}={};", s.id, ids.join(",")));
    }
    sha(&text)
}

/// Ids a notice names as not installed (`chosen element X is not installed`,
/// `option X is not installed`, `... grants unknown element X`).
pub fn missing_in_notices(notices: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for n in notices {
        let about_missing = n.contains("is not installed") || n.contains("unknown element");
        if !about_missing {
            continue;
        }
        let mut ids = n
            .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
            .filter(|w| w.starts_with("ID_"));
        if n.contains("unknown element") {
            out.extend(ids.next_back().map(str::to_string));
        } else {
            out.extend(ids.take(1).map(str::to_string));
        }
    }
    out
}

fn see(d: &Driver<'_>, id: &str) -> Seen {
    let opened = catch_unwind(AssertUnwindSafe(|| d.app.load_character(id)));
    let snap = match opened {
        Err(_) => {
            return Seen {
                panicked: true,
                ..Seen::default()
            }
        }
        Ok(Err(_)) => return Seen::default(),
        Ok(Ok(s)) => s,
    };
    let host = Host {
        id: id.to_string(),
        snap,
        gen: 0,
    };
    let offered = catch_unwind(AssertUnwindSafe(|| offered_digest(d, &host)));
    let (snapshot, value) = snapshot_digest(&host.snap);
    let seen = Seen {
        opened: true,
        panicked: offered.is_err(),
        snapshot,
        offered: offered.unwrap_or_default(),
        chosen: chosen(&host.snap),
        listed_missing: missing_in_notices(&host.snap.notices),
        value,
    };
    let _ = d.app.close_character(id, true);
    seen
}

/// Open every member (never saving) and the L1 builder of each edition.
pub fn measure(app: &AppState, sample: &Sample) -> Measure {
    let d = Driver::new(app);
    let mut m = Measure::default();
    for member in &sample.members {
        m.chars.insert(member.id.clone(), see(&d, &member.id));
    }
    for rs in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let Some(h) = d.create(rs, TEST_SCORES) else {
            continue;
        };
        let digest = offered_digest(&d, &h);
        m.builder.insert(format!("{rs:?}"), digest);
        d.drop_host(h);
    }
    m
}
