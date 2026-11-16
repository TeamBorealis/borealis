//! The element universe: store elements plus the content the engine derives (`engine.md` 1.5):
//! Multiclass elements, ASI / feat class features, implicit support tags. Expressions are parsed
//! once here.

use crate::adapter::{Adaptation, EditionAdapter, StandardAdapter, USED_AS_WRITTEN};
use crate::diag::{code, DiagSink, Diagnostic, Severity};
use crate::ids;
use crate::profile::{profile_for, RulesetProfile};
use crate::tables::ProfileTables;
use borealis_content::expr::{EquippedExpr, RequirementsExpr, SupportsExpr};
use borealis_content::{ContentStore, Element, Rule, Ruleset, Setter, StoredElement};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Class levels that get Ability Score Improvement / Feat alternatives.
pub const ASI_LEVELS: [u32; 9] = [4, 6, 8, 10, 12, 14, 16, 18, 19];

/// A parsed condition: absent, a parsed expression, or text that failed to parse (never true).
#[derive(Debug, Clone)]
pub enum Cond<E> {
    /// No condition.
    None,
    /// A parsed expression.
    Expr(E),
    /// Unparsable text.
    Invalid,
}

impl<E> Cond<E> {
    pub(crate) fn from_parse<T>(
        r: Result<Option<T>, borealis_content::expr::ExprError>,
        wrap: fn(T) -> E,
    ) -> Self {
        match r {
            Ok(None) => Self::None,
            Ok(Some(v)) => Self::Expr(wrap(v)),
            Err(_) => Self::Invalid,
        }
    }
}

/// A stat value: literal or a reference to another group (optionally negated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    /// An integer literal.
    Lit(i32),
    /// Reference to a group by (lower-cased) name.
    Ref {
        /// Group name.
        name: String,
        /// Subtract instead of add.
        neg: bool,
    },
}

impl Val {
    /// Parse a stat `value` / `maximum` (`engine.md` 5.4): `["-"] (integer | statname)`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let t = text.trim();
        let t = t.strip_prefix('+').unwrap_or(t);
        if let Ok(n) = t.parse::<i32>() {
            return Self::Lit(n);
        }
        let (neg, rest) = match t.strip_prefix('-') {
            Some(r) => (true, r.trim()),
            None => (false, t),
        };
        if let Ok(n) = rest.parse::<i32>() {
            return Self::Lit(if neg { -n } else { n });
        }
        Self::Ref {
            name: rest.to_lowercase(),
            neg,
        }
    }
}

/// Parsed stat rule.
#[derive(Debug, Clone)]
pub struct UStat {
    /// Lower-cased group name.
    pub name: String,
    /// Value.
    pub value: Val,
    /// Per-rule clamp.
    pub maximum: Option<Val>,
    /// Stacking label.
    pub bonus: Option<String>,
    /// Required manager level.
    pub level: u32,
    /// Display label (`alt` or the element name).
    pub label: String,
    /// Inline (display only) rule.
    pub inline: bool,
    /// Element-level style requirements.
    pub req: Cond<RequirementsExpr>,
    /// Equipped condition.
    pub equipped: Cond<EquippedExpr>,
    /// Owning element id (provenance).
    pub owner_id: String,
    /// Owning element name (provenance).
    pub owner_name: String,
    /// The rule's `value` text (provenance).
    pub value_text: String,
    /// The rule's `requirements` text (provenance).
    pub req_text: Option<String>,
    /// The rule's `equipped` text (provenance).
    pub equipped_text: Option<String>,
}

