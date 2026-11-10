//! Picker rows for the creator's reference fields (spec 5.1, `homebrew_reference_options`): loaded elements plus the
//! pack's unsaved new elements, filtered by type, tags, sources, edition and (for spells) the Magic filter bar, sorted
//! by name and paged. Ids travel in the rows and are never shown.

use crate::dto::{Page, Ruleset};
use crate::elements::summary;
use crate::error::ApiResult;
use crate::gear_facts::spell_facts;
use crate::homebrew_dto::{
    HomebrewPageRequest, HomebrewPickerFilters, HomebrewPickerRow, HomebrewPickerTarget,
};
use crate::state::AppState;
use borealis_authoring::form::{assign_ids, form_to_elements, CatalogBlockCodec, FormContext};
use borealis_content::{ContentStore, Element};
use std::collections::{BTreeMap, HashSet};

/// Most rows one page holds.
const MAX_PAGE: usize = 500;

/// The picker type of spell lists (WB-11, the spell form's lists): one row per list a loaded or drafted class
/// declares (`spellcasting.list.supports`, a plain tag), its name as id and name. Not an element type.
pub const SPELL_LIST_TYPE: &str = "Spell List";

/// A `spellcasting.list.supports` that is one plain tag ("Wizard"), not an expression.
fn plain_list_tag(supports: &str) -> Option<&str> {
    let t = supports.trim();
    let plain = !t.is_empty()
        && t.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '\'' | '_'));
    (plain && !t.starts_with("ID_")).then_some(t)
}

/// One row per spell list `elements` declare, by name (first spelling and book kept), filtered by `query`.
fn spell_list_rows<'a>(
    elements: impl Iterator<Item = (&'a Element, bool)>,
    query: &str,
    page: HomebrewPageRequest,
) -> Page<HomebrewPickerRow> {
    let mut lists: BTreeMap<String, HomebrewPickerRow> = BTreeMap::new();
    for (el, draft) in elements {
        let Some(name) = el
            .spellcasting
            .as_ref()
            .and_then(|s| s.list.as_ref())
            .and_then(|l| plain_list_tag(&l.supports))
        else {
            continue;
        };
        if !name.to_lowercase().contains(query) {
            continue;
        }
        lists
            .entry(name.to_lowercase())
            .or_insert_with(|| HomebrewPickerRow {
                id: name.to_string(),
                name: name.to_string(),
                ty: SPELL_LIST_TYPE.to_string(),
                summary: String::new(),
                prerequisites: None,
                source_name: el.source.clone(),
                edition: None,
                draft,
                level: None,
                school: None,
                spell: None,
            });
    }
    let total = lists.len();
    let items = lists
        .into_values()
        .skip(page.offset)
        .take(page.limit.min(MAX_PAGE))
        .collect();
    Page { total, items }
}

/// One candidate before paging: the element and whether it is an unsaved draft of the pack.
struct Candidate<'a> {
    element: &'a Element,
    draft: bool,
}

fn matches_edition(el: &Element, wanted: Option<Ruleset>) -> bool {
    match (wanted, el.ruleset) {
        (None | Some(Ruleset::Any), _) | (_, None | Some(Ruleset::Any)) => true,
        (Some(w), Some(r)) => w == r,
    }
}

fn spell_level(el: &Element) -> Option<u32> {
    el.setter_value("level").and_then(|v| v.trim().parse().ok())
}

/// The element passes the target and the filters (the query is matched on the name, any case).
fn passes(
    el: &Element,
    target: &HomebrewPickerTarget,
    query: &str,
    filters: &HomebrewPickerFilters,
) -> bool {
    if !target.types.is_empty() && !target.types.contains(&el.ty) {
        return false;
    }
    if let Some(tags) = target.tags.as_ref().filter(|t| !t.is_empty()) {
        if !tags.iter().any(|t| el.supports.iter().any(|s| s == t)) {
            return false;
        }
    }
    if !query.is_empty() && !el.name.to_lowercase().contains(query) {
        return false;
    }
    if let Some(sources) = filters.sources.as_ref().filter(|s| !s.is_empty()) {
        if !sources.contains(&el.source) {
            return false;
        }
    }
    if !matches_edition(el, filters.edition) {
        return false;
    }
    spell_filters_pass(el, filters)
}

fn spell_filters_pass(el: &Element, f: &HomebrewPickerFilters) -> bool {
    let wants_spell = f.spell_school.is_some()
        || f.spell_levels.as_ref().is_some_and(|l| !l.is_empty())
        || f.ritual == Some(true)
        || f.concentration == Some(true);
    if !wants_spell {
        return true;
    }
    if el.ty != "Spell" {
        return false;
    }
    if let Some(school) = f.spell_school.as_deref().filter(|s| !s.is_empty()) {
        if !el
            .setter_value("school")
            .is_some_and(|s| s.trim().eq_ignore_ascii_case(school))
        {
            return false;
        }
    }
    if let Some(levels) = f.spell_levels.as_ref().filter(|l| !l.is_empty()) {
        if !spell_level(el).is_some_and(|l| levels.contains(&l)) {
            return false;
        }
    }
    let facts = spell_facts(el);
    (f.ritual != Some(true) || facts.as_ref().is_some_and(|s| s.ritual))
        && (f.concentration != Some(true) || facts.as_ref().is_some_and(|s| s.concentration))
}

