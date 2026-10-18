//! Which builder hosts can offer an element (content-level, deterministic): an element is put on
//! a character by the elements that grant it or own a select it matches; climbing those up gives
//! the class, archetype, origin or feat a host has to take. Used to scope stage 2 to one
//! community repo stacked on A-core instead of re-running every host for every repo.

use borealis_content::{ContentStore, Rule};
use std::collections::{BTreeMap, BTreeSet};

/// Climb limit (grant / select steps from an element up to its host roots).
pub const MAX_CLIMB: usize = 8;
const ORIGINS: &[&str] = &["Race", "Species", "Background"];

/// The hosts one set of elements needs.
#[derive(Default, Debug)]
pub struct Plan {
    pub classes: BTreeSet<String>,
    /// archetype -> the classes that offer it
    pub archetypes: BTreeMap<String, BTreeSet<String>>,
    pub origins: BTreeSet<String>,
    pub feats: BTreeSet<String>,
    /// some element has no root: run every class the builder offers
    pub all_classes: bool,
    /// every element on a climb path (what a scoped host needs to explore)
    pub path: BTreeSet<String>,
}

/// Reverse index: element -> the elements that can put it on a character.
pub struct Offerers<'a> {
    store: &'a ContentStore,
    granted_by: BTreeMap<String, BTreeSet<String>>,
    /// (select type, support token or id) -> select owners; token "" = no supports (any)
    selected_by: BTreeMap<(String, String), BTreeSet<String>>,
    class_named: BTreeMap<String, String>,
}

/// Lower-case tokens of a supports expression (`a||b`, `a,b`, `!(c)`), as `select_index` reads them.
pub fn tokens(expr: &str) -> BTreeSet<String> {
    expr.split(['|', '&', '!', '(', ')', ','])
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

impl<'a> Offerers<'a> {
    pub fn new(store: &'a ContentStore) -> Self {
        let mut granted_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut selected_by: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        let mut class_named = BTreeMap::new();
        for se in store.iter() {
            let e = &se.element;
            if e.ty == "Class" {
                class_named
                    .entry(e.name.to_lowercase())
                    .or_insert_with(|| e.id.clone());
            }
            for r in &e.rules {
                match r {
                    Rule::Grant(g) => {
                        granted_by
                            .entry(g.id.clone())
                            .or_default()
                            .insert(e.id.clone());
                    }
                    Rule::Select(s) => {
                        let ty = s.ty.to_lowercase();
                        let toks = s.supports.as_deref().map(tokens).unwrap_or_default();
                        if toks.is_empty() {
                            selected_by
                                .entry((ty.clone(), String::new()))
                                .or_default()
                                .insert(e.id.clone());
                        }
                        for t in toks {
                            selected_by
                                .entry((ty.clone(), t))
                                .or_default()
                                .insert(e.id.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        Self {
            store,
            granted_by,
            selected_by,
            class_named,
        }
    }

    /// Elements one step up from `id`.
    fn parents(&self, id: &str) -> BTreeSet<String> {
        let mut out = self.granted_by.get(id).cloned().unwrap_or_default();
        let Some(e) = self.store.element(id) else {
            return out;
        };
        let ty = e.ty.to_lowercase();
        let mut keys: Vec<String> = vec![String::new(), id.to_lowercase()];
        keys.extend(e.supports.iter().map(|s| s.to_lowercase()));
        for k in keys {
            if let Some(o) = self.selected_by.get(&(ty.clone(), k.clone())) {
                out.extend(o.iter().cloned());
            }
            // a spell names the classes whose lists hold it
            if e.ty == "Spell" {
                if let Some(c) = self.class_named.get(&k) {
                    out.insert(c.clone());
                }
            }
        }
        out.remove(id);
        out
    }

    /// Adds the hosts that can offer `id` to `plan`; false when none was found.
    pub fn place(&self, id: &str, plan: &mut Plan) -> bool {
        let mut found = false;
        let mut seen = BTreeSet::from([id.to_string()]);
        let mut level: BTreeSet<String> = self.parents(id);
        let mut archetypes: BTreeSet<String> = BTreeSet::new();
        for _ in 0..MAX_CLIMB {
            let mut next = BTreeSet::new();
            for p in level {
                if !seen.insert(p.clone()) {
                    continue;
                }
                plan.path.insert(p.clone());
                let Some(e) = self.store.element(&p) else {
                    continue;
                };
                match e.ty.as_str() {
                    "Class" => {
                        found = true;
                        for a in &archetypes {
                            plan.archetypes
                                .entry(a.clone())
                                .or_default()
                                .insert(p.clone());
                        }
                        if archetypes.is_empty() {
                            plan.classes.insert(p);
                        }
                        continue;
                    }
                    "Feat" => {
                        found = true;
                        plan.feats.insert(p);
                        continue;
                    }
                    t if ORIGINS.contains(&t) => {
                        found = true;
                        plan.origins.insert(p);
                        continue;
                    }
                    "Archetype" => {
                        archetypes.insert(p.clone());
                    }
                    _ => {}
                }
                next.extend(self.parents(&p));
            }
            if next.is_empty() {
                break;
            }
            level = next;
        }
        found
    }
}