impl UStat {
    /// The rule's conditions as one line (`level >= 3; requires ...; equipped ...`), if any.
    #[must_use]
    pub fn condition_text(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.level > 1 {
            parts.push(format!("level >= {}", self.level));
        }
        if let Some(r) = &self.req_text {
            parts.push(format!("requires {r}"));
        }
        if let Some(e) = &self.equipped_text {
            parts.push(format!("equipped {e}"));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// Parsed rule, parallel to `Element::rules`.
#[derive(Debug, Clone)]
pub enum URule {
    /// A grant rule.
    Grant {
        /// Rule requirements.
        req: Cond<RequirementsExpr>,
    },
    /// A select rule.
    Select {
        /// Rule requirements.
        req: Cond<RequirementsExpr>,
        /// Supports filter.
        supports: Cond<SupportsExpr>,
    },
    /// A stat rule.
    Stat(Box<UStat>),
    /// Anything else (no behaviour).
    Other,
}

/// Whether an element belongs to the universe's edition or was authored for the other one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditionStatus {
    /// Same edition, or edition-neutral (`any`).
    Native,
    /// Authored for another edition.
    Foreign {
        /// The authoring edition.
        from: Ruleset,
        /// The profile accepts it at all (`false` = never offered, never valid).
        accepted: bool,
        /// The adapter changed the element.
        converted: bool,
        /// Adapter notes or the rejection reason.
        notes: Vec<String>,
    },
}

impl EditionStatus {
    /// May the element be used at all (stored choices included)?
    #[must_use]
    pub fn is_accepted(&self) -> bool {
        match self {
            Self::Native => true,
            Self::Foreign { accepted, .. } => *accepted,
        }
    }

    /// Is the element from another edition?
    #[must_use]
    pub fn is_foreign(&self) -> bool {
        matches!(self, Self::Foreign { .. })
    }
}

/// An element prepared for evaluation.
#[derive(Debug, Clone)]
pub struct UElem {
    /// The element (store element or derived). An element the engine uses as the store holds it is the store's own
    /// allocation (shared, not copied); one it changes (an adapted or normalised copy, a patched class) is its own.
    pub el: Arc<Element>,
    /// Parsed rules (shared with the universe an engine built over this one takes the element over into, IzDfk;
    /// `Arc::make_mut` to change them).
    pub rules: Arc<Vec<URule>>,
    /// Element-level requirements.
    pub req: Arc<Cond<RequirementsExpr>>,
    /// Effective supports tags (explicit + implicit + support names).
    pub supports: Arc<Vec<String>>,
    /// Number of leading rules copied from a class into a Multiclass element (their owner
    /// exclusion is disabled).
    pub copied_rules: usize,
    /// True for engine-generated elements.
    pub derived: bool,
    /// How the element relates to the universe's edition.
    pub edition: EditionStatus,
    /// `allow duplicate` setter, or a repeatable feat (`data.repeatable.allowed`, or `data.repeatable: true`).
    pub allow_duplicate: bool,
    /// Ability bonuses of the entries of rewritten `ability_*` list selects, per rule index.
    pub list_effects: crate::custom_rules::Effects,
    /// Structured feat prerequisites (`data.prerequisite`).
    pub prereq: crate::custom_rules::Prereq,
    /// Several instances may exist; each gets its own slot keys.
    pub repeatable: bool,
    /// Foreign element that a native element replaces one to one (`maps_to` kind `equivalent`);
    /// never offered for new picks.
    pub superseded: bool,
    /// IM16: the tags of the 2014 elements this native 2024 element replaces (`maps_to` kind
    /// `equivalent`, 2024 universe only). A supports expression the element's own tags do not
    /// match is matched against these too ([`crate::eval::supports_match`]).
    pub replaced_supports: Vec<String>,
    /// IM16: the ids of the 2014 elements this native element replaces; an `ID_` term of the
    /// fallback matching names them.
    pub replaced_ids: Vec<String>,
    /// Stat rules of each `toggle` rule, applied only while the toggle is on.
    pub toggle_effects: Vec<crate::play_rules::ToggleEffects>,
    /// A usable `Skill` element's own proficiency (`<skill>:proficiency`), applied while the character has it
    /// ([`crate::content_skills`]).
    pub skill_proficiency: Option<UStat>,
    /// Foreign element the profile accepts only as its `legacy_alias` type (a 2024 species
    /// that was a race, in a 2014 character). The foreign options of its own selects are
    /// accepted with it (`Graph::inherits_owner_acceptance`).
    pub legacy_accepted: bool,
    /// IM14: its book's edition is the user's choice (Sources page): that book's key. A
    /// character that already holds it keeps it whatever the edition; new picks follow the
    /// edition (R4).
    pub user_edition_book: Option<String>,
    /// The store element it was prepared from (`None` for one the engine generates). An engine built over a previous
    /// one takes a prepared element over when the new store holds this same allocation (IzDfk).
    pub(crate) src: Option<Arc<Element>>,
    /// A universe-wide pass changed it after it was prepared (an origin increase, appended or legacy subclass tags,
    /// a gated grant): it is prepared again rather than taken over.
    pub(crate) touched: bool,
}

impl UElem {
    pub(crate) fn build(
        el: Element,
        copied_rules: usize,
        derived: bool,
        edition: EditionStatus,
        supports: Vec<String>,
    ) -> Self {
        Self::build_shared(Arc::new(el), copied_rules, derived, edition, supports)
    }

    /// [`UElem::build`] of an element that may be shared (the store's own): it stays shared unless normalising it
    /// changes it, then the universe keeps the normalised copy.
    pub(crate) fn build_shared(
        mut el: Arc<Element>,
        copied_rules: usize,
        derived: bool,
        edition: EditionStatus,
        supports: Vec<String>,
    ) -> Self {
        // normalising leaves every other element as it is (both return at once for them)
        let list_effects =
            if crate::item_conds::normalizes(&el) || crate::custom_rules::normalizes(&el) {
                let mut own = el.as_ref().clone();
                crate::item_conds::normalize(&mut own);
                let effects = crate::custom_rules::normalize(&mut own);
                if own != *el {
                    el = Arc::new(own);
                }
                effects
            } else {
                crate::custom_rules::Effects::new()
            };
        let repeatable = crate::custom_rules::is_repeatable(&el);
        let prereq = crate::custom_rules::Prereq::from_element(&el);
        let owner_name = el.name.clone();
        let toggle_effects = crate::play_rules::toggle_stat_effects(&el);
        let skill_proficiency = crate::content_skills::proficiency_stat(&el);
        let rules = Arc::new(
            el.rules
                .iter()
                .map(|r| parse_rule(r, &el.id, &owner_name))
                .collect(),
        );
        let req = Arc::new(Cond::from_parse(el.parse_requirements(), |e| e));
        let allow_duplicate = el
            .setter_value("allow duplicate")
            .and_then(borealis_content::setter::parse_bool)
            .unwrap_or(false)
            || repeatable;
        Self {
            el,
            rules,
            req,
            supports: Arc::new(supports),
            copied_rules,
            derived,
            edition,
            allow_duplicate,
            list_effects,
            prereq,
            repeatable,
            superseded: false,
            replaced_supports: Vec::new(),
            replaced_ids: Vec::new(),
            toggle_effects,
            skill_proficiency,
            legacy_accepted: false,
            user_edition_book: None,
            src: None,
            touched: false,
        }
    }

    /// Element id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.el.id
    }

    /// Hit die size from the `hd` setter.
    #[must_use]
    pub fn hit_die(&self) -> Option<u32> {
        self.el
            .setter_value("hd")
            .and_then(|v| v.trim().trim_start_matches(['d', 'D']).parse().ok())
    }

    /// Level number for Level elements.
    #[must_use]
    pub fn level_number(&self) -> Option<u32> {
        self.el.setter_value("level")?.trim().parse().ok()
    }

    /// Spell level for spells.
    #[must_use]
    pub fn spell_level(&self) -> Option<u32> {
        self.el.setter_value("level")?.trim().parse().ok()
    }

    /// Setter truthiness.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.el
            .setter_value(name)
            .and_then(borealis_content::setter::parse_bool)
            .unwrap_or(false)
    }
}

fn parse_rule(rule: &Rule, owner_id: &str, owner_name: &str) -> URule {
    match rule {
        Rule::Grant(g) => URule::Grant {
            req: g.requirements.as_deref().map_or(Cond::None, |t| {
                Cond::from_parse(RequirementsExpr::parse(t), |e| e)
            }),
        },
        Rule::Select(s) => URule::Select {
            req: s.requirements.as_deref().map_or(Cond::None, |t| {
                Cond::from_parse(RequirementsExpr::parse(t), |e| e)
            }),
            supports: s.supports.as_deref().map_or(Cond::None, |t| {
                Cond::from_parse(SupportsExpr::parse(t), |e| e)
            }),
        },
        Rule::Stat(s) => URule::Stat(Box::new(UStat {
            name: s.name.trim().to_lowercase(),
            value: Val::parse(&s.value),
            maximum: s.maximum.as_deref().map(Val::parse),
            bonus: s.bonus.clone().filter(|b| !b.is_empty()),
            level: s.level.unwrap_or(1),
            label: s
                .alt
                .clone()
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| owner_name.to_string()),
            inline: s.inline.unwrap_or(false),
            req: s.requirements.as_deref().map_or(Cond::None, |t| {
                Cond::from_parse(RequirementsExpr::parse(t), |e| e)
            }),
            equipped: s.equipped.as_deref().map_or(Cond::None, |t| {
                Cond::from_parse(EquippedExpr::parse(t), |e| e)
            }),
            owner_id: owner_id.to_string(),
            owner_name: owner_name.to_string(),
            value_text: s.value.trim().to_string(),
            req_text: s.requirements.clone().filter(|t| !t.trim().is_empty()),
            equipped_text: s.equipped.clone().filter(|t| !t.trim().is_empty()),
        })),
        Rule::Unknown(_) | Rule::Custom(_) => URule::Other,
    }
}

