//! Stage 2: turn the detection into a plan. Classes get a feature per level, a subclass gate and
//! the root element; subclasses are linked to their parent class in the library.

use std::collections::HashSet;

use borealis_content::{ContentStore, Element, Rule};

use super::fewshot::in_edition;
use super::types::{
    Edition, ImportKind, ImportPlan, ItemType, NeedsChoice, OptionPicks, Outline, OutlineFeature,
    OutlinePart, ParentCandidate, ParentLink, PickCount, PlanGroup, PlannedElement, Role,
    SourceExcerpt,
};
use crate::import::ids::make_id;

/// Levels a class has features at.
pub(crate) const LEVELS: std::ops::RangeInclusive<u32> = 1..=20;
const DEFAULT_SUBCLASS_LEVEL: u32 = 3;
const SCORE_NAME: u32 = 100;
const SCORE_SUPPORTS: u32 = 90;
const SCORE_CONTAINS_NAME: u32 = 60;
const SCORE_PARTIAL: u32 = 50;
const MIN_RESOLVE_SCORE: u32 = SCORE_CONTAINS_NAME;

fn norm(s: &str) -> String {
    let words: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    words.join(" ")
}

fn contains_words(hay: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {hay} ").contains(&format!(" {needle} "))
}

struct Cand {
    link: ParentLink,
    aliases: Vec<String>,
}

fn archetype_select(store: &ContentStore, class: &Element) -> Option<(String, String, u32)> {
    class.rules.iter().find_map(|r| {
        let Rule::Grant(g) = r else { return None };
        let feature = store.element(&g.id)?;
        feature.rules.iter().find_map(|fr| match fr {
            Rule::Select(s) if s.ty == "Archetype" => Some((
                s.supports.clone().unwrap_or_default(),
                s.name.clone(),
                g.level.unwrap_or(1),
            )),
            _ => None,
        })
    })
}

fn candidate(store: &ContentStore, class: &Element) -> Option<Cand> {
    let (supports, select_name, gate) = archetype_select(store, class)?;
    let mut levels: Vec<u32> = vec![gate];
    for a in store.by_type("Archetype").map(|s| &s.element) {
        if !a.supports.contains(&supports) {
            continue;
        }
        levels.extend(a.rules.iter().filter_map(|r| match r {
            Rule::Grant(g) => g.level,
            _ => None,
        }));
    }
    levels.sort_unstable();
    levels.dedup();
    let aliases = [class.name.as_str(), supports.as_str(), select_name.as_str()]
        .iter()
        .map(|a| norm(a))
        .collect();
    Some(Cand {
        link: ParentLink {
            class_id: class.id.clone(),
            class_name: class.name.clone(),
            supports,
            subclass_levels: levels,
            in_source: false,
        },
        aliases,
    })
}

fn candidates(store: &ContentStore, edition: Edition) -> Vec<Cand> {
    let mut c: Vec<Cand> = store
        .by_type("Class")
        .map(|s| &s.element)
        .filter(|e| in_edition(e, edition))
        .filter_map(|e| candidate(store, e))
        .collect();
    c.sort_by(|a, b| a.link.class_id.cmp(&b.link.class_id));
    c
}

fn score(c: &Cand, hint: &str) -> u32 {
    let h = norm(hint);
    if h.is_empty() {
        return 0;
    }
    let (name, supports, select) = (&c.aliases[0], &c.aliases[1], &c.aliases[2]);
    if h == *name {
        SCORE_NAME
    } else if h == *supports || h == *select {
        SCORE_SUPPORTS
    } else if contains_words(&h, name) {
        SCORE_CONTAINS_NAME
    } else if (!supports.is_empty()
        && (contains_words(&h, supports) || contains_words(supports, &h)))
        || (!select.is_empty() && (contains_words(&h, select) || contains_words(select, &h)))
    {
        SCORE_PARTIAL
    } else {
        0
    }
}

