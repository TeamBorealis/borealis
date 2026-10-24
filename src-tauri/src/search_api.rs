//! The search surface (local-search-spec.md section 11): the `search` command over the Rules and Lore keyword
//! indexes, the rules passage behind a `rules:` hit, the index status, and the Lore-index lookup that
//! `search_compendium` uses for its description tier.

use crate::compendium::{is_listed, parse_ruleset, MAX_QUERY_CHARS};
use crate::dto::Page;
use crate::elements::summary;
use crate::error::{ApiError, ApiResult};
use crate::ids::ui_type;
use crate::sanitize::sanitize_html;
use crate::search_service::IndexStatus;
use crate::state::{AppState, ContentHub};
use borealis_content::{Element, Ruleset};
use borealis_search::{
    base_key, parse_rules_key, Corpus, Edition, EditionScope, Hit, SearchConfig, SearchEngine,
    SearchRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Hits read from one corpus per request (pages are cut from these; `total` is capped by it).
const POOL: usize = 400;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
/// `type` of a rules-text section hit.
pub const RULES_HIT_TYPE: &str = "rules";
const PATH_SEPARATOR: &str = " > ";

/// What to search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    /// Rules + Lore.
    #[default]
    All,
    /// Rules text, conditions, glossary.
    Rules,
    /// Every element with text.
    Lore,
    /// Campaign notes (S2.1; no campaigns yet, always empty).
    Sessions,
}

/// `search` request.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    /// Scope (default `all`).
    #[serde(default)]
    pub scope: SearchScope,
    /// Free text.
    #[serde(default)]
    pub query: String,
    /// `dnd2014`, `dnd2024` or `any` (default).
    #[serde(default)]
    pub edition: Option<String>,
    /// Campaign id (sessions scope).
    #[serde(default)]
    pub campaign: Option<String>,
    /// Page size (default 50, at most 200).
    #[serde(default)]
    pub limit: usize,
    /// Page start.
    #[serde(default)]
    pub offset: usize,
    /// Contract element types to keep (`rules` keeps rules-text sections); none = all.
    #[serde(default)]
    pub types: Option<Vec<String>>,
    /// Source names to keep; none = all.
    #[serde(default)]
    pub sources: Option<Vec<String>>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// Stable citation key (`<element id>[#n]`, `rules:<pack>/<section>[#n]`).
    pub doc_key: String,
    /// Element name or section title.
    pub title: String,
    /// Heading path of a rules section (`Combat > Actions > Dash`); empty for elements.
    pub path: String,
    /// Contract element type, or `rules` for a rules-text section.
    #[serde(rename = "type")]
    pub ty: String,
    /// Edition of the text; `None` = edition neutral.
    pub ruleset: Option<Ruleset>,
    /// Source display name.
    pub source: String,
    /// Plain-text snippet.
    pub snippet: String,
    /// Highlighted `[start, end)` ranges in `snippet`, in UTF-16 code units (JavaScript string offsets).
    pub highlights: Vec<[usize; 2]>,
    /// The element to open; `None` for a rules section (open it with `get_search_passage`).
    pub element_id: Option<String>,
}

/// A rules-text section for the description pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPassage {
    /// The key it was opened with.
    pub doc_key: String,
    /// Section title.
    pub title: String,
    /// Heading path.
    pub path: String,
    /// Sanitised section HTML.
    pub html: String,
    /// Source display name.
    pub source: String,
    /// Edition of the text.
    pub ruleset: Ruleset,
    /// Page in the source document.
    pub page: Option<u32>,
    /// Licence id (`CC-BY-4.0`).
    pub license: String,
    /// The attribution statement the licence requires.
    pub attribution: String,
}

fn edition_scope(ruleset: Option<Ruleset>) -> EditionScope {
    match ruleset {
        Some(Ruleset::Dnd2014) => EditionScope::Only(Edition::Y2014),
        Some(Ruleset::Dnd2024) => EditionScope::Only(Edition::Y2024),
        _ => EditionScope::Any,
    }
}

fn neutral(r: Option<Ruleset>) -> Option<Ruleset> {
    r.filter(|r| *r != Ruleset::Any)
}

