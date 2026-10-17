//! Prerequisite reading shared by the corpus sweeps and the acceptance harness: what a feat's
//! requirements ask of a host character (5etools `Needs`, det-5e-3) and the feat chains a feat
//! names (Aurora, det-aurora-1). Content-level only (`borealis_content`), so the engine sweeps
//! (`fivetools_corpus`, `aurora_corpus_sweep`) and the app-level harness (`acceptance`) build
//! their hosts from the same reading. Included with `#[path]`; each user takes what it needs.
#![allow(dead_code)]

use borealis_content::expr::{BoolExpr, RequirementLeaf, RequirementsExpr, TestValue};
use borealis_content::{ContentStore, Element, Ruleset};
use std::collections::HashMap;

/// Ability keys of requirement tests, in score order.
pub const ABILITY_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// How deep prior-feat prerequisites are followed (a feat needing a feat needing a feat).
pub const MAX_PRIOR_FEATS: usize = 3;
/// Longest feat prerequisite chain followed ([`feat_chain`]).
pub const MAX_CHAIN: usize = 6;
/// Id prefix of the engine's generated "Feat (n)" improvement options (`universe.rs`
/// `generate_improvements`: `ID_INTERNAL_CLASS_FEATURE_FEAT_<n>_<CLASS>`, named "Feat (4)").
pub const FEAT_OPTION_PREFIX: &str = "ID_INTERNAL_CLASS_FEATURE_FEAT_";

/// The 2014 "Feat" option of an ability score improvement select, as the 5etools sweep finds it.
pub fn is_feat_option_5et(id: &str, name: &str) -> bool {
    id.starts_with(FEAT_OPTION_PREFIX) || name == "Feat"
}

/// The "Feat" option of an ability score improvement select, as the Aurora sweep finds it.
pub fn is_feat_option_aurora(name: &str) -> bool {
    name == "Feat" || name.starts_with("Feat (")
}

/// What a feat's requirements ask of a character: the first buildable alternative of every
/// OR, negations ignored (a fresh character has none of the excluded elements).
#[derive(Debug, Default, Clone)]
pub struct Needs {
    pub level: u32,
    pub caster: bool,
    pub class: Option<String>,
    pub race: Option<String>,
    pub feats: Vec<String>,
    pub scores: Vec<(usize, i32)>,
}

/// Reads [`Needs`] against one store; `ours` gives the edition of the imported elements and
/// `class_id` turns a class name of a level test (`[fighter:4]`) into an element id.
pub struct NeedsReader<'a> {
    pub store: &'a ContentStore,
    pub ours: &'a HashMap<String, Ruleset>,
    pub class_id: fn(Ruleset, &str) -> String,
}

/// 5etools class id of a class name (`ID_5ET_PHB_CLASS_FIGHTER`, `ID_5ET_XPHB_CLASS_...`).
pub fn class_id_5et(rs: Ruleset, name: &str) -> String {
    let book = if rs == Ruleset::Dnd2024 {
        "XPHB"
    } else {
        "PHB"
    };
    format!(
        "ID_5ET_{book}_CLASS_{}",
        name.to_uppercase().replace(' ', "_")
    )
}