/// Find the parent class of a subclass in the library by name or `supports`.
pub(crate) fn resolve_parent(
    store: &ContentStore,
    edition: Edition,
    hint: Option<&str>,
    group: &str,
) -> Result<ParentLink, NeedsChoice> {
    let all = candidates(store, edition);
    let mut scored: Vec<ParentCandidate> = all
        .iter()
        .map(|c| ParentCandidate {
            link: c.link.clone(),
            score: hint.map_or(0, |h| score(c, h)),
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.link.class_name.cmp(&b.link.class_name))
    });
    let top = scored.first().map_or(0, |c| c.score);
    let tied = scored.iter().filter(|c| c.score == top).count();
    if top >= MIN_RESOLVE_SCORE && tied == 1 {
        return Ok(scored.remove(0).link);
    }
    let reason = match (hint, top) {
        (None, _) | (Some(""), _) => "the source does not name the parent class".to_string(),
        (Some(h), 0) => format!("no class in the library matches \"{h}\""),
        (Some(h), _) => format!("\"{h}\" matches several classes"),
    };
    scored.retain(|c| c.score == top || top < MIN_RESOLVE_SCORE);
    Err(NeedsChoice {
        group: group.to_string(),
        reason,
        candidates: scored,
    })
}

/// The link for a class the user picked.
pub(crate) fn link_for(
    store: &ContentStore,
    edition: Edition,
    class_id: &str,
) -> Option<ParentLink> {
    candidates(store, edition)
        .into_iter()
        .find(|c| c.link.class_id == class_id)
        .map(|c| c.link)
}

/// Snap `level` to the nearest allowed level (ties go up).
fn snap(level: u32, allowed: &[u32]) -> u32 {
    allowed
        .iter()
        .copied()
        .min_by_key(|a| (a.abs_diff(level), u32::MAX - *a))
        .unwrap_or(level)
}

/// Locate `quote` (or `name`) in the source.
pub(crate) fn locate(source: &str, quote: &str, name: &str) -> Option<SourceExcerpt> {
    super::locate::locate_section(source, quote, name)
}

struct Builder<'a> {
    prefix: &'a str,
    source: &'a str,
    used: HashSet<String>,
    out: Vec<PlannedElement>,
    notes: Vec<String>,
}

impl Builder<'_> {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        ty: &str,
        name: &str,
        role: Role,
        level: Option<u32>,
        owner: Option<&str>,
        brief: &str,
        quote: &str,
    ) -> String {
        let id = make_id(self.prefix, ty, name, &self.used);
        self.used.insert(id.clone());
        self.out.push(PlannedElement {
            id: id.clone(),
            ty: ty.to_string(),
            name: name.to_string(),
            role,
            level,
            owner: owner.map(str::to_string),
            brief: brief.to_string(),
            excerpt: locate(self.source, quote, name),
            parent: None,
            picks: None,
            prerequisite_level: None,
        });
        id
    }

    fn get(&mut self, id: &str) -> Option<&mut PlannedElement> {
        self.out.iter_mut().find(|e| e.id == id)
    }

    /// A [`Role::Part`] or [`Role::Option`] element of `parent`.
    fn push_child(
        &mut self,
        ty: &str,
        part: &OutlinePart,
        role: Role,
        level: Option<u32>,
        parent: &str,
    ) -> String {
        let id = self.push(
            ty,
            &part.name,
            role,
            level,
            None,
            &part.summary,
            &part.quote,
        );
        if let Some(e) = self.get(&id) {
            e.parent = Some(parent.to_string());
        }
        id
    }

    /// The named parts and the pick-N options of the feature or trait `f` (planned as
    /// `parent_id`, of type `parent_ty`, owned by `owner_name`), each its own element. The
    /// parent keeps its text; its brief says the parts are written separately.
    fn push_children(
        &mut self,
        f: &OutlineFeature,
        parent_id: &str,
        parent_ty: &str,
        owner_name: &str,
        level: Option<u32>,
        with_parts: bool,
    ) {
        let ty = part_type(parent_ty);
        let mut names: Vec<String> = Vec::new();
        if with_parts {
            for p in unique_parts(&f.parts, &f.name) {
                self.push_child(ty, p, Role::Part, level, parent_id);
                names.push(p.name.clone());
            }
        }
        let options = unique_parts(&f.options, &f.name);
        if !options.is_empty() {
            let first = level.unwrap_or(1);
            let choose = picks_of(&f.choose, first);
            let opening = choose.first().map_or(first, |c| c.level);
            // A prerequisite that would leave the first pick empty is not one.
            let usable = options
                .iter()
                .any(|o| o.level.map_or(true, |l| l <= opening));
            for o in &options {
                let id = self.push_child(ty, o, Role::Option, level, parent_id);
                if let (true, Some(l)) = (usable, o.level.filter(|l| *l > opening)) {
                    if let Some(e) = self.get(&id) {
                        e.prerequisite_level = Some(l);
                    }
                }
            }
            if let Some(e) = self.get(parent_id) {
                e.picks = Some(OptionPicks {
                    key: option_key(owner_name, &f.name),
                    ty: ty.to_string(),
                    choose,
                });
            }
            names.extend(options.iter().map(|o| o.name.clone()));
        }
        if !names.is_empty() {
            if let Some(e) = self.get(parent_id) {
                e.brief = format!(
                    "{} Its named parts and options ({}) are separate elements, granted or offered for you: keep their text in the description, but write no rules for them.",
                    e.brief,
                    names.join(", ")
                )
                .trim()
                .to_string();
            }
        }
    }
}