fn ruleset_of_tag(tag: &str) -> Option<Ruleset> {
    match tag {
        "2014" => Some(Ruleset::Dnd2014),
        "2024" => Some(Ruleset::Dnd2024),
        _ => None,
    }
}

/// Byte ranges of `text` as UTF-16 offsets; ranges off a char boundary are dropped.
fn utf16_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<[usize; 2]> {
    let at = |b: usize| text.get(..b).map(|s| s.encode_utf16().count());
    ranges
        .iter()
        .filter(|(s, e)| s < e)
        .filter_map(|(s, e)| Some([at(*s)?, at(*e)?]))
        .collect()
}

/// Content types of the store whose contract type is in `wanted` (the index stores content types).
fn content_types(hub: &ContentHub, wanted: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in hub.store.iter() {
        let ty = &s.element.ty;
        if wanted.iter().any(|w| w == ui_type(ty)) && !out.contains(ty) {
            out.push(ty.clone());
        }
    }
    out
}

fn request(query: &str, edition: EditionScope, types: Vec<String>) -> SearchRequest {
    SearchRequest {
        edition,
        types,
        limit: POOL,
        ..SearchRequest::new(query)
    }
}

struct Filters<'a> {
    types: Option<&'a [String]>,
    sources: Option<&'a [String]>,
}

impl Filters<'_> {
    fn element(&self, e: &Element) -> bool {
        is_listed(e, self.types)
            && self
                .sources
                .map_or(true, |s| s.is_empty() || s.contains(&e.source))
    }

    fn section(&self, source: &str) -> bool {
        self.types.map_or(true, |t| {
            t.is_empty() || t.iter().any(|x| x == RULES_HIT_TYPE)
        }) && self
            .sources
            .map_or(true, |s| s.is_empty() || s.iter().any(|x| x == source))
    }
}

fn element_hit(hub: &ContentHub, hit: &Hit, filters: &Filters<'_>) -> Option<SearchHit> {
    let el = hub.element(base_key(&hit.doc_key))?;
    if !filters.element(el) {
        return None;
    }
    Some(SearchHit {
        doc_key: hit.doc_key.clone(),
        title: el.name.clone(),
        path: String::new(),
        ty: ui_type(&el.ty).to_string(),
        ruleset: neutral(el.ruleset),
        source: summary(&hub.store, el).source_name,
        snippet: hit.snippet.clone(),
        highlights: utf16_ranges(&hit.snippet, &hit.highlights),
        element_id: Some(el.id.clone()),
    })
}

fn section_hit(hit: &Hit, filters: &Filters<'_>) -> Option<SearchHit> {
    filters.section(&hit.source).then(|| SearchHit {
        doc_key: hit.doc_key.clone(),
        title: hit.title.clone(),
        path: hit.path.clone(),
        ty: RULES_HIT_TYPE.into(),
        ruleset: ruleset_of_tag(&hit.ruleset),
        source: hit.source.clone(),
        snippet: hit.snippet.clone(),
        highlights: utf16_ranges(&hit.snippet, &hit.highlights),
        element_id: None,
    })
}

fn corpus_hits(
    engine: &SearchEngine,
    hub: &ContentHub,
    corpus: Corpus,
    q: &SearchQuery,
    ruleset: Option<Ruleset>,
) -> ApiResult<Vec<SearchHit>> {
    let filters = Filters {
        types: q.types.as_deref(),
        sources: q.sources.as_deref(),
    };
    let types = match (corpus, q.types.as_deref()) {
        (Corpus::Lore, Some(t)) if !t.is_empty() => {
            let raw = content_types(hub, t);
            if raw.is_empty() {
                return Ok(Vec::new());
            }
            raw
        }
        _ => Vec::new(),
    };
    let req = request(q.query.trim(), edition_scope(ruleset), types);
    let hits = engine
        .search(corpus, &req, &SearchConfig::default())
        .map_err(|e| ApiError::internal(format!("search failed: {e}")))?;
    Ok(hits
        .iter()
        .filter_map(|h| {
            if parse_rules_key(&h.doc_key).is_some() {
                section_hit(h, &filters)
            } else {
                element_hit(hub, h, &filters)
            }
        })
        .collect())
}

