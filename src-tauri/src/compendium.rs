//! Compendium services: ranked search with filters, element detail and facets.

use crate::dto::{ElementSummary, Page};
use crate::dto_ext::{CompendiumFacets, CompendiumQuery, ElementDetail, ElementFact, Facet};
use crate::elements::{plain_text, summary};
use crate::error::{ApiError, ApiResult};
use crate::gear_info::{category, cost_cp, weapon_properties};
use crate::ids::{title_case, ui_type};
use crate::sanitize::sanitize_html;
use crate::state::AppState;
use borealis_content::{ContentStore, Element, Rule, Ruleset};
use std::collections::BTreeMap;

const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 500;
const MAX_QUERY_CHARS: usize = 200;
/// Plumbing types the compendium never lists.
const HIDDEN_TYPES: [&str; 4] = ["multiclass", "asi", "list", "source"];
/// Engine-derived type searchable on request (`types: ["multiclass"]`): the level-up flyout reads
/// each class's multiclass prerequisite from it.
const MULTICLASS_TYPE: &str = "Multiclass";
const TYPE_LABELS: [(&str, &str); 16] = [
    ("race", "Races"),
    ("subrace", "Subraces"),
    ("class", "Classes"),
    ("archetype", "Subclasses"),
    ("fighting-style", "Fighting styles"),
    ("background", "Backgrounds"),
    ("feat", "Feats"),
    ("spell", "Spells"),
    ("weapon", "Weapons"),
    ("armor", "Armor"),
    ("item", "Gear"),
    ("magic-item", "Magic items"),
    ("skill", "Skills"),
    ("language", "Languages"),
    ("class-feature", "Class features"),
    ("racial-trait", "Racial traits"),
];
/// Setters shown as plain facts, in order.
const FACT_SETTERS: [(&str, &str); 10] = [
    ("level", "Level"),
    ("school", "School"),
    ("time", "Casting time"),
    ("range", "Range"),
    ("duration", "Duration"),
    ("rarity", "Rarity"),
    ("armorClass", "Armor class"),
    ("stealth", "Stealth"),
    ("strength", "Strength required"),
    ("speed", "Speed"),
];

fn type_label(ty: &str) -> String {
    TYPE_LABELS.iter().find(|(id, _)| *id == ty).map_or_else(
        || title_case(&ty.replace('-', " ")),
        |(_, l)| (*l).to_string(),
    )
}

fn is_listed(el: &Element, wanted_types: Option<&[String]>) -> bool {
    let ty = ui_type(&el.ty);
    match wanted_types {
        Some(t) if !t.is_empty() => t.iter().any(|w| w == ty),
        _ => !HIDDEN_TYPES.contains(&ty),
    }
}

/// 0 exact, 1 prefix, 2 word prefix, 3 substring of the name; `None` when the name misses.
fn name_rank(name: &str, needle: &str) -> Option<u8> {
    let n = name.to_lowercase();
    if n == needle {
        Some(0)
    } else if n.starts_with(needle) {
        Some(1)
    } else if n
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(needle))
    {
        Some(2)
    } else if n.contains(needle) {
        Some(3)
    } else {
        None
    }
}

const DESCRIPTION_RANK: u8 = 4;

fn parse_ruleset(text: Option<&str>) -> ApiResult<Option<Ruleset>> {
    match text {
        None | Some("any" | "") => Ok(None),
        Some("dnd2014") => Ok(Some(Ruleset::Dnd2014)),
        Some("dnd2024") => Ok(Some(Ruleset::Dnd2024)),
        Some(_) => Err(ApiError::invalid("Unknown ruleset.")),
    }
}

impl AppState {
    /// Search the whole content store. Name matches rank above description matches; ties sort by
    /// element type then name.
    ///
    /// # Errors
    /// `invalid_argument` for an overlong query or unknown ruleset.
    pub fn search_compendium(&self, q: &CompendiumQuery) -> ApiResult<Page<ElementSummary>> {
        if q.query.chars().count() > MAX_QUERY_CHARS {
            return Err(ApiError::invalid("The search text is too long."));
        }
        let ruleset = parse_ruleset(q.ruleset.as_deref())?;
        let hub = self.hub()?;
        let needle = q.query.trim().to_lowercase();
        let mut hits: Vec<(u8, &Element)> = hub
            .store
            .iter()
            .map(|s| &s.element)
            .chain(hub.derived_elements(MULTICLASS_TYPE))
            .filter(|e| is_listed(e, q.types.as_deref()))
            .filter(|e| {
                q.sources
                    .as_ref()
                    .map_or(true, |s| s.is_empty() || s.contains(&e.source))
            })
            .filter(|e| ruleset.map_or(true, |r| e.ruleset.map_or(true, |x| x.compatible_with(r))))
            .filter_map(|e| {
                if needle.is_empty() {
                    return Some((0, e));
                }
                name_rank(&e.name, &needle)
                    .or_else(|| {
                        plain_text(&e.description)
                            .to_lowercase()
                            .contains(&needle)
                            .then_some(DESCRIPTION_RANK)
                    })
                    .map(|r| (r, e))
            })
            .collect();
        hits.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| ui_type(&a.1.ty).cmp(ui_type(&b.1.ty)))
                .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        let total = hits.len();
        let take = if q.limit == 0 {
            DEFAULT_PAGE
        } else {
            q.limit.min(MAX_PAGE)
        };
        let items = hits
            .into_iter()
            .skip(q.offset)
            .take(take)
            .map(|(_, e)| summary(&hub.store, e))
            .collect();
        Ok(Page { total, items })
    }

    /// One element with sanitised description, facts and grants.
    ///
    /// # Errors
    /// `not_found` for an unknown id.
    pub fn get_element(&self, element_id: &str) -> ApiResult<ElementDetail> {
        let hub = self.hub()?;
        let el = hub
            .element(element_id)
            .ok_or_else(|| ApiError::not_found("That element does not exist."))?;
        Ok(ElementDetail {
            summary: summary(&hub.store, el),
            description_html: sanitize_html(&el.description),
            facts: facts(el),
            grants: grants(&hub.store, el),
        })
    }

    /// Type and source filters with their counts.
    ///
    /// # Errors
    /// `internal` when the content lock is poisoned.
    pub fn compendium_facets(&self) -> ApiResult<CompendiumFacets> {
        let hub = self.hub()?;
        let mut types: BTreeMap<&str, usize> = BTreeMap::new();
        let mut sources: BTreeMap<&str, usize> = BTreeMap::new();
        for e in hub.store.iter().map(|s| &s.element) {
            if !is_listed(e, None) {
                continue;
            }
            *types.entry(ui_type(&e.ty)).or_default() += 1;
            *sources.entry(e.source.as_str()).or_default() += 1;
        }
        Ok(CompendiumFacets {
            types: types
                .into_iter()
                .map(|(id, count)| Facet {
                    id: id.into(),
                    label: type_label(id),
                    count,
                })
                .collect(),
            sources: sources
                .into_iter()
                .map(|(id, count)| Facet {
                    id: id.into(),
                    label: id.into(),
                    count,
                })
                .collect(),
        })
    }
}