/// Element types that fill the same select slot: the 2024 `Species` type is the `Race` slot;
/// typed `Monster` stat blocks (the bundled SRD beasts) also fill `Companion` selects, filtered
/// by the select's supports like Aurora's own companions.
#[must_use]
pub fn type_family(ty: &str) -> &'static [&'static str] {
    match ty {
        "Race" | "Species" => &["Race", "Species"],
        "Companion" => &["Companion", "Monster"],
        // a skill a `Skill` element adds is offered by the skill choices, which are `Proficiency` selects tagged Skill
        // (`crate::content_skills`); imports never write `Skill` elements
        "Proficiency" => &["Proficiency", "Skill"],
        _ => &[],
    }
}

/// Does an element of type `candidate` satisfy a select of type `wanted`?
#[must_use]
pub fn type_fits(wanted: &str, candidate: &str) -> bool {
    wanted == candidate || type_family(wanted).contains(&candidate)
}

/// Types that fill a select of type `wanted` only when the candidate comes from the other
/// edition (IM8): 2024 optional features (invocations, metamagic, maneuvers) are typed `Option`
/// while the 2014 class selects that offer them are `Class Feature` selects, and the other way
/// round. A same-edition candidate never crosses types this way.
#[must_use]
pub fn cross_edition_types(wanted: &str) -> &'static [&'static str] {
    match wanted {
        "Class Feature" => &["Option"],
        "Option" => &["Class Feature"],
        _ => &[],
    }
}

/// Upper-case, id-safe form of a class name.
#[must_use]
pub fn sanitize_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// All elements the engine can see.
#[derive(Debug)]
pub struct Universe {
    elems: Vec<UElem>,
    by_id: HashMap<String, usize>,
    by_type: HashMap<String, Vec<usize>>,
    /// Problems found while preparing the universe.
    pub diagnostics: Vec<Diagnostic>,
    ruleset: Ruleset,
    tables: ProfileTables,
    creation_roots: Vec<String>,
    exhaustion: crate::play_rules::ExhaustionRules,
    /// Skills content adds besides the eighteen (`Skill` elements, [`crate::content_skills`]).
    skills: Vec<crate::content_skills::ContentSkill>,
    /// Aurora's proxy items, indexed on the first lookup of a proxy id (B-06).
    proxies: std::sync::OnceLock<crate::proxy::ProxyTable>,
    /// `maps_to` kind `equivalent` pairs with both elements loaded: mapper id -> the
    /// counterpart it declares itself equal to (an imported PHB spell and its SRD copy).
    equivalents: HashMap<String, String>,
    /// Id -> index of its equivalence class in `equivalent_classes` (every id joined to
    /// another by an `equivalent` edge, followed through any number of hops).
    equivalent_class: HashMap<String, usize>,
    /// The ids of each equivalence class, in load order.
    equivalent_classes: Vec<Vec<String>>,
    /// Support element id -> name, which every element's effective tags read: an engine built over this one takes no
    /// prepared element over when it changed.
    support_names: HashMap<String, String>,
    /// How many prepared elements were taken over from the previous universe ([`Universe::with_adapter_reusing`]).
    reused: usize,
}

/// The former type an element names (`legacy_alias`, `Race` on a 2024 `Species`) when the
/// profile rejects its own type but accepts it as that former type; `None` otherwise.
fn legacy_alias_accepted<'e>(el: &'e Element, profile: &dyn RulesetProfile) -> Option<&'e str> {
    let from = el
        .ruleset
        .filter(|r| !r.compatible_with(profile.ruleset()))?;
    let alias = el.legacy_alias.as_deref().filter(|a| !a.is_empty())?;
    (!profile.accepts_foreign(&el.ty, from) && profile.accepts_legacy_alias(alias, from))
        .then_some(alias)
}

/// The former type a foreign element is also used as (`Race` for a 2024 `Species` in a 2014
/// character), for the compatibility note; `None` when it names none or the profile refuses it.
fn legacy_alias_note<'e>(el: &'e Element, profile: &dyn RulesetProfile) -> Option<&'e str> {
    let from = el
        .ruleset
        .filter(|r| !r.compatible_with(profile.ruleset()))?;
    el.legacy_alias
        .as_deref()
        .filter(|a| !a.is_empty() && *a != el.ty && profile.accepts_legacy_alias(a, from))
}

/// Element types followed from a background when looking for its own ability increase (IM9a):
/// its features, variants and characteristics (`Background*`), plus what a background can hand
/// out that may carry the increase instead of the background itself: a feat and its feat
/// features (a 2024-style origin feat), a generic option, a `Grants` bundle, and an Aurora
/// ability score improvement element. Not followed: spells, proficiencies, languages, items,
/// class or species content, which never carry an origin increase (class features would pull in
/// the level 4 improvements, which are not origin increases).
fn is_background_part(ty: &str) -> bool {
    ty.starts_with("Background")
        || matches!(
            ty,
            "Feat" | "Feat Feature" | "Option" | "Grants" | "Ability Score Improvement"
        )
}

/// Element types that make up a species (its traits, lineage options, subraces).
fn is_origin_part(ty: &str) -> bool {
    matches!(
        ty,
        "Species" | "Race" | "Sub Race" | "Racial Trait" | "Species Trait Option"
    )
}

/// Union-find over `equivalent` edges: id -> class index, and the ids of each class in the
/// order they first appear.
fn equivalence_classes(edges: &[(String, String)]) -> (HashMap<String, usize>, Vec<Vec<String>>) {
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut ids: Vec<&str> = Vec::new();
    for (a, b) in edges {
        for x in [a.as_str(), b.as_str()] {
            index.entry(x).or_insert_with(|| {
                ids.push(x);
                ids.len() - 1
            });
        }
    }
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for (a, b) in edges {
        let (ra, rb) = (
            find(&mut parent, index[a.as_str()]),
            find(&mut parent, index[b.as_str()]),
        );
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut root_class: HashMap<usize, usize> = HashMap::new();
    let mut classes: Vec<Vec<String>> = Vec::new();
    let mut class_of = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        let root = find(&mut parent, i);
        let c = *root_class.entry(root).or_insert_with(|| {
            classes.push(Vec::new());
            classes.len() - 1
        });
        classes[c].push((*id).to_string());
        class_of.insert((*id).to_string(), c);
    }
    (class_of, classes)
}

