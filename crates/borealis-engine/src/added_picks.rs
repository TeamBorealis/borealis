//! B3 (GitHub #35): extra feats, spells, languages and proficiencies a DM grants, added in the
//! builder ("Add Feat", "Add Spell", "Add Language", "Add Proficiency").
//!
//! An added pick reuses Aurora's own mechanism for extras, its "Additional ..." proxy items
//! ([`crate::proxy`]): the pick is an inventory line holding the proxy of the chosen element,
//! equipped (which enables its grant), hidden from the sheet inventory, and marked
//! [`crate::state::InventoryItem::added_pick`]. So an added feat behaves exactly like one from
//! an Aurora "Additional Feat" item: the same grant, the same derive, sheet and file shape, and
//! a build that predates the flag still reads it (as an equipped "Additional Feat" item). The
//! flag only moves the line out of the equipment list and into its builder section.
//!
//! What is offered ([`options`]): the targets of the kind's generated proxy category in the
//! character's edition and unrestricted sources, without the ones the character already has
//! (a repeatable element stays offered; a spell is "had" when its block knows it). An option
//! *qualifies* when its requirements and structured prerequisites hold (a spell: also on the
//! class's lists, of a level it casts). Only qualifying options are offered, unless the HR3
//! house rule "Ignore feat prerequisites" (GitHub #24) is on: then every option is, the rest
//! marked, and an added line's grant bypasses its target's requirements while the rule is on
//! (as U12's "Override requirements" does). Turning the rule off keeps the pick but withholds
//! what fails, as HR3 does for a feat slot.

use crate::graph::Graph;
use crate::snapshot::ElementRef;
use crate::state::CharacterState;
use crate::universe::{Cond, UElem, Universe};
use borealis_content::Rule;
use serde::{Deserialize, Serialize};

/// What kind of extra a pick is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddedKind {
    /// A feat (Feats section).
    Feat,
    /// A spell of one spellcasting class (Magic, that class's tab).
    Spell,
    /// A language (Languages section).
    Language,
    /// A skill, saving throw, armor, weapon or tool proficiency (Proficiencies section).
    Proficiency,
}

impl AddedKind {
    /// The content element type of the kind.
    #[must_use]
    pub fn element_type(self) -> &'static str {
        match self {
            Self::Feat => "Feat",
            Self::Spell => "Spell",
            Self::Language => "Language",
            Self::Proficiency => "Proficiency",
        }
    }

    /// The kind of an element type, if it is one.
    #[must_use]
    pub fn of_type(ty: &str) -> Option<Self> {
        [Self::Feat, Self::Spell, Self::Language, Self::Proficiency]
            .into_iter()
            .find(|k| k.element_type() == ty)
    }
}

/// One element an Add action offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOption {
    /// Element id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Content source name.
    pub source: String,
    /// The element's prerequisite text (empty for none).
    pub prerequisite: String,
    /// Spell level (spells only; 0 = cantrip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Proficiency group: `Skill`, `Saving Throw`, `Armor`, `Weapon`, `Tool` or `Other`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The character meets its requirements and prerequisites (a spell: and it is on the
    /// class's lists, of a level the class casts). False only while "Ignore feat
    /// prerequisites" is on: otherwise such an option is not offered.
    pub qualifies: bool,
}

/// One pick added in the builder, as the snapshot lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedPickSnapshot {
    /// The inventory line that holds it (what removing it names).
    pub uid: String,
    /// Kind.
    pub kind: AddedKind,
    /// The picked element.
    pub element: ElementRef,
    /// The spellcasting class a spell was added to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spellcasting: Option<String>,
    /// The element is registered on the character. False while it is withheld: its
    /// prerequisites fail and "Ignore feat prerequisites" is off (or its content is missing).
    pub applied: bool,
    /// HR3: it applies only because "Ignore feat prerequisites" is on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prerequisites_unmet: bool,
}

/// The proficiency group of a Proficiency element (its supports, else its name).
#[must_use]
pub fn proficiency_group(el: &borealis_content::Element) -> &'static str {
    let has = |s: &str| el.supports.iter().any(|x| x == s);
    let named = |p: &str| el.name.starts_with(p);
    if has("Skill") {
        "Skill"
    } else if has("Saving Throw") || named("Saving Throw") {
        "Saving Throw"
    } else if ["Light Armor", "Medium Armor", "Heavy Armor", "Shield"]
        .iter()
        .any(|s| has(s))
        || named("Armor")
    {
        "Armor"
    } else if has("Weapon") || named("Weapon") {
        "Weapon"
    } else if has("Tool") || has("Gaming Set") || has("Musical Instrument") || named("Tool") {
        "Tool"
    } else {
        "Other"
    }
}

impl<'u> Graph<'u> {
    /// Does the character qualify for `target` as an added pick (for spell block `cast`)? Its
    /// requirements and structured prerequisites hold; a spell is also on the class's lists and
    /// of a level the class casts. An unreadable requirement never holds (C1).
    pub(crate) fn added_pick_qualifies(&self, target: &UElem, cast: Option<&str>) -> bool {
        if matches!(target.req, Cond::Invalid) || !self.prereq_holds(target) {
            return false;
        }
        if !crate::eval::requirement_holds(&target.req, &self.env()) {
            return false;
        }
        if target.el.ty != "Spell" {
            return true;
        }
        let Some((lists, top)) = cast.and_then(|c| crate::spells::class_spell_lists(self, c))
        else {
            return false;
        };
        let level = crate::spells::spell_level(target);
        let refs: Vec<&borealis_content::SpellcastingList> = lists.iter().collect();
        (level == 0 || level <= top) && crate::spells::on_lists(&refs, target)
    }

