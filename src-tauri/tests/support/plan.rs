//! Which builder hosts can offer an element (content-level, deterministic): an element is put on
//! a character by the elements that grant it or own a select it matches; climbing those up gives
//! the class, archetype, origin or feat a host has to take. Used to scope stage 2 to one
//! community repo stacked on A-core instead of re-running every host for every repo.

use borealis_content::expr::{BoolExpr, SupportsExpr, SupportsLeaf};
use borealis_content::{ContentStore, Element, Rule};
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
    /// select owner -> its selects as (lower-case type, raw supports), for [`Self::place_precise`]
    selects_of: BTreeMap<String, Vec<(String, String)>>,
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
        let mut selects_of: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
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
                        selects_of
                            .entry(e.id.clone())
                            .or_default()
                            .push((ty.clone(), s.supports.clone().unwrap_or_default()));
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
            selects_of,
            class_named,
        }
    }

    /// Elements one step up from `id`. `precise`: a select owner counts only when one of its
    /// selects of the element's type matches the element by the whole supports expression (a
    /// select of "Upgrade, Smith A" does not offer an upgrade of "Smith B"), as the builder
    /// matches it; otherwise any one supports token links them.
    fn parents_of(&self, id: &str, precise: bool) -> BTreeSet<String> {
        let mut out = self.granted_by.get(id).cloned().unwrap_or_default();
        let Some(e) = self.store.element(id) else {
            return out;
        };
        let ty = e.ty.to_lowercase();
        let mut keys: Vec<String> = vec![String::new(), id.to_lowercase()];
        keys.extend(e.supports.iter().map(|s| s.to_lowercase()));
        for k in keys {
            if let Some(o) = self.selected_by.get(&(ty.clone(), k.clone())) {
                out.extend(
                    o.iter()
                        .filter(|owner| !precise || self.owner_matches(owner, &ty, e))
                        .cloned(),
                );
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

    /// True when a select of `owner` of type `ty` offers `el` by its whole supports expression.
    fn owner_matches(&self, owner: &str, ty: &str, el: &Element) -> bool {
        self.selects_of
            .get(owner)
            .is_some_and(|v| v.iter().any(|(t, raw)| t == ty && supports_match(raw, el)))
    }

    /// Adds the hosts that can offer `id` to `plan`; false when none was found.
    pub fn place(&self, id: &str, plan: &mut Plan) -> bool {
        self.place_with(id, plan, false)
    }

    /// [`Self::place`] climbing only through selects that offer each element by their whole
    /// supports expression (see [`Self::parents_of`]).
    pub fn place_precise(&self, id: &str, plan: &mut Plan) -> bool {
        self.place_with(id, plan, true)
    }

    /// Climbs from `id` breadth-first. Each branch carries the nearest archetype it passed (an
    /// archetype maps only to the classes above it on its own branch, not to every class the
    /// climb reaches after any archetype); a node is visited once per carried archetype.
    fn place_with(&self, id: &str, plan: &mut Plan, precise: bool) -> bool {
        type Step = (String, Option<String>);
        let mut found = false;
        let mut seen: BTreeSet<Step> = BTreeSet::from([(id.to_string(), None)]);
        let mut level: BTreeSet<Step> = self
            .parents_of(id, precise)
            .into_iter()
            .map(|p| (p, None))
            .collect();
        for _ in 0..MAX_CLIMB {
            let mut next = BTreeSet::new();
            for (p, carried) in level {
                if !seen.insert((p.clone(), carried.clone())) {
                    continue;
                }
                plan.path.insert(p.clone());
                let Some(e) = self.store.element(&p) else {
                    continue;
                };
                let carry = match e.ty.as_str() {
                    "Class" => {
                        found = true;
                        match carried {
                            Some(a) => {
                                plan.archetypes.entry(a).or_default().insert(p);
                            }
                            None => {
                                plan.classes.insert(p);
                            }
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
                    "Archetype" => Some(p.clone()),
                    _ => carried,
                };
                next.extend(
                    self.parents_of(&p, precise)
                        .into_iter()
                        .map(|q| (q, carry.clone())),
                );
            }
            if next.is_empty() {
                break;
            }
            level = next;
        }
        found
    }
}

/// True when a select's raw supports expression offers `el`: a tag matches the element's id or
/// one of its supports (case-insensitive), a dynamic `$(...)` part may match anything, and blank
/// supports match every element of the type. An unparsable expression matches (generous).
pub fn supports_match(raw: &str, el: &Element) -> bool {
    fn eval(e: &BoolExpr<SupportsLeaf>, el: &Element) -> bool {
        match e {
            BoolExpr::Leaf(SupportsLeaf::Tag(t)) => {
                t.eq_ignore_ascii_case(&el.id)
                    || el.supports.iter().any(|s| t.eq_ignore_ascii_case(s))
            }
            BoolExpr::Leaf(SupportsLeaf::Dynamic(_)) => true,
            BoolExpr::Not(inner) => !eval(inner, el),
            BoolExpr::And(items) => items.iter().all(|x| eval(x, el)),
            BoolExpr::Or(items) => items.iter().any(|x| eval(x, el)),
        }
    }
    match SupportsExpr::parse(raw) {
        Ok(Some(expr)) => eval(expr.root(), el),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{Offerers, Plan};

    #[test]
    fn an_archetype_maps_only_to_the_classes_on_its_own_branch() {
        // a feature granted by an archetype of class A and directly by class B
        let elements = serde_json::json!([
            {"id": "ID_T_CLASS_A", "type": "Class", "name": "A", "rules": [
                {"kind": "select", "type": "Archetype", "name": "Path", "supports": "A Path"}]},
            {"id": "ID_T_CLASS_B", "type": "Class", "name": "B", "rules": [
                {"kind": "grant", "type": "Class Feature", "id": "ID_T_FEATURE"}]},
            {"id": "ID_T_ARCH_A", "type": "Archetype", "name": "AA", "supports": ["A Path"],
             "rules": [{"kind": "grant", "type": "Archetype Feature", "id": "ID_T_FEATURE"}]},
            {"id": "ID_T_FEATURE", "type": "Archetype Feature", "name": "F"}
        ]);
        let elements: Vec<serde_json::Value> = elements
            .as_array()
            .expect("list")
            .iter()
            .map(|e| {
                let mut e = e.clone();
                e["source"] = "T".into();
                e
            })
            .collect();
        let file = serde_json::json!({ "format": "borealis-content/1", "elements": elements });
        let file = borealis_content::ContentFile::from_json_str(&file.to_string()).expect("file");
        let store = borealis_content::ContentStoreBuilder::new()
            .add_file("t.json", borealis_content::PackOrigin::Custom, file)
            .build();
        let mut plan = Plan::default();
        assert!(Offerers::new(&store).place("ID_T_FEATURE", &mut plan));
        assert_eq!(plan.classes.iter().collect::<Vec<_>>(), ["ID_T_CLASS_B"]);
        let a = plan.archetypes.get("ID_T_ARCH_A").expect("archetype");
        assert_eq!(a.iter().collect::<Vec<_>>(), ["ID_T_CLASS_A"]);
    }
}
