//! The acceptance harness's builder driver: every host is a real character in an [`AppState`],
//! changed only through the commands the builder UI calls (`create_character`,
//! `apply_selection`, `level_up`, `set_base_score`), and "offered" means returned by
//! `get_slot_options` (or listed in the spellcasting block / `search_items`), never
//! `Universe::candidates_for`.

use borealis_content::Ruleset;
use borealis_lib::dto::{AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch};
use borealis_lib::dto_ext::{EquipRequest, ItemFilter};
use borealis_lib::state::{AppPaths, AppState};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

pub const TEST_SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];
pub const CASTER_SCORES: [i32; 6] = [12, 12, 14, 15, 15, 15];
const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
const PAGE: usize = 200;
const FILL_PASSES: usize = 60;
/// How deep [`Driver::explore`] follows selects opened by an explored option.
pub const EXPLORE_DEPTH: usize = 2;
/// Slot types [`Driver::explore`] never tries alternatives in (their own hosts cover them).
const NOT_EXPLORED: &[&str] = &["Class", "Multiclass", "Archetype"];
pub const MAX_LEVEL: u32 = 20;
/// Slot types the automatic fill leaves to the harness (the sweeps' list, both sources).
pub const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Race Variant",
    "Species",
    "Background",
    "Class",
    "Archetype",
    "Feat",
    "Multiclass",
];

pub fn app_state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// An app whose data dir is `data` (created when missing).
pub fn app_state_at(data: PathBuf) -> AppState {
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data,
    };
    AppState::new(paths).expect("state")
}

/// An app over a copy of `template` (an installed app's data dir): the same packs, as after a
/// restart, without installing them again.
pub fn app_state_copy(template: &std::path::Path) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("temp dir");
    super::files::copy_tree(template, &dir.path().join("data")).expect("copy app data");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().join("data"),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// Every id offered anywhere, with the first host that offered it.
#[derive(Default)]
pub struct Offers {
    pub found: BTreeMap<String, String>,
    /// The values each open list slot (a `select_param`) offered, by slot title.
    pub lists: BTreeMap<String, BTreeSet<BTreeSet<String>>>,
    /// Elements an item host picked: whether the pick applied, and why not.
    pub applied: BTreeMap<String, (bool, String)>,
}

impl Offers {
    pub fn add(&mut self, id: &str, host: &str) {
        self.found
            .entry(id.to_string())
            .or_insert_with(|| host.to_string());
    }

    /// Everything `other` found, kept where this one already has an entry.
    pub fn merge(&mut self, other: Offers) {
        for (id, host) in other.found {
            self.add(&id, &host);
        }
        for (title, sets) in other.lists {
            self.lists.entry(title).or_default().extend(sets);
        }
        for (id, a) in other.applied {
            self.applied.entry(id).or_insert(a);
        }
    }
}

/// Slot type of a `select_param` list (`ui_type("List")`).
const LIST_SLOT: &str = "list";

/// One character of the builder.
pub struct Host {
    pub id: String,
    pub snap: CharacterSnapshot,
    /// bumped on every change of `snap` (keys [`Driver::options`]' cache)
    pub gen: u64,
}

impl Host {
    pub fn set(&mut self, snap: CharacterSnapshot) {
        self.snap = snap;
        self.gen += 1;
    }
}

/// Options of one slot of one host, valid while the host's `gen` is unchanged.
type OptionCache =
    std::cell::RefCell<std::collections::HashMap<(String, String), (u64, Vec<(String, String)>)>>;

pub struct Driver<'a> {
    pub app: &'a AppState,
    pub made: std::cell::Cell<usize>,
    cache: OptionCache,
}

impl<'a> Driver<'a> {
    pub fn new(app: &'a AppState) -> Self {
        Self {
            app,
            made: std::cell::Cell::new(0),
            cache: OptionCache::default(),
        }
    }

