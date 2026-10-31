//! Content defects of stage 2: an element no host offers because the installed content can never
//! put it on a character, so Aurora (which reads the same content) never offers it either. Such
//! a row asserts that verdict instead of an offer. The causes, all read from the installed
//! content (no id is named here):
//!
//! * **unreachable** - every element that could put it on a character (an owner of a select of
//!   its type whose supports name it or one of its supports, or an element granting it) is
//!   itself unreachable: nothing grants it, no select offers it, and it is no host root. A
//!   subclass that grants a misspelled feature id leaves the real feature's select orphaned.
//! * **no parent** - nothing grants it and no select of its type names it or its supports (a
//!   subclass whose class is not installed in this setup).
//! * **requirement** - its requirements can never hold: every alternative needs an id that this
//!   setup does not define (Aurora compares ids ordinally; a missing id is never held). Such an
//!   element is also unreachable for the elements below it (a feat whose select is the only
//!   offerer, when the feat requires another class's feature that is not installed).
//! * **inactive rule** - every grant or select that names it has requirements that never hold
//!   (they need an element no character can hold here: its only granter is not installed, or a
//!   later repo replaced that granter with a copy that no longer grants it); when only its
//!   selects are inactive it is still granted, but never offered.
//! * **known list only** (spells) - listed only by `<list known="true">` of casters that do not
//!   prepare, with no grant and no select that can offer it (engine.md 10.2): by design.
//!
//! Both checks are generous to the content (any supports token links a select, any grant counts,
//! every host root type is reachable), so a row is called a defect only when no reading of the
//! content can reach it.

use crate::support::plan::{supports_match, tokens};
use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr};
use borealis_content::{ContentStore, Element, Rule, Ruleset, SpellcastingList};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Element types a host takes directly (class, origins, feats, spells, gear, options), so they
/// are reachable without a parent.
const ROOT_TYPES: &[&str] = &[
    "Class",
    "Race",
    "Species",
    "Background",
    "Feat",
    "Spell",
    "Item",
    "Weapon",
    "Armor",
    "Magic Item",
    "Option",
];

/// Types whose selects the engine also generates (improvement and feat options, multiclass
/// entries, proxy items): a missing content parent proves nothing for them, so they are roots
/// too.
const GENERATED_SELECT_TYPES: &[&str] = &[
    "Ability Score Improvement",
    "Feat",
    "Multiclass",
    "Language",
    "Proficiency",
];

/// Reachable without a parent ([`ROOT_TYPES`] and [`GENERATED_SELECT_TYPES`]).
fn is_root(ty: &str) -> bool {
    ROOT_TYPES.contains(&ty) || GENERATED_SELECT_TYPES.contains(&ty)
}

/// One way onto a character: (owner, true for a select / false for a grant, the rule's own
/// requirements). The rule fires only when its requirements hold.
type Edge = (String, bool, String);

/// Why a not-offered element is a content defect, or `None`.
pub(super) struct Defects {
    parents: HashMap<String, BTreeSet<String>>,
    /// element -> the grants and selects that can put it on a character
    edges: HashMap<String, BTreeSet<Edge>>,
    reachable: HashSet<String>,
    /// ids some element grants
    granted: HashSet<String>,
}