/// Rules and Lore scores are not comparable: interleave by rank (Lore first), dropping keys seen already
/// (conditions and glossary entries sit in both corpora).
fn interleave(lore: Vec<SearchHit>, rules: Vec<SearchHit>) -> Vec<SearchHit> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(lore.len() + rules.len());
    let (mut a, mut b) = (lore.into_iter(), rules.into_iter());
    loop {
        let (x, y) = (a.next(), b.next());
        if x.is_none() && y.is_none() {
            break;
        }
        for hit in [x, y].into_iter().flatten() {
            if seen.insert(base_key(&hit.doc_key).to_string()) {
                out.push(hit);
            }
        }
    }
    out
}

fn page(hits: Vec<SearchHit>, q: &SearchQuery) -> Page<SearchHit> {
    let take = if q.limit == 0 {
        DEFAULT_LIMIT
    } else {
        q.limit.min(MAX_LIMIT)
    };
    Page {
        total: hits.len(),
        items: hits.into_iter().skip(q.offset).take(take).collect(),
    }
}

impl AppState {
    /// Start the background index build (startup, after content has loaded).
    ///
    /// # Errors
    /// `internal` if the content lock is poisoned.
    pub fn start_search_index(&self) -> ApiResult<()> {
        self.search.request_sync(self.hub()?);
        Ok(())
    }

    /// Build or update the indexes now, blocking (tests and tools).
    ///
    /// # Errors
    /// `internal` when the build fails.
    pub fn build_search_index_now(&self) -> ApiResult<()> {
        let hub = self.hub()?;
        if self.search.sync_now(&hub) {
            Ok(())
        } else {
            Err(ApiError::internal(
                self.search.status().message.unwrap_or_default(),
            ))
        }
    }

    /// The keyword index status.
    #[must_use]
    pub fn search_index_status(&self) -> IndexStatus {
        self.search.status()
    }

    /// Keyword search over Rules, Lore or both (`all`). Until the indexes are ready, Lore falls back to the
    /// compendium scan (no snippets highlighted) and Rules is empty; the first call starts the build.
    ///
    /// # Errors
    /// `invalid_argument` for an overlong query or unknown edition.
    pub fn search(&self, q: &SearchQuery) -> ApiResult<Page<SearchHit>> {
        if q.query.chars().count() > MAX_QUERY_CHARS {
            return Err(ApiError::invalid("The search text is too long."));
        }
        let ruleset = parse_ruleset(q.edition.as_deref())?;
        if q.query.trim().is_empty() || q.scope == SearchScope::Sessions {
            return Ok(Page {
                total: 0,
                items: Vec::new(),
            });
        }
        let hub = self.hub()?;
        if !self.search.is_started() {
            self.search.request_sync(std::sync::Arc::clone(&hub));
        }
        let reader = self.search.reader();
        let Some(engine) = reader.as_ref().and_then(|g| g.as_ref()) else {
            return self.search_fallback(q, ruleset);
        };
        let hits = match q.scope {
            SearchScope::Rules => corpus_hits(engine, &hub, Corpus::Rules, q, ruleset)?,
            SearchScope::Lore => corpus_hits(engine, &hub, Corpus::Lore, q, ruleset)?,
            SearchScope::All | SearchScope::Sessions => interleave(
                corpus_hits(engine, &hub, Corpus::Lore, q, ruleset)?,
                corpus_hits(engine, &hub, Corpus::Rules, q, ruleset)?,
            ),
        };
        Ok(page(hits, q))
    }