/// Element type of a part or option of an element of type `parent_ty` (the bundled packs'
/// shapes: sub-features share their feature's type, species traits are `Racial Trait`).
pub(crate) fn part_type(parent_ty: &str) -> &'static str {
    match parent_ty {
        "Class" | "Class Feature" => "Class Feature",
        "Archetype" | "Archetype Feature" => "Archetype Feature",
        "Background" | "Background Feature" => "Background Feature",
        _ => "Racial Trait",
    }
}

/// A `supports` key from names (no characters the supports grammar reads as operators).
pub(crate) fn support_key(parts: &[&str]) -> String {
    let joined = parts.join(" ");
    let cleaned: String = joined
        .chars()
        .map(|c| if ",|&!()[]".contains(c) { ' ' } else { c })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `supports` key of the options of `feature` (owned by `owner`).
pub(crate) fn option_key(owner: &str, feature: &str) -> String {
    if norm(feature).starts_with(&norm(owner)) {
        support_key(&[feature])
    } else {
        support_key(&[owner, feature])
    }
}

/// Picks by level: as the source says (levels below the feature's own raised to it), else one
/// pick at the feature's level.
fn picks_of(choose: &[PickCount], first: u32) -> Vec<PickCount> {
    let mut out: Vec<PickCount> = choose
        .iter()
        .filter(|c| c.count > 0)
        .map(|c| PickCount {
            level: c.level.max(first),
            count: c.count,
        })
        .collect();
    out.sort_by_key(|c| c.level);
    out.dedup_by(|b, a| {
        let same = a.level == b.level;
        if same {
            a.count += b.count;
        }
        same
    });
    if out.is_empty() {
        out.push(PickCount {
            level: first,
            count: 1,
        });
    }
    out
}

/// Parts with a name, once each, never the parent itself.
fn unique_parts<'a>(parts: &'a [OutlinePart], parent: &str) -> Vec<&'a OutlinePart> {
    let mut seen = vec![norm(parent)];
    let mut out = Vec::new();
    for p in parts {
        let n = norm(&p.name);
        if n.is_empty() || seen.contains(&n) {
            continue;
        }
        seen.push(n);
        out.push(p);
    }
    out
}

/// Names of the parts and options of every feature of `o` (a feature row with one of these
/// names is the same thing listed twice, not a feature of its own).
fn sub_item_names(o: &Outline) -> Vec<String> {
    o.features
        .iter()
        .flat_map(|f| f.parts.iter().chain(&f.options))
        .map(|p| norm(&p.name))
        .collect()
}

fn is_gate(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("subclass") || n.contains("archetype")
}

/// Level of the class's subclass choice: the outline's `subclass_level`, else its first
/// subclass-named feature, else 3.
fn gate_level(o: &Outline) -> u32 {
    o.subclass_level
        .or_else(|| {
            o.features
                .iter()
                .filter(|f| is_gate(&f.name))
                .map(|f| f.level)
                .min()
        })
        .unwrap_or(DEFAULT_SUBCLASS_LEVEL)
}