/// Decides how an element relates to the profile's edition and applies the adapter.
fn classify(
    el: &Arc<Element>,
    profile: &dyn RulesetProfile,
    adapter: &dyn EditionAdapter,
) -> (Arc<Element>, EditionStatus) {
    let target = profile.ruleset();
    let Some(from) = el.ruleset.filter(|r| !r.compatible_with(target)) else {
        return (Arc::clone(el), EditionStatus::Native);
    };
    let rejected = |reason: String| EditionStatus::Foreign {
        from,
        accepted: false,
        converted: false,
        notes: vec![reason],
    };
    let alias = legacy_alias_accepted(el, profile);
    if !profile.accepts_foreign(&el.ty, from) && alias.is_none() {
        let reason = format!(
            "{} content of type {} is not usable in {}",
            from.as_str(),
            el.ty,
            profile.label()
        );
        return (Arc::clone(el), rejected(reason));
    }
    match adapter.adapt(el, from, profile) {
        Adaptation::Unchanged => (
            Arc::clone(el),
            EditionStatus::Foreign {
                from,
                accepted: true,
                converted: false,
                notes: std::iter::once(USED_AS_WRITTEN.to_string())
                    .chain(
                        legacy_alias_note(el, profile)
                            .map(|a| format!("{} used as a {a} (its former type)", el.ty)),
                    )
                    .collect(),
            },
        ),
        Adaptation::Converted { element, notes } => (
            Arc::new(*element),
            EditionStatus::Foreign {
                from,
                accepted: true,
                converted: true,
                notes,
            },
        ),
        Adaptation::Rejected { reason } => (Arc::clone(el), rejected(reason)),
    }
}

fn spell_implicit_supports(el: &Element) -> Vec<String> {
    let mut out = Vec::new();
    let truthy = |n: &str| {
        el.setter_value(n)
            .and_then(borealis_content::setter::parse_bool)
            .unwrap_or(false)
    };
    if let Some(l) = el.setter_value("level") {
        out.push(l.trim().to_string());
    }
    if let Some(s) = el.setter_value("school").filter(|s| !s.is_empty()) {
        out.push(s.to_string());
        out.push(s.to_lowercase());
    }
    if truthy("isRitual") {
        out.push("Ritual".into());
        out.push("ID_INTERNAL_SUPPORT_RITUAL".into());
    }
    if truthy("isConcentration") {
        out.push("ID_INTERNAL_SUPPORT_CONCENTRATION".into());
    }
    for (setter, tag) in [
        ("hasVerbalComponent", "ID_INTERNAL_SUPPORT_VERBAL"),
        ("hasSomaticComponent", "ID_INTERNAL_SUPPORT_SOMATIC"),
        ("hasMaterialComponent", "ID_INTERNAL_SUPPORT_MATERIAL"),
    ] {
        if truthy(setter) {
            out.push(tag.into());
        }
    }
    if let Some(t) = el.setter_value("time") {
        let t = t.to_lowercase();
        if t.contains("bonus action") {
            out.push("ID_INTERNAL_SUPPORT_BONUS_ACTION".into());
        } else if t.contains("reaction") {
            out.push("ID_INTERNAL_SUPPORT_REACTION".into());
        } else if t.contains("action") {
            out.push("ID_INTERNAL_SUPPORT_ACTION".into());
        }
    }
    out
}

/// `Companion` stat blocks are tagged with their `type`, `size` and `challenge` setters
/// (`content-format.md` 6.12), so `Beast,(Tiny||Small),(1/4||0)` filters them.
fn companion_implicit_supports(el: &Element) -> Vec<String> {
    ["type", "size", "challenge"]
        .into_iter()
        .filter_map(|n| el.setter_value(n))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn title_case(word: &str) -> String {
    let mut c = word.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase()
            .chain(c.flat_map(char::to_lowercase))
            .collect()
    })
}

/// Typed `Monster` stat blocks carry the same tags as an Aurora `Companion` (`Beast`, `Tiny`,
/// `1/4`) so companion selects can offer them; lower-case type and size are added too. A
/// creature without a challenge rating (decision IM12) carries no CR tag, so a select that
/// restricts by CR never offers it, while one that does not may.
fn monster_implicit_supports(el: &Element) -> Vec<String> {
    let Ok(m) = borealis_content::MonsterData::from_element(el) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let ty = m.creature_type.as_str();
    out.push(title_case(ty));
    out.push(ty.to_string());
    for size in &m.size {
        out.push(title_case(size.as_str()));
        out.push(size.as_str().to_string());
    }
    out.extend(m.cr.map(|cr| cr.as_text()));
    out
}