    /// A level-1 character with feats and multiclassing on and the given base scores.
    pub fn create(&self, rs: Ruleset, scores: [i32; 6]) -> Option<Host> {
        let request = NewCharacterRequest {
            name: "Acceptance host".into(),
            ruleset: rs,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch {
                feats: Some(true),
                multiclassing: Some(true),
                ..OptionsPatch::default()
            },
            portrait: None,
            group: None,
        };
        let mut snap = self.app.create_character(rs, &request).ok()?;
        self.made.set(self.made.get() + 1);
        for (key, v) in ABILITIES.iter().zip(scores) {
            if let Ok(s) = self.app.set_base_score(&snap.id, key, v) {
                snap = s;
            }
        }
        Some(Host {
            id: snap.id.clone(),
            snap,
            gen: 0,
        })
    }

    /// Close and delete a host (keeps the data dir small).
    pub fn drop_host(&self, host: Host) {
        self.cache.borrow_mut().retain(|k, _| k.0 != host.id);
        let _ = self.app.close_character(&host.id, true);
        let _ = self.app.delete_character(&host.id);
    }

    /// `(id, name)` of every option of a slot, all pages.
    /// The answer is cached per host state (`gen`): the builder asks the same thing again only
    /// after a change.
    pub fn options(&self, host: &Host, slot_id: &str) -> Vec<(String, String)> {
        let key = (host.id.clone(), slot_id.to_string());
        if let Some((g, v)) = self.cache.borrow().get(&key) {
            if *g == host.gen {
                return v.clone();
            }
        }
        let out = self.fetch_options(host, slot_id);
        self.cache.borrow_mut().insert(key, (host.gen, out.clone()));
        out
    }

    fn fetch_options(&self, host: &Host, slot_id: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        loop {
            let Ok(page) =
                self.app
                    .get_slot_options(&host.id, slot_id, None, Some(PAGE), Some(out.len()))
            else {
                return out;
            };
            let n = page.items.len();
            out.extend(
                page.items
                    .into_iter()
                    .map(|o| (o.summary.id, o.summary.name)),
            );
            if n == 0 || out.len() >= page.total {
                return out;
            }
        }
    }

    /// Record everything the host offers now: open slots' options, chosen ids, spell lists.
    pub fn collect(&self, host: &Host, label: &str, offers: &mut Offers) {
        for slot in &host.snap.slots {
            if let Some(c) = &slot.chosen {
                offers.add(&c.id, label);
            }
            if slot.chosen.is_none() {
                let options = self.options(host, &slot.id);
                if slot.element_type.eq_ignore_ascii_case(LIST_SLOT) {
                    let values = options.iter().map(|o| o.1.clone()).collect();
                    offers
                        .lists
                        .entry(slot.title.clone())
                        .or_default()
                        .insert(values);
                }
                for (id, _) in options {
                    offers.add(&id, label);
                }
            }
        }
        for class in &host.snap.spellcasting.classes {
            for s in &class.spells {
                offers.add(&s.id, label);
            }
        }
    }

    fn apply(&self, host: &mut Host, slot_id: &str, choice: &str) -> bool {
        match self.app.apply_selection(&host.id, slot_id, choice) {
            Ok(s) => {
                host.set(s);
                true
            }
            Err(_) => false,
        }
    }

    /// Choose `target` in the first open slot that offers it.
    pub fn pick(&self, host: &mut Host, target: &str) -> bool {
        let open: Vec<String> = host
            .snap
            .slots
            .iter()
            .filter(|s| s.chosen.is_none())
            .map(|s| s.id.clone())
            .collect();
        for slot in open {
            if self.options(host, &slot).iter().any(|(id, _)| id == target) {
                return self.apply(host, &slot, target);
            }
        }
        false
    }