    fn search_fallback(
        &self,
        q: &SearchQuery,
        ruleset: Option<Ruleset>,
    ) -> ApiResult<Page<SearchHit>> {
        if q.scope == SearchScope::Rules {
            return Ok(Page {
                total: 0,
                items: Vec::new(),
            });
        }
        let found = self.search_compendium(&crate::dto_ext::CompendiumQuery {
            query: q.query.clone(),
            types: q.types.clone(),
            sources: q.sources.clone(),
            ruleset: ruleset.map(|r| r.as_str().to_string()),
            limit: q.limit,
            offset: q.offset,
        })?;
        let items = found
            .items
            .into_iter()
            .map(|s| SearchHit {
                doc_key: s.id.clone(),
                title: s.name,
                path: String::new(),
                ruleset: (s.rulesets.len() == 1).then(|| s.rulesets[0]),
                ty: s.ty,
                source: s.source_name,
                snippet: s.short_description,
                highlights: Vec::new(),
                element_id: Some(s.id),
            })
            .collect();
        Ok(Page {
            total: found.total,
            items,
        })
    }

    /// The rules-text section behind a `rules:` doc key, sanitised, with its licence attribution.
    ///
    /// # Errors
    /// `not_found` for an unknown key.
    pub fn get_search_passage(&self, doc_key: &str) -> ApiResult<SearchPassage> {
        let missing = || ApiError::not_found("That rules section does not exist.");
        let (pack, section) = parse_rules_key(doc_key).ok_or_else(missing)?;
        let docs = self.search.docs_pack(pack).ok_or_else(missing)?;
        let s = docs.section(section).ok_or_else(missing)?;
        Ok(SearchPassage {
            doc_key: doc_key.to_string(),
            title: s.path.last().cloned().unwrap_or_default(),
            path: s.path.join(PATH_SEPARATOR),
            html: sanitize_html(s.html),
            source: docs.name().to_string(),
            ruleset: docs.ruleset(),
            page: s.page,
            license: docs.license().to_string(),
            attribution: docs.attribution().to_string(),
        })
    }

    /// Element ids of Lore index hits for `needle`, best first; `None` when the index is not ready (or for the
    /// engine-derived Multiclass type, which is not indexed) so the caller scans descriptions instead.
    pub(crate) fn lore_element_ids(
        &self,
        hub: &ContentHub,
        needle: &str,
        types: Option<&[String]>,
    ) -> Option<Vec<String>> {
        if types.is_some_and(|t| t.iter().any(|x| x == "multiclass")) {
            return None;
        }
        let reader = self.search.reader();
        let engine = reader.as_ref()?.as_ref()?;
        let raw = match types {
            Some(t) if !t.is_empty() => content_types(hub, t),
            _ => Vec::new(),
        };
        if types.is_some_and(|t| !t.is_empty()) && raw.is_empty() {
            return Some(Vec::new());
        }
        let req = request(needle, EditionScope::Any, raw);
        let hits = engine
            .search(Corpus::Lore, &req, &SearchConfig::default())
            .map_err(|e| tracing::warn!("search: compendium lookup failed: {e}"))
            .ok()?;
        let mut seen = HashSet::new();
        Some(
            hits.into_iter()
                .map(|h| base_key(&h.doc_key).to_string())
                .filter(|id| seen.insert(id.clone()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_ranges_become_utf16_offsets() {
        let text = "Caf\u{e9} fire \u{1F525} fire";
        let first = text.find("fire").unwrap();
        let last = text.rfind("fire").unwrap();
        let got = utf16_ranges(text, &[(first, first + 4), (last, last + 4), (1, 1)]);
        assert_eq!(got, vec![[5, 9], [13, 17]]);
        assert!(
            utf16_ranges("\u{e9}", &[(1, 2)]).is_empty(),
            "inside a char"
        );
    }

    #[test]
    fn interleave_alternates_and_drops_duplicates() {
        let hit = |k: &str| SearchHit {
            doc_key: k.into(),
            title: k.into(),
            path: String::new(),
            ty: "x".into(),
            ruleset: None,
            source: String::new(),
            snippet: String::new(),
            highlights: Vec::new(),
            element_id: None,
        };
        let out = interleave(
            vec![hit("a"), hit("c#2"), hit("d")],
            vec![hit("b"), hit("c")],
        );
        let keys: Vec<&str> = out.iter().map(|h| h.doc_key.as_str()).collect();
        assert_eq!(keys, ["a", "b", "c#2", "d"]);
    }
}
