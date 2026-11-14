//! Picker rows for the creator's reference fields (spec 5.1, `homebrew_reference_options`): loaded elements plus the
//! pack's unsaved new elements, filtered by type, tags, sources, edition and (for spells) the Magic filter bar, sorted
//! by name and paged. Ids travel in the rows and are never shown.

use crate::dto::{Page, Ruleset};
use crate::elements::summary;
use crate::error::ApiResult;
use crate::gear_facts::spell_facts;
use crate::homebrew_drafts::DraftFile;
use crate::homebrew_dto::{
    HomebrewPageRequest, HomebrewPickerFilters, HomebrewPickerRow, HomebrewPickerTarget,
};
use crate::homebrew_packs::CreatorPack;
use crate::state::{AppState, ContentHub};
use borealis_authoring::form::{
    assign_ids, feat_parameter_words, form_to_elements, item_categories, loaded_tags, open_types,
    setter_names, CatalogBlockCodec, FormContext, ProvisionalClass, WithElements,
    ELEMENT_TYPE_PICKER, FEAT_PARAMETER_PICKER, ITEM_CATEGORY_PICKER, SETTER_NAME_PICKER,
    TAG_PICKER,
};
use borealis_content::{ContentStore, Element};
use std::collections::{BTreeMap, HashSet};

/// Most rows one page holds.
const MAX_PAGE: usize = 500;

/// The picker type of spell lists (WB-11, the spell form's lists): one row per list a loaded or drafted class
/// declares (`spellcasting.list.supports`: a plain tag, or each list tag of an expression), its name as id and name.
/// Not an element type.
pub const SPELL_LIST_TYPE: &str = borealis_authoring::spell_lists::SPELL_LIST_PICKER;

/// The picker type of subclasses' expanded lists (SPELL LISTS, spec 12.4): one row per subclass that has one (its own
/// `spellcasting.extends` by spell id or a feature's it grants), and per subclass of a non-bundled pack whose class
/// casts (a list it may start). The subclass's id and name. Not an element type.
pub const EXPANDED_LIST_TYPE: &str = "Expanded Spell List";

/// A `spellcasting.list.supports` that is one plain tag ("Wizard"), not an expression.
pub(crate) fn plain_list_tag(supports: &str) -> Option<&str> {
    let t = supports.trim();
    let plain = !t.is_empty()
        && t.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '\'' | '_'));
    (plain && !t.starts_with("ID_")).then_some(t)
}