    /// The first option named `name` of the first open slot of `ty` (prefers ids with `prefix`).
    pub fn find_named(&self, host: &Host, ty: &str, name: &str, prefix: &str) -> Option<String> {
        let slot = host
            .snap
            .slots
            .iter()
            .find(|s| s.chosen.is_none() && s.element_type.eq_ignore_ascii_case(ty))?;
        let mut hits: Vec<String> = self
            .options(host, &slot.id)
            .into_iter()
            .filter(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
            .collect();
        hits.sort_by_key(|id| (!id.starts_with(prefix), id.len(), id.clone()));
        hits.into_iter().next()
    }

    /// Fill open slots with their first option, as a user would (types in [`NOT_FILLED`] left).
    /// Every option of every slot it fills is recorded as offered under `label` first, so the
    /// selects that open (and close) along the way are measured too.
    pub fn fill(&self, host: &mut Host, label: &str, offers: &mut Offers) {
        let mut refused = HashSet::new();
        for _ in 0..FILL_PASSES {
            let open = host.snap.slots.iter().find(|s| {
                s.chosen.is_none()
                    && !NOT_FILLED
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case(&s.element_type))
                    && !refused.contains(&s.id)
            });
            let Some(slot) = open.map(|s| s.id.clone()) else {
                return;
            };
            let opts = self.options(host, &slot);
            for (id, _) in &opts {
                offers.add(id, label);
            }
            let first = opts.into_iter().next();
            let ok = first.is_some_and(|(id, _)| self.apply(host, &slot, &id));
            if !ok {
                refused.insert(slot);
            }
        }
    }

    /// Try every option of every open slot that owns a select (`owners`, see
    /// [`select_owners`]): apply it, record what the slots it opens offer, recurse
    /// [`EXPLORE_DEPTH`] deep, then clear it again. `memo` holds the options already explored
    /// (once per source and edition: what an option opens does not depend on the host). Order
    /// follows the snapshot's slots and options, so a run is deterministic.
    pub fn explore(
        &self,
        host: &mut Host,
        ctx: &mut Explore<'_>,
        label: &str,
        offers: &mut Offers,
    ) {
        let slots: Vec<String> = host
            .snap
            .slots
            .iter()
            .filter(|s| {
                s.chosen.is_none()
                    && !NOT_EXPLORED
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case(&s.element_type))
            })
            .map(|s| s.id.clone())
            .collect();
        self.explore_slots(host, ctx, &slots, EXPLORE_DEPTH, label, offers);
    }

    fn explore_slots(
        &self,
        host: &mut Host,
        ctx: &mut Explore<'_>,
        slots: &[String],
        depth: usize,
        label: &str,
        offers: &mut Offers,
    ) {
        if depth == 0 {
            return;
        }
        for slot in slots {
            for (opt, _) in self.options(host, slot) {
                offers.add(&opt, label);
                if !ctx.owners.contains(&opt) || !ctx.memo.insert(opt.clone()) {
                    continue;
                }
                let before: BTreeSet<String> =
                    host.snap.slots.iter().map(|s| s.id.clone()).collect();
                if !self.apply(host, slot, &opt) {
                    continue;
                }
                ctx.explored += 1;
                let opened: Vec<String> = host
                    .snap
                    .slots
                    .iter()
                    .filter(|s| !before.contains(&s.id))
                    .map(|s| s.id.clone())
                    .collect();
                for s in host.snap.slots.iter().filter(|s| opened.contains(&s.id)) {
                    if let Some(c) = &s.chosen {
                        offers.add(&c.id, label);
                    }
                }
                self.explore_slots(host, ctx, &opened, depth - 1, label, offers);
                for s in &opened {
                    for (id, _) in self.options(host, s) {
                        offers.add(&id, label);
                    }
                }
                match self.app.clear_selection(&host.id, slot) {
                    Ok(s) => host.set(s),
                    Err(_) => return,
                }
            }
        }
    }

    pub fn level_up(&self, host: &mut Host) -> bool {
        match self.app.level_up(&host.id, "main") {
            Ok(s) => {
                host.set(s);
                true
            }
            Err(_) => false,
        }
    }

    /// Take the "Feat" option of every open ability score improvement select.
    pub fn open_feat_selects(&self, host: &mut Host) {
        for _ in 0..4 {
            let found = host
                .snap
                .slots
                .iter()
                .filter(|s| s.chosen.is_none())
                .find_map(|s| {
                    self.options(host, &s.id)
                        .into_iter()
                        .find(|(id, name)| {
                            super::prereq::is_feat_option_5et(id, name)
                                || super::prereq::is_feat_option_aurora(name)
                        })
                        .map(|(id, _)| (s.id.clone(), id))
                });
            let Some((slot, id)) = found else { return };
            if !self.apply(host, &slot, &id) {
                return;
            }
        }
    }

    /// Turn on character options (`Option` elements a requirement names), as the options
    /// dialog does. An option the builder refuses is left off.
    pub fn enable_options(&self, host: &mut Host, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let patch = OptionsPatch {
            custom: Some(ids.iter().map(|id| (id.clone(), true)).collect()),
            ..OptionsPatch::default()
        };
        if let Ok(s) = self.app.set_character_options(&host.id, &patch) {
            host.set(s);
        }
    }

    /// Add `item` to the inventory, equip it and attune to it when it needs attunement, as a
    /// user would. True when the item is held (attuned if it needs it).
    pub fn hold_item(&self, host: &mut Host, item: &str) -> bool {
        let Ok(s) = self.app.add_item(&host.id, item, 1, false, None) else {
            return false;
        };
        host.set(s);
        let line = |h: &Host| {
            h.snap
                .inventory
                .items
                .iter()
                .find(|i| i.item_id == item)
                .cloned()
        };
        let Some(added) = line(host) else {
            return false;
        };
        if let Ok(s) = self.app.equip_item(&host.id, &added.id, EquipRequest::Auto) {
            host.set(s);
        }
        let Some(now) = line(host) else {
            return false;
        };
        if now.requires_attunement && !now.attuned {
            if let Ok(s) = self.app.set_attuned(&host.id, &now.id, true) {
                host.set(s);
            }
        }
        // an item that cannot be equipped still registers while carried
        line(host).is_some_and(|i| i.attuned || !i.requires_attunement)
    }

    /// Every item `search_items` lists for a character of this host's edition.
    pub fn items(&self, host: &Host, offers: &mut Offers, label: &str) {
        let mut offset = 0;
        loop {
            let filter = ItemFilter {
                category: None,
                query: None,
                source: None,
                limit: Some(PAGE),
                offset: Some(offset),
                character_id: Some(host.id.clone()),
            };
            let Ok(page) = self.app.search_items(&filter) else {
                return;
            };
            let n = page.items.len();
            for i in page.items {
                offers.add(&i.summary.id, label);
            }
            offset += n;
            if n == 0 || offset >= page.total {
                return;
            }
        }
    }
}

