//! Building chunks for the Rules and Lore corpora (spec section 3).

use crate::analyzer::normalize_name;
use crate::chunk::{Chunk, EditionTag, PackInput};
use crate::error::{Result, SearchError};
use crate::text::{html_blocks, split_blocks, LORE_MAX_WORDS, RULES_MAX_WORDS};
use borealis_content::{ContentStore, Ruleset, StoredElement};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// Element types that also belong to the Rules corpus (conditions and glossary rules).
pub const RULES_ELEMENT_TYPES: &[&str] = &["Condition", "Rules Glossary Entry"];
/// `type` value of rules-text sections.
pub const RULES_SECTION_TYPE: &str = "Rules";
/// Separator of heading paths in the `headings` field.
pub const PATH_SEPARATOR: &str = " > ";
/// Expected `format` of a rules-text pack.
pub const DOCS_FORMAT: &str = "borealis-docs/1";
const MAX_FACTS: usize = 12;
const MAX_FACT_VALUE_CHARS: usize = 40;

#[derive(Debug, Deserialize)]
struct DocsInfo {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    attribution: String,
}

#[derive(Debug, Deserialize)]
struct DocsSection {
    id: String,
    path: Vec<String>,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    html: String,
}

/// One section of a rules-text pack as read for display (the passage behind a `rules:` doc key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocsSectionRef<'a> {
    /// Section id (the part of the doc key after `rules:<pack>/`).
    pub id: &'a str,
    /// Heading path, outermost first.
    pub path: &'a [String],
    /// Page in the source PDF.
    pub page: Option<u32>,
    /// The section's HTML fragment (pack text, not sanitised).
    pub html: &'a str,
}

/// Split a rules doc key (`rules:<pack>/<section>[#n]`) into pack id and section id.
#[must_use]
pub fn parse_rules_key(doc_key: &str) -> Option<(&str, &str)> {
    let rest = crate::index::base_key(doc_key).strip_prefix("rules:")?;
    let (pack, section) = rest.split_once('/')?;
    (!pack.is_empty() && !section.is_empty()).then_some((pack, section))
}

/// A parsed `borealis-docs/1` rules-text pack.
#[derive(Debug, Deserialize)]
pub struct DocsPack {
    format: String,
    info: DocsInfo,
    ruleset: Ruleset,
    sections: Vec<DocsSection>,
}

impl DocsPack {
    /// Parse `docs.json` text.
    ///
    /// # Errors
    /// JSON errors, or [`SearchError::InvalidDocs`] for a wrong `format`.
    pub fn parse(json: &str) -> Result<Self> {
        let pack: Self = serde_json::from_str(json)?;
        if pack.format != DOCS_FORMAT {
            return Err(SearchError::InvalidDocs(format!(
                "format {:?}, expected {DOCS_FORMAT}",
                pack.format
            )));
        }
        Ok(pack)
    }

    /// Pack id (`srd-5.2.1-rules-text`).
    #[must_use]
    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// Display name of the pack.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Licence id of the text (`CC-BY-4.0`).
    #[must_use]
    pub fn license(&self) -> &str {
        &self.info.license
    }

    /// The attribution statement the licence requires.
    #[must_use]
    pub fn attribution(&self) -> &str {
        &self.info.attribution
    }

    /// Ruleset of the text.
    #[must_use]
    pub fn ruleset(&self) -> Ruleset {
        self.ruleset
    }

    /// The section with this id.
    #[must_use]
    pub fn section(&self, id: &str) -> Option<DocsSectionRef<'_>> {
        self.sections
            .iter()
            .find(|s| s.id == id)
            .map(|s| DocsSectionRef {
                id: &s.id,
                path: &s.path,
                page: s.page,
                html: &s.html,
            })
    }

    /// Number of sections, empty ones included.
    #[must_use]
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    fn edition(&self) -> EditionTag {
        EditionTag::from(Some(self.ruleset))
    }

    /// Chunks of this pack. Sections without text are skipped. A section whose title equals an entry of
    /// `duplicate_titles` (normalised) and that sits under a conditions or glossary heading is skipped too: the
    /// typed element of that name is already in the Rules corpus.
    #[must_use]
    pub fn chunks(&self, duplicate_titles: &HashSet<String>) -> Vec<Chunk> {
        let edition = self.edition();
        let source = self.info.name.clone();
        let mut out = Vec::new();
        for section in &self.sections {
            let blocks = html_blocks(&section.html);
            let Some(title) = section.path.last() else {
                continue;
            };
            if blocks.is_empty()
                || (is_reference_path(&section.path)
                    && duplicate_titles.contains(&normalize_name(title)))
            {
                continue;
            }
            let parts = split_blocks(&blocks, RULES_MAX_WORDS);
            let split = parts.len() > 1;
            for (i, body) in parts.into_iter().enumerate() {
                out.push(Chunk {
                    doc_key: part_key(&format!("rules:{}/{}", self.info.id, section.id), i, split),
                    name: title.clone(),
                    headings: section.path.join(PATH_SEPARATOR),
                    body,
                    ty: RULES_SECTION_TYPE.to_string(),
                    edition,
                    pack: self.info.id.clone(),
                    source: source.clone(),
                    campaign: None,
                    is_primary: i == 0,
                });
            }
        }
        out
    }
}

