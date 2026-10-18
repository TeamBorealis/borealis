//! A carried-over SRD option select offers the book's own list.
//!
//! An imported class feature that takes the bundled SRD feature's rules
//! ([`crate::srd_features::portable`]) keeps the SRD's `Class Feature` selects over the SRD's
//! own option elements (SRD 5.1 `Favored Terrain`: seven terrains). The book may list more: the
//! PHB's Natural Explorer names eight ("arctic, coast, desert, forest, grassland, mountain,
//! swamp, or the Underdark"). The import reads that list from the feature's own text
//! ("Choose one type of <select name>: a, b, or c.") and, when most of it is the SRD's list,
//! adds one option element per missing entry, supporting a tag of the book's own; the select
//! then supports the SRD tag or that tag. Deterministic, never AI (IM1).

use super::{new_element, State};
use crate::ids::IdKind;
use crate::srd_features::{srd_option_select, SrdFeatures};
use crate::util::{id_fragment, title_case};
use borealis_content::{Element, Rule, Ruleset};
use std::collections::{BTreeMap, BTreeSet};

/// The feature whose selects are extended.
pub struct Owner<'a> {
    /// Feature name (`Natural Explorer`).
    pub feature: &'a str,
    /// Class name.
    pub class: &'a str,
    /// Source abbreviation of the feature (`PHB`).
    pub source: &'a str,
    /// Source display name.
    pub source_display: &'a str,
    /// Ruleset of the feature.
    pub ruleset: Ruleset,
    /// The feature's plain text.
    pub text: &'a str,
}

/// Extend the SRD option selects of `rules` with the entries the book lists and the SRD lacks;
/// returns the new option elements.
pub fn extend(
    srd: &SrdFeatures,
    st: &mut State,
    owner: &Owner<'_>,
    rules: &mut [Rule],
) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut tags: BTreeMap<String, Option<String>> = BTreeMap::new();
    for rule in rules.iter_mut() {
        let Rule::Select(s) = rule else { continue };
        if s.ty != "Class Feature" || !srd_option_select(s.supports.as_deref()) {
            continue;
        }
        let Some(tag) = s
            .supports
            .clone()
            .filter(|t| !t.contains(['|', ',', '&', '!']))
        else {
            continue;
        };
        let own = tags
            .entry(tag.clone())
            .or_insert_with(|| {
                let book = book_list(owner.text, &s.name)?;
                let missing = missing(&book, srd.option_names(owner.ruleset, &tag));
                if missing.is_empty() {
                    return None;
                }
                let own = format!("5ET_{}_{}", id_fragment(owner.source), id_fragment(&s.name));
                for item in missing {
                    elements.push(option_element(st, owner, &s.name, &own, &item));
                }
                Some(own)
            })
            .clone();
        if let Some(own) = own {
            s.supports = Some(format!("{tag}||{own}"));
        }
    }
    elements
}

fn option_element(
    st: &mut State,
    owner: &Owner<'_>,
    label: &str,
    tag: &str,
    item: &str,
) -> Element {
    let name = title_case(item);
    let id = st.ids.id(
        IdKind::ClassFeature,
        owner.source,
        &[owner.class, label, &name],
        &format!("{}|{}|option|{label}|{name}", owner.class, owner.source),
    );
    let mut el = new_element(
        &id,
        &name,
        "Class Feature",
        owner.source_display,
        owner.ruleset,
    );
    el.supports = vec![tag.to_string()];
    el.description = format!(
        "<p>{name}: one of the {} choices of {}.</p>",
        label.to_lowercase(),
        owner.feature
    );
    el
}

/// The list after "type of <label>:" up to the end of its sentence, lower case, without
/// "or" / "and" / "the" (`["arctic", ..., "swamp", "underdark"]`).
fn book_list(text: &str, label: &str) -> Option<Vec<String>> {
    let lower = text.to_lowercase();
    let needle = format!("type of {}:", label.to_lowercase());
    let rest = &lower[lower.find(&needle)? + needle.len()..];
    let sentence = &rest[..rest.find('.').unwrap_or(rest.len())];
    let items: Vec<String> = sentence
        .split(',')
        .flat_map(|p| p.split(" or "))
        .flat_map(|p| p.split(" and "))
        .map(|p| {
            let p = p.trim();
            let p = p
                .strip_prefix("or ")
                .or_else(|| p.strip_prefix("and "))
                .unwrap_or(p);
            p.strip_prefix("the ").unwrap_or(p).trim().to_string()
        })
        .filter(|p| !p.is_empty())
        .collect();
    (items.len() > 1).then_some(items)
}