impl NeedsReader<'_> {
    /// An id a host can use: it exists and, for origins, is of the feat's edition.
    pub fn usable(&self, rs: Ruleset, id: &str) -> bool {
        self.store.element(id).is_some_and(|e| {
            !matches!(e.ty.as_str(), "Race" | "Species" | "Sub Race")
                || e.ruleset.or_else(|| self.ours.get(id).copied()) == Some(rs)
        })
    }

    pub fn buildable(&self, rs: Ruleset, expr: &BoolExpr<RequirementLeaf>) -> bool {
        match expr {
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => self.usable(rs, id),
            BoolExpr::Leaf(RequirementLeaf::Test(_)) | BoolExpr::Not(_) => true,
            BoolExpr::And(items) => items.iter().all(|e| self.buildable(rs, e)),
            BoolExpr::Or(items) => items.iter().any(|e| self.buildable(rs, e)),
        }
    }

    fn collect(&self, rs: Ruleset, expr: &BoolExpr<RequirementLeaf>, needs: &mut Needs) {
        match expr {
            BoolExpr::Not(_) => {}
            BoolExpr::And(items) => items.iter().for_each(|e| self.collect(rs, e, needs)),
            BoolExpr::Or(items) => {
                if let Some(first) = items
                    .iter()
                    .find(|e| self.buildable(rs, e))
                    .or(items.first())
                {
                    self.collect(rs, first, needs);
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => {
                match self.store.element(id).map(|e| e.ty.as_str()) {
                    Some("Race" | "Species" | "Sub Race") => {
                        needs.race.get_or_insert_with(|| id.clone());
                    }
                    Some("Feat") => needs.feats.push(id.clone()),
                    Some("Class") => {
                        needs.class.get_or_insert_with(|| id.clone());
                    }
                    _ => {}
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Test(t)) => {
                let key = t.normalized_key();
                let n = match &t.value {
                    TestValue::Number(n) => u32::try_from(*n).unwrap_or(0),
                    TestValue::Text(_) => 0,
                };
                let feature = key.strip_prefix("feature:").map(str::to_string);
                let class_id = self.class_id;
                match (key.as_str(), feature.as_deref()) {
                    ("character" | "level", _) => needs.level = needs.level.max(n),
                    ("spellcasting", _) => needs.caster = true,
                    (_, Some("spellcasting" | "pact magic")) => needs.caster = true,
                    (k, _) if ABILITY_KEYS.contains(&k) => {
                        let i = ABILITY_KEYS.iter().position(|a| *a == k).unwrap_or(0);
                        needs.scores.push((i, i32::try_from(n).unwrap_or(0)));
                    }
                    (k, None) if self.store.element(&class_id(rs, k)).is_some() => {
                        needs.class.get_or_insert_with(|| class_id(rs, k));
                        needs.level = needs.level.max(n);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The needs of an element's requirements, prior feats' own needs included.
    pub fn needs(&self, rs: Ruleset, el: &Element, depth: usize, needs: &mut Needs) {
        let Ok(Some(expr)) = RequirementsExpr::parse(&el.requirements) else {
            return;
        };
        let before = needs.feats.len();
        self.collect(rs, expr.root(), needs);
        if depth >= MAX_PRIOR_FEATS {
            return;
        }
        // a prior feat's own prerequisites come first
        let prior: Vec<String> = needs.feats[before..].to_vec();
        for id in prior {
            if let Some(feat) = self.store.element(&id) {
                let mut inner = Needs::default();
                self.needs(rs, feat, depth + 1, &mut inner);
                needs.level = needs.level.max(inner.level);
                needs.caster |= inner.caster;
                needs.class = needs.class.take().or(inner.class);
                needs.race = needs.race.take().or(inner.race);
                needs.scores.extend(inner.scores);
                let at = needs.feats.iter().position(|f| *f == id).unwrap_or(0);
                for f in inner.feats.into_iter().rev() {
                    if !needs.feats.contains(&f) {
                        needs.feats.insert(at, f);
                    }
                }
            }
        }
    }
}

/// Elements of type `ty` a requirements string names, each once, in order.
pub fn required_of_type(store: &ContentStore, requirements: &str, ty: &str) -> Vec<String> {
    let Ok(Some(expr)) = RequirementsExpr::parse(requirements) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for id in expr.referenced_ids() {
        if store.element(id).is_some_and(|e| e.ty == ty) && !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// The feats a feat's requirements name, prerequisites first (feat chains such as Elemental
/// Adept 1-5 or Rune Knowledge 1-3), each once.
pub fn feat_chain(store: &ContentStore, id: &str) -> Vec<String> {
    fn walk(store: &ContentStore, id: &str, depth: usize, out: &mut Vec<String>) {
        let Some(el) = store.element(id) else { return };
        if depth > MAX_CHAIN {
            return;
        }
        for req in required_of_type(store, &el.requirements, "Feat") {
            if !out.contains(&req) {
                walk(store, &req, depth + 1, out);
                if !out.contains(&req) {
                    out.push(req);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(store, id, 0, &mut out);
    out.retain(|f| f != id);
    out
}