/// A subclass-named class row at another level than the choice ("Subclass feature" at 7): it
/// only says when subclasses gain features, so it is no element of its own.
fn is_marker(f: &OutlineFeature, gate: u32) -> bool {
    is_gate(&f.name) && f.level != gate
}

/// Levels where the subclasses of an in-source class gain features: the choice level plus the
/// class's "subclass feature" rows, else plus every level its in-source subclasses use.
fn in_source_levels(o: &Outline, subclasses: &[&Outline]) -> Vec<u32> {
    let gate = gate_level(o);
    let mut levels: Vec<u32> = o
        .features
        .iter()
        .filter(|f| is_marker(f, gate))
        .map(|f| f.level)
        .collect();
    if levels.is_empty() {
        levels = subclasses
            .iter()
            .flat_map(|s| s.features.iter().map(|f| f.level))
            .filter(|l| *l >= gate)
            .collect();
    }
    levels.push(gate);
    levels.sort_unstable();
    levels.dedup();
    levels
}

/// The class of the same source a subclass belongs to: the one its hint names, or the only
/// class of the source when the hint names none.
pub(crate) fn in_source_class<'a>(classes: &'a [String], hint: Option<&str>) -> Option<&'a str> {
    let h = norm(hint.unwrap_or_default());
    if h.is_empty() {
        return match classes {
            [only] => Some(only.as_str()),
            _ => None,
        };
    }
    let hits: Vec<&String> = classes
        .iter()
        .filter(|c| {
            let n = norm(c);
            h == n || contains_words(&h, &n)
        })
        .collect();
    match hits.as_slice() {
        [one] => Some(one.as_str()),
        _ => None,
    }
}

/// A link to the in-source class `class_name`; [`rebuild`] fills its id and levels.
pub(crate) fn in_source_link(class_name: &str) -> ParentLink {
    ParentLink {
        class_id: String::new(),
        class_name: class_name.to_string(),
        supports: super::normalize::subclass_key(class_name),
        subclass_levels: Vec::new(),
        in_source: true,
    }
}

/// Links to the planned classes of `plan` (parent choices for its subclasses).
pub(crate) fn planned_class_links(plan: &ImportPlan) -> Vec<ParentLink> {
    plan.groups
        .iter()
        .filter(|g| g.kind == ImportKind::Class)
        .filter_map(|g| {
            let o = g.outline.as_ref()?;
            let mut link = in_source_link(&o.name);
            link.class_id = g.root_id.clone()?;
            link.subclass_levels = in_source_levels(o, &[]);
            Some(link)
        })
        .collect()
}

fn class_group(b: &mut Builder, o: &Outline) -> String {
    let class_id = make_id(b.prefix, "Class", &o.name, &b.used);
    b.used.insert(class_id.clone());
    let sub_level = gate_level(o);
    let subs = sub_item_names(o);
    for level in LEVELS {
        let mut here: Vec<_> = o
            .features
            .iter()
            .filter(|f| f.level == level && !is_marker(f, sub_level))
            .filter(|f| !subs.contains(&norm(&f.name)))
            .collect();
        if level == sub_level && !here.iter().any(|f| is_gate(&f.name)) {
            let name = format!("{} Subclass", o.name);
            b.push(
                "Class Feature",
                &name,
                Role::SubclassGate,
                Some(level),
                Some(&class_id),
                "Subclass choice",
                "",
            );
        }
        if level == 1
            && o.caster().is_some()
            && !o
                .features
                .iter()
                .any(|f| f.level <= 3 && super::caster::is_caster_feature(&f.name))
        {
            let name = format!("{} Spellcasting", o.name);
            let brief = o.caster().map_or_else(String::new, |s| {
                format!("Spellcasting: ability {}, {} caster, spell list {:?}. Describe it in prose only: the slot, cantrip and spell rules are generated for you.", s.ability, s.progression, s.list)
            });
            b.push(
                "Class Feature",
                &name,
                Role::ClassFeature,
                Some(1),
                Some(&class_id),
                &brief,
                "",
            );
        }
        here.sort_by(|a, b| a.name.cmp(&b.name));
        for f in here {
            let role = if is_gate(&f.name) {
                Role::SubclassGate
            } else {
                Role::ClassFeature
            };
            let id = b.push(
                "Class Feature",
                &f.name,
                role,
                Some(level),
                Some(&class_id),
                &f.summary,
                &f.quote,
            );
            if role == Role::ClassFeature {
                b.push_children(f, &id, "Class Feature", &o.name, Some(level), true);
            }
        }
    }
    // The class id was reserved first; push the root last so features come first in the order.
    b.used.remove(&class_id);
    let id = b.push(
        "Class",
        &o.name,
        Role::Class,
        None,
        None,
        &o.summary,
        &o.quote,
    );
    for e in b
        .out
        .iter_mut()
        .filter(|e| e.owner.as_deref() == Some(class_id.as_str()))
    {
        e.owner = Some(id.clone());
    }
    id
}