    /// Does the character already have `target` (for spell block `cast`), so that adding it
    /// again would do nothing? A repeatable element never counts as had.
    fn has_added_target(&self, target: &UElem, cast: Option<&str>) -> bool {
        if target.el.ty == "Spell" {
            return self
                .nodes_of_type("Spell")
                .any(|(_, n)| n.e.el.id == target.el.id && n.spell_class.as_deref() == cast);
        }
        !target.allow_duplicate && self.held_or_equivalent(&target.el.id)
    }

    /// Is node `id` the element an added pick's line grants (the direct target of its proxy)?
    pub(crate) fn is_added_target(&self, id: crate::graph::NodeId) -> bool {
        let Some(parent) = self.nodes[id].parent else {
            return false;
        };
        self.nodes[parent]
            .item_uid
            .as_deref()
            .is_some_and(|uid| self.added_items.contains(uid))
    }

    /// HR3 for an added pick's target node: it is registered only because "Ignore feat
    /// prerequisites" is on.
    pub(crate) fn added_target_needs_override(&self, id: crate::graph::NodeId) -> bool {
        self.ignore_feat_prereqs
            && self.is_added_target(id)
            && !self.added_pick_qualifies(self.nodes[id].e, self.nodes[id].spell_class.as_deref())
    }
}

/// The element and spellcasting name that proxy item `item` grants.
fn proxy_grant<'u>(uni: &'u Universe, item: &str) -> Option<(&'u UElem, Option<String>)> {
    let proxy = uni.get(item)?;
    let Some(Rule::Grant(g)) = proxy.el.rules.first() else {
        return None;
    };
    Some((uni.get(&g.id)?, g.spellcasting.clone()))
}

/// The options of `kind` (for spell block `cast`) the character may add, sorted: spells by
/// level then name, the rest by name.
pub(crate) fn options(
    graph: &Graph<'_>,
    uni: &Universe,
    state: &CharacterState,
    kind: AddedKind,
    cast: Option<&str>,
) -> Vec<AddOption> {
    let Some(category) =
        crate::proxy::category_for_type(kind.element_type(), cast.unwrap_or_default())
    else {
        return Vec::new();
    };
    let ignore = state.house_rules.ignore_feat_prerequisites;
    let mut out: Vec<AddOption> = uni
        .proxies()
        .in_category(&category)
        .map(|p| p.target)
        .filter(|t| graph.offers_edition(t))
        .filter(|t| {
            !graph
                .restricted
                .is_restricted_element(&t.el.source, &t.el.ty)
        })
        .filter(|t| !matches!(t.req, Cond::Invalid) && !graph.has_added_target(t, cast))
        .filter_map(|t| {
            let qualifies = graph.added_pick_qualifies(t, cast);
            (qualifies || ignore).then(|| option(t, kind, qualifies))
        })
        .collect();
    out.sort_by_cached_key(|o| (o.level, o.name.to_lowercase()));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn option(t: &UElem, kind: AddedKind, qualifies: bool) -> AddOption {
    AddOption {
        id: t.el.id.clone(),
        name: t.el.name.clone(),
        source: t.el.source.clone(),
        prerequisite: t.el.prerequisite.trim().to_string(),
        level: (kind == AddedKind::Spell).then(|| crate::spells::spell_level(t)),
        group: (kind == AddedKind::Proficiency).then(|| proficiency_group(&t.el).to_string()),
        qualifies,
    }
}

/// The proxy id that adds `id` as a pick of `kind` (for spell block `cast`).
pub(crate) fn proxy_for(
    uni: &Universe,
    kind: AddedKind,
    id: &str,
    cast: Option<&str>,
) -> Option<String> {
    let category = crate::proxy::category_for_type(kind.element_type(), cast.unwrap_or_default())?;
    uni.proxies()
        .in_category(&category)
        .find(|p| p.target.el.id == id)
        .map(|p| p.id)
}

/// The added picks of `state`, in inventory order.
pub(crate) fn snapshots(
    graph: &Graph<'_>,
    uni: &Universe,
    state: &CharacterState,
) -> Vec<AddedPickSnapshot> {
    state
        .inventory
        .items
        .iter()
        .filter(|line| line.added_pick)
        .filter_map(|line| {
            let (target, cast) = proxy_grant(uni, &line.item)?;
            let kind = AddedKind::of_type(&target.el.ty)?;
            let node = graph.nodes.iter().enumerate().find(|(i, n)| {
                n.alive
                    && n.e.el.id == target.el.id
                    && graph.is_added_target(*i)
                    && graph.item_uid_of(*i) == Some(line.uid.as_str())
            });
            Some(AddedPickSnapshot {
                uid: line.uid.clone(),
                kind,
                element: crate::gear::element_ref(target),
                spellcasting: cast,
                applied: node.is_some(),
                prerequisites_unmet: node
                    .is_some_and(|(i, _)| graph.added_target_needs_override(i)),
            })
        })
        .collect()
}