fn is_reference_path(path: &[String]) -> bool {
    path.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        p.contains("glossary") || p.contains("condition")
    })
}

fn part_key(base: &str, index: usize, split: bool) -> String {
    if split {
        format!("{base}#{}", index + 1)
    } else {
        base.to_string()
    }
}

/// Pack id of a stored element: the first path segment of the file it was loaded from.
fn pack_of(stored: &StoredElement) -> String {
    stored
        .file
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_short_fact(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty() && v.chars().count() <= MAX_FACT_VALUE_CHARS && !v.contains('\n')
}

/// `name: value` facts: short setters first, then short top-level strings and string lists of `data`
/// (creature type, alignment, challenge rating, category, ...).
fn facts_line(stored: &StoredElement) -> String {
    let e = &stored.element;
    let setters = e
        .setters
        .iter()
        .filter(|s| is_short_fact(&s.value) && !s.name.eq_ignore_ascii_case("description"))
        .map(|s| format!("{}: {}", s.name, s.value.trim()));
    let data = e.data.iter().filter_map(|(key, value)| match value {
        Value::String(s) if is_short_fact(s) => Some(format!("{key}: {}", s.trim())),
        Value::Array(items) => {
            let strings: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            let joined = strings.join(", ");
            (strings.len() == items.len() && is_short_fact(&joined))
                .then(|| format!("{key}: {joined}"))
        }
        _ => None,
    });
    setters
        .chain(data)
        .take(MAX_FACTS)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Text carried by structured `data` (monster traits and actions): every object with a string `text`, as
/// `name: text` when it has a `name`. Depth-first, keys in alphabetical order.
fn data_blocks(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get("text") {
                let line = match map.get("name").and_then(Value::as_str) {
                    Some(name) => format!("{name}: {text}"),
                    None => text.clone(),
                };
                out.extend(html_blocks(&line));
            }
            for (key, child) in map {
                if key != "text" {
                    data_blocks(child, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| data_blocks(v, out)),
        _ => {}
    }
}

/// Chunks of one element: its name, facts line, description and structured text. Empty for elements hidden from
/// the compendium, nameless ones, and ones with no text at all.
#[must_use]
pub fn element_chunks(stored: &StoredElement) -> Vec<Chunk> {
    let e = &stored.element;
    if e.name.trim().is_empty()
        || e.compendium
            .as_ref()
            .is_some_and(|c| c.display == Some(false))
    {
        return Vec::new();
    }
    let mut blocks = html_blocks(&e.description);
    for value in e.data.values() {
        data_blocks(value, &mut blocks);
    }
    let facts = facts_line(stored);
    if !facts.is_empty() {
        blocks.insert(0, facts);
    }
    if blocks.is_empty() {
        return Vec::new();
    }
    let parts = split_blocks(&blocks, LORE_MAX_WORDS);
    let split = parts.len() > 1;
    let pack = pack_of(stored);
    parts
        .into_iter()
        .enumerate()
        .map(|(i, body)| Chunk {
            doc_key: part_key(&e.id, i, split),
            name: e.name.clone(),
            headings: e.ty.clone(),
            body,
            ty: e.ty.clone(),
            edition: EditionTag::from(e.ruleset),
            pack: pack.clone(),
            source: e.source.clone(),
            campaign: None,
            is_primary: i == 0,
        })
        .collect()
}

fn group_by_pack(chunks: impl IntoIterator<Item = Chunk>) -> Vec<PackInput> {
    let mut groups: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for c in chunks {
        groups.entry(c.pack.clone()).or_default().push(c);
    }
    groups
        .into_iter()
        .map(|(pack, chunks)| PackInput::new(pack, chunks))
        .collect()
}

/// The Lore corpus: every element with text, grouped by pack.
#[must_use]
pub fn lore_packs(store: &ContentStore) -> Vec<PackInput> {
    group_by_pack(store.iter().flat_map(element_chunks))
}

/// Normalised names of the Condition / glossary elements of `store`, to de-duplicate rules-text sections.
fn rules_element_titles(store: &ContentStore) -> HashSet<String> {
    store
        .iter()
        .filter(|s| RULES_ELEMENT_TYPES.contains(&s.element.ty.as_str()))
        .map(|s| normalize_name(&s.element.name))
        .collect()
}

/// The Rules corpus: the rules-text packs (`docs_json` = the text of each `docs.json`) plus the Condition and
/// glossary elements of `store` (grouped by their own pack). Rules-text sections that duplicate such an element
/// are dropped (the element wins: it opens in the existing detail view).
///
/// # Errors
/// Any pack that fails to parse.
pub fn rules_packs(docs_json: &[&str], store: &ContentStore) -> Result<Vec<PackInput>> {
    let titles = rules_element_titles(store);
    let mut packs: Vec<PackInput> = Vec::new();
    for json in docs_json {
        let docs = DocsPack::parse(json)?;
        packs.push(PackInput::new(docs.id().to_string(), docs.chunks(&titles)));
    }
    let element_chunks = store
        .iter()
        .filter(|s| RULES_ELEMENT_TYPES.contains(&s.element.ty.as_str()))
        .flat_map(element_chunks);
    packs.extend(group_by_pack(element_chunks));
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS: &str = r#"{
      "format": "borealis-docs/1",
      "info": {"id": "t-rules-text", "name": "Test Rules"},
      "ruleset": "dnd2014",
      "sections": [
        {"id": "combat", "path": ["Combat"], "page": 1, "html": ""},
        {"id": "combat/dash", "path": ["Combat", "Dash"], "page": 2, "html": "<p>Gain extra movement.</p>"},
        {"id": "appendix/conditions/prone", "path": ["Appendix", "Conditions", "Prone"], "page": 3, "html": "<p>Prone text.</p>"},
        {"id": "combat/prone", "path": ["Combat", "Prone"], "page": 4, "html": "<p>Dropping prone.</p>"}
      ]
    }"#;

    #[test]
    fn rules_text_sections_become_chunks_with_citation_keys() {
        let docs = DocsPack::parse(DOCS).unwrap();
        let chunks = docs.chunks(&HashSet::new());
        let keys: Vec<&str> = chunks.iter().map(|c| c.doc_key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "rules:t-rules-text/combat/dash",
                "rules:t-rules-text/appendix/conditions/prone",
                "rules:t-rules-text/combat/prone"
            ]
        );
        assert_eq!(chunks[0].headings, "Combat > Dash");
        assert_eq!(chunks[0].name, "Dash");
        assert_eq!(chunks[0].edition, EditionTag::Y2014);
    }

    #[test]
    fn duplicates_of_typed_elements_are_dropped_only_under_reference_headings() {
        let docs = DocsPack::parse(DOCS).unwrap();
        let titles: HashSet<String> = ["prone".to_string()].into();
        let keys: Vec<String> = docs
            .chunks(&titles)
            .into_iter()
            .map(|c| c.doc_key)
            .collect();
        assert!(!keys.iter().any(|k| k.contains("appendix/conditions")));
        assert!(keys.iter().any(|k| k.ends_with("combat/prone")));
    }

    #[test]
    fn long_sections_split_with_numbered_keys_and_one_primary() {
        let para = format!("<p>{}</p>", "word ".repeat(200));
        let json = DOCS.replace(
            "<p>Gain extra movement.</p>",
            &format!("{para}{para}{para}"),
        );
        let chunks = DocsPack::parse(&json).unwrap().chunks(&HashSet::new());
        let dash: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| c.doc_key.starts_with("rules:t-rules-text/combat/dash"))
            .collect();
        assert!(dash.len() >= 2);
        assert_eq!(dash[0].doc_key, "rules:t-rules-text/combat/dash#1");
        assert_eq!(dash.iter().filter(|c| c.is_primary).count(), 1);
    }

    #[test]
    fn wrong_format_is_rejected() {
        let bad = DOCS.replace("borealis-docs/1", "borealis-docs/2");
        assert!(matches!(
            DocsPack::parse(&bad),
            Err(SearchError::InvalidDocs(_))
        ));
    }

    #[test]
    fn rules_keys_resolve_to_their_section() {
        assert_eq!(
            parse_rules_key("rules:t-rules-text/combat/dash#2"),
            Some(("t-rules-text", "combat/dash"))
        );
        assert_eq!(parse_rules_key("ID_SPELL_FIREBALL"), None);
        assert_eq!(parse_rules_key("rules:nopath"), None);
        let docs = DocsPack::parse(DOCS).unwrap();
        let s = docs.section("combat/dash").expect("section");
        assert_eq!(
            (s.path.join(PATH_SEPARATOR).as_str(), s.page),
            ("Combat > Dash", Some(2))
        );
        assert_eq!(s.html, "<p>Gain extra movement.</p>");
        assert!(docs.section("combat/missing").is_none());
        assert_eq!(docs.ruleset(), Ruleset::Dnd2014);
    }
}