fn row(store: &ContentStore, c: &Candidate<'_>) -> HomebrewPickerRow {
    let el = c.element;
    let s = summary(store, el);
    let short = el
        .setter_value("short")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map_or(s.short_description, str::to_string);
    HomebrewPickerRow {
        id: el.id.clone(),
        name: el.name.clone(),
        ty: el.ty.clone(),
        summary: short,
        prerequisites: s.prerequisites,
        source_name: el.source.clone(),
        edition: el.ruleset.filter(|r| *r != Ruleset::Any),
        draft: c.draft,
        level: s.level,
        school: s.school,
        spell: s.spell,
    }
}

impl AppState {
    /// Picker rows (spec 5.1): loaded elements, and with `draft_id` the unsaved new elements of that draft's pack
    /// (with the id they would get if saved now), each once, sorted by name.
    ///
    /// # Errors
    /// `not_found` for an unknown draft.
    pub fn homebrew_reference_options(
        &self,
        target: &HomebrewPickerTarget,
        query: &str,
        filters: &HomebrewPickerFilters,
        page: HomebrewPageRequest,
        draft_id: Option<&str>,
    ) -> ApiResult<Page<HomebrewPickerRow>> {
        let hub = self.hub()?;
        let query = query.trim().to_lowercase();
        let drafted = match draft_id {
            Some(id) => self.hb_unsaved_elements(id)?,
            None => Vec::new(),
        };
        if target.types.iter().any(|t| t == SPELL_LIST_TYPE) {
            let own = drafted.iter().map(|e| (e, true));
            let loaded = hub.store.iter().map(|s| (&s.element, false));
            return Ok(spell_list_rows(own.chain(loaded), &query, page));
        }
        let mut seen: HashSet<&str> = HashSet::new();
        let mut found: Vec<Candidate<'_>> = drafted
            .iter()
            .filter(|e| passes(e, target, &query, filters))
            .filter(|e| seen.insert(e.id.as_str()))
            .map(|element| Candidate {
                element,
                draft: true,
            })
            .collect();
        found.extend(
            hub.store
                .iter()
                .map(|s| &s.element)
                .filter(|e| passes(e, target, &query, filters))
                .filter(|e| seen.insert(e.id.as_str()))
                .map(|element| Candidate {
                    element,
                    draft: false,
                }),
        );
        found.sort_by(|a, b| {
            a.element
                .name
                .to_lowercase()
                .cmp(&b.element.name.to_lowercase())
                .then_with(|| a.element.source.cmp(&b.element.source))
                .then_with(|| a.element.id.cmp(&b.element.id))
        });
        let total = found.len();
        let items = found
            .iter()
            .skip(page.offset)
            .take(page.limit.min(MAX_PAGE))
            .map(|c| row(&hub.store, c))
            .collect();
        Ok(Page { total, items })
    }

    /// The new (never saved) elements of the drafts of `draft_id`'s pack, the open one included, with provisional
    /// ids.
    fn hb_unsaved_elements(&self, draft_id: &str) -> ApiResult<Vec<Element>> {
        let open = self.hb_drafts.get(&self.paths.data, draft_id)?;
        let Ok(pack) = self.hb_pack(&open.pack_id) else {
            return Ok(Vec::new());
        };
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let mut ids = self.hb_ids(&hub, &pack)?;
        let mut drafts = self.hb_drafts.list(&self.paths.data)?;
        if !drafts.iter().any(|d| d.draft_id == open.draft_id) {
            drafts.push(open);
        }
        Ok(drafts
            .into_iter()
            .filter(|d| d.pack_id == pack.id() && d.element_id.is_none())
            .filter_map(|d| form_to_elements(&assign_ids(&d.form, &mut ids), &cx).ok())
            .flatten()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spell(name: &str, level: &str, school: &str, ritual: bool) -> Element {
        let mut e = Element::new(format!("ID_T_SPELL_{name}"), name, "Spell", "Test");
        e.setters
            .push(borealis_content::Setter::new("level", level));
        e.setters
            .push(borealis_content::Setter::new("school", school));
        if ritual {
            e.setters
                .push(borealis_content::Setter::new("isRitual", "true"));
        }
        e
    }

    fn target(types: &[&str]) -> HomebrewPickerTarget {
        HomebrewPickerTarget {
            types: types.iter().map(|t| (*t).to_string()).collect(),
            tags: None,
            multiple: None,
        }
    }

    #[test]
    fn rows_pass_type_query_edition_and_spell_filters() {
        let mut fire = spell("Firebolt", "0", "Evocation", false);
        fire.ruleset = Some(Ruleset::Dnd2014);
        let alarm = spell("Alarm", "1", "Abjuration", true);
        let none = HomebrewPickerFilters::default();
        assert!(passes(&fire, &target(&["Spell"]), "fire", &none));
        assert!(!passes(&fire, &target(&["Feat"]), "", &none));
        assert!(!passes(&fire, &target(&[]), "zap", &none));
        let only_2024 = HomebrewPickerFilters {
            edition: Some(Ruleset::Dnd2024),
            ..HomebrewPickerFilters::default()
        };
        assert!(!passes(&fire, &target(&[]), "", &only_2024));
        assert!(
            passes(&alarm, &target(&[]), "", &only_2024),
            "no edition: both"
        );
        let rituals = HomebrewPickerFilters {
            ritual: Some(true),
            spell_levels: Some(vec![1]),
            spell_school: Some("abjuration".into()),
            ..HomebrewPickerFilters::default()
        };
        assert!(passes(&alarm, &target(&[]), "", &rituals));
        assert!(!passes(&fire, &target(&[]), "", &rituals));
        let mut tagged = target(&[]);
        tagged.tags = Some(vec!["Wizard".into()]);
        assert!(!passes(&alarm, &tagged, "", &none));
    }
}