fn subclass_group(b: &mut Builder, o: &Outline, parent: &ParentLink) -> String {
    let root_id = make_id(b.prefix, "Archetype", &o.name, &b.used);
    b.used.insert(root_id.clone());
    let allowed: Vec<u32> = parent.subclass_levels.clone();
    let subs = sub_item_names(o);
    for f in o.features.iter().filter(|f| !subs.contains(&norm(&f.name))) {
        let level = if allowed.is_empty() || allowed.contains(&f.level) {
            f.level
        } else {
            let s = snap(f.level, &allowed);
            b.notes.push(format!(
                "{}: \"{}\" is at level {} in the source; {} subclasses only gain features at {:?}, so it was moved to {s}",
                o.name, f.name, f.level, parent.class_name, allowed
            ));
            s
        };
        let id = b.push(
            "Archetype Feature",
            &f.name,
            Role::SubclassFeature,
            Some(level),
            Some(&root_id),
            &f.summary,
            &f.quote,
        );
        b.push_children(f, &id, "Archetype Feature", &o.name, Some(level), true);
    }
    b.used.remove(&root_id);
    let id = b.push(
        "Archetype",
        &o.name,
        Role::Subclass,
        None,
        None,
        &o.summary,
        &o.quote,
    );
    for e in b
        .out
        .iter_mut()
        .filter(|e| e.owner.as_deref() == Some(root_id.as_str()))
    {
        e.owner = Some(id.clone());
    }
    id
}

