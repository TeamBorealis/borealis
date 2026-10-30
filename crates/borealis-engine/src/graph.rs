//! The instance graph: elements registered on a character, expanded from the stored inputs by
//! applying grant and select rules to a fixpoint (with per-instance acquisition,
//! slot keys and the owner environment).

use crate::diag::{code, DiagSink, Diagnostic, Severity};
use crate::eval::{
    requirement_holds, requirement_ids, supports_cond_match, DynamicSupports, EquipState, IdCounts,
    ReqEnv,
};
use crate::ids;
use crate::state::{CharacterState, Choice, ClassSlot, SlotKey};
use crate::universe::{Cond, EditionStatus, UElem, URule, UStat, Universe};
use borealis_content::expr::{Dynamic, SupportsExpr};
use borealis_content::{Rule, SourceRestriction};
use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Index of a node.
pub type NodeId = usize;

/// Which progression a node belongs to (decides which level gates its rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mgr {
    /// Character level.
    Main,
    /// Levels of the main class.
    MainClass,
    /// Levels of the multiclass started at this character level.
    Multi(u32),
}

/// How a node entered the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Top-level registration (option, level, equipped item).
    Root,
    /// Attached by the engine itself (multiclass level markers).
    Manual,
    /// Granted by rule index `rule` of the parent.
    Granted(usize),
    /// Chosen in slot `slot` of the parent.
    Selected(usize),
}

/// A select slot of a node.
#[derive(Debug, Clone)]
pub struct SlotNode {
    /// Stable key.
    pub key: SlotKey,
    /// Rule index in the owner.
    pub rule: usize,
    /// 1-based ordinal.
    pub ordinal: u32,
    /// Node chosen in this slot.
    pub chosen: Option<NodeId>,
    /// Choice of a `List` select.
    pub list_choice: Option<i64>,
    /// A stored choice that is currently not allowed.
    pub rejected: Option<Choice>,
    /// A stored choice naming an element that is not installed. The slot stays open (offers
    /// its options), as Aurora leaves such a select empty; the choice is kept until replaced.
    pub missing: Option<String>,
}

/// A registered element instance.
#[derive(Debug, Clone)]
pub struct Node<'u> {
    /// The element.
    pub e: &'u UElem,
    /// Parent instance.
    pub parent: Option<NodeId>,
    /// Acquisition.
    pub origin: Origin,
    /// Progression manager.
    pub mgr: Mgr,
    /// Children (granted and selected) in insertion order.
    pub children: Vec<NodeId>,
    /// Open select slots.
    pub slots: Vec<SlotNode>,
    /// False once removed.
    pub alive: bool,
    /// Spellcasting class a spell was acquired for.
    pub spell_class: Option<String>,
    /// Always-prepared acquisition.
    pub always_prepared: bool,
    /// Inventory item uid for item roots.
    pub item_uid: Option<String>,
    /// Item attuned flag for item roots.
    pub attuned: bool,
    /// Preset feat parameters (`grant_feat`), keyed by normalised parameter name.
    pub params: std::collections::BTreeMap<String, String>,
    /// Instance tag of a repeatable element, so each take has its own slot keys.
    pub instance: Option<String>,
}

/// An item that takes part in the graph as a root.
#[derive(Debug, Clone)]
pub struct ItemRoot<'u> {
    /// Element (base item or adorner).
    pub e: &'u UElem,
    /// Inventory uid.
    pub uid: String,
    /// Attuned.
    pub attuned: bool,
}

/// Filters [`Graph::choice_allowed_relaxed`] may switch off (U14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Relax {
    /// Ignore the edition: the profile's acceptance and the "other edition" option.
    pub edition: bool,
    /// Ignore the character's source restriction.
    pub restriction: bool,
    /// Ignore the candidate's own requirements and prerequisites (an unreadable requirement
    /// never holds; C1 asks which options it hides).
    pub requirements: bool,
}

impl Relax {
    /// Every filter applies.
    pub(crate) const NONE: Self = Self {
        edition: false,
        restriction: false,
        requirements: false,
    };
}

/// Static inputs of an expansion run.
pub struct GraphInputs<'u> {
    /// Ability hint for requirements.
    pub abilities: [i32; 6],
    /// Statistic sums of the previous iteration.
    pub stat_sums: HashMap<String, i32>,
    /// Roots for equipped items.
    pub items: Vec<ItemRoot<'u>>,
    /// Equipment facts for equipped conditions.
    pub equip: EquipState,
    /// Carried items without an equip slot, whose resources run while carried.
    pub carried: Vec<ItemRoot<'u>>,
}

/// The graph.
pub struct Graph<'u> {
    pub(crate) uni: &'u Universe,
    /// Nodes (removed ones stay with `alive == false`).
    pub nodes: Vec<Node<'u>>,
    /// Top-level nodes in order.
    pub roots: Vec<NodeId>,
    pub(crate) ids: IdCounts,
    /// Lower-cased types of the live elements (`[type:spell]` requirement terms).
    pub(crate) types: IdCounts,
    /// Names of the live elements as `eval::feature_name_key` (`[feature:<name>]` terms).
    pub(crate) names: IdCounts,
    /// Live elements with a spellcasting block (`[feature:spellcasting]`).
    pub(crate) spell_blocks: usize,
    pub(crate) class_levels: HashMap<String, i32>,
    pub(crate) mgr_levels: HashMap<Mgr, u32>,
    /// Converged ability hint used by requirements.
    pub abilities: [i32; 6],
    /// Statistic sums of the previous iteration.
    pub stat_sums: HashMap<String, i32>,
    pub(crate) selections: HashMap<SlotKey, (Choice, Option<u32>)>,
    /// Slots whose pick was replaced at least once (they offer current-level options).
    pub(crate) retrained: HashSet<SlotKey>,
    pub(crate) restricted: SourceRestriction,
    pub(crate) equip: EquipState,
    pub(crate) version: u64,
    /// Set when any requirement read a statistic group.
    pub stat_used: std::cell::Cell<bool>,
    pub(crate) slot_cache: RefCell<SlotCache>,
    /// Diagnostics collected while expanding.
    pub diags: DiagSink,
    pub(crate) ruleset_hooks: &'static dyn crate::profile::RulesetProfile,
    /// Foreign-edition content may be offered for new picks (resolved from state and profile).
    pub(crate) allow_cross_edition: bool,
    /// Play-time state (toggles, exhaustion).
    pub(crate) runtime: crate::runtime::Runtime,
    /// Inventory uids whose grants bypass the granted elements' own requirements (U12).
    pub(crate) overridden_items: std::collections::HashSet<String>,
    /// Rolled resource maximums per inventory uid (`InventoryItem.rolled_max`).
    pub(crate) rolled_max: HashMap<String, std::collections::BTreeMap<String, u32>>,
    /// Carried items without an equip slot: not nodes, only their resource rules are read
    /// (`play_rules::resource_snapshots`).
    pub(crate) carried: Vec<ItemRoot<'u>>,
}

const MAX_PASSES: usize = 60;
/// Grant/select nesting limit. Real content nests a handful of levels (level, class, feature,
/// option, granted proficiency); anything deeper is malformed and would only grow the tree.
const MAX_NEST_DEPTH: usize = 64;

