//! The climb pass of an option host ([`super::option_host`]): when the needed ids alone offer
//! nothing, the host also takes what a user would take on the way down to the option: the
//! elements on its climb paths, the items that carry them, the character options they need, the
//! subclass whose own spell list names a spell, a new multiclass level for a multiclass entry
//! (or for a class only whose grants reach a needed id), the elements that raise a counter a
//! requirement tests. Its configurations are ranked by what they reach ([`rank_configs`]).
//! Everything is read from the installed content; no id is named here.

use super::items;
use super::{Edition, NON_OFFERABLE};
use crate::support::plan::{supports_match, tokens, Plan};
use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr, TestValue};
use borealis_content::{ContentStore, Rule};
use std::collections::BTreeSet;

/// Items one climb pass carries at most (a class-gated item is carried by every class's host;
/// the ones of other classes do nothing).
const MAX_CLIMB_ITEMS: usize = 24;

/// Element type of a class's multiclass entry (offered by a new multiclass level's select).
pub(super) const MULTICLASS_TYPE: &str = "Multiclass";

/// Configurations a climb pass tries at most.
const MAX_CLIMB_CONFIGS: usize = 6;

/// Items an option host's climb pass carries: items on the climb paths, needed ids that are items,
/// and items that grant the option or a needed id outright, in id order, at most [`MAX_CLIMB_ITEMS`].
pub(super) fn climb_items(
    store: &ContentStore,
    target: &str,
    roots: &Plan,
    groups: &[Vec<String>],
) -> Vec<String> {
    let needed: BTreeSet<&str> = groups
        .iter()
        .flatten()
        .map(String::as_str)
        .chain(std::iter::once(target))
        .collect();
    let is_item = |p: &&str| store.element(p).is_some_and(|x| items::is_item_type(&x.ty));
    // a needed id that is itself an item (a requirement naming a weapon, the armor that sets a
    // counter a requirement tests) is carried first
    let first: Vec<String> = needed
        .iter()
        .copied()
        .filter(|n| *n != target)
        .filter(is_item)
        .map(|n| n.to_string())
        .collect();
    let mut out: BTreeSet<String> = roots
        .path
        .iter()
        .map(String::as_str)
        .filter(is_item)
        .map(str::to_string)
        .collect();
    for se in store.iter() {
        let el = &se.element;
        let grants = items::is_item_type(&el.ty)
            && el.rules.iter().any(|r| {
                matches!(r, Rule::Grant(g) if needed.contains(g.id.as_str())
                    && g.requirements.as_deref().unwrap_or("").trim().is_empty())
            });
        if grants {
            out.insert(el.id.clone());
        }
    }
    let rest = out.into_iter().filter(|x| !first.contains(x));
    first
        .iter()
        .cloned()
        .chain(rest)
        .take(MAX_CLIMB_ITEMS)
        .collect()
}

/// An element on a climb path that an option host picks where a slot offers it: not the class,
/// level and origin roots (taken first) and not an element type no select offers.
pub(super) fn on_climb(store: &ContentStore, id: &str) -> bool {
    store.element(id).is_some_and(|x| {
        !matches!(
            x.ty.as_str(),
            "Class" | "Level" | "Race" | "Species" | "Background" | "Sub Race" | "Race Variant"
        ) && !NON_OFFERABLE.iter().any(|(t, _)| *t == x.ty)
    })
}

/// Setters one counter group holds at most.
const MAX_SETTERS: usize = 12;