fn normalize(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// Book entries the SRD options lack, when most of the book's list is the SRD's (otherwise the
/// sentence is not the list of these options and nothing is added).
fn missing(book: &[String], srd: &[String]) -> Vec<String> {
    let known: BTreeSet<String> = srd.iter().map(|n| normalize(n)).collect();
    let (have, lack): (Vec<&String>, Vec<&String>) =
        book.iter().partition(|b| known.contains(&normalize(b)));
    if have.len() * 2 < book.len() {
        return Vec::new();
    }
    lack.into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, Ruleset};
    use serde_json::json;

    const TEXT: &str = "Choose one type of favored terrain: arctic, coast, desert, forest, \
        grassland, mountain, swamp, or the Underdark. When you make an Intelligence check...";

    fn srd() -> SrdFeatures {
        let names = [
            "Arctic",
            "Coast",
            "Desert",
            "Forest",
            "Grassland",
            "Mountain",
            "Swamp",
        ];
        let elements: Vec<_> = names
            .iter()
            .map(|n| {
                json!({"id": format!("ID_T_{n}"), "name": n, "type": "Class Feature",
                "source": "SRD", "supports": ["Favored Terrain"]})
            })
            .collect();
        let file: ContentFile = serde_json::from_value(json!({
            "format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements}))
        .unwrap();
        let mut f = SrdFeatures::default();
        f.index_file(&file, None);
        f
    }

    fn owner(text: &str) -> Owner<'_> {
        Owner {
            feature: "Natural Explorer",
            class: "Ranger",
            source: "PHB",
            source_display: "Player's Handbook",
            ruleset: Ruleset::Dnd2014,
            text,
        }
    }

    fn terrain_selects() -> Vec<Rule> {
        [1, 6]
            .into_iter()
            .map(|level| {
                serde_json::from_value(json!({"kind": "select", "type": "Class Feature",
                    "name": "Favored Terrain", "level": level, "supports": "Favored Terrain"}))
                .unwrap()
            })
            .collect()
    }

    fn supports(rules: &[Rule]) -> Vec<String> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Select(s) => s.supports.clone(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_book_list_adds_what_the_srd_lacks_once() {
        let mut st = State::default();
        let mut rules = terrain_selects();
        let added = extend(&srd(), &mut st, &owner(TEXT), &mut rules);
        assert_eq!(added.len(), 1, "{added:?}");
        assert_eq!(added[0].name, "Underdark");
        assert_eq!(added[0].ty, "Class Feature");
        assert_eq!(added[0].supports, ["5ET_PHB_FAVORED_TERRAIN"]);
        assert_eq!(
            supports(&rules),
            ["Favored Terrain||5ET_PHB_FAVORED_TERRAIN"; 2]
        );
    }

    #[test]
    fn the_srd_list_or_another_sentence_changes_nothing() {
        let srd = srd();
        for text in [
            "Choose one type of favored terrain: arctic, coast, desert, forest, grassland, \
             mountain, or swamp.",
            "Choose one type of favored terrain: ice, sand, or lava.",
            "You are familiar with one type of natural environment.",
        ] {
            let mut st = State::default();
            let mut rules = terrain_selects();
            assert!(extend(&srd, &mut st, &owner(text), &mut rules).is_empty());
            assert_eq!(supports(&rules), ["Favored Terrain"; 2], "{text}");
        }
    }

    #[test]
    fn list_parsing() {
        assert_eq!(
            book_list(
                "Choose a type of favored enemy: aberrations, beasts or undead.",
                "Favored Enemy"
            ),
            Some(vec!["aberrations".into(), "beasts".into(), "undead".into()])
        );
        assert_eq!(
            book_list("Choose a type of favored enemy: undead.", "Favored Enemy"),
            None
        );
    }
}