fn fact(label: &str, value: impl Into<String>) -> Option<ElementFact> {
    let value = value.into();
    (!value.trim().is_empty()).then(|| ElementFact {
        label: label.into(),
        value: value.trim().into(),
    })
}

fn flag(el: &Element, name: &str) -> bool {
    el.setter_value(name)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

fn components(el: &Element) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (setter, letter) in [
        ("hasVerbalComponent", "V"),
        ("hasSomaticComponent", "S"),
        ("hasMaterialComponent", "M"),
    ] {
        if flag(el, setter) {
            parts.push(letter.into());
        }
    }
    let mut text = parts.join(", ");
    if let Some(m) = el
        .setter_value("materialComponent")
        .filter(|m| !m.trim().is_empty())
    {
        text.push_str(&format!(" ({})", m.trim()));
    }
    text
}

fn cost_text(el: &Element) -> String {
    el.setter("cost").map_or_else(String::new, |c| {
        let unit = c.attribute("currency").unwrap_or("gp");
        format!("{} {unit}", c.value.trim())
    })
}

fn damage_text(el: &Element) -> String {
    el.setter("damage").map_or_else(String::new, |d| {
        let kind = d.attribute("type").unwrap_or_default();
        format!("{} {kind}", d.value.trim())
    })
}

pub(crate) fn facts(el: &Element) -> Vec<ElementFact> {
    let mut out: Vec<ElementFact> = Vec::new();
    out.extend(fact("Type", type_label(ui_type(&el.ty))));
    out.extend(fact("Source", el.source.clone()));
    out.extend(fact("Prerequisite", plain_text(&el.prerequisite)));
    for (setter, label) in FACT_SETTERS {
        if let Some(v) = el.setter_value(setter) {
            let v = if setter == "level" && v.trim() == "0" {
                "Cantrip"
            } else {
                v
            };
            out.extend(fact(label, v));
        }
    }
    if el.ty == "Spell" {
        out.extend(fact("Components", components(el)));
        if flag(el, "isConcentration") {
            out.extend(fact("Concentration", "Yes"));
        }
        if flag(el, "isRitual") {
            out.extend(fact("Ritual", "Yes"));
        }
    } else {
        out.extend(fact("Category", category(el)));
        if cost_cp(el) > 0 {
            out.extend(fact("Cost", cost_text(el)));
        }
        out.extend(fact("Damage", damage_text(el)));
        out.extend(fact("Properties", weapon_properties(el).join(", ")));
        if let Some(w) = el.setter("weight").map(|w| w.value.clone()) {
            out.extend(fact("Weight", w));
        }
        if crate::gear_info::requires_attunement(el) {
            out.extend(fact("Attunement", "Required"));
        }
    }
    out
}

fn name_of(store: &ContentStore, id: &str) -> String {
    store
        .element(id)
        .map_or_else(|| id.to_string(), |e| e.name.clone())
}

fn grants(store: &ContentStore, el: &Element) -> Vec<String> {
    let mut out = Vec::new();
    for rule in el.all_rules() {
        match rule {
            Rule::Grant(g) => {
                let name = name_of(store, &g.id);
                out.push(
                    g.level
                        .filter(|l| *l > 1)
                        .map_or(name.clone(), |l| format!("{name} (level {l})")),
                );
            }
            Rule::Select(s) => {
                let n = s.number.unwrap_or(1);
                let what = if s.name.is_empty() {
                    s.ty.clone()
                } else {
                    s.name.clone()
                };
                out.push(format!("Choose {n}: {what}"));
            }
            Rule::Stat(_) | Rule::Unknown(_) | Rule::Custom(_) => {}
        }
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::name_rank;

    #[test]
    fn name_rank_orders_exact_prefix_word_substring() {
        assert_eq!(name_rank("Fire", "fire"), Some(0));
        assert_eq!(name_rank("Fireball", "fire"), Some(1));
        assert_eq!(name_rank("Wall of Fire", "fire"), Some(2));
        assert_eq!(name_rank("Campfire", "fire"), Some(3));
        assert_eq!(name_rank("Shield", "fire"), None);
    }
}