/// Any-of groups of the elements that set a content counter the option's (or a needed id's)
/// requirements test (`[smith:armor:1]`, raised by a stat rule of an earlier upgrade): the
/// climb pass picks one where it is offered, as a user does before the option shows. Only
/// `:`-keys no builder term reads (not `level...`, `spellcasting...`, a class level or an
/// ability) and no element spends (a points budget); groups already present are left out.
pub(super) fn stat_groups(
    store: &ContentStore,
    target: &str,
    groups: &[Vec<String>],
) -> Vec<Vec<String>> {
    fn tests(e: &BoolExpr<RequirementLeaf>, out: &mut BTreeSet<String>) {
        match e {
            BoolExpr::Leaf(RequirementLeaf::Test(t)) => {
                let key = t.normalized_key();
                let builtin = key.starts_with("level") || key.starts_with("spellcasting");
                if key.contains(':') && !builtin && matches!(t.value, TestValue::Number(n) if n > 0)
                {
                    out.insert(key);
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Id(_)) | BoolExpr::Not(_) => {}
            BoolExpr::And(items) | BoolExpr::Or(items) => {
                items.iter().for_each(|x| tests(x, out));
            }
        }
    }
    let mut keys = BTreeSet::new();
    for id in groups.iter().flatten().map(String::as_str).chain([target]) {
        let Some(el) = store.element(id) else {
            continue;
        };
        if let Ok(Some(expr)) = RequirementsExpr::parse(&el.requirements) {
            tests(expr.root(), &mut keys);
        }
    }
    // a budget some element spends (`modular:points -1`) is not a counter earlier picks raise:
    // picking its setters changes what the budget allows, so it is left to the other picks
    let spent = |key: &str| {
        store.iter().any(|se| {
            se.element.rules.iter().any(|r| {
                matches!(r, Rule::Stat(s) if s.name.eq_ignore_ascii_case(key)
                    && s.value.trim().parse::<i64>().is_ok_and(|n| n < 0))
            })
        })
    };
    let mut out: Vec<Vec<String>> = Vec::new();
    for key in keys.into_iter().filter(|k| !spent(k)) {
        let setters: Vec<String> = store
            .iter()
            .filter(|se| se.element.id != target)
            .filter(|se| {
                se.element.rules.iter().any(|r| {
                    matches!(r, Rule::Stat(s) if s.name.eq_ignore_ascii_case(&key)
                        && s.value.trim().parse::<i64>().is_ok_and(|n| n > 0))
                })
            })
            .map(|se| se.element.id.clone())
            .take(MAX_SETTERS)
            .collect();
        if !setters.is_empty() && !groups.contains(&setters) && !out.contains(&setters) {
            out.push(setters);
        }
    }
    out
}

/// The multiclass entries (`Multiclass` elements named as the class) of the classes a needed
/// group reaches when it reaches none of `class` (by a grant or select of that class or one of
/// its archetypes): the climb pass takes that class as a multiclass level.
pub(super) fn multiclass_entries(
    e: &Edition<'_>,
    class: &str,
    groups: &[Vec<String>],
) -> Vec<String> {
    let store = &e.ins.store;
    let mut names: BTreeSet<String> = BTreeSet::new();
    // the class's own multiclass node (the store keeps it on the class; the engine makes the
    // Multiclass element from it)
    let mut out: Vec<String> = Vec::new();
    for g in groups {
        let mut p = Plan::default();
        for id in g {
            e.offerers.place_precise(id, &mut p);
        }
        let reached: BTreeSet<&String> = p
            .classes
            .iter()
            .chain(p.archetypes.values().flatten())
            .collect();
        if reached.is_empty() || reached.iter().any(|c| c.as_str() == class) {
            continue;
        }
        for c in reached
            .into_iter()
            .filter(|c| e.builder_classes.contains(c))
        {
            if let Some(x) = store.element(c) {
                names.insert(x.name.to_lowercase());
                out.extend(x.multiclass.as_ref().map(|m| m.id.clone()));
            }
        }
    }
    if names.is_empty() {
        return Vec::new();
    }
    out.extend(
        store
            .iter()
            .filter(|se| se.element.ty == MULTICLASS_TYPE)
            .filter(|se| names.contains(&se.element.name.to_lowercase()))
            .map(|se| se.element.id.clone()),
    );
    out.sort();
    out.dedup();
    out
}

/// The classes (builder class ids, by name) whose level gates a select that offers `target` by
/// its whole supports expression (`[level:<class>:N]` in the select's requirements).
fn select_level_classes(e: &Edition<'_>, target: &str) -> BTreeSet<String> {
    fn level_keys(x: &BoolExpr<RequirementLeaf>, out: &mut BTreeSet<String>) {
        match x {
            BoolExpr::Leaf(RequirementLeaf::Test(t)) => {
                if let Some(c) = t.normalized_key().strip_prefix("level:") {
                    out.insert(c.to_string());
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Id(_)) | BoolExpr::Not(_) => {}
            BoolExpr::And(items) | BoolExpr::Or(items) => {
                items.iter().for_each(|i| level_keys(i, out));
            }
        }
    }
    let store = &e.ins.store;
    let Some(el) = store.element(target) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    for se in store.iter() {
        for r in &se.element.rules {
            let Rule::Select(s) = r else { continue };
            if !s.ty.eq_ignore_ascii_case(&el.ty)
                || !supports_match(s.supports.as_deref().unwrap_or(""), el)
            {
                continue;
            }
            let req = s.requirements.as_deref().unwrap_or("");
            if let Ok(Some(expr)) = RequirementsExpr::parse(req) {
                level_keys(expr.root(), &mut names);
            }
        }
    }
    e.builder_classes
        .iter()
        .filter(|c| {
            store
                .element(c)
                .is_some_and(|x| names.contains(&x.name.to_lowercase()))
        })
        .cloned()
        .collect()
}

/// One host configuration: a class and the archetype taken on it.
type Config = (String, Option<String>);

/// The climb pass's configurations, ranked by what they reach: each configuration scores 2 when
/// the option's own select (matched by its whole supports expression) climbs to it and 1 per
/// needed group whose ids climb to it (doubled when they reach its archetype; a class root counts
/// for every archetype of the class). The roots of the option and of the needed ids join
/// `candidates`; ties keep the older order (needs class, own select owners, roots, fallbacks).
pub(super) fn rank_configs(
    e: &Edition<'_>,
    target: &str,
    groups: &[Vec<String>],
    candidates: Vec<Config>,
) -> Vec<Config> {
    let usable = |c: &String| e.builder_classes.contains(c);
    let mut plans: Vec<(u32, Plan)> = Vec::new();
    let mut own = Plan::default();
    e.offerers.place_precise(target, &mut own);
    // a select gated by a class level (`[level:barbarian:2]`) belongs to that class even when
    // its owner reaches no class (a feature an item grants)
    own.classes.extend(select_level_classes(e, target));
    plans.push((2, own));
    for g in groups {
        let mut p = Plan::default();
        for id in g {
            e.offerers.place_precise(id, &mut p);
        }
        plans.push((1, p));
    }
    let mut configs: Vec<Config> = Vec::new();
    let mut add = |c: Config| {
        if !configs.contains(&c) {
            configs.push(c);
        }
    };
    candidates.into_iter().for_each(&mut add);
    for (_, p) in &plans {
        for (a, cs) in &p.archetypes {
            for c in cs.iter().filter(|c| usable(c)) {
                add((c.clone(), Some(a.clone())));
            }
        }
        for c in p.classes.iter().filter(|c| usable(c)) {
            add((c.clone(), None));
        }
    }
    // an archetype root is the more specific reach (its own selects), so it counts double
    let reach = |(c, a): &Config, p: &Plan| -> u32 {
        if a.as_ref()
            .is_some_and(|a| p.archetypes.get(a).is_some_and(|cs| cs.contains(c)))
        {
            2
        } else {
            u32::from(p.classes.contains(c))
        }
    };
    let score = |cfg: &Config| -> u32 { plans.iter().map(|(w, p)| w * reach(cfg, p)).sum() };
    let mut scored: Vec<(u32, Config)> = configs.into_iter().map(|c| (score(&c), c)).collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// The climb pass's configurations and roots for a spell: the elements whose own spell list
/// (or list extension) names the spell or one of its supports come first (a subclass with its
/// own list of fusion-element spells; the class is then the subclass's class), then the owners
/// of a slots-only spell select ([`slots_select_hosts`]), then `classes`. Other targets keep
/// `classes` and `roots`.
pub(super) fn spell_list_hosts(
    e: &Edition<'_>,
    target: &str,
    classes: Vec<(String, Option<String>)>,
    mut roots: Plan,
) -> (Vec<(String, Option<String>)>, Plan) {
    let store = &e.ins.store;
    let Some(spell) = store.element(target).filter(|x| x.ty == "Spell") else {
        return (classes, roots);
    };
    let keys: BTreeSet<String> = spell
        .supports
        .iter()
        .map(|s| s.to_lowercase())
        .chain(std::iter::once(target.to_lowercase()))
        .collect();
    let mut first: Vec<(String, Option<String>)> = Vec::new();
    let usable = |c: &String| e.builder_classes.contains(c);
    for se in store.iter() {
        let owner = &se.element;
        let Some(sc) = &owner.spellcasting else {
            continue;
        };
        let names = sc
            .list
            .iter()
            .chain(&sc.extends)
            .any(|l| tokens(&l.supports).iter().any(|t| keys.contains(t)));
        if !names {
            continue;
        }
        match owner.ty.as_str() {
            "Class" if usable(&owner.id) => first.push((owner.id.clone(), None)),
            _ => {
                let mut q = Plan::default();
                e.offerers.place(&owner.id, &mut q);
                let archetype = (owner.ty == "Archetype").then(|| owner.id.clone());
                for c in q.classes.iter().filter(|c| usable(c)) {
                    first.push((c.clone(), archetype.clone()));
                }
                for (a, cs) in &q.archetypes {
                    for c in cs.iter().filter(|c| usable(c)) {
                        first.push((c.clone(), Some(a.clone())));
                    }
                }
                roots.path.insert(owner.id.clone());
                roots.path.extend(q.path);
                roots.origins.extend(q.origins);
            }
        }
    }
    // then the owners of a spell select that offers any spell of an allowed level
    // (`$(spellcasting:slots)` with no class list term: Magical Secrets, engine.md 10.3), whose
    // dynamic supports the reverse index cannot link to any one spell
    let slots = slots_select_hosts(e, spell, &mut roots);
    let room = if slots.is_empty() {
        MAX_CLIMB_CONFIGS
    } else {
        MAX_CLIMB_CONFIGS - MAX_SLOTS_CONFIGS
    };
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for c in first.into_iter().take(room) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    for c in slots.into_iter().take(MAX_SLOTS_CONFIGS).chain(classes) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out.truncate(MAX_CLIMB_CONFIGS);
    (out, roots)
}

/// Configurations a spell's climb pass keeps for owners of a slots-only spell select.
const MAX_SLOTS_CONFIGS: usize = 2;

/// The configurations (class, archetype) of the owners of a spell select whose supports offer
/// `spell` through `$(spellcasting:slots)` without a `$(spellcasting:list)` term; each owner
/// joins the climb paths of `roots`. Owners in store order.
fn slots_select_hosts(
    e: &Edition<'_>,
    spell: &borealis_content::Element,
    roots: &mut Plan,
) -> Vec<(String, Option<String>)> {
    let store = &e.ins.store;
    let usable = |c: &String| e.builder_classes.contains(c);
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for se in store.iter() {
        let owner = &se.element;
        let offers = owner.rules.iter().any(|r| {
            let Rule::Select(s) = r else { return false };
            let raw = s.supports.as_deref().unwrap_or("").to_lowercase();
            s.ty.eq_ignore_ascii_case("Spell")
                && raw.contains("$(spellcasting:slots)")
                && !raw.contains("$(spellcasting:list)")
                && supports_match(&raw, spell)
        });
        if !offers {
            continue;
        }
        let mut q = Plan::default();
        e.offerers.place(&owner.id, &mut q);
        if owner.ty == "Class" && usable(&owner.id) {
            out.push((owner.id.clone(), None));
        }
        for c in q.classes.iter().filter(|c| usable(c)) {
            out.push((c.clone(), None));
        }
        for (a, cs) in &q.archetypes {
            for c in cs.iter().filter(|c| usable(c)) {
                out.push((c.clone(), Some(a.clone())));
            }
        }
        roots.path.insert(owner.id.clone());
        roots.path.extend(q.path);
    }
    let mut seen = Vec::new();
    out.retain(|c| {
        let new = !seen.contains(c);
        seen.push(c.clone());
        new
    });
    out
}