type SlotCache = HashMap<(u64, u32), Rc<HashSet<String>>>;

/// The notice for a stored choice whose element is not installed.
fn missing_choice(id: &str) -> Diagnostic {
    Diagnostic::new(
        code::SELECTION_MISSING_CONTENT,
        Severity::Warning,
        format!("chosen element {id} is not installed"),
    )
    .at(id.to_string())
}

impl<'u> Graph<'u> {
    /// Build and expand the graph for a state.
    #[must_use]
    pub fn build(uni: &'u Universe, state: &CharacterState, inputs: GraphInputs<'u>) -> Self {
        let mut mgr_levels = HashMap::new();
        let total = state.level();
        mgr_levels.insert(Mgr::Main, total);
        mgr_levels.insert(Mgr::MainClass, state.class_level(&ClassSlot::Main));
        for l in &state.levels {
            if let ClassSlot::Multi { starting_level } = l {
                mgr_levels.insert(Mgr::Multi(*starting_level), state.class_level(l));
            }
        }
        let selections = state
            .selections
            .iter()
            .map(|s| (s.slot.clone(), (s.choice.clone(), s.replace_level)))
            .collect();
        let mut g = Self {
            uni,
            nodes: Vec::new(),
            roots: Vec::new(),
            ids: IdCounts::default(),
            types: IdCounts::default(),
            names: IdCounts::default(),
            spell_blocks: 0,
            class_levels: HashMap::new(),
            mgr_levels,
            abilities: inputs.abilities,
            stat_sums: inputs.stat_sums,
            selections,
            retrained: state
                .slot_history
                .iter()
                .filter(|h| !h.replaced.is_empty())
                .map(|h| h.slot.clone())
                .collect(),
            restricted: SourceRestriction::excluding(state.restricted_sources.iter().cloned()),
            equip: inputs.equip,
            version: 0,
            stat_used: std::cell::Cell::new(false),
            slot_cache: RefCell::new(HashMap::new()),
            diags: DiagSink::default(),
            ruleset_hooks: crate::profile::profile_for(state.ruleset),
            allow_cross_edition: state.allow_cross_edition.unwrap_or_else(|| {
                crate::profile::profile_for(state.ruleset).default_allow_cross_edition()
            }),
            runtime: crate::runtime::Runtime::from_state(state),
            overridden_items: state
                .inventory
                .items
                .iter()
                .filter(|i| i.override_requirements)
                .map(|i| i.uid.clone())
                .collect(),
            rolled_max: state
                .inventory
                .items
                .iter()
                .filter(|i| !i.rolled_max.is_empty())
                .map(|i| (i.uid.clone(), i.rolled_max.clone()))
                .collect(),
            carried: inputs.carried,
        };
        for opt in &state.options {
            match uni.get(opt) {
                Some(e) => {
                    g.add_root(e, None);
                }
                None => g.diags.push(
                    Diagnostic::new(
                        code::SELECTION_MISSING_CONTENT,
                        Severity::Warning,
                        format!("option {opt} is not installed"),
                    )
                    .at(opt.clone()),
                ),
            }
        }
        for root in uni.creation_roots() {
            if let Some(e) = uni.get(root) {
                g.add_root(e, None);
            }
        }
        for n in 1..=total {
            let id = ids::level_id(n);
            let Some(e) = uni.get(&id) else {
                g.diags.push(
                    Diagnostic::new(
                        code::CONTENT_MISSING_ELEMENT,
                        Severity::Error,
                        format!("level element {id} is missing"),
                    )
                    .at(id),
                );
                continue;
            };
            let node = g.add_root(e, None);
            let is_start = matches!(state.levels.get((n - 1) as usize), Some(ClassSlot::Multi { starting_level }) if *starting_level == n);
            if is_start {
                if let Some(m) = uni.get(&format!("{}{n}", ids::MULTICLASS_LEVEL_PREFIX)) {
                    g.add_child(node, m, Origin::Manual, Mgr::Main);
                }
            }
        }
        for it in inputs.items {
            g.add_root(it.e, Some((it.uid, it.attuned)));
        }
        g.run();
        // choices under content that is gone (a removed race's sub race) have no slot in the
        // graph; they are still the character's and still lost
        for sel in &state.selections {
            if let Choice::Element(id) = &sel.choice {
                if uni.get(id).is_none() && !ids::is_generated_with_class(id) {
                    g.diags.push(missing_choice(id));
                }
            }
        }
        g
    }

    /// A slot whose pick was replaced (or carries an imported retrain level).
    pub(crate) fn is_retrained(&self, key: &SlotKey) -> bool {
        self.retrained.contains(key) || self.selections.get(key).and_then(|s| s.1).is_some()
    }

    /// Manager level (class level for class managers, character level for `Main`).
    #[must_use]
    pub fn mgr_level(&self, m: Mgr) -> u32 {
        self.mgr_levels.get(&m).copied().unwrap_or(0)
    }

    /// Content-supplied profile tables of the universe.
    #[must_use]
    pub fn tables(&self) -> &crate::tables::ProfileTables {
        self.uni.tables()
    }

    /// Total character level.
    #[must_use]
    pub fn total_level(&self) -> u32 {
        self.mgr_level(Mgr::Main)
    }

    /// Count of a live element id.
    #[must_use]
    pub fn count_id(&self, id: &str) -> usize {
        self.ids.count(id)
    }

