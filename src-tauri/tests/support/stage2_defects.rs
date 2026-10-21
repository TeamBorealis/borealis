//! Content defects of stage 2: an element no host offers because the installed content can never
//! put it on a character, so Aurora (which reads the same content) never offers it either. Such
//! a row asserts that verdict instead of an offer. Two causes, both read from the installed
//! content (no id is named here):
//!
//! * **unreachable** - every element that could put it on a character (an owner of a select of
//!   its type whose supports name it or one of its supports, or an element granting it) is
//!   itself unreachable: nothing grants it, no select offers it, and it is no host root. A
//!   subclass that grants a misspelled feature id leaves the real feature's select orphaned.
//! * **no parent** - nothing grants it and no select of its type names it or its supports (a
//!   subclass whose class is not installed in this setup).
//! * **requirement** - its requirements can never hold: every alternative needs an id that this
//!   setup does not define (Aurora compares ids ordinally; a missing id is never held).
//!
//! Both checks are generous to the content (any supports token links a select, any grant counts,
//! every host root type is reachable), so a row is called a defect only when no reading of the
//! content can reach it.

use crate::support::plan::tokens;
use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr};
use borealis_content::{ContentStore, Rule};
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

/// Why a not-offered element is a content defect, or `None`.
pub(super) struct Defects {
    parents: HashMap<String, BTreeSet<String>>,
    reachable: HashSet<String>,
}

impl Defects {
    pub(super) fn new(store: &ContentStore) -> Self {
        let mut granted_by: HashMap<String, BTreeSet<String>> = HashMap::new();
        // (select type, supports token) -> owners; token "" = no supports (any element)
        let mut selected_by: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for se in store.iter() {
            let e = &se.element;
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
                        let keys: Vec<String> = if toks.is_empty() {
                            vec![String::new()]
                        } else {
                            toks.into_iter().collect()
                        };
                        for t in keys {
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
        let mut parents: HashMap<String, BTreeSet<String>> = HashMap::new();
        for se in store.iter() {
            let e = &se.element;
            let mut out = granted_by.get(&e.id).cloned().unwrap_or_default();
            let ty = e.ty.to_lowercase();
            let keys = std::iter::once(String::new())
                .chain(std::iter::once(e.id.to_lowercase()))
                .chain(e.supports.iter().map(|s| s.to_lowercase()));
            for k in keys {
                if let Some(o) = selected_by.get(&(ty.clone(), k)) {
                    out.extend(o.iter().cloned());
                }
            }
            out.remove(&e.id);
            parents.insert(e.id.clone(), out);
        }
        // reachable: the roots, then every element with a reachable parent
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        for (child, ps) in &parents {
            for p in ps {
                children.entry(p.as_str()).or_default().push(child.as_str());
            }
        }
        let mut reachable: HashSet<String> = HashSet::new();
        let mut queue: Vec<&str> = store
            .iter()
            .filter(|se| is_root(&se.element.ty))
            .map(|se| se.element.id.as_str())
            .collect();
        while let Some(id) = queue.pop() {
            if !reachable.insert(id.to_string()) {
                continue;
            }
            if let Some(kids) = children.get(id) {
                queue.extend(kids.iter().copied());
            }
        }
        Self { parents, reachable }
    }

    /// The defect that keeps `id` off every character, or `None` when the content can reach it.
    /// `defined` says whether an id exists in this setup (store or engine universe).
    pub(super) fn of(
        &self,
        store: &ContentStore,
        id: &str,
        defined: &dyn Fn(&str) -> bool,
    ) -> Option<&'static str> {
        let el = store.element(id)?;
        if requirement_never_holds(&el.requirements, defined) {
            return Some(
                "never offered in this setup: its requirements need ids no installed content \
                 defines (a typo, or content of a repo not installed here)",
            );
        }
        if is_root(&el.ty) {
            return None;
        }
        let parents = self.parents.get(id)?;
        if parents.is_empty() {
            // a subclass or sub race no installed class or race has a select for
            return Some(
                "never offered in this setup: nothing grants it and no select of its type names \
                 it or its supports (its class or race is not installed, or the supports differ)",
            );
        }
        parents
            .iter()
            .all(|p| !self.reachable.contains(p))
            .then_some(
                "content defect: every element that could put it on a character is itself \
                 unreachable (nothing grants it, no select offers it)",
            )
    }
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
    let fillable = |toks: &BTreeSet<String>| {
        store.iter().any(|se| {
            let e = &se.element;
            e.ty == ty
                && (toks.contains(&e.id.to_lowercase())
                    || e.supports.iter().any(|s| toks.contains(&s.to_lowercase())))
        })
    };
    !candidates.is_empty()
        && candidates.iter().all(|s| {
            let raw = s.supports.as_deref().unwrap_or("");
            let toks = tokens(raw);
            !toks.is_empty() && !raw.contains("$(") && !fillable(&toks)
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
}