fn same(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// (names, link) of every planned class: subclasses of it gain features at the levels its
/// outline and its in-source subclasses say.
fn in_source_classes(plan: &ImportPlan) -> Vec<(Vec<String>, ParentLink)> {
    plan.groups
        .iter()
        .filter(|g| g.kind == ImportKind::Class)
        .filter_map(|g| {
            let o = g.outline.as_ref()?;
            let names = vec![g.name.clone(), o.name.clone()];
            let subs: Vec<&Outline> = plan
                .groups
                .iter()
                .filter(|s| {
                    s.parent.as_ref().is_some_and(|p| {
                        p.in_source && names.iter().any(|n| same(n, &p.class_name))
                    })
                })
                .filter_map(|s| s.outline.as_ref())
                .collect();
            let mut link = in_source_link(&o.name);
            link.class_id = g.root_id.clone()?;
            link.subclass_levels = in_source_levels(o, &subs);
            Some((names, link))
        })
        .collect()
}

/// Recompute `plan.elements` (and group root ids) from the groups.
pub(crate) fn rebuild(plan: &mut ImportPlan, store: &ContentStore, prefix: &str, source: &str) {
    let mut b = Builder {
        prefix,
        source,
        used: store.iter().map(|s| s.element.id.clone()).collect(),
        out: Vec::new(),
        notes: Vec::new(),
    };
    // Classes first: subclasses of a class from the same source link to its planned id.
    for g in &mut plan.groups {
        g.root_id = None;
        if let (ImportKind::Class, Some(o)) = (g.kind, &g.outline) {
            g.root_id = Some(class_group(&mut b, o));
        }
    }
    let classes = in_source_classes(plan);
    for g in &mut plan.groups {
        match (g.kind, &g.outline, &g.parent) {
            (ImportKind::Class, ..) => {}
            (ImportKind::Subclass, Some(o), Some(p)) => {
                let link = classes
                    .iter()
                    .find(|(names, _)| p.in_source && names.iter().any(|n| same(n, &p.class_name)))
                    .map_or_else(|| p.clone(), |(_, l)| l.clone());
                g.root_id = Some(subclass_group(&mut b, o, &link));
                g.parent = Some(link);
            }
            (ImportKind::Subclass, _, _) | (ImportKind::Subrace, ..) => {}
            (kind, _, _) => {
                let ty = kind.element_type_for(g.item_type);
                g.root_id = Some(standalone_group(&mut b, g, ty));
            }
        }
    }
    // Subraces last: those of a species from the same source link to its planned id.
    let species: Vec<(String, String)> = plan
        .groups
        .iter()
        .filter(|g| g.kind == ImportKind::Species)
        .filter_map(|g| Some((g.name.clone(), g.root_id.clone()?)))
        .collect();
    for g in plan
        .groups
        .iter_mut()
        .filter(|g| g.kind == ImportKind::Subrace)
    {
        if let Some(p) = g.parent.as_mut().filter(|p| p.in_source) {
            p.class_id = species
                .iter()
                .find(|(n, _)| same(n, &p.class_name))
                .map(|(_, id)| id.clone())
                .unwrap_or_default();
        }
        let linked = g.parent.as_ref().is_some_and(|p| !p.class_id.is_empty());
        let ty = if linked {
            subrace_type(plan.edition)
        } else {
            b.notes.push(format!(
                "{}: its parent species was not found, so it is imported as a species of its own",
                g.name
            ));
            ImportKind::Species.element_type()
        };
        g.root_id = Some(standalone_group(&mut b, g, ty));
    }
    plan.elements = b.out;
    plan.notes = b.notes;
}

/// Element type of a subrace in `edition` (the bundled packs: 2014 `Sub Race`, 2024 lineages
/// are `Species Trait Option`s).
pub(crate) fn subrace_type(edition: Edition) -> &'static str {
    match edition {
        Edition::Y2014 => "Sub Race",
        Edition::Y2024 => "Species Trait Option",
    }
}

/// Root, traits (parts) and trait options of a standalone group; rules and lore groups become
/// one text-only element.
fn standalone_group(b: &mut Builder, g: &PlanGroup, ty: &str) -> String {
    let brief = g
        .outline
        .as_ref()
        .map(|o| o.summary.clone())
        .unwrap_or_default();
    let quote = g
        .outline
        .as_ref()
        .map(|o| o.quote.clone())
        .unwrap_or_default();
    let role = if g.kind == ImportKind::Rules {
        Role::Text
    } else {
        Role::Standalone
    };
    let id = b.push(ty, &g.name, role, None, None, &brief, &quote);
    let Some(o) = g.outline.as_ref().filter(|_| role == Role::Standalone) else {
        return id;
    };
    let part_ty = part_type(ty);
    let mut names = Vec::new();
    for f in &o.features {
        let part = OutlinePart {
            name: f.name.clone(),
            summary: f.summary.clone(),
            quote: f.quote.clone(),
            level: None,
        };
        if unique_parts(std::slice::from_ref(&part), &g.name).is_empty()
            || names.contains(&norm(&f.name))
        {
            continue;
        }
        names.push(norm(&f.name));
        let trait_id = b.push_child(part_ty, &part, Role::Part, None, &id);
        b.push_children(f, &trait_id, part_ty, &g.name, None, false);
    }
    if !names.is_empty() {
        if let Some(e) = b.get(&id) {
            e.brief = format!(
                "{} Its named traits are separate elements granted for you: keep their text in the description, but write no rules for them.",
                e.brief
            )
            .trim()
            .to_string();
        }
    }
    id
}