impl Defects {
    pub(super) fn new(store: &ContentStore, defined: &dyn Fn(&str) -> bool) -> Self {
        // granted id -> (owner, grant requirements)
        let mut granted_by: HashMap<String, BTreeSet<(String, String)>> = HashMap::new();
        // (select type, support token) -> (owner, select requirements); token "" = no supports
        let mut selected_by: HashMap<(String, String), BTreeSet<(String, String)>> = HashMap::new();
        for se in store.iter() {
            let e = &se.element;
            for r in &e.rules {
                match r {
                    Rule::Grant(g) => {
                        granted_by
                            .entry(g.id.clone())
                            .or_default()
                            .insert((e.id.clone(), g.requirements.clone().unwrap_or_default()));
                    }
                    Rule::Select(s) => {
                        let ty = s.ty.to_lowercase();
                        let toks = s.supports.as_deref().map(tokens).unwrap_or_default();
                        let keys: Vec<String> = if toks.is_empty() {
                            vec![String::new()]
                        } else {
                            toks.into_iter().collect()
                        };
                        let req = s.requirements.clone().unwrap_or_default();
                        for t in keys {
                            selected_by
                                .entry((ty.clone(), t))
                                .or_default()
                                .insert((e.id.clone(), req.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut edges: HashMap<String, BTreeSet<Edge>> = HashMap::new();
        for se in store.iter() {
            let e = &se.element;
            let mut out: BTreeSet<Edge> = granted_by
                .get(&e.id)
                .into_iter()
                .flatten()
                .map(|(o, req)| (o.clone(), false, req.clone()))
                .collect();
            let ty = e.ty.to_lowercase();
            let keys = std::iter::once(String::new())
                .chain(std::iter::once(e.id.to_lowercase()))
                .chain(e.supports.iter().map(|s| s.to_lowercase()));
            for k in keys {
                if let Some(o) = selected_by.get(&(ty.clone(), k)) {
                    out.extend(o.iter().map(|(o, req)| (o.clone(), true, req.clone())));
                }
            }
            out.retain(|(o, _, _)| *o != e.id);
            edges.insert(e.id.clone(), out);
        }
        let parents: HashMap<String, BTreeSet<String>> = edges
            .iter()
            .map(|(id, es)| (id.clone(), es.iter().map(|x| x.0.clone()).collect()))
            .collect();
        // reachable: the roots, then every element with a reachable parent whose grant or
        // select can fire (its own requirements may hold)
        let mut children: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (child, es) in &edges {
            for (p, _, req) in es {
                children
                    .entry(p.as_str())
                    .or_default()
                    .push((child.as_str(), req.as_str()));
            }
        }
        // an element whose requirements name an installed element no character can hold is
        // unreachable too (an invocation requiring a pact boon whose supports no select names):
        // each round only removes elements, so it settles
        let mut reachable = reach(store, &children, defined);
        for _ in 0..MAX_REACH_ROUNDS {
            let holdable = |id: &str| holdable(store, &reachable, defined, id);
            let next = reach(store, &children, &holdable);
            if next.len() == reachable.len() {
                break;
            }
            reachable = next;
        }
        let granted = granted_by.into_keys().collect();
        Self {
            parents,
            edges,
            reachable,
            granted,
        }
    }

    /// True when some character can hold `id`: defined in this setup and, when installed
    /// content defines it, reachable.
    fn holdable(&self, store: &ContentStore, defined: &dyn Fn(&str) -> bool, id: &str) -> bool {
        holdable(store, &self.reachable, defined, id)
    }

    /// The defect that keeps `id` off every character, or `None` when the content can reach it.
    /// `defined` says whether an id exists in this setup (store or engine universe).
    pub(super) fn of(
        &self,
        store: &ContentStore,
        id: &str,
        rs: Ruleset,
        defined: &dyn Fn(&str) -> bool,
    ) -> Option<&'static str> {
        let el = store.element(id)?;
        if requirement_never_holds(&el.requirements, defined) {
            return Some(
                "never offered in this setup: its requirements need ids no installed content \
                 defines (a typo, or content of a repo not installed here)",
            );
        }
        let holdable = |x: &str| self.holdable(store, defined, x);
        if requirement_never_holds(&el.requirements, &holdable) {
            return Some(
                "content defect: its requirements need an element that no installed content can \
                 put on a character (nothing grants it, no select offers it)",
            );
        }
        if worn_counters_exclusive(store, el, &holdable) {
            return Some(
                "content defect: its requirements need two counters at once that only different \
                 body armors set, and a character wears one armor (a selection whose own \
                 requirements fail is dropped on re-evaluation, engine.md 3.3)",
            );
        }
        if el.ty == "Spell" && known_list_only(store, el) {
            return Some(
                "by design (engine.md 10.2): listed only by the known list of a caster that does \
                 not prepare spells; such a caster knows only selected or granted spells, and no \
                 select or grant in this setup offers it",
            );
        }
        let parents = self.parents.get(id)?;
        // every element that grants it or owns a select naming it is tagged for the other
        // ruleset: those are excluded from this edition's option lists (decisions-engine.md
        // B-12), and this element is excluded from theirs
        let other_edition = |p: &String| {
            store
                .element(p)
                .and_then(|x| x.ruleset)
                .is_some_and(|r| r != rs && r != Ruleset::Any)
        };
        if rs != Ruleset::Any && !parents.is_empty() && parents.iter().all(other_edition) {
            return Some(
                "never offered in this edition: every element that grants it or has a select \
                 naming it belongs to the other ruleset, whose elements this edition's option \
                 lists exclude (decisions-engine.md B-12)",
            );
        }
        // nothing grants it and every select naming it asks for supports it lacks: ids joined
        // with `,` (and) outside the id-range mode, so no element satisfies both (engine.md 5.2).
        // A proficiency is a root only through its proxy item, which never counts as offering
        // (proxy-items.md D5), so the rule holds for it too.
        let select_root = el.ty == "Proficiency";
        if (select_root || !is_root(&el.ty))
            && !parents.is_empty()
            && !self.granted.contains(id)
            && parents.iter().all(|p| !select_matches(store, p, el))
        {
            return Some(
                "content defect: nothing grants it and no select of its type matches it by the \
                 whole supports expression (engine.md 5.2: `,` is and; an id list needs `|`)",
            );
        }
        if is_root(&el.ty) {
            return None;
        }
        if parents.is_empty() {
            // a subclass or sub race no installed class or race has a select for
            return Some(
                "never offered in this setup: nothing grants it and no select of its type names \
                 it or its supports (its class or race is not installed, or the supports differ)",
            );
        }
        if parents.iter().all(|p| !self.reachable.contains(p)) {
            return Some(
                "content defect: every element that could put it on a character is itself \
                 unreachable (nothing grants it, no select offers it)",
            );
        }
        // a grant or select fires only when its own requirements hold: one that needs an
        // element no character can hold in this setup (its only granter is missing, or a later
        // repo replaced it with a copy that no longer grants it) never offers anything
        let edges = self.edges.get(id)?;
        let live = |(owner, _, req): &Edge| {
            self.reachable.contains(owner) && !requirement_never_holds(req, &holdable)
        };
        if !edges.iter().any(live) {
            return Some(
                "never offered in this setup: every grant or select that could put it on a \
                 character requires an element no installed content can put on a character",
            );
        }
        (edges.iter().any(|e| e.1) && !edges.iter().filter(|e| e.1).any(live)).then_some(
            "never offered in this setup: only granted; every select naming it requires an \
             element no installed content can put on a character",
        )
    }
}

/// The counter keys each conjunct of `req` asks for (`[a:1]` or `[a:1]|[b:1]` is one conjunct);
/// an id conjunct adds the conjuncts of that element's own requirements (it must hold at the
/// same time). Conjuncts of any other form are left out (they only make it harder to hold).
fn counter_conjuncts(store: &ContentStore, req: &str, depth: usize) -> Vec<BTreeSet<String>> {
    fn keys(e: &BoolExpr<RequirementLeaf>, out: &mut BTreeSet<String>) -> bool {
        match e {
            BoolExpr::Leaf(RequirementLeaf::Test(t)) => {
                let positive =
                    matches!(t.value, borealis_content::expr::TestValue::Number(n) if n > 0);
                out.insert(t.normalized_key());
                positive
            }
            BoolExpr::Or(items) => items.iter().all(|x| keys(x, out)),
            _ => false,
        }
    }
    let Ok(Some(expr)) = RequirementsExpr::parse(req) else {
        return Vec::new();
    };
    let items: Vec<&BoolExpr<RequirementLeaf>> = match expr.root() {
        BoolExpr::And(items) => items.iter().collect(),
        other => vec![other],
    };
    let mut out = Vec::new();
    for item in items {
        if let BoolExpr::Leaf(RequirementLeaf::Id(id)) = item {
            if let Some(x) = store.element(id).filter(|_| depth > 0) {
                out.extend(counter_conjuncts(store, &x.requirements, depth - 1));
            }
            continue;
        }
        let mut k = BTreeSet::new();
        if keys(item, &mut k)
            && k.iter()
                .all(|key| key.contains(':') && !key.starts_with("level"))
        {
            out.push(k);
        }
    }
    out
}

/// True when `el`'s requirements (and those of the ids they name) need two counter conjuncts
/// at once, every holdable element raising any of their counters is body armor, and no one
/// armor raises a counter of both: a character wears one body armor.
fn worn_counters_exclusive(
    store: &ContentStore,
    el: &Element,
    holdable: &dyn Fn(&str) -> bool,
) -> bool {
    let conj = counter_conjuncts(store, &el.requirements, 1);
    if conj.len() < 2 {
        return false;
    }
    // the holdable setters of each counter
    let setters = |key: &str| -> Vec<&Element> {
        store
            .iter()
            .map(|se| &se.element)
            .filter(|e| holdable(&e.id))
            .filter(|e| {
                e.rules.iter().any(|r| {
                    matches!(r, Rule::Stat(s) if s.name.eq_ignore_ascii_case(key)
                        && s.value.trim().parse::<i64>().map_or(true, |n| n > 0))
                })
            })
            .collect()
    };
    let body_armor =
        |e: &Element| e.ty == "Armor" && e.setter_value("slot").is_some_and(|v| v.trim() == "body");
    let sets: Vec<BTreeSet<String>> = conj
        .iter()
        .map(|keys| {
            keys.iter()
                .flat_map(|k| setters(k))
                .map(|e| e.id.clone())
                .collect()
        })
        .collect();
    let all_armor = conj.iter().flatten().all(|k| {
        let s = setters(k);
        !s.is_empty() && s.iter().all(|e| body_armor(e))
    });
    if !all_armor {
        return false;
    }
    // two conjuncts whose setters share no armor can never hold together
    sets.iter()
        .enumerate()
        .any(|(i, a)| sets.iter().skip(i + 1).any(|b| a.is_disjoint(b)))
}

/// Rounds of [`Defects::new`] that drop elements requiring an unholdable element (each round
/// follows one more link of such a chain; real chains are short).
const MAX_REACH_ROUNDS: usize = 8;

/// The roots, then every element with a reachable parent; an element whose requirements never
/// hold under `holds` (they name ids no character can have) puts nothing on a character.
fn reach(
    store: &ContentStore,
    children: &HashMap<&str, Vec<(&str, &str)>>,
    holds: &dyn Fn(&str) -> bool,
) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: Vec<&str> = store
        .iter()
        .filter(|se| is_root(&se.element.ty))
        .map(|se| se.element.id.as_str())
        .collect();
    while let Some(id) = queue.pop() {
        let never = store
            .element(id)
            .is_some_and(|e| requirement_never_holds(&e.requirements, holds));
        if never || !reachable.insert(id.to_string()) {
            continue;
        }
        if let Some(kids) = children.get(id) {
            queue.extend(
                kids.iter()
                    .filter(|(_, req)| !requirement_never_holds(req, holds))
                    .map(|(kid, _)| *kid),
            );
        }
    }
    reachable
}

/// True when `id` is defined in this setup and, when installed content defines it, reachable.
fn holdable(
    store: &ContentStore,
    reachable: &HashSet<String>,
    defined: &dyn Fn(&str) -> bool,
    id: &str,
) -> bool {
    defined(id) && (store.element(id).is_none() || reachable.contains(id))
}

/// True when a select of `owner` of `el`'s type offers `el` by its whole supports expression
/// (generous: a tag matches the id or a support, any case; a dynamic part matches).
fn select_matches(store: &ContentStore, owner: &str, el: &Element) -> bool {
    store.element(owner).is_some_and(|o| {
        o.rules.iter().any(|r| {
            matches!(r, Rule::Select(s) if s.ty.eq_ignore_ascii_case(&el.ty)
                && supports_match(s.supports.as_deref().unwrap_or(""), el))
        })
    })
}

/// The content verdict of a settle-invariant violation, or `None` when the content does not
/// explain it (each read from the installed content; Aurora reads the same content and ends in
/// the same state):
///
/// * an empty required select no installed element can fill ([`unfillable_select`]);
/// * grant expansion that does not settle while some element requires its own absence
///   (`!<own id>` as a necessary condition): it is ungranted because it is present and granted
///   again because it is absent, every pass (engine.md 3.2 runs the same churn);
/// * negative spell slots where the content itself writes a negative value to a slot stat of
///   that caster (engine.md 10.5: the sheet shows the group sum).
pub(super) fn invariant_verdict(
    store: &ContentStore,
    v: &borealis_engine::invariants::Violation,
) -> Option<&'static str> {
    use borealis_engine::invariants::kind;
    if unfillable_select(store, v) {
        return Some("content defect: no installed element can fill this required select");
    }
    if v.kind == kind::NOT_CONVERGED && store.iter().any(|se| requires_own_absence(&se.element)) {
        return Some(
            "content defect: an element requires its own absence, so each grant pass ungrants \
             it and the next grants it again (Aurora runs the same churn)",
        );
    }
    let caster = v.element.as_deref().map(str::to_lowercase);
    let negative = v.kind == kind::SLOTS && v.message.starts_with("negative spell slots");
    if negative && caster.is_some_and(|c| writes_negative_slots(store, &c)) {
        return Some(
            "content defect: the content writes a negative value to this caster's spell slot \
             stats (Aurora shows the same group sum)",
        );
    }
    None
}

/// True when `el`'s requirements need its own id absent (a top-level `!<own id>` conjunct).
fn requires_own_absence(el: &Element) -> bool {
    fn needs_absent(e: &BoolExpr<RequirementLeaf>, id: &str) -> bool {
        match e {
            BoolExpr::Not(inner) => {
                matches!(inner.as_ref(), BoolExpr::Leaf(RequirementLeaf::Id(x)) if x == id)
            }
            BoolExpr::And(items) => items.iter().any(|x| needs_absent(x, id)),
            _ => false,
        }
    }
    matches!(RequirementsExpr::parse(&el.requirements), Ok(Some(expr)) if needs_absent(expr.root(), &el.id))
}

/// True when some stat rule adds a negative literal to `<caster>:spellcasting:slots:<n>`.
fn writes_negative_slots(store: &ContentStore, caster: &str) -> bool {
    let prefix = format!("{caster}:spellcasting:slots:");
    store.iter().any(|se| {
        se.element.rules.iter().any(|r| {
            matches!(r, Rule::Stat(s) if s.name.to_lowercase().starts_with(&prefix)
                && s.value.trim().parse::<i64>().is_ok_and(|n| n < 0))
        })
    })
}

/// True when `v` is an empty required select that no installed element can ever fill: no
/// element of the select's type carries any token of its supports (a language select whose
/// support tag only a repo outside this setup gives its languages). Aurora shows the same
/// empty select.
pub(super) fn unfillable_select(
    store: &ContentStore,
    v: &borealis_engine::invariants::Violation,
) -> bool {
    if v.kind != borealis_engine::invariants::kind::EMPTY_SELECT {
        return false;
    }
    let Some(owner) = v.element.as_deref().and_then(|id| store.element(id)) else {
        return false;
    };
    // the message is "required select <name> (<type>) offers nothing"
    let Some(head) = v.message.strip_suffix(") offers nothing") else {
        return false;
    };
    let Some((name, ty)) = head
        .strip_prefix("required select ")
        .and_then(|h| h.rsplit_once(" ("))
    else {
        return false;
    };
    let selects: Vec<_> = owner
        .rules
        .iter()
        .filter_map(|r| match r {
            Rule::Select(s) if s.ty == ty => Some(s),
            _ => None,
        })
        .collect();
    let named: Vec<_> = selects.iter().filter(|s| s.name == name).collect();
    let candidates = if named.is_empty() {
        selects.iter().collect()
    } else {
        named
    };
    !candidates.is_empty()
        && candidates.iter().all(|s| {
            supports_unfillable(store, ty, s.supports.as_deref())
                || supports_unmatched(store, ty, s.supports.as_deref())
        })
}

/// True when no installed element of `ty` that a character can be offered matches the select's
/// whole `supports` expression (`supports_unfillable` asks for any one token): a 2024 class's
/// `Skill,PHB24 Wizard` when another repo redefined every skill without the `PHB24 Wizard` tag,
/// or a tag only 2014 elements carry that their 2024 versions replace (IM11 keeps the 2014
/// element's own tags off its equivalent). Aurora reads the same content.
fn supports_unmatched(store: &ContentStore, ty: &str, supports: Option<&str>) -> bool {
    let raw = supports.unwrap_or("").trim();
    if raw.is_empty() || raw.contains("$(") {
        return false;
    }
    let replaced: HashSet<&str> = store
        .iter()
        .filter_map(|se| se.element.maps_to.as_ref())
        .filter(|m| m.kind == Some(borealis_content::MapKind::Equivalent))
        .filter_map(|m| m.dnd2014.as_deref())
        .collect();
    !store.iter().any(|se| {
        let e = &se.element;
        e.ty == ty && !replaced.contains(e.id.as_str()) && supports_match(raw, e)
    })
}

/// True when a select of `ty` with `supports` can never offer anything: it names support
/// tokens (no expansion) and no installed element of `ty` carries any of them.
pub(super) fn supports_unfillable(store: &ContentStore, ty: &str, supports: Option<&str>) -> bool {
    let raw = supports.unwrap_or("");
    let toks = tokens(raw);
    !toks.is_empty()
        && !raw.contains("$(")
        && !store.iter().any(|se| {
            let e = &se.element;
            e.ty == ty
                && (toks.contains(&e.id.to_lowercase())
                    || e.supports.iter().any(|s| toks.contains(&s.to_lowercase())))
        })
}

/// True when a select's supports expand a caster's spell list (`$(spellcasting:list)`).
fn names_caster_list(raw: &str) -> bool {
    raw.to_lowercase().contains("$(spellcasting:list)")
}

/// True when a spell list entry names `spell` (its id, or a supports expression it matches).
fn names_spell(list: &SpellcastingList, spell: &Element) -> bool {
    if list.is_spell_id() {
        list.supports.trim().eq_ignore_ascii_case(&spell.id)
    } else {
        supports_match(&list.supports, spell)
    }
}

/// True when `spell` is listed only by `<list known="true">` entries of casters with
/// `prepare` not true (engine.md 10.2: such a caster knows only the spells it selects or is
/// granted), nothing grants it, and no spell select can offer it: no other select matches it
/// (a `$(spellcasting:slots)` part may match any level), and every select of a caster's list
/// (`$(spellcasting:list)`) belongs to a caster whose lists (and the `all` extensions) do not
/// name it. A list select without a caster name may offer anything, so the spell is then not
/// called unlisted.
fn known_list_only(store: &ContentStore, spell: &Element) -> bool {
    let mut listed = false;
    let mut caster_lists: HashSet<String> = HashSet::new();
    let mut all_extends = false;
    for se in store.iter() {
        let e = &se.element;
        for r in &e.rules {
            match r {
                Rule::Grant(g) if g.id == spell.id => return false,
                Rule::Select(s) if s.ty.eq_ignore_ascii_case("Spell") => {
                    let raw = s.supports.as_deref().unwrap_or("");
                    if names_caster_list(raw) {
                        if s.spellcasting.is_none() {
                            return false;
                        }
                    } else if supports_match(raw, spell) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        let Some(sc) = &e.spellcasting else { continue };
        let entries: Vec<&SpellcastingList> = sc.list.iter().chain(&sc.extends).collect();
        let named: Vec<&&SpellcastingList> =
            entries.iter().filter(|l| names_spell(l, spell)).collect();
        if named.is_empty() {
            continue;
        }
        if sc.all == Some(true) {
            all_extends = true;
        }
        if let Some(n) = &sc.name {
            caster_lists.insert(n.to_lowercase());
        }
        let non_preparing = sc.prepare != Some(true);
        if !named.iter().all(|l| non_preparing && l.known == Some(true)) {
            return false;
        }
        listed = true;
    }
    if !listed || all_extends {
        return false;
    }
    // a dynamic select of a caster whose list names the spell would offer it
    !store.iter().any(|se| {
        se.element.rules.iter().any(|r| match r {
            Rule::Select(s) if s.ty.eq_ignore_ascii_case("Spell") => {
                names_caster_list(s.supports.as_deref().unwrap_or(""))
                    && s.spellcasting
                        .as_deref()
                        .is_some_and(|n| caster_lists.contains(&n.to_lowercase()))
            }
            _ => false,
        })
    })
}

/// True when `requirements` is false on every character: each alternative names an id
/// `defined` rejects (negations of missing ids hold; tests and present ids may hold).
fn requirement_never_holds(requirements: &str, defined: &dyn Fn(&str) -> bool) -> bool {
    fn possible(e: &BoolExpr<RequirementLeaf>, defined: &dyn Fn(&str) -> bool) -> bool {
        match e {
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => defined(id),
            BoolExpr::Leaf(RequirementLeaf::Test(_)) | BoolExpr::Not(_) => true,
            BoolExpr::And(items) => items.iter().all(|x| possible(x, defined)),
            BoolExpr::Or(items) => items.iter().any(|x| possible(x, defined)),
        }
    }
    match RequirementsExpr::parse(requirements) {
        Ok(Some(expr)) => !possible(expr.root(), defined),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::requirement_never_holds;

    #[test]
    fn a_requirement_naming_only_missing_ids_never_holds() {
        let defined = |id: &str| id == "ID_KNOWN";
        assert!(requirement_never_holds("ID_MISSING", &defined));
        assert!(requirement_never_holds("ID_KNOWN,ID_MISSING", &defined));
        assert!(!requirement_never_holds("ID_KNOWN||ID_MISSING", &defined));
        assert!(!requirement_never_holds("!ID_MISSING", &defined));
        assert!(!requirement_never_holds("[level:5],ID_KNOWN", &defined));
        assert!(!requirement_never_holds("", &defined));
    }

    #[test]
    fn ids_joined_with_a_comma_match_no_element() {
        let el = borealis_content::Element {
            id: "ID_B".into(),
            ty: "Proficiency".into(),
            ..Default::default()
        };
        assert!(!crate::support::plan::supports_match("ID_A,ID_B", &el));
        assert!(crate::support::plan::supports_match("ID_A|ID_B", &el));
    }

    /// A store of synthetic elements (one custom file, in order).
    fn store_of(elements: serde_json::Value) -> borealis_content::ContentStore {
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
        borealis_content::ContentStoreBuilder::new()
            .add_file("t.json", borealis_content::PackOrigin::Custom, file)
            .build()
    }

    #[test]
    fn a_rule_whose_requirements_never_hold_puts_nothing_on_a_character() {
        // a feat whose selects/grants require markers: one nothing grants, one a race grants
        let store = store_of(serde_json::json!([
            {"id": "ID_T_FEAT", "type": "Feat", "name": "F", "rules": [
                {"kind": "select", "type": "Feat Feature", "name": "Dead",
                 "supports": "T Benefit", "requirements": "ID_T_MARKER"},
                {"kind": "select", "type": "Feat Feature", "name": "Live", "supports": "T Live"},
                {"kind": "grant", "type": "Feat Feature", "id": "ID_T_GRANTED",
                 "requirements": "ID_T_OTHER_MARKER"}]},
            {"id": "ID_T_RACE", "type": "Race", "name": "R", "rules": [
                {"kind": "grant", "type": "Grants", "id": "ID_T_OTHER_MARKER"}]},
            {"id": "ID_T_MARKER", "type": "Grants", "name": "M"},
            {"id": "ID_T_OTHER_MARKER", "type": "Grants", "name": "O"},
            {"id": "ID_T_SELECTED", "type": "Feat Feature", "name": "S",
             "supports": ["T Benefit"], "rules": [
                {"kind": "select", "type": "Feat Feature", "name": "Ability",
                 "supports": "ID_T_CHILD"}]},
            {"id": "ID_T_CHILD", "type": "Feat Feature", "name": "C"},
            {"id": "ID_T_GRANTED", "type": "Feat Feature", "name": "G", "supports": ["T Benefit"]},
            {"id": "ID_T_LIVE", "type": "Feat Feature", "name": "L", "supports": ["T Live"]}
        ]));
        let defined = |_: &str| true;
        let d = super::Defects::new(&store, &defined);
        let of = |id: &str| d.of(&store, id, borealis_content::Ruleset::Dnd2014, &defined);
        assert!(of("ID_T_SELECTED").is_some_and(|w| w.contains("every grant or select")));
        assert!(of("ID_T_CHILD").is_some_and(|w| w.contains("itself unreachable")));
        assert!(of("ID_T_GRANTED").is_some_and(|w| w.contains("only granted")));
        assert_eq!(of("ID_T_LIVE"), None);
    }
}