/// State of [`Driver::explore`] for one source and edition.
pub struct Explore<'a> {
    pub owners: &'a BTreeSet<String>,
    pub memo: BTreeSet<String>,
    pub explored: usize,
}

/// Ids of elements that open a select on a character: a select rule of their own or in an
/// element they grant (followed [`OWNER_DEPTH`] deep).
pub fn select_owners(store: &borealis_content::ContentStore) -> BTreeSet<String> {
    use borealis_content::Rule;
    let mut out = BTreeSet::new();
    let mut grants: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for se in store.iter() {
        let e = &se.element;
        if e.rules.iter().any(|r| matches!(r, Rule::Select(_))) {
            out.insert(e.id.clone());
        }
        for r in &e.rules {
            if let Rule::Grant(g) = r {
                grants.entry(e.id.as_str()).or_default().push(g.id.as_str());
            }
        }
    }
    for _ in 0..OWNER_DEPTH {
        let more: Vec<String> = grants
            .iter()
            .filter(|(id, kids)| !out.contains(**id) && kids.iter().any(|k| out.contains(*k)))
            .map(|(id, _)| (*id).to_string())
            .collect();
        if more.is_empty() {
            break;
        }
        out.extend(more);
    }
    out
}

/// How many grant steps [`select_owners`] follows.
pub const OWNER_DEPTH: usize = 4;