/// The parent species of a subrace: a species of the same source its hint names (or the only
/// one), else a library species of the edition with a subrace choice, as a link whose
/// `supports` is the key the subrace must carry. `None` when neither is found.
pub(crate) fn subrace_parent(
    store: &ContentStore,
    edition: Edition,
    species: &[String],
    hint: Option<&str>,
) -> Option<ParentLink> {
    if let Some(name) = in_source_class(species, hint) {
        return Some(ParentLink {
            class_id: String::new(),
            class_name: name.to_string(),
            supports: support_key(&[name, subrace_word(edition)]),
            subclass_levels: Vec::new(),
            in_source: true,
        });
    }
    let h = norm(hint?);
    let race = store
        .by_type("Race")
        .chain(store.by_type("Species"))
        .map(|s| &s.element)
        .filter(|e| in_edition(e, edition))
        .find(|e| norm(&e.name) == h || contains_words(&h, &norm(&e.name)))?;
    let key = subrace_select(store, race)?;
    Some(ParentLink {
        class_id: race.id.clone(),
        class_name: race.name.clone(),
        supports: key,
        subclass_levels: Vec::new(),
        in_source: false,
    })
}

fn subrace_word(edition: Edition) -> &'static str {
    match edition {
        Edition::Y2014 => "Subrace",
        Edition::Y2024 => "Lineage",
    }
}

/// The `supports` of a library species' subrace or lineage choice (on the species or one of
/// the traits it grants).
fn subrace_select(store: &ContentStore, race: &Element) -> Option<String> {
    let pick = |el: &Element| {
        el.rules.iter().find_map(|r| match r {
            Rule::Select(s) if matches!(s.ty.as_str(), "Sub Race" | "Species Trait Option") => {
                s.supports.clone()
            }
            _ => None,
        })
    };
    pick(race).or_else(|| {
        race.rules.iter().find_map(|r| match r {
            Rule::Grant(g) => store.element(&g.id).and_then(pick),
            _ => None,
        })
    })
}

/// A text-only group for a rules or lore section the detection listed in `other`.
pub(crate) fn text_group(name: &str, what: &str, note: &str) -> PlanGroup {
    let mut g = standalone(ImportKind::Rules, name, None);
    g.outline = Some(Outline {
        name: name.to_string(),
        summary: format!("{what}. {note}")
            .trim_matches([' ', '.'])
            .to_string(),
        hit_die: None,
        subclass_level: None,
        spellcasting: None,
        features: Vec::new(),
        quote: String::new(),
    });
    g
}

/// A plan group for a standalone item.
pub(crate) fn standalone(kind: ImportKind, name: &str, item_type: Option<ItemType>) -> PlanGroup {
    PlanGroup {
        kind,
        name: name.to_string(),
        root_id: None,
        outline: None,
        parent: None,
        choice: None,
        item_type: item_type.filter(|_| kind == ImportKind::Item),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_the_nearest_allowed_level_preferring_higher() {
        assert_eq!(snap(9, &[3, 7, 10, 15, 18]), 10);
        assert_eq!(snap(5, &[3, 7]), 7);
        assert_eq!(snap(3, &[3, 7]), 3);
    }

    #[test]
    fn picks_default_to_one_at_the_feature_level_and_never_come_earlier() {
        let p = |level, count| PickCount { level, count };
        assert_eq!(picks_of(&[], 2), vec![p(2, 1)]);
        assert_eq!(
            picks_of(&[p(6, 1), p(1, 2), p(2, 1), p(9, 0)], 2),
            vec![p(2, 3), p(6, 1)]
        );
    }

    #[test]
    fn option_keys_carry_no_supports_operators() {
        assert_eq!(
            option_key("Warden", "Charms (Wild, Tame)"),
            "Warden Charms Wild Tame"
        );
        assert_eq!(option_key("Warden", "Warden Charms"), "Warden Charms");
        assert_eq!(part_type("Race"), "Racial Trait");
        assert_eq!(part_type("Archetype Feature"), "Archetype Feature");
    }

    #[test]
    fn locates_a_quote_paragraph() {
        let src = "Intro\n\nRally Banner.\nYou raise it.\n\nOther";
        let ex = locate(src, "rally banner.", "x").unwrap();
        assert_eq!((ex.start_line, ex.end_line), (3, 4));
        assert!(locate(src, "", "nothing here").is_none());
    }
}
