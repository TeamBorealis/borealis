//! Stage 2: turn the detection into a plan. Classes get a feature per level, a subclass gate and
//! the root element; subclasses are linked to their parent class in the library.

use std::collections::HashSet;

use borealis_content::{ContentStore, Element, Rule};

use super::fewshot::in_edition;
use super::types::{
    Edition, ImportKind, ImportPlan, NeedsChoice, Outline, ParentCandidate, ParentLink, PlanGroup,
    PlannedElement, Role, SourceExcerpt,
};
use crate::import::ids::make_id;

/// Levels a class has features at.
pub(crate) const LEVELS: std::ops::RangeInclusive<u32> = 1..=20;
const DEFAULT_SUBCLASS_LEVEL: u32 = 3;
const SNIPPET_CHARS: usize = 160;
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
    let lines: Vec<&str> = source.lines().collect();
    let find = |needle: &str| {
        let n = needle.trim().to_lowercase();
        (!n.is_empty())
            .then(|| lines.iter().position(|l| l.to_lowercase().contains(&n)))
            .flatten()
    };
    let start = find(quote).or_else(|| find(name))?;
    let mut end = start;
    while end + 1 < lines.len() && !lines[end + 1].trim().is_empty() {
        end += 1;
    }
    let snippet: String = lines[start..=end]
        .join(" ")
        .chars()
        .take(SNIPPET_CHARS)
        .collect();
    Some(SourceExcerpt {
        start_line: start + 1,
        end_line: end + 1,
        snippet,
    })
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
        });
        id
    }
}

fn is_gate(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("subclass") || n.contains("archetype")
}

fn class_group(b: &mut Builder, o: &Outline) -> String {
    let class_id = make_id(b.prefix, "Class", &o.name, &b.used);
    b.used.insert(class_id.clone());
    let sub_level = o.subclass_level.unwrap_or(DEFAULT_SUBCLASS_LEVEL);
    for level in LEVELS {
        let mut here: Vec<_> = o.features.iter().filter(|f| f.level == level).collect();
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
            b.push(
                "Class Feature",
                &f.name,
                role,
                Some(level),
                Some(&class_id),
                &f.summary,
                &f.quote,
            );
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
    for f in &o.features {
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
        b.push(
            "Archetype Feature",
            &f.name,
            Role::SubclassFeature,
            Some(level),
            Some(&root_id),
            &f.summary,
            &f.quote,
        );
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

/// Recompute `plan.elements` (and group root ids) from the groups.
pub(crate) fn rebuild(plan: &mut ImportPlan, store: &ContentStore, prefix: &str, source: &str) {
    let mut b = Builder {
        prefix,
        source,
        used: store.iter().map(|s| s.element.id.clone()).collect(),
        out: Vec::new(),
        notes: Vec::new(),
    };
    for g in &mut plan.groups {
        g.root_id = None;
        match (g.kind, &g.outline, &g.parent) {
            (ImportKind::Class, Some(o), _) => g.root_id = Some(class_group(&mut b, o)),
            (ImportKind::Subclass, Some(o), Some(p)) => {
                g.root_id = Some(subclass_group(&mut b, o, p))
            }
            (ImportKind::Subclass | ImportKind::Class, _, _) => {}
            (kind, _, _) => {
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
                g.root_id = Some(b.push(
                    kind.element_type(),
                    &g.name,
                    Role::Standalone,
                    None,
                    None,
                    &brief,
                    &quote,
                ));
            }
        }
    }
    plan.elements = b.out;
    plan.notes = b.notes;
}

/// A plan group for a standalone item.
pub(crate) fn standalone(kind: ImportKind, name: &str) -> PlanGroup {
    PlanGroup {
        kind,
        name: name.to_string(),
        root_id: None,
        outline: None,
        parent: None,
        choice: None,
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
    fn locates_a_quote_paragraph() {
        let src = "Intro\n\nRally Banner.\nYou raise it.\n\nOther";
        let ex = locate(src, "rally banner.", "x").unwrap();
        assert_eq!((ex.start_line, ex.end_line), (3, 4));
        assert!(locate(src, "", "nothing here").is_none());
    }
}