fn implicit_supports(el: &Element) -> Vec<String> {
    match el.ty.as_str() {
        "Spell" => spell_implicit_supports(el),
        "Companion" => companion_implicit_supports(el),
        "Monster" => monster_implicit_supports(el),
        "Armor" | "Weapon" => vec![el.name.clone(), el.name.to_lowercase()],
        "Language" => ["Standard", "Exotic", "Secret", "Monster", "Character"]
            .into_iter()
            .filter(|n| {
                el.setter_value(n)
                    .and_then(borealis_content::setter::parse_bool)
                    .unwrap_or(false)
            })
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

impl Universe {
    /// Build the universe for a store and ruleset with the [`StandardAdapter`].
    #[must_use]
    pub fn new(store: &ContentStore, ruleset: Ruleset) -> Self {
        Self::with_adapter(store, ruleset, &StandardAdapter)
    }

    /// Build the universe for a store and ruleset; foreign-edition elements go through `adapter`.
    #[must_use]
    pub fn with_adapter(
        store: &ContentStore,
        ruleset: Ruleset,
        adapter: &dyn EditionAdapter,
    ) -> Self {
        Self::with_adapter_reusing(store, ruleset, adapter, None)
    }

    /// [`Universe::with_adapter`], taking over the prepared elements of `previous` (a universe of the same ruleset
    /// built with the same adapter, over a store that is still alive) whose store element is one the new store holds
    /// as the same allocation, that is no class, that no universe-wide pass changed, and with the same user edition
    /// book; and none at all when a support name changed. Each element is prepared on its own from its store element
    /// and the support names, so a taken-over element is the one preparing it again would make; the universe-wide
    /// passes then run over all of them as in a build from scratch (IzDfk: a creator save on a phone with a large
    /// library spent most of its time preparing every element again).
    #[must_use]
    pub(crate) fn with_adapter_reusing(
        store: &ContentStore,
        ruleset: Ruleset,
        adapter: &dyn EditionAdapter,
        previous: Option<&Universe>,
    ) -> Self {
        let profile = profile_for(ruleset);
        let support_names: HashMap<&str, &str> = store
            .by_type("Support")
            .map(|s| (s.element.id.as_str(), s.element.name.as_str()))
            .collect();
        let owned_support_names: HashMap<String, String> = support_names
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let previous =
            previous.filter(|p| p.ruleset == ruleset && p.support_names == owned_support_names);
        let mut uni = Self {
            elems: Vec::new(),
            by_id: HashMap::new(),
            by_type: HashMap::new(),
            diagnostics: Vec::new(),
            ruleset,
            tables: ProfileTables::default(),
            creation_roots: Vec::new(),
            exhaustion: crate::play_rules::ExhaustionRules::default(),
            skills: Vec::new(),
            proxies: std::sync::OnceLock::new(),
            equivalents: HashMap::new(),
            equivalent_class: HashMap::new(),
            equivalent_classes: Vec::new(),
            support_names: owned_support_names,
            reused: 0,
        };
        let eff = |el: &Element| -> Vec<String> {
            let mut s = el.supports.clone();
            s.extend(implicit_supports(el));
            let mut names = Vec::new();
            for t in &s {
                if t.starts_with(ids::SUPPORT_PREFIX) {
                    if let Some(n) = support_names.get(t.as_str()) {
                        names.push((*n).to_string());
                    }
                }
            }
            s.extend(names);
            s.dedup();
            s
        };
        // each element is prepared on its own (in parallel), then the universe takes them in load order
        let taken_over = |stored: &StoredElement| -> Option<UElem> {
            if stored.element.ty == "Class" {
                return None;
            }
            // by id only (no proxy lookup): the universe's own element of that id
            let previous = previous?;
            let prev = &previous.elems[*previous.by_id.get(&stored.element.id)?];
            let same = !prev.derived
                && !prev.touched
                && prev.el.ty != "Class"
                && prev
                    .src
                    .as_ref()
                    .is_some_and(|s| Arc::ptr_eq(s, &stored.element))
                && prev.user_edition_book == stored.user_edition_book;
            same.then(|| {
                let mut u = (*prev).clone();
                // set again by `mark_superseded` below, as for a prepared one
                u.superseded = false;
                u
            })
        };
        let prepare = |stored: &&StoredElement| {
            let diagnostics =
                crate::custom_rules::content_diagnostics(&stored.element, stored.origin);
            if let Some(u) = taken_over(stored) {
                return (diagnostics, None, None, u, true);
            }
            let (mut el, edition) = classify(&stored.element, profile, adapter);
            let mut class = None;
            let mut multi = None;
            if el.ty == "Class" {
                class = Some(el.as_ref().clone());
                if let Some(mc) = el.multiclass.clone() {
                    let derived = Self::multiclass_element(&el, &mc);
                    patch_class(Arc::make_mut(&mut el), &mc);
                    let supports = eff(&derived);
                    let mut m = UElem::build(
                        derived,
                        el.rules.len().saturating_sub(1),
                        true,
                        edition.clone(),
                        supports,
                    );
                    m.user_edition_book.clone_from(&stored.user_edition_book);
                    multi = Some(m);
                    // the copied-rule count is the class rules *before* the patch appended one rule
                }
            }
            let supports = eff(&el);
            let legacy = legacy_alias_accepted(&stored.element, profile).is_some();
            let mut u = UElem::build_shared(el, 0, false, edition, supports);
            u.legacy_accepted = legacy && u.edition.is_accepted();
            u.user_edition_book.clone_from(&stored.user_edition_book);
            u.src = Some(Arc::clone(&stored.element));
            (diagnostics, class, multi, u, false)
        };
        let stored: Vec<&StoredElement> = store.iter().collect();
        let prepared: Vec<_> = stored.par_iter().map(prepare).collect();
        let mut classes: Vec<Element> = Vec::new();
        for (diagnostics, class, multi, u, reused) in prepared {
            uni.diagnostics.extend(diagnostics);
            uni.reused += usize::from(reused);
            classes.extend(class);
            if let Some(m) = multi {
                uni.push(m);
            }
            uni.push(u);
        }
        // fix copied_rules of multiclass elements: class rules before patching
        // (computed again below for clarity)
        for class in &classes {
            if let Some(mc) = &class.multiclass {
                if let Some(&i) = uni.by_id.get(&mc.id) {
                    uni.elems[i].copied_rules = class.rules.len();
                }
            }
        }
        uni.add_origin_increases(profile, adapter);
        uni.generate_improvements(store, &classes, &eff);
        uni.mark_superseded();
        crate::appended_supports::share(&mut uni.elems, store, ruleset, &support_names);
        uni.index_equivalents();
        crate::legacy_subclass::adopt(&mut uni.elems, &uni.by_id, profile);
        uni.load_profile_tables();
        uni.add_creation_languages(profile);
        uni.exhaustion = crate::play_rules::ExhaustionRules::load(&uni);
        uni.skills = uni.collect_skills();
        uni
    }

    /// The usable `Skill` elements as skills: this edition's (and edition-neutral) ones first, then the accepted
    /// other-edition ones.
    fn collect_skills(&self) -> Vec<crate::content_skills::ContentSkill> {
        let usable = |u: &&UElem| u.edition.is_accepted() && !u.superseded;
        let of_type = || {
            self.by_type(crate::content_skills::SKILL_TYPE)
                .filter(usable)
        };
        let native = of_type().filter(|u| !u.edition.is_foreign());
        let foreign = of_type().filter(|u| u.edition.is_foreign());
        crate::content_skills::collect(native.chain(foreign).map(|u| u.el.as_ref()))
    }

    /// Skills content adds besides the eighteen, in load order.
    #[must_use]
    pub fn content_skills(&self) -> &[crate::content_skills::ContentSkill] {
        &self.skills
    }

    /// Exhaustion numbers and the stat rules of each level (`exhaustion` rule of the content).
    #[must_use]
    pub fn exhaustion(&self) -> &crate::play_rules::ExhaustionRules {
        &self.exhaustion
    }

    /// Tables the content pack supplies for this universe's ruleset (`ID_2024_PROFILE_TABLES`).
    #[must_use]
    pub fn tables(&self) -> &ProfileTables {
        &self.tables
    }

    /// Ids of engine-generated elements every character of this universe registers at creation
    /// (the origin languages step).
    #[must_use]
    pub fn creation_roots(&self) -> &[String] {
        &self.creation_roots
    }

    /// Add the derived Weapon element of every feature whose text says it counts as a weapon
    /// ([`crate::feature_weapon`], [`crate::EngineSettings::feature_weapons`]).
    pub(crate) fn add_feature_weapons(&mut self) {
        let derived: Vec<(Element, EditionStatus, Vec<String>)> = self
            .elems
            .iter()
            .filter_map(|u| {
                let w = crate::feature_weapon::derive(&u.el)?;
                (!self.by_id.contains_key(&w.id)).then(|| {
                    let mut supports = w.supports.clone();
                    supports.extend(implicit_supports(&w));
                    (w, u.edition.clone(), supports)
                })
            })
            .collect();
        for (el, edition, supports) in derived {
            self.push(UElem::build(el, 0, true, edition, supports));
        }
    }

    /// Elements that an element authored for this universe's ruleset replaces one to one
    /// (`maps_to` kind `equivalent`) are hidden from new picks. That includes an element for Both
    /// editions (IM17, `any`), as a bundled core language is replaced by its 2024 version: a 2024
    /// character sees the 2024 text only, a 2014 character the Both one (second review M1).
    fn mark_superseded(&mut self) {
        let ruleset = self.ruleset;
        let replaced: std::collections::HashSet<String> = self
            .elems
            .iter()
            .filter(|u| u.el.ruleset == Some(ruleset))
            .filter_map(|u| u.el.maps_to.as_ref())
            .filter(|m| m.kind == Some(borealis_content::MapKind::Equivalent))
            .flat_map(|m| [m.dnd2014.clone(), m.dnd2024.clone()])
            .flatten()
            .collect();
        for u in &mut self.elems {
            u.superseded = u.el.ruleset != Some(ruleset) && replaced.contains(&u.el.id);
        }
    }

    /// Index the `maps_to` kind `equivalent` pairs whose two elements are both loaded (same
    /// type): one rules element under two ids. Every edge of every element counts (a 5etools
    /// reprint maps to both its 2014 and its 2024 counterpart), and the ids joined by any chain
    /// of edges form one equivalence class.
    fn index_equivalents(&mut self) {
        let mut map = HashMap::new();
        let mut edges: Vec<(String, String)> = Vec::new();
        for u in &self.elems {
            let Some(m) = u.el.maps_to.as_ref() else {
                continue;
            };
            if m.kind != Some(borealis_content::MapKind::Equivalent) {
                continue;
            }
            for target in [&m.dnd2014, &m.dnd2024].into_iter().flatten() {
                let same = self
                    .get(target)
                    .is_some_and(|t| t.el.id != u.el.id && t.el.ty == u.el.ty);
                if same {
                    map.insert(u.el.id.clone(), target.clone());
                    edges.push((u.el.id.clone(), target.clone()));
                }
            }
        }
        let (class_of, classes) = equivalence_classes(&edges);
        self.equivalents = map;
        self.equivalent_class = class_of;
        self.equivalent_classes = classes;
    }

    /// The loaded counterpart `id` declares itself equivalent to.
    #[must_use]
    pub fn equivalent_target(&self, id: &str) -> Option<&str> {
        self.equivalents.get(id).map(String::as_str)
    }

    /// Every other id of the element `id` (its whole equivalence class minus itself), for the
    /// duplicate check.
    #[must_use]
    pub fn equivalent_ids<'a>(&'a self, id: &'a str) -> Vec<&'a str> {
        self.equivalent_class
            .get(id)
            .map(|&c| {
                self.equivalent_classes[c]
                    .iter()
                    .map(String::as_str)
                    .filter(|x| *x != id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn load_profile_tables(&mut self) {
        let found = self
            .elems
            .iter()
            .find(|u| ProfileTables::is_tables_element(&u.el) && !u.edition.is_foreign());
        if let Some(u) = found {
            self.tables = ProfileTables::from_element(&u.el);
        }
    }

    /// Common plus N Standard languages at creation (profile hook `creation_languages`).
    fn add_creation_languages(&mut self, profile: &dyn RulesetProfile) {
        let count = profile.creation_languages();
        if count == 0 || self.tables.standard_languages.is_empty() {
            return;
        }
        let common = self
            .by_type("Language")
            .find(|u| u.el.name == "Common" && u.el.ruleset == Some(self.ruleset))
            .map(|u| u.el.id.clone());
        let Some(common) = common else { return };
        let mut el = Element {
            id: ids::CREATION_LANGUAGES.to_string(),
            name: "Languages".into(),
            ty: "Internal".into(),
            ..Element::default()
        };
        el.rules.push(Rule::Grant(borealis_content::GrantRule {
            id: common,
            ty: Some("Language".into()),
            ..borealis_content::GrantRule::default()
        }));
        el.rules.push(Rule::Select(borealis_content::SelectRule {
            ty: "Language".into(),
            name: "Languages".into(),
            number: Some(count),
            supports: Some("Standard".into()),
            ..borealis_content::SelectRule::default()
        }));
        self.creation_roots.push(el.id.clone());
        self.push(UElem::build(el, 0, true, EditionStatus::Native, Vec::new()));
    }

    fn multiclass_element(class: &Element, mc: &borealis_content::Multiclass) -> Element {
        let mut m = Element::new(
            mc.id.clone(),
            class.name.clone(),
            "Multiclass",
            class.source.clone(),
        );
        m.ruleset = class.ruleset;
        m.supports = class.supports.clone();
        m.setters = class.setters.clone();
        for s in &mc.setters {
            m.setters.push(Setter::new(s.name.clone(), s.value.clone()));
        }
        m.description = if mc.description.is_empty() {
            class.description.clone()
        } else {
            mc.description.clone()
        };
        m.rules = class.rules.iter().chain(mc.rules.iter()).cloned().collect();
        let mut parts = Vec::new();
        if !class.requirements.is_empty() {
            parts.push(format!("({})", class.requirements));
        }
        parts.push(format!("!{}", class.id));
        if !mc.requirements.is_empty() {
            parts.push(format!("({})", mc.requirements));
        }
        m.requirements = parts.join("&&");
        m.prerequisite = mc.prerequisite.clone();
        m
    }

    fn generate_improvements(
        &mut self,
        store: &ContentStore,
        classes: &[Element],
        eff: &dyn Fn(&Element) -> Vec<String>,
    ) {
        let (Some(asi_t), Some(feat_t)) = (
            store.element(ids::TEMPLATE_ASI),
            store.element(ids::TEMPLATE_FEAT),
        ) else {
            if !classes.is_empty() {
                self.diagnostics.push(Diagnostic::new(
                    code::CONTENT_MISSING_ELEMENT,
                    Severity::Warning,
                    "improvement templates missing: ability score improvements are not generated",
                ));
            }
            return;
        };
        // one set per class name: the ids follow Aurora's name formula, so two classes of one
        // name (the 2014 and 2024 Wizard) share it instead of listing every option twice
        let mut done = std::collections::HashSet::new();
        for class in classes {
            let up = sanitize_id(&class.name);
            if !done.insert(up.clone()) {
                continue;
            }
            for n in ASI_LEVELS {
                for (kind, tmpl, label, sel_label) in [
                    (
                        "ASI",
                        asi_t,
                        "Ability Score Improvement",
                        "Ability Score Increase",
                    ),
                    ("FEAT", feat_t, "Feat", "Feat"),
                ] {
                    let mut e = tmpl.clone();
                    e.id = format!("ID_INTERNAL_CLASS_FEATURE_{kind}_{n}_{up}");
                    e.name = format!("{label} ({n})");
                    e.supports = vec![
                        "Improvement Option".into(),
                        class.name.clone(),
                        n.to_string(),
                    ];
                    for r in &mut e.rules {
                        if let Rule::Select(s) = r {
                            s.name = format!("{sel_label} ({} {n})", class.name);
                            s.level = Some(n);
                        }
                    }
                    let supports = eff(&e);
                    self.push(UElem::build(e, 0, true, EditionStatus::Native, supports));
                }
            }
        }
    }

    /// IM9: every accepted foreign species with no ability increase anywhere in its parts goes
    /// through [`EditionAdapter::origin_increase`]; the element is rebuilt in place (same id,
    /// type and index) and the change is added to its compatibility notes. IM9a: the backgrounds
    /// (any edition) whose parts raise a score are passed along, so the added increase applies
    /// only while the character has none of them.
    fn add_origin_increases(&mut self, profile: &dyn RulesetProfile, adapter: &dyn EditionAdapter) {
        let backgrounds: &[usize] = self.by_type.get("Background").map_or(&[], Vec::as_slice);
        let with_own_increase: Vec<String> = backgrounds
            .par_iter()
            .filter(|&&i| self.raises_ability_in(i, is_background_part))
            .map(|&i| self.elems[i].el.id.clone())
            .collect();
        for i in 0..self.elems.len() {
            let u = &self.elems[i];
            let EditionStatus::Foreign {
                from,
                accepted: true,
                notes,
                ..
            } = &u.edition
            else {
                continue;
            };
            if !matches!(u.el.ty.as_str(), "Species" | "Race")
                || self.raises_ability_in(i, is_origin_part)
            {
                continue;
            }
            let Some((el, added)) =
                adapter.origin_increase(&u.el, *from, profile, &with_own_increase)
            else {
                continue;
            };
            let notes: Vec<String> = notes
                .iter()
                .filter(|n| *n != USED_AS_WRITTEN)
                .cloned()
                .chain(added)
                .collect();
            let edition = EditionStatus::Foreign {
                from: *from,
                accepted: true,
                converted: true,
                notes,
            };
            let mut rebuilt =
                UElem::build(el, u.copied_rules, u.derived, edition, u.supports.to_vec());
            rebuilt.legacy_accepted = u.legacy_accepted;
            rebuilt.user_edition_book.clone_from(&u.user_edition_book);
            rebuilt.touched = true;
            self.elems[i] = rebuilt;
        }
    }

    /// Does the origin element at `root` (a species or a background), one of its granted parts,
    /// or an option one of their selects can pick (lineages, subraces, background variants, a
    /// background's feats) raise an ability score? Only elements whose type passes `is_part` are
    /// followed ([`is_origin_part`] for species: no feats; [`is_background_part`] for
    /// backgrounds). Static, as for species (det-engine-7): one option that could raise a score
    /// is enough, whichever option the character picks. `seen` guards grant/select cycles.
    fn raises_ability_in(&self, root: usize, is_part: fn(&str) -> bool) -> bool {
        let mut stack = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(i) = stack.pop() {
            if !seen.insert(i) {
                continue;
            }
            let u = &self.elems[i];
            if u.el.rules.iter().any(crate::adapter::raises_ability) {
                return true;
            }
            for (rule, parsed) in u.el.rules.iter().zip(u.rules.iter()) {
                match (rule, parsed) {
                    (Rule::Grant(g), _) => stack.extend(
                        self.by_id
                            .get(&g.id)
                            .copied()
                            .filter(|&j| is_part(&self.elems[j].el.ty)),
                    ),
                    (Rule::Select(s), URule::Select { supports, .. }) if is_part(&s.ty) => {
                        stack.extend(self.candidates_for(&s.ty).filter_map(|c| {
                            crate::eval::supports_cond_match(supports, c, &crate::eval::NoDynamics)
                                .then(|| self.by_id.get(&c.el.id).copied())
                                .flatten()
                        }));
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn push(&mut self, u: UElem) {
        self.note_bad_expressions(&u);
        let idx = self.elems.len();
        self.by_type.entry(u.el.ty.clone()).or_default().push(idx);
        self.by_id.insert(u.el.id.clone(), idx);
        self.elems.push(u);
    }

    fn note_bad_expressions(&mut self, u: &UElem) {
        let mut bad = Vec::new();
        if matches!(*u.req, Cond::Invalid) {
            bad.push("requirements".to_string());
        }
        for (i, r) in u.rules.iter().enumerate() {
            let invalid = match r {
                URule::Grant { req } => matches!(req, Cond::Invalid),
                URule::Select { req, supports } => {
                    matches!(req, Cond::Invalid) || matches!(supports, Cond::Invalid)
                }
                URule::Stat(s) => {
                    matches!(s.req, Cond::Invalid) || matches!(s.equipped, Cond::Invalid)
                }
                URule::Other => false,
            };
            if invalid {
                bad.push(format!("rule {i}"));
            }
        }
        if !bad.is_empty() {
            self.diagnostics.push(
                Diagnostic::new(
                    code::CONTENT_BAD_EXPRESSION,
                    Severity::Warning,
                    format!(
                        "{} has unparsable expressions in: {}",
                        u.el.id,
                        bad.join(", ")
                    ),
                )
                .at(u.el.id.clone()),
            );
        }
    }

    /// Look an element up by id. Aurora's proxy items ([`crate::proxy`]) resolve here too,
    /// although no listing contains them.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&UElem> {
        match self.by_id.get(id) {
            Some(&i) => Some(&self.elems[i]),
            None if crate::proxy::is_proxy_id(id) => self.proxy_table().get(self, id),
            None => None,
        }
    }

    /// Aurora's proxy items of this universe, indexed on first use (B-06).
    pub(crate) fn proxy_table(&self) -> &crate::proxy::ProxyTable {
        self.proxies
            .get_or_init(|| crate::proxy::ProxyTable::build(self))
    }

    /// The element at a load-order index (as enumerated by [`Universe::iter`]).
    pub(crate) fn at(&self, index: usize) -> Option<&UElem> {
        self.elems.get(index)
    }

    /// Elements that can fill a select of this type (see [`type_family`]).
    pub fn candidates_for<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = &'a UElem> + 'a {
        let fam = type_family(ty);
        let tys: Vec<&str> = if fam.is_empty() {
            vec![ty]
        } else {
            fam.to_vec()
        };
        tys.into_iter().flat_map(move |t| self.by_type(t))
    }

    /// [`Self::candidates_for`] plus the other-edition elements of a crossing type
    /// ([`cross_edition_types`]); `cross` = the character allows content from the other edition.
    pub fn select_candidates<'a>(
        &'a self,
        ty: &'a str,
        cross: bool,
    ) -> impl Iterator<Item = &'a UElem> + 'a {
        let extra: &'static [&'static str] = if cross { cross_edition_types(ty) } else { &[] };
        self.candidates_for(ty).chain(
            extra
                .iter()
                .flat_map(move |t| self.by_type(t).filter(|c| c.edition.is_foreign())),
        )
    }

    /// The place of `e` among this universe's elements; `None` for an element of another universe.
    pub(crate) fn index_of(&self, e: &UElem) -> Option<usize> {
        let size = std::mem::size_of::<UElem>();
        let at = std::ptr::from_ref(e)
            .addr()
            .checked_sub(self.elems.as_ptr().addr())?;
        let i = at / size;
        (at % size == 0 && i < self.elems.len()).then_some(i)
    }

    /// All elements of a type, in load order (derived ones after their class).
    pub fn by_type<'a>(&'a self, ty: &str) -> impl Iterator<Item = &'a UElem> + 'a {
        self.by_type
            .get(ty)
            .into_iter()
            .flat_map(|v| v.iter())
            .map(|&i| &self.elems[i])
    }

    /// A class by name, preferring the universe's own edition over foreign-edition twins
    /// that share the name (a 2024 Barbarian must never read the 2014 progression table).
    #[must_use]
    pub fn class_named(&self, name: &str) -> Option<&UElem> {
        let mut foreign = None;
        for c in self.by_type("Class").filter(|c| c.el.name == name) {
            if !c.edition.is_foreign() && c.el.ruleset.is_none_or(|r| r == self.ruleset) {
                return Some(c);
            }
            foreign.get_or_insert(c);
        }
        foreign
    }

    /// Iterate all elements.
    pub fn iter(&self) -> impl Iterator<Item = &UElem> {
        self.elems.iter()
    }

    /// The universe as text, maps sorted, for comparing two builds (the reused count left out).
    #[cfg(test)]
    pub(crate) fn canonical(&self) -> String {
        fn sorted<V: std::fmt::Debug + Ord>(m: &HashMap<String, V>) -> String {
            let mut v: Vec<_> = m.iter().collect();
            v.sort();
            format!("{v:?}")
        }
        format!(
            "{:?}\n{:?}\n{}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{}\n{}\n{:?}\n{}",
            self.elems,
            self.diagnostics,
            sorted(&self.by_id),
            sorted(&self.by_type),
            self.ruleset,
            self.tables,
            self.creation_roots,
            self.exhaustion,
            self.skills,
            sorted(&self.equivalents),
            sorted(&self.equivalent_class),
            self.equivalent_classes,
            sorted(&self.support_names),
        )
    }

    /// How many prepared elements were taken over from the universe this one was built over (0 for a build from
    /// scratch).
    #[must_use]
    pub fn reused_elements(&self) -> usize {
        self.reused
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// The ruleset the universe was prepared for.
    #[must_use]
    pub fn ruleset(&self) -> Ruleset {
        self.ruleset
    }

    /// Record a one-time diagnostic.
    pub fn report(&self, sink: &mut DiagSink) {
        for d in &self.diagnostics {
            sink.push(d.clone());
        }
    }
}

/// Patch the main class for multiclassing (`engine.md` 1.5).
fn patch_class(class: &mut Element, mc: &borealis_content::Multiclass) {
    let not_mc = format!("!{}", mc.id);
    class.requirements = if class.requirements.is_empty() {
        not_mc.clone()
    } else {
        format!("({})&&{not_mc}", class.requirements)
    };
    let mut req = format!("{}&&", ids::OPT_MULTICLASSING);
    if !mc.requirements.is_empty() {
        req.push_str(&format!("({})&&", mc.requirements));
    }
    req.push_str(&not_mc);
    class.rules.push(Rule::Grant(borealis_content::GrantRule {
        id: ids::MULTICLASS_PREREQ.into(),
        ty: Some("Grants".into()),
        requirements: Some(req),
        ..borealis_content::GrantRule::default()
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn val_parsing() {
        assert_eq!(Val::parse("3"), Val::Lit(3));
        assert_eq!(Val::parse("+2"), Val::Lit(2));
        assert_eq!(Val::parse("-4"), Val::Lit(-4));
        assert_eq!(
            Val::parse("-Warlock:Spellcasting:Slots:Count"),
            Val::Ref {
                name: "warlock:spellcasting:slots:count".into(),
                neg: true
            }
        );
        assert_eq!(
            Val::parse("Dexterity:Modifier"),
            Val::Ref {
                name: "dexterity:modifier".into(),
                neg: false
            }
        );
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_id("Blood Hunter"), "BLOOD_HUNTER");
        assert_eq!(sanitize_id("Fighter"), "FIGHTER");
    }

    #[test]
    fn equivalence_classes_follow_every_edge_across_hops() {
        // T11: the 5etools copy maps to both counterparts; the 2024 one maps to the 2014 one
        let e = |a: &str, b: &str| (a.to_string(), b.to_string());
        let edges = [
            e("5ET_PHB", "SRD14"),
            e("5ET_PHB", "SRD24"),
            e("SRD24", "SRD14"),
            e("5ET_XPHB", "SRD24"),
            e("OTHER_A", "OTHER_B"),
        ];
        let (class_of, classes) = equivalence_classes(&edges);
        let c = class_of["5ET_PHB"];
        for id in ["SRD14", "SRD24", "5ET_XPHB"] {
            assert_eq!(class_of[id], c, "{id}");
        }
        assert_eq!(classes[c].len(), 4);
        assert_ne!(class_of["OTHER_A"], c);
        assert_eq!(class_of["OTHER_A"], class_of["OTHER_B"]);
    }
}