    /// Requirement environment over all live ids.
    #[must_use]
    pub fn env(&self) -> ReqEnv<'_> {
        ReqEnv {
            ids: &self.ids,
            exclude: None,
            abilities: self.abilities,
            total_level: i32::try_from(self.total_level()).unwrap_or(i32::MAX),
            class_levels: &self.class_levels,
            stats: &self.stat_sums,
            rule_class_level: None,
            stat_used: Some(&self.stat_used),
            types: Some(&self.types),
            names: Some(&self.names),
            spell_blocks: self.spell_blocks > 0,
        }
    }

    fn add_root(&mut self, e: &'u UElem, item: Option<(String, bool)>) -> NodeId {
        let id = self.push_node(e, None, Origin::Root, Mgr::Main);
        if let Some((uid, attuned)) = item {
            self.nodes[id].item_uid = Some(uid);
            self.nodes[id].attuned = attuned;
        }
        self.roots.push(id);
        id
    }

    /// Inventory uid of the item a node belongs to (the item root or one of its grants).
    #[must_use]
    pub fn item_uid_of(&self, node: NodeId) -> Option<&str> {
        let mut at = Some(node);
        while let Some(id) = at {
            if let Some(uid) = self.nodes[id].item_uid.as_deref() {
                return Some(uid);
            }
            at = self.nodes[id].parent;
        }
        None
    }

    /// The stored rolled maximum of resource `resource` on the item of `node`.
    #[must_use]
    pub fn rolled_max_of(&self, node: NodeId, resource: &str) -> Option<u32> {
        self.rolled_max_of_item(self.item_uid_of(node)?, resource)
    }

    /// The stored rolled maximum of resource `resource` on inventory line `uid`.
    #[must_use]
    pub fn rolled_max_of_item(&self, uid: &str, resource: &str) -> Option<u32> {
        self.rolled_max.get(uid)?.get(resource).copied()
    }

    /// Carried items without an equip slot whose resources run while carried.
    #[must_use]
    pub fn carried_items(&self) -> &[ItemRoot<'u>] {
        &self.carried
    }

    fn add_child(&mut self, parent: NodeId, e: &'u UElem, origin: Origin, mgr: Mgr) -> NodeId {
        let id = self.push_node(e, Some(parent), origin, mgr);
        self.nodes[parent].children.push(id);
        if e.repeatable {
            self.nodes[id].instance = Some(self.instance_tag(parent, origin));
        }
        id
    }

    fn push_node(
        &mut self,
        e: &'u UElem,
        parent: Option<NodeId>,
        origin: Origin,
        mgr: Mgr,
    ) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            e,
            parent,
            origin,
            mgr,
            children: Vec::new(),
            slots: Vec::new(),
            alive: true,
            spell_class: None,
            always_prepared: false,
            item_uid: None,
            attuned: false,
            params: std::collections::BTreeMap::new(),
            instance: None,
        });
        self.ids.add(&e.el.id);
        self.types.add(&e.el.ty.to_lowercase());
        self.names.add(&crate::eval::feature_name_key(&e.el.name));
        self.spell_blocks += usize::from(e.el.spellcasting.is_some());
        self.version += 1;
        if matches!(e.el.ty.as_str(), "Class" | "Multiclass") {
            self.refresh_class_levels();
        }
        id
    }

    /// Remove a node and everything below it.
    fn remove_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        let mut touched_class = false;
        while let Some(n) = stack.pop() {
            if !self.nodes[n].alive {
                continue;
            }
            self.nodes[n].alive = false;
            let eid = self.nodes[n].e.el.id.clone();
            self.ids.remove(&eid);
            self.types.remove(&self.nodes[n].e.el.ty.to_lowercase());
            let el = &self.nodes[n].e.el;
            self.names.remove(&crate::eval::feature_name_key(&el.name));
            self.spell_blocks -= usize::from(el.spellcasting.is_some());
            touched_class |= matches!(self.nodes[n].e.el.ty.as_str(), "Class" | "Multiclass");
            stack.extend(self.nodes[n].children.clone());
        }
        if let Some(p) = self.nodes[id].parent {
            self.nodes[p].children.retain(|c| *c != id);
            for s in &mut self.nodes[p].slots {
                if s.chosen == Some(id) {
                    s.chosen = None;
                }
            }
        }
        self.version += 1;
        if touched_class {
            self.refresh_class_levels();
        }
    }

    fn refresh_class_levels(&mut self) {
        self.class_levels.clear();
        for n in &self.nodes {
            if n.alive && matches!(n.e.el.ty.as_str(), "Class" | "Multiclass") {
                let lvl = i32::try_from(self.mgr_level(n.mgr)).unwrap_or(0);
                self.class_levels
                    .insert(n.e.el.name.trim().to_lowercase(), lvl);
            }
        }
    }

    /// Re-run expansion after the hints changed.
    pub fn run(&mut self) {
        for pass in 0..MAX_PASSES {
            let mut changed = false;
            for r in self.roots.clone() {
                changed |= self.process_node(r);
            }
            if !changed {
                return;
            }
            if pass + 1 == MAX_PASSES {
                self.diags.push(Diagnostic::new(
                    code::EVAL_NOT_CONVERGED,
                    Severity::Error,
                    "grant and selection expansion did not reach a fixpoint",
                ));
            }
        }
    }

    /// Counts of the ids in `only` among the live elements `owner` granted (and everything below
    /// them): the owner environment of a grant rule's requirement, which reads no other id.
    fn granted_subtree_counts(&self, owner: NodeId, only: &HashSet<&str>) -> IdCounts {
        let mut c = IdCounts::default();
        if only.is_empty() {
            return c;
        }
        let mut stack: Vec<NodeId> = self.nodes[owner]
            .children
            .iter()
            .copied()
            .filter(|ch| matches!(self.nodes[*ch].origin, Origin::Granted(_)))
            .collect();
        while let Some(n) = stack.pop() {
            if !self.nodes[n].alive {
                continue;
            }
            let id = self.nodes[n].e.el.id.as_str();
            if only.contains(id) {
                c.add(id);
            }
            stack.extend(self.nodes[n].children.iter().copied());
        }
        c
    }

    fn mgr_for_selected(&self, parent: NodeId, target: &UElem) -> Mgr {
        let p = &self.nodes[parent];
        match target.el.ty.as_str() {
            "Class" => Mgr::MainClass,
            "Multiclass" => Mgr::Multi(p.e.level_number().unwrap_or(1)),
            "Class Feature" | "Archetype" | "Archetype Feature" if p.mgr != Mgr::Main => p.mgr,
            _ => Mgr::Main,
        }
    }

    fn process_node(&mut self, id: NodeId) -> bool {
        if !self.nodes[id].alive {
            return false;
        }
        let e = self.nodes[id].e;
        let level = self.mgr_level(self.nodes[id].mgr);
        let mut changed = self.process_select_rules(id, e, level);
        changed |= self.process_grants(id, e, level);
        changed |= self.process_slots(id, e);
        for c in self.nodes[id].children.clone() {
            changed |= self.process_node(c);
        }
        changed
    }

    fn slot_key(&self, id: NodeId, rule: usize, ordinal: u32) -> SlotKey {
        let n = &self.nodes[id];
        SlotKey {
            owner: n.e.el.id.clone(),
            rule: u32::try_from(rule).unwrap_or(u32::MAX),
            ordinal,
            instance: n.item_uid.clone().or_else(|| n.instance.clone()),
        }
    }

    /// Where a repeatable element came from: the granting rule or the filled slot of its parent.
    fn instance_tag(&self, parent: NodeId, origin: Origin) -> String {
        let p = &self.nodes[parent];
        let base = match origin {
            Origin::Selected(si) => p.slots.get(si).map_or_else(
                || format!("{}:s{si}", p.e.el.id),
                |s| format!("{}:{}:{}", s.key.owner, s.key.rule, s.key.ordinal),
            ),
            Origin::Granted(ri) => format!("{}:g{ri}", p.e.el.id),
            Origin::Root | Origin::Manual => p.e.el.id.clone(),
        };
        match &p.instance {
            Some(i) => format!("{base}@{i}"),
            None => base,
        }
    }

    /// Value of a named parameter of an element instance: preset by `grant_feat` or chosen in
    /// its `select_param` slot.
    #[must_use]
    pub fn param_value(&self, node: NodeId, key: &str) -> Option<String> {
        let n = &self.nodes[node];
        if let Some(v) = n.params.get(key) {
            return Some(v.clone());
        }
        n.slots.iter().find_map(|sl| {
            let Rule::Select(r) = &n.e.el.rules[sl.rule] else {
                return None;
            };
            let is_param = crate::custom_rules::kind_of(r) == Some("param")
                && r.extra
                    .attributes
                    .get(crate::custom_rules::X_KEY)
                    .map(String::as_str)
                    == Some(key);
            let choice = sl.list_choice.filter(|_| is_param)?;
            r.items
                .iter()
                .find(|i| i.id == choice)
                .map(|i| i.text.clone())
        })
    }

    fn preset_param(&self, node: NodeId, rule: &borealis_content::SelectRule) -> bool {
        crate::custom_rules::kind_of(rule) == Some("param")
            && rule
                .extra
                .attributes
                .get(crate::custom_rules::X_KEY)
                .is_some_and(|k| self.nodes[node].params.contains_key(k))
    }

    /// May list entry `item` be chosen in a rewritten `select_param` slot? With
    /// `distinct_per_instance` another instance of the same element may not hold it.
    pub(crate) fn list_item_allowed(&self, owner: NodeId, rule_index: usize, item: i64) -> bool {
        let n = &self.nodes[owner];
        let Rule::Select(r) = &n.e.el.rules[rule_index] else {
            return false;
        };
        let Some(entry) = r.items.iter().find(|i| i.id == item) else {
            return false;
        };
        let distinct = crate::custom_rules::kind_of(r) == Some("param")
            && r.extra
                .attributes
                .contains_key(crate::custom_rules::X_DISTINCT);
        if !distinct {
            return true;
        }
        let Some(key) = r.extra.attributes.get(crate::custom_rules::X_KEY) else {
            return true;
        };
        !self.nodes.iter().enumerate().any(|(i, o)| {
            i != owner
                && o.alive
                && o.e.el.id == n.e.el.id
                && self.param_value(i, key).as_deref() == Some(entry.text.as_str())
        })
    }

    /// Does a live element carry this feature name (`Fighting Style`, `Spellcasting`)? The same
    /// check as the `[feature:<name>]` requirement term.
    fn has_feature_named(&self, name: &str) -> bool {
        self.env().has_feature(name)
    }

    /// Structured feat prerequisites (`data.prerequisite`) of a candidate.
    pub(crate) fn prereq_holds(&self, cand: &UElem) -> bool {
        let p = &cand.prereq;
        if p.level_min.is_some_and(|l| self.total_level() < l) {
            return false;
        }
        if !p.abilities_any.is_empty()
            && !p
                .abilities_any
                .iter()
                .any(|(i, min)| self.abilities[*i] >= *min)
        {
            return false;
        }
        match p.requires_feature.as_deref() {
            // "Spellcasting" is the 2024 "Spellcasting or Pact Magic feature" rule: strict class
            // spell slots, so a feat or species spellcasting block does not satisfy it (B-23)
            Some(f) if f.trim().eq_ignore_ascii_case("spellcasting") => {
                self.env().has_class_spell_slots()
            }
            Some(f) => self.has_feature_named(f),
            None => true,
        }
    }

    /// Stat rules produced by the chosen entries of rewritten `ability_*` list selects.
    fn list_effect_rules(&self, node: NodeId) -> Vec<&'u UStat> {
        let n = &self.nodes[node];
        let mut out = Vec::new();
        for (ri, entries) in &n.e.list_effects {
            let chosen = n
                .slots
                .iter()
                .find(|s| s.rule == *ri)
                .and_then(|s| s.list_choice);
            if let Some((_, stats)) = chosen.and_then(|c| entries.iter().find(|(id, _)| *id == c)) {
                out.extend(stats.iter());
            }
        }
        out
    }

    fn process_select_rules(&mut self, id: NodeId, e: &'u UElem, level: u32) -> bool {
        let mut changed = false;
        for (ri, rule) in e.el.rules.iter().enumerate() {
            let Rule::Select(s) = rule else { continue };
            let URule::Select { req, .. } = &e.rules[ri] else {
                continue;
            };
            let active = s.level.unwrap_or(1) <= level
                && requirement_holds(req, &self.env())
                && !self.preset_param(id, s);
            let has = self.nodes[id].slots.iter().any(|sl| sl.rule == ri);
            if active && crate::custom_rules::kind_of(s) == Some("mastery") {
                changed |= self.resize_mastery_slots(id, ri, s);
                continue;
            }
            if active && !has {
                for ord in 1..=s.number.unwrap_or(1).max(1) {
                    let key = self.slot_key(id, ri, ord);
                    self.nodes[id].slots.push(SlotNode {
                        key,
                        rule: ri,
                        ordinal: ord,
                        chosen: None,
                        list_choice: None,
                        rejected: None,
                        missing: None,
                    });
                }
                changed = true;
            } else if !active && has {
                let chosen: Vec<NodeId> = self.nodes[id]
                    .slots
                    .iter()
                    .filter(|sl| sl.rule == ri)
                    .filter_map(|sl| sl.chosen)
                    .collect();
                self.nodes[id].slots.retain(|sl| sl.rule != ri);
                for c in chosen {
                    self.remove_subtree(c);
                }
                changed = true;
            }
        }
        changed
    }

    /// Grow or shrink the Weapon Mastery slots of a rewritten `weapon_mastery` select to the
    /// class table value at the current class level.
    fn resize_mastery_slots(
        &mut self,
        id: NodeId,
        ri: usize,
        s: &borealis_content::rules::SelectRule,
    ) -> bool {
        let want = crate::play_rules::mastery_slot_count(self, id, s).max(1);
        let have = u32::try_from(
            self.nodes[id]
                .slots
                .iter()
                .filter(|sl| sl.rule == ri)
                .count(),
        )
        .unwrap_or(0);
        if have == want {
            return false;
        }
        if have < want {
            for ord in have + 1..=want {
                let key = self.slot_key(id, ri, ord);
                self.nodes[id].slots.push(SlotNode {
                    key,
                    rule: ri,
                    ordinal: ord,
                    chosen: None,
                    list_choice: None,
                    rejected: None,
                    missing: None,
                });
            }
            return true;
        }
        let dropped: Vec<NodeId> = self.nodes[id]
            .slots
            .iter()
            .filter(|sl| sl.rule == ri && sl.ordinal > want)
            .filter_map(|sl| sl.chosen)
            .collect();
        self.nodes[id]
            .slots
            .retain(|sl| !(sl.rule == ri && sl.ordinal > want));
        for c in dropped {
            self.remove_subtree(c);
        }
        true
    }

    /// `requirements_param` of a `grant_spell`: every named parameter of the owner has the value.
    fn param_gate_holds(&self, id: NodeId, e: &UElem, ri: usize) -> bool {
        let Rule::Grant(g) = &e.el.rules[ri] else {
            return true;
        };
        let Some(text) = g.extra.attributes.get(crate::custom_rules::X_REQ_PARAM) else {
            return true;
        };
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text)
        else {
            return false;
        };
        map.iter().all(|(k, v)| {
            let key = crate::custom_rules::param_key(k);
            self.param_value(id, &key)
                .zip(v.as_str())
                .is_some_and(|(have, want)| have.eq_ignore_ascii_case(want))
        })
    }

    fn grant_rule_holds(&self, id: NodeId, e: &UElem, ri: usize) -> bool {
        let URule::Grant { req } = &e.rules[ri] else {
            return false;
        };
        if !self.param_gate_holds(id, e, ri) {
            return false;
        }
        if matches!(req, Cond::None) {
            return true;
        }
        if ri < e.copied_rules {
            return requirement_holds(req, &self.env());
        }
        let ex = self.granted_subtree_counts(id, &requirement_ids(req));
        let mut env = self.env();
        env.exclude = Some(&ex);
        requirement_holds(req, &env)
    }

    /// Does node `id` grant without checking its targets' own requirements? Only the root of an
    /// item line with "Override requirements" on (U12): what those targets grant in turn is
    /// checked as usual, and the grant rule's own `requirements` and level still apply.
    fn bypasses_target_requirements(&self, id: NodeId) -> bool {
        self.nodes[id]
            .item_uid
            .as_deref()
            .is_some_and(|uid| self.overridden_items.contains(uid))
    }

    /// The targets the root of item line `uid` grants by rules that hold, that exist and that
    /// are not applied because their own requirements fail (U12, issue 3): what "Override
    /// requirements" would apply. Empty when the line is not equipped or nothing is withheld.
    #[must_use]
    pub fn withheld_item_grants(&self, uid: &str) -> Vec<&'u UElem> {
        let mut out: Vec<&'u UElem> = Vec::new();
        for (id, n) in self.nodes.iter().enumerate() {
            if !n.alive || n.item_uid.as_deref() != Some(uid) {
                continue;
            }
            let e = n.e;
            let level = self.mgr_level(n.mgr);
            for (ri, rule) in e.el.rules.iter().enumerate() {
                let Rule::Grant(g) = rule else { continue };
                if g.level.unwrap_or(1) > level || !self.grant_rule_holds(id, e, ri) {
                    continue;
                }
                let Some(target) = self.uni.get(&g.id) else {
                    continue;
                };
                if matches!(target.req, Cond::None) || out.iter().any(|t| t.el.id == target.el.id) {
                    continue;
                }
                let applied = n.children.iter().any(|c| {
                    self.nodes[*c].alive
                        && matches!(self.nodes[*c].origin, Origin::Granted(_))
                        && self.nodes[*c].e.el.id == target.el.id
                });
                if !applied {
                    out.push(target);
                }
            }
        }
        out
    }

    fn process_grants(&mut self, id: NodeId, e: &'u UElem, level: u32) -> bool {
        let mut changed = false;
        let bypass = self.bypasses_target_requirements(id);
        // Pass A: revalidate existing grants
        for c in self.nodes[id].children.clone() {
            let Origin::Granted(ri) = self.nodes[c].origin else {
                continue;
            };
            if !self.nodes[c].alive {
                continue;
            }
            let Rule::Grant(g) = &e.el.rules[ri] else {
                continue;
            };
            let ok = g.level.unwrap_or(1) <= level
                && self.grant_rule_holds(id, e, ri)
                && (bypass || requirement_holds(&self.nodes[c].e.req, &self.env()));
            if !ok {
                self.remove_subtree(c);
                changed = true;
            }
        }
        // Pass B: new grants
        for (ri, rule) in e.el.rules.iter().enumerate() {
            let Rule::Grant(g) = rule else { continue };
            if g.level.unwrap_or(1) > level || !self.grant_rule_holds(id, e, ri) {
                continue;
            }
            let Some(target) = self.uni.get(&g.id) else {
                self.diags.push(
                    Diagnostic::new(
                        code::CONTENT_MISSING_ELEMENT,
                        Severity::Warning,
                        format!("{} grants unknown element {}", e.el.id, g.id),
                    )
                    .at(e.el.id.clone()),
                );
                continue;
            };
            if !bypass && !requirement_holds(&target.req, &self.env()) {
                continue;
            }
            let already = self.nodes[id].children.iter().any(|c| {
                self.nodes[*c].alive
                    && matches!(self.nodes[*c].origin, Origin::Granted(_))
                    && self.nodes[*c].e.el.id == target.el.id
            });
            if already || self.nesting_refused(id, target, "grants") {
                continue;
            }
            let mgr = self.nodes[id].mgr;
            let c = self.add_child(id, target, Origin::Granted(ri), mgr);
            self.nodes[c].params = crate::custom_rules::preset_params(g);
            self.nodes[c].spell_class = g.spellcasting.clone();
            self.nodes[c].always_prepared = g.prepared.unwrap_or(false);
            changed = true;
        }
        changed
    }

    /// Would adding `target` below node `id` repeat an element of `id`'s own chain (a grant or
    /// selection cycle, e.g. an element that grants itself), or nest past [`MAX_NEST_DEPTH`]?
    /// Malformed content must never recurse without end, so such a child is not added and a
    /// `content.grant_cycle` diagnostic names the chain.
    fn nesting_refused(&mut self, id: NodeId, target: &UElem, verb: &str) -> bool {
        let mut chain: Vec<&str> = Vec::new();
        let mut repeat = false;
        let mut cur = Some(id);
        while let Some(n) = cur {
            let eid = self.nodes[n].e.el.id.as_str();
            chain.push(eid);
            repeat |= eid == target.el.id;
            cur = self.nodes[n].parent;
        }
        if !repeat && chain.len() < MAX_NEST_DEPTH {
            return false;
        }
        let owner = chain[0].to_string();
        let message = if repeat {
            let upto = chain.iter().position(|x| *x == target.el.id).unwrap_or(0);
            let mut path: Vec<&str> = chain[..=upto].to_vec();
            path.reverse();
            format!(
                "{owner} {verb} {}, which is already in its own chain ({}); not expanded again",
                target.el.id,
                path.join(" > ")
            )
        } else {
            format!(
                "{owner} {verb} {} more than {MAX_NEST_DEPTH} levels deep; not expanded",
                target.el.id
            )
        };
        self.diags
            .push(Diagnostic::new(code::CONTENT_GRANT_CYCLE, Severity::Warning, message).at(owner));
        true
    }

    fn process_slots(&mut self, id: NodeId, e: &'u UElem) -> bool {
        let mut changed = false;
        for si in 0..self.nodes[id].slots.len() {
            let (key, ri, chosen) = {
                let s = &self.nodes[id].slots[si];
                (s.key.clone(), s.rule, s.chosen)
            };
            let Rule::Select(rule) = &e.el.rules[ri] else {
                continue;
            };
            let stored = self.selections.get(&key).cloned();
            if rule.is_list() {
                let item = match stored {
                    Some((Choice::ListItem(i), _)) if self.list_item_allowed(id, ri, i) => Some(i),
                    _ => None,
                };
                self.nodes[id].slots[si].list_choice = item;
                continue;
            }
            let wanted = match &stored {
                Some((Choice::Element(eid), _)) => Some(eid.clone()),
                _ => None,
            };
            if let Some(c) = chosen {
                let cur = self.nodes[c].e.el.id.clone();
                let keep = wanted.as_deref() == Some(cur.as_str())
                    && self.choice_allowed(id, ri, self.nodes[c].e, true);
                if !keep {
                    if wanted.as_deref() == Some(cur.as_str()) {
                        self.nodes[id].slots[si].rejected = Some(Choice::Element(cur));
                    }
                    self.remove_subtree(c);
                    changed = true;
                }
                if keep {
                    continue;
                }
            }
            self.nodes[id].slots[si].missing = None;
            let Some(wanted) = wanted else {
                self.nodes[id].slots[si].rejected = None;
                continue;
            };
            let Some(target) = self.uni.get(&wanted) else {
                self.nodes[id].slots[si].rejected = None;
                self.nodes[id].slots[si].missing = Some(wanted.clone());
                if !ids::is_generated_with_class(&wanted) {
                    self.diags.push(missing_choice(&wanted));
                }
                continue;
            };
            if self.nesting_refused(id, target, "selects") {
                self.nodes[id].slots[si].rejected = Some(Choice::Element(wanted));
                continue;
            }
            if self.restore_allowed(id, ri, target) {
                let mgr = self.mgr_for_selected(id, target);
                let c = self.add_child(id, target, Origin::Selected(si), mgr);
                self.nodes[c].spell_class = rule.spellcasting.clone();
                self.nodes[c].always_prepared = rule.prepared.unwrap_or(false);
                self.nodes[id].slots[si].chosen = Some(c);
                self.nodes[id].slots[si].rejected = None;
                changed = true;
            } else {
                self.nodes[id].slots[si].rejected = Some(Choice::Element(wanted));
            }
        }
        changed
    }

    /// May a stored choice be restored? Aurora re-validates a choice while it is registered, so
    /// the choice's own id counts as held (`engine.md` 3.2, all character ids): a requirement
    /// like `[points:2]|<own id>`, which spends the points it checks, keeps the pick once made.
    /// The keep check of a restored choice sees it the same way, so restore and keep agree (else
    /// every rebuild of the graph undoes and redoes the pick and the evaluation never settles).
    fn restore_allowed(&mut self, owner: NodeId, rule_index: usize, cand: &'u UElem) -> bool {
        self.ids.add(&cand.el.id);
        let ok = self.choice_allowed(owner, rule_index, cand, true);
        self.ids.remove(&cand.el.id);
        ok
    }

    /// May `cand` be offered for a NEW pick? Strict mode hides foreign-edition content; stored
    /// choices are replayed and only need the profile to accept the element.
    pub(crate) fn offers_edition(&self, cand: &UElem) -> bool {
        cand.edition.is_accepted()
            && !cand.superseded
            && (self.allow_cross_edition || !cand.edition.is_foreign())
    }

    /// Is `cand` a foreign option chosen by a select of (or below) a foreign element the
    /// character already uses from the same edition (the lineage or spellcasting-ability choice
    /// of a 2024 species in a 2014 character, accepted as a species or through its
    /// `legacy_alias`)? It belongs to that element and is accepted and offered with it, whatever
    /// the profile says about its own type and whether the other edition is allowed for new picks.
    fn inherits_owner_acceptance(&self, owner: NodeId, cand: &UElem) -> bool {
        let EditionStatus::Foreign { from, .. } = &cand.edition else {
            return false;
        };
        let mut at = Some(owner);
        while let Some(n) = at {
            let e = self.nodes[n].e;
            // IM14: under content the character kept although its book's edition changed, the
            // same book's options stay usable (a kept class's features)
            if e.edition_set_by_user && !e.edition.is_accepted() {
                return cand.edition_set_by_user
                    && matches!(&e.edition, EditionStatus::Foreign { from: f, .. } if f == from);
            }
            if e.legacy_accepted || (e.edition.is_foreign() && e.edition.is_accepted()) {
                return matches!(&e.edition, EditionStatus::Foreign { from: f, .. } if f == from);
            }
            at = self.nodes[n].parent;
        }
        false
    }

    /// Does the candidate pass the slot's option filter (`engine.md` 6.2)? `existing` skips the
    /// source restriction and duplicate removal: stored choices are replayed as they are (a
    /// restricted source or a duplicate granted elsewhere never unregisters a choice, only the
    /// type, supports, ruleset and requirement checks do).
    pub(crate) fn choice_allowed(
        &self,
        owner: NodeId,
        rule_index: usize,
        cand: &UElem,
        existing: bool,
    ) -> bool {
        let dyns = SpellDynamics::new(self, owner, rule_index);
        self.choice_allowed_with(owner, rule_index, cand, existing, &dyns)
    }

    /// [`Self::choice_allowed`] with a prepared spell token context for the same owner and rule
    /// (an option list builds it once for all candidates).
    pub(crate) fn choice_allowed_with(
        &self,
        owner: NodeId,
        rule_index: usize,
        cand: &UElem,
        existing: bool,
        dyns: &SpellDynamics<'_, 'u>,
    ) -> bool {
        self.choice_allowed_inner(owner, rule_index, cand, existing, dyns, false)
    }

    /// [`Self::choice_allowed_with`]; `held_ok` lets an element the character already has pass
    /// the duplicate check (what the slot would offer to a character without it).
    pub(crate) fn choice_allowed_inner(
        &self,
        owner: NodeId,
        rule_index: usize,
        cand: &UElem,
        existing: bool,
        dyns: &SpellDynamics<'_, 'u>,
        held_ok: bool,
    ) -> bool {
        self.choice_allowed_relaxed(
            owner,
            rule_index,
            cand,
            existing,
            dyns,
            held_ok,
            Relax::NONE,
        )
    }

    /// [`Self::choice_allowed_inner`] with some filters switched off: would `cand` be offered if
    /// the edition (`relax.edition`) or the source restriction (`relax.restriction`) did not hide
    /// it? Only the empty-slot explanation asks (U14).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn choice_allowed_relaxed(
        &self,
        owner: NodeId,
        rule_index: usize,
        cand: &UElem,
        existing: bool,
        dyns: &SpellDynamics<'_, 'u>,
        held_ok: bool,
        relax: Relax,
    ) -> bool {
        let n = &self.nodes[owner];
        let (Rule::Select(s), URule::Select { supports, .. }) =
            (&n.e.el.rules[rule_index], &n.e.rules[rule_index])
        else {
            return false;
        };
        let alternative = crate::custom_rules::alternatives_of(s).contains(&cand.el.id.as_str());
        let inherited = self.inherits_owner_acceptance(owner, cand);
        let type_ok = alternative
            || crate::universe::type_fits(&s.ty, &cand.el.ty)
            || cand.el.legacy_alias.as_deref() == Some(s.ty.as_str())
            || (cand.edition.is_foreign()
                && crate::universe::cross_edition_types(&s.ty).contains(&cand.el.ty.as_str()));
        // IM14: a stored choice of a book whose edition the user set is replayed whatever the
        // edition (the character took it before the change); new picks follow R4
        let kept = existing && cand.edition_set_by_user;
        if !type_ok || !(cand.edition.is_accepted() || inherited || kept || relax.edition) {
            return false;
        }
        if !existing {
            // a superseded foreign element (its native equivalent replaces it) is still one a
            // character of its own edition uses: the relaxed check counts it
            let offered = if relax.edition {
                true
            } else if inherited {
                !cand.superseded
            } else {
                self.offers_edition(cand)
            };
            if !offered {
                return false;
            }
            if !relax.restriction
                && self
                    .restricted
                    .is_restricted_element(&cand.el.source, &cand.el.ty)
            {
                return false;
            }
        }
        // the supports filter before the duplicate check: most candidates of a long list fail it,
        // and every check up to the requirement is a pure test, so the order does not change the
        // answer (the requirement, which notes statistic reads, stays last)
        let supported = dyns
            .replaced_known_spell(s, cand)
            .unwrap_or_else(|| supports_cond_match(supports, cand, dyns));
        if !alternative && !supported {
            return false;
        }
        if !existing {
            let mastery = crate::custom_rules::kind_of(s) == Some("mastery");
            if !mastery
                && !held_ok
                && !cand.allow_duplicate
                && self.held_or_equivalent(&cand.el.id)
                && !self.repicks_known_spell(s, cand)
            {
                return false;
            }
        }
        if crate::custom_rules::kind_of(s) == Some("mastery")
            && !crate::play_rules::mastery_allows(self, s, cand)
        {
            return false;
        }
        if relax.requirements {
            return true;
        }
        if !self.prereq_holds(cand) {
            return false;
        }
        let mut env = self.env();
        env.rule_class_level =
            (n.mgr != Mgr::Main).then(|| i32::try_from(self.mgr_level(n.mgr)).unwrap_or(0));
        requirement_holds(&cand.req, &env)
    }

    /// The character has `id` or a `maps_to` equivalent of it (the duplicate check
    /// of `engine.md` 6.2 step 4 is by element; two ids of one element are one element).
    fn held_or_equivalent(&self, id: &str) -> bool {
        self.ids.count(id) > 0
            || self
                .uni
                .equivalent_ids(id)
                .iter()
                .any(|x| self.ids.count(x) > 0)
    }

    /// A `prepared` spell select of a spellcasting class may choose a spell that class already
    /// knows and does not yet have always prepared (Spell Mastery and Signature Spells choose
    /// from the spellbook): the pick makes it always prepared, it is no duplicate.
    fn repicks_known_spell(&self, s: &borealis_content::SelectRule, cand: &UElem) -> bool {
        let (Some(true), Some(class)) = (s.prepared, s.spellcasting.as_deref()) else {
            return false;
        };
        if cand.el.ty != "Spell" {
            return false;
        }
        let mut known = false;
        for n in self
            .nodes
            .iter()
            .filter(|n| n.alive && n.e.el.id == cand.el.id)
        {
            if n.always_prepared {
                return false;
            }
            known |= n.spell_class.as_deref() == Some(class);
        }
        known
    }

    /// Nodes in pre-order (roots in order, children in insertion order), live only.
    #[must_use]
    pub fn preorder(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            if !self.nodes[n].alive {
                continue;
            }
            out.push(n);
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    /// Equipment state with counts-as grants filled from the live ids.
    #[must_use]
    pub fn equip_state(&self) -> EquipState {
        let mut e = self.equip.clone();
        e.counts_any = self.ids.count(ids::COUNTS_AS_ARMOR) > 0;
        e.counts_kind = [
            self.ids.count(ids::COUNTS_AS_LIGHT) > 0,
            self.ids.count(ids::COUNTS_AS_MEDIUM) > 0,
            self.ids.count(ids::COUNTS_AS_HEAVY) > 0,
        ];
        e
    }

    /// Active stat rules in pre-order. `at_level` replaces every manager level (used for
    /// multiclass and spell level lookups).
    #[must_use]
    pub fn active_stat_rules(&self, at_level: Option<u32>) -> Vec<&'u UStat> {
        self.stat_rules(at_level, None)
    }

    /// Active stat rules plus the considered-but-inactive ones with the reason (provenance).
    #[must_use]
    pub fn stat_rules_with_skipped(
        &self,
        at_level: Option<u32>,
    ) -> (Vec<&'u UStat>, Vec<(&'u UStat, String)>) {
        let mut skipped = Vec::new();
        let active = self.stat_rules(at_level, Some(&mut skipped));
        (active, skipped)
    }

    fn stat_rules(
        &self,
        at_level: Option<u32>,
        mut skipped: Option<&mut Vec<(&'u UStat, String)>>,
    ) -> Vec<&'u UStat> {
        let eq = self.equip_state();
        let env = self.env();
        let table = matches!(
            self.ruleset_hooks.proficiency_source(),
            crate::profile::ProficiencySource::Table(_)
        );
        let mut out = Vec::new();
        for id in self.preorder() {
            let n = &self.nodes[id];
            let lvl = at_level.unwrap_or_else(|| self.mgr_level(n.mgr));
            let own = n.e.rules.iter().filter_map(|r| match r {
                URule::Stat(st) => Some(&**st),
                _ => None,
            });
            let toggled = crate::play_rules::active_toggle_stats(self, id);
            for st in own.chain(self.list_effect_rules(id)).chain(toggled) {
                if st.inline {
                    continue;
                }
                let why = if st.level > lvl {
                    Some(format!("needs level {}, level here is {lvl}", st.level))
                } else if table && st.name == "proficiency" && n.e.el.ty == "Level" {
                    Some("replaced by the ruleset's proficiency table".to_string())
                } else if !crate::eval::equipped_holds(&st.equipped, &eq) {
                    Some("equipped condition is false".to_string())
                } else if !requirement_holds(&st.req, &env) {
                    Some("requirements are not met".to_string())
                } else {
                    None
                };
                match (why, skipped.as_deref_mut()) {
                    (None, _) => out.push(st),
                    (Some(w), Some(sink)) => sink.push((st, w)),
                    (Some(_), None) => {}
                }
            }
        }
        out.extend(crate::play_rules::exhaustion_stats(self));
        out
    }

    /// Names of stat groups with an active rule at a character-wide level (cached).
    #[must_use]
    pub fn groups_at_level(&self, level: u32) -> Rc<HashSet<String>> {
        let key = (self.version, level);
        if let Some(hit) = self.slot_cache.borrow().get(&key) {
            return Rc::clone(hit);
        }
        let names: HashSet<String> = self
            .active_stat_rules(Some(level))
            .iter()
            .map(|r| r.name.clone())
            .collect();
        let rc = Rc::new(names);
        let mut cache = self.slot_cache.borrow_mut();
        cache.retain(|k, _| k.0 == self.version);
        cache.insert(key, Rc::clone(&rc));
        rc
    }

    /// Highest spell level with a slot group at `level` for a spellcasting name (0 = none).
    #[must_use]
    pub fn highest_slot_level(&self, class: &str, level: u32) -> u32 {
        let names = self.groups_at_level(level);
        let lc = class.to_lowercase();
        (1..=9)
            .rev()
            .find(|n| names.contains(&format!("{lc}:spellcasting:slots:{n}")))
            .unwrap_or(0)
    }

    /// Live nodes of an element type.
    pub fn nodes_of_type<'a>(
        &'a self,
        ty: &'a str,
    ) -> impl Iterator<Item = (NodeId, &'a Node<'u>)> + 'a {
        self.nodes
            .iter()
            .enumerate()
            .filter(move |(_, n)| n.alive && n.e.el.ty == ty)
    }

    /// Does any live node carry this element id?
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.ids.count(id) > 0
    }
}