/// One row per spell list `elements` declare, by name (first spelling and book kept), filtered by `query`. A list
/// written as an expression (`(Cleric||Wizard)`, `Wizard,(Abjuration||Evocation)`) declares each list tag it names.
fn spell_list_rows<'a>(
    elements: impl Iterator<Item = (&'a Element, bool)>,
    query: &str,
    page: HomebrewPageRequest,
) -> Page<HomebrewPickerRow> {
    let mut lists: BTreeMap<String, HomebrewPickerRow> = BTreeMap::new();
    for (el, draft) in elements {
        let Some(list) = el.spellcasting.as_ref().and_then(|s| s.list.as_ref()) else {
            continue;
        };
        for name in borealis_authoring::spell_lists::list_tags(&list.supports) {
            if !name.to_lowercase().contains(query) {
                continue;
            }
            lists
                .entry(name.to_lowercase())
                .or_insert_with(|| HomebrewPickerRow {
                    id: name.clone(),
                    name,
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
    }
    let total = lists.len();
    let items = lists
        .into_values()
        .skip(page.offset)
        .take(page.limit.min(MAX_PAGE))
        .collect();
    Page { total, items }
}

/// The features a subclass grants that are loaded.
pub(crate) fn granted<'s>(store: &'s ContentStore, sub: &Element) -> Vec<&'s Element> {
    sub.rules
        .iter()
        .filter_map(|r| match r {
            borealis_content::Rule::Grant(g) => store.element(&g.id),
            _ => None,
        })
        .collect()
}

/// The subclass has an expanded list (its own, or a feature's it grants).
pub(crate) fn has_expanded_list(store: &ContentStore, sub: &Element) -> bool {
    !borealis_authoring::pack::expanded_ids(sub).is_empty()
        || granted(store, sub)
            .iter()
            .any(|f| !borealis_authoring::pack::expanded_ids(f).is_empty())
}

/// The class list a subclass's class declares (`Warlock`), from the class the subclass names (`data.class`).
pub(crate) fn class_list_name(store: &ContentStore, sub: &Element) -> Option<String> {
    let class = sub
        .data
        .get("class")
        .and_then(serde_json::Value::as_str)
        .and_then(|id| store.element(id))?;
    class
        .spellcasting
        .as_ref()
        .filter(|s| s.list.is_some())
        .and_then(|s| s.name.clone())
        .filter(|n| !n.trim().is_empty())
}

/// One row per subclass with an expanded list, or of a non-bundled pack whose class casts.
fn expanded_list_rows(
    store: &ContentStore,
    query: &str,
    page: HomebrewPageRequest,
) -> Page<HomebrewPickerRow> {
    let mut found: Vec<HomebrewPickerRow> = store
        .by_type("Archetype")
        .filter(|s| {
            has_expanded_list(store, &s.element)
                || (!is_bundled(&s.file) && class_list_name(store, &s.element).is_some())
        })
        .filter(|s| s.element.name.to_lowercase().contains(query))
        .map(|s| HomebrewPickerRow {
            ty: EXPANDED_LIST_TYPE.to_string(),
            ..row(
                store,
                &Candidate {
                    element: &s.element,
                    draft: false,
                },
            )
        })
        .collect();
    found.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    let total = found.len();
    let items = found
        .into_iter()
        .skip(page.offset)
        .take(page.limit.min(MAX_PAGE))
        .collect();
    Page { total, items }
}

/// One row per name (the generic form's open types and setter names, WB-19): the name as id and name, filtered by
/// `query`, in the order given.
fn word_rows(
    names: Vec<String>,
    ty: &str,
    query: &str,
    page: HomebrewPageRequest,
) -> Page<HomebrewPickerRow> {
    let found: Vec<String> = names
        .into_iter()
        .filter(|n| n.to_lowercase().contains(query))
        .collect();
    let total = found.len();
    let items = found
        .into_iter()
        .skip(page.offset)
        .take(page.limit.min(MAX_PAGE))
        .map(|name| HomebrewPickerRow {
            id: name.clone(),
            name,
            ty: ty.to_string(),
            summary: String::new(),
            prerequisites: None,
            source_name: String::new(),
            edition: None,
            draft: false,
            level: None,
            school: None,
            spell: None,
        })
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

/// The order of picker rows: by name (any case), then book, then id.
fn picker_order(a: &Element, b: &Element) -> std::cmp::Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.source.cmp(&b.source))
        .then_with(|| a.id.cmp(&b.id))
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

/// The picker row of a loaded (or the pack's own) element, as every picker shows it.
pub(crate) fn picker_row(
    store: &ContentStore,
    element: &Element,
    draft: bool,
) -> HomebrewPickerRow {
    row(store, &Candidate { element, draft })
}

/// The file belongs to a bundled pack (`<pack>/<path>`).
pub(crate) fn is_bundled(file: &str) -> bool {
    let pack = file.split('/').next().unwrap_or_default();
    crate::state::BUNDLED_PACKS.contains(&pack)
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
        if target.types.iter().any(|t| t == EXPANDED_LIST_TYPE) {
            return Ok(expanded_list_rows(&hub.store, &query, page));
        }
        if target.types.iter().any(|t| t == SPELL_LIST_TYPE) {
            let own = drafted.iter().map(|e| (e, true));
            let loaded = hub.store.iter().map(|s| (&s.element, false));
            return Ok(spell_list_rows(own.chain(loaded), &query, page));
        }
        // the generic form (spec 4.12): the open types loaded content uses, the setter names a type uses
        let words = WithElements {
            base: hub.store.as_ref(),
            extra: &drafted,
        };
        if target.types.iter().any(|t| t == ELEMENT_TYPE_PICKER) {
            return Ok(word_rows(
                open_types(&words),
                ELEMENT_TYPE_PICKER,
                &query,
                page,
            ));
        }
        // a feat's own choices (a background's origin feat, spec 4.3): their names, or one choice's options
        if target.types.first().map(String::as_str) == Some(FEAT_PARAMETER_PICKER) {
            let feat = target.types.get(1).map_or("", String::as_str);
            let param = target.types.get(2).map(String::as_str);
            let words = feat_parameter_words(&words, feat, param);
            return Ok(word_rows(words, FEAT_PARAMETER_PICKER, &query, page));
        }
        // the item forms' categories and the Tags field's suggestions (spec 4.9, 3.2): words loaded content uses
        if target.types.iter().any(|t| t == ITEM_CATEGORY_PICKER) {
            let names = item_categories(&words);
            return Ok(word_rows(names, ITEM_CATEGORY_PICKER, &query, page));
        }
        if target.types.first().map(String::as_str) == Some(TAG_PICKER) {
            let mut names: Vec<String> = Vec::new();
            for ty in &target.types[1..] {
                for tag in loaded_tags(&words, ty) {
                    if !names.iter().any(|n| n.eq_ignore_ascii_case(&tag)) {
                        names.push(tag);
                    }
                }
            }
            names.sort_by_key(|n| n.to_lowercase());
            return Ok(word_rows(names, TAG_PICKER, &query, page));
        }
        if target.types.iter().any(|t| t == SETTER_NAME_PICKER) {
            let names = target
                .types
                .iter()
                .filter(|t| *t != SETTER_NAME_PICKER)
                .flat_map(|t| setter_names(&words, t))
                .collect();
            return Ok(word_rows(names, SETTER_NAME_PICKER, &query, page));
        }
        // "Start from a template" (spec 11, CS1): the bundled SRD elements only, never imported or own ones
        let srd_only = filters.srd_only == Some(true);
        let mut seen: HashSet<&str> = HashSet::new();
        let mut found: Vec<Candidate<'_>> = drafted
            .iter()
            .filter(|_| !srd_only)
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
                .filter(|s| !srd_only || is_bundled(&s.file))
                .map(|s| &s.element)
                .filter(|e| passes(e, target, &query, filters))
                .filter(|e| seen.insert(e.id.as_str()))
                .map(|element| Candidate {
                    element,
                    draft: false,
                }),
        );
        found.sort_by(|a, b| picker_order(a.element, b.element));
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
    /// ids. A new class draft offered here records the id it is offered under (`DraftFile.provisional`, INW8I), so a
    /// subclass that picks it follows the class to its saved id.
    pub(crate) fn hb_unsaved_elements(&self, draft_id: &str) -> ApiResult<Vec<Element>> {
        let open = self.hb_drafts.get(&self.paths.data, draft_id)?;
        let Ok(pack) = self.hb_pack(&open.pack_id) else {
            return Ok(Vec::new());
        };
        let hub = self.hub()?;
        let codec = CatalogBlockCodec;
        let cx = FormContext::from_store(&hub.store, &codec);
        let mut drafts = self.hb_drafts.list(&self.paths.data)?;
        let open_listed = drafts.iter().any(|d| d.draft_id == open.draft_id);
        if !open_listed {
            drafts.push(open.clone());
        }
        let unsaved = self.hb_pack_unsaved(&hub, &pack, &cx, drafts)?;
        for (d, els) in &unsaved {
            let Some(p) = ProvisionalClass::of(els).filter(|p| !d.provisional.contains(p)) else {
                continue;
            };
            let mut next = d.clone();
            next.provisional.push(p);
            // a draft with a file keeps the record there; the open one, held in memory, keeps it there
            let kept = if d.draft_id == open.draft_id && !open_listed {
                self.hb_drafts.hold(next)
            } else {
                self.hb_drafts.put(&self.paths.data, next)
            };
            if let Err(e) = kept {
                tracing::warn!(
                    "a class draft's provisional id was not recorded: {}",
                    e.message
                );
            }
        }
        Ok(unsaved.into_iter().flat_map(|(_, els)| els).collect())
    }

    /// Every new (never saved) draft of `pack` among `drafts`, with its elements under the provisional ids they have
    /// now: one id generator over the pack, the drafts in the order they were made (then by draft id), so two drafts
    /// of one name get the same two ids (`X`, `X_2`) wherever they are asked for, and a newer draft never takes an
    /// older one's id (INW8I). A draft whose form can't be written yet is left out.
    pub(crate) fn hb_pack_unsaved(
        &self,
        hub: &ContentHub,
        pack: &CreatorPack,
        cx: &FormContext<'_>,
        drafts: Vec<DraftFile>,
    ) -> ApiResult<Vec<(DraftFile, Vec<Element>)>> {
        let mut ids = self.hb_ids(hub, pack)?;
        let mut own: Vec<DraftFile> = drafts
            .into_iter()
            .filter(|d| d.pack_id == pack.id() && d.element_id.is_none())
            .collect();
        own.sort_by(|a, b| {
            (a.created, &a.updated_at, &a.draft_id).cmp(&(b.created, &b.updated_at, &b.draft_id))
        });
        own.dedup_by(|a, b| a.draft_id == b.draft_id);
        Ok(own
            .into_iter()
            .filter_map(|d| {
                let els = form_to_elements(&assign_ids(&d.form, &mut ids), cx).ok()?;
                Some((d, els))
            })
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

    fn caster(name: &str, list: &str) -> Element {
        let mut e = Element::new(format!("ID_T_CLASS_{name}"), name, "Class", "Test");
        e.spellcasting = Some(
            serde_json::from_value(serde_json::json!({"name": name, "list": {"supports": list}}))
                .expect("spellcasting"),
        );
        e
    }

    fn list_names(elements: &[Element]) -> Vec<String> {
        let page = spell_list_rows(
            elements.iter().map(|e| (e, false)),
            "",
            HomebrewPageRequest {
                offset: 0,
                limit: 50,
            },
        );
        page.items.into_iter().map(|r| r.name).collect()
    }

    #[test]
    fn spell_lists_written_as_expressions_are_offered_by_their_list_tags() {
        let elements = [
            caster("Wizard", "Wizard"),
            caster("Eldritch Knight", "Wizard,(Abjuration||Evocation)"),
            caster("Mystic", "(Cleric||Oracle)"),
            caster("Spellblade", "(Wizard||Bard),(abjuration||Illusion),1"),
            caster("Hexer", "ID_T_SPELL_HEX"),
        ];
        assert_eq!(
            list_names(&elements),
            ["Bard", "Cleric", "Oracle", "Wizard"],
            "the class lists an expression names, once each; schools, levels and spell ids are not lists"
        );
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

    fn tagged(mut e: Element, tags: &[&str]) -> Element {
        e.supports = tags.iter().map(|t| (*t).to_string()).collect();
        e
    }

    fn with(mut e: Element, setter: &str, value: &str) -> Element {
        e.setters.push(borealis_content::Setter::new(setter, value));
        e
    }

    /// Elements for the mock's filter fixture: spells of several lists, levels, schools and books, a feat and a class
    /// option.
    fn fixture_elements() -> Vec<Element> {
        let mut fire = tagged(
            spell("Fire Bolt", "0", "Evocation", false),
            &["Wizard", "Sorcerer"],
        );
        fire.ruleset = Some(Ruleset::Dnd2014);
        let mut ball = tagged(spell("Fireball", "3", "Evocation", false), &["Wizard"]);
        ball.ruleset = Some(Ruleset::Dnd2024);
        let mut other_book = tagged(spell("Fireball", "3", "Evocation", false), &["Wizard"]);
        other_book.id = "ID_U_SPELL_FIREBALL".into();
        other_book.source = "Other".into();
        let alarm = tagged(
            spell("Alarm", "1", "Abjuration", true),
            &["Wizard", "Ranger"],
        );
        let bless = with(
            tagged(
                spell("Bless", "1", "enchantment", false),
                &["Cleric", "Paladin"],
            ),
            "isConcentration",
            "true",
        );
        let lower = tagged(spell("Hex", "1", "Enchantment", false), &["warlock"]);
        let upper = tagged(
            spell("Hunter's Mark", "1", "Divination", false),
            &["Ranger"],
        );
        let mut feat = tagged(
            Element::new("ID_T_FEAT_ALERT", "Alert", "Feat", "Test"),
            &["Origin Feat", "Feat"],
        );
        feat.ruleset = Some(Ruleset::Dnd2024);
        let option = tagged(
            Element::new("ID_T_OPTION_FIRE_FORM", "Fire Form", "Option", "Test"),
            &["Elemental Form"],
        );
        vec![
            fire, ball, other_book, alarm, bless, lower, upper, feat, option,
        ]
    }

    /// The filters as the UI sends them (fields left out when not set).
    fn filters_json(f: &HomebrewPickerFilters) -> serde_json::Value {
        let mut v = serde_json::json!({});
        if let Some(s) = &f.sources {
            v["sources"] = serde_json::json!(s);
        }
        if let Some(e) = f.edition {
            v["edition"] = serde_json::json!(e);
        }
        if let Some(s) = &f.spell_school {
            v["spellSchool"] = serde_json::json!(s);
        }
        if let Some(l) = &f.spell_levels {
            v["spellLevels"] = serde_json::json!(l);
        }
        if let Some(r) = f.ritual {
            v["ritual"] = serde_json::json!(r);
        }
        if let Some(c) = f.concentration {
            v["concentration"] = serde_json::json!(c);
        }
        v
    }

    /// Hlw0Y: the picker rows the backend answers for a set of targets, queries and filters, written for the web mock's
    /// `referenceRows` to answer the same (`ui/src/api/__fixtures__/homebrew-picker-rows.json`, read by
    /// `homebrew-picker-rows.test.ts`; `BOREALIS_UPDATE_SNAPSHOTS=1` rewrites it).
    #[test]
    fn the_mock_fixture_holds_the_backend_s_picker_rows() {
        use serde_json::json;
        let elements = fixture_elements();
        let store = borealis_content::ContentStoreBuilder::new().build();
        let loaded: Vec<serde_json::Value> = elements
            .iter()
            .map(|e| {
                let mut v = serde_json::to_value(summary(&store, e)).expect("json");
                v["tags"] = json!(e.supports);
                v
            })
            .collect();
        let tags = |types: &[&str], tags: &[&str]| HomebrewPickerTarget {
            types: types.iter().map(|t| (*t).to_string()).collect(),
            tags: Some(tags.iter().map(|t| (*t).to_string()).collect()),
            multiple: None,
        };
        let none = HomebrewPickerFilters::default();
        let cases: Vec<(&str, HomebrewPickerTarget, &str, HomebrewPickerFilters)> = vec![
            ("every spell", target(&["Spell"]), "", none.clone()),
            ("any type", target(&[]), "", none.clone()),
            ("a feat picker", target(&["Feat"]), "", none.clone()),
            ("two types", target(&["Feat", "Option"]), "", none.clone()),
            ("spells by name", target(&["Spell"]), "fire", none.clone()),
            (
                "a name's middle, any case",
                target(&["Spell"]),
                "  BALL ",
                none.clone(),
            ),
            (
                "two words in order",
                target(&["Spell"]),
                "fire bolt",
                none.clone(),
            ),
            (
                "two words out of order",
                target(&["Spell"]),
                "bolt fire",
                none.clone(),
            ),
            (
                "words of the summary",
                target(&["Spell"]),
                "evocation",
                none.clone(),
            ),
            (
                "the Wizard list",
                tags(&["Spell"], &["Wizard"]),
                "",
                none.clone(),
            ),
            (
                "one of two lists",
                tags(&["Spell"], &["Cleric", "Ranger"]),
                "",
                none.clone(),
            ),
            (
                "tags match exactly",
                tags(&["Spell"], &["wizard"]),
                "",
                none.clone(),
            ),
            (
                "a lower-case tag",
                tags(&["Spell"], &["warlock"]),
                "",
                none.clone(),
            ),
            (
                "a tag and a name",
                tags(&["Spell"], &["Wizard"]),
                "fire",
                none.clone(),
            ),
            (
                "origin feats",
                tags(&["Feat"], &["Origin Feat"]),
                "",
                none.clone(),
            ),
            (
                "a class option's group",
                tags(&["Option"], &["Elemental Form"]),
                "",
                none.clone(),
            ),
            (
                "no tags is no tag filter",
                tags(&["Spell"], &[]),
                "",
                none.clone(),
            ),
            (
                "a school, any case",
                target(&["Spell"]),
                "",
                HomebrewPickerFilters {
                    spell_school: Some("enchantment".into()),
                    ..none.clone()
                },
            ),
            (
                "levels",
                target(&["Spell"]),
                "",
                HomebrewPickerFilters {
                    spell_levels: Some(vec![0, 3]),
                    ..none.clone()
                },
            ),
            (
                "rituals",
                target(&[]),
                "",
                HomebrewPickerFilters {
                    ritual: Some(true),
                    ..none.clone()
                },
            ),
            (
                "concentration",
                target(&["Spell"]),
                "",
                HomebrewPickerFilters {
                    concentration: Some(true),
                    ..none.clone()
                },
            ),
            (
                "a Magic filter leaves other types out",
                target(&[]),
                "",
                HomebrewPickerFilters {
                    spell_levels: Some(vec![1]),
                    ..none.clone()
                },
            ),
            (
                "a book",
                target(&["Spell"]),
                "",
                HomebrewPickerFilters {
                    sources: Some(vec!["Other".into()]),
                    ..none.clone()
                },
            ),
            (
                "the 2014 edition",
                target(&[]),
                "",
                HomebrewPickerFilters {
                    edition: Some(Ruleset::Dnd2014),
                    ..none.clone()
                },
            ),
            (
                "the 2024 edition and a list",
                tags(&["Spell"], &["Wizard"]),
                "",
                HomebrewPickerFilters {
                    edition: Some(Ruleset::Dnd2024),
                    ..none.clone()
                },
            ),
        ];
        let rows: Vec<serde_json::Value> = cases
            .iter()
            .map(|(case, target, query, filters)| {
                let q = query.trim().to_lowercase();
                let mut found: Vec<&Element> = elements
                    .iter()
                    .filter(|e| passes(e, target, &q, filters))
                    .collect();
                found.sort_by(|a, b| picker_order(a, b));
                json!({
                    "case": case,
                    "target": target,
                    "query": query,
                    "filters": filters_json(filters),
                    "ids": found.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&json!({"loaded": loaded, "cases": rows}))
            .expect("json")
            + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ui/src/api/__fixtures__/homebrew-picker-rows.json");
        if std::env::var_os("BOREALIS_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &text).expect("write");
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            on_disk == text,
            "ui/src/api/__fixtures__/homebrew-picker-rows.json is stale: run this test with BOREALIS_UPDATE_SNAPSHOTS=1"
        );
    }
}