/// Spellcasting block of a live node.
#[derive(Debug, Clone)]
pub struct SpellInfo<'u> {
    /// Owning node.
    pub node: NodeId,
    /// The block.
    pub sc: &'u borealis_content::Spellcasting,
    /// Spellcasting name.
    pub name: String,
}

impl<'u> Graph<'u> {
    /// All spellcasting blocks of live nodes.
    #[must_use]
    pub fn spell_infos(&self) -> Vec<SpellInfo<'u>> {
        self.preorder()
            .into_iter()
            .filter_map(|id| {
                let sc = self.nodes[id].e.el.spellcasting.as_ref()?;
                Some(SpellInfo {
                    node: id,
                    sc,
                    name: sc.name.clone().unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// Dynamic `$(spellcasting:...)` tokens for one select rule.
///
/// The graph is borrowed immutably, so what the tokens read from it (the highest slot level, the
/// class spell lists) is computed once on first use and reused for every candidate.
pub struct SpellDynamics<'g, 'u> {
    graph: &'g Graph<'u>,
    class: Option<String>,
    level: u32,
    /// A slot of the rule was reopened by a spell replacement (`replace_spell`).
    retrained: bool,
    owner: Option<NodeId>,
    slot_level: OnceCell<u32>,
    lists: OnceCell<ClassSpellLists<'u>>,
}

/// The spell lists of the live spellcasting blocks that serve one spellcasting name.
struct ClassSpellLists<'u> {
    /// Single spell ids (`is_spell_id` entries), trimmed.
    ids: HashSet<&'u str>,
    /// Parsed expression entries (unparsable or empty entries never match).
    exprs: Vec<SupportsExpr>,
}

impl<'g, 'u> SpellDynamics<'g, 'u> {
    /// Context for rule `rule_index` of `owner`.
    #[must_use]
    pub fn new(graph: &'g Graph<'u>, owner: NodeId, rule_index: usize) -> Self {
        let n = &graph.nodes[owner];
        let (class, rule_level) = match &n.e.el.rules[rule_index] {
            Rule::Select(s) => (s.spellcasting.clone(), s.level.unwrap_or(1)),
            _ => (None, 1),
        };
        // a retrained slot uses the current class level (spell swap)
        let retrained = n
            .slots
            .iter()
            .filter(|s| s.rule == rule_index)
            .any(|s| graph.is_retrained(&s.key));
        let level = if retrained {
            graph.mgr_level(n.mgr).max(rule_level)
        } else {
            rule_level
        };
        Self {
            graph,
            class,
            level,
            retrained,
            owner: Some(owner),
            slot_level: OnceCell::new(),
            lists: OnceCell::new(),
        }
    }

    /// Context with explicit class and level.
    #[must_use]
    pub fn with(graph: &'g Graph<'u>, class: Option<String>, level: u32) -> Self {
        Self {
            graph,
            class,
            level,
            retrained: false,
            owner: None,
            slot_level: OnceCell::new(),
            lists: OnceCell::new(),
        }
    }

    /// Highest spell level with a slot group for the class at this context's level.
    fn slot_level(&self, class: &str) -> u32 {
        *self
            .slot_level
            .get_or_init(|| self.graph.highest_slot_level(class, self.level))
    }

    /// The lists of every live block serving `class` (a block of that name, or an `extend`
    /// block for it or for all names).
    fn lists(&self, class: &str) -> &ClassSpellLists<'u> {
        self.lists.get_or_init(|| {
            let mut out = ClassSpellLists {
                ids: HashSet::new(),
                exprs: Vec::new(),
            };
            for info in self.graph.spell_infos().iter().filter(|i| {
                i.sc.extend != Some(true) && i.name == class
                    || i.sc.extend == Some(true)
                        && (i.sc.all == Some(true) || i.name.eq_ignore_ascii_case(class))
            }) {
                for l in info.sc.list.iter().chain(info.sc.extends.iter()) {
                    if l.is_spell_id() {
                        out.ids.insert(l.supports.trim());
                    } else if let Ok(Some(expr)) = l.expr() {
                        out.exprs.push(expr);
                    }
                }
            }
            out
        })
    }

    /// A replaced known spell of a select authored as `$(spellcasting:list),<spell levels>`
    /// (the SRD packs write the level filter out, Aurora writes `$(spellcasting:slots)`): the
    /// new spell may be of any level the class has slots for at its current level (SRD 5.1
    /// Warlock, Sorcerer, Bard, Ranger: "another spell ... which also must be of a level for
    /// which you have spell slots"), as Aurora's `$(spellcasting:slots)` evaluates. `None` when
    /// the select is not such a reopened slot (the authored supports apply).
    fn replaced_known_spell(
        &self,
        rule: &borealis_content::SelectRule,
        element: &UElem,
    ) -> Option<bool> {
        if !self.retrained || !literal_level_spell_select(rule.supports.as_deref().unwrap_or("")) {
            return None;
        }
        let class = self.class.as_ref()?;
        let h = self.slot_level(class);
        Some(
            self.list_matches(element)
                && element.spell_level().is_some_and(|l| (1..=h).contains(&l)),
        )
    }

    fn list_matches(&self, element: &UElem) -> bool {
        let Some(class) = &self.class else {
            return false;
        };
        let lists = self.lists(class);
        if lists.ids.contains(element.el.id.as_str()) {
            let spell_level = element.spell_level().unwrap_or(0);
            if spell_level != 0 && spell_level <= self.slot_level(class).max(1) {
                return true;
            }
        }
        lists
            .exprs
            .iter()
            .any(|e| crate::eval::supports_match(e, element, &crate::eval::NoDynamics))
    }
}

/// `$(spellcasting:list),1` / `$(spellcasting:list),(1||2)`: a class list and leveled spell
/// levels only (no cantrips, no other terms).
fn literal_level_spell_select(supports: &str) -> bool {
    let mut parts = supports.split(',');
    if parts.next().map(str::trim) != Some("$(spellcasting:list)") {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    !rest.is_empty()
        && rest.iter().all(|p| {
            let levels: Vec<&str> = p
                .split("||")
                .map(|t| t.trim().trim_matches(|c| c == '(' || c == ')').trim())
                .collect();
            levels
                .iter()
                .all(|t| t.parse::<u32>().is_ok_and(|n| (1..=9).contains(&n)))
        })
}

impl DynamicSupports for SpellDynamics<'_, '_> {
    fn matches(&self, d: &Dynamic, element: &UElem) -> bool {
        match d {
            Dynamic::SpellcastingList => self.list_matches(element),
            Dynamic::SpellcastingSlots => {
                let Some(class) = &self.class else {
                    return false;
                };
                let h = self.slot_level(class);
                element.spell_level().is_some_and(|l| (1..=h).contains(&l))
            }
            Dynamic::Other(token) => {
                // `$(param:<Name>)`: the value chosen by a `select_param` of the same instance
                // is a support tag of the candidate (a spell list name)
                let Some(name) = token.trim().strip_prefix("param:") else {
                    return false;
                };
                let key = crate::custom_rules::param_key(name);
                self.owner
                    .and_then(|o| self.graph.param_value(o, &key))
                    .is_some_and(|v| element.supports.contains(&v))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_level_spell_selects() {
        assert!(literal_level_spell_select("$(spellcasting:list),1"));
        assert!(literal_level_spell_select("$(spellcasting:list),(1||2||3)"));
        assert!(!literal_level_spell_select("$(spellcasting:list),0"));
        assert!(!literal_level_spell_select(
            "$(spellcasting:list),$(spellcasting:slots)"
        ));
        assert!(!literal_level_spell_select("$(spellcasting:list)"));
        assert!(!literal_level_spell_select("Warlock,1"));
    }

    #[test]
    fn mgr_equality() {
        assert_ne!(Mgr::Multi(3), Mgr::Multi(4));
        assert_eq!(Mgr::Main, Mgr::Main);
    }
}
