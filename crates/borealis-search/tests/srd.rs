//! Behaviour on the bundled SRD corpora (SRD 5.1 / 5.2.1 packs, rules-text packs): exact names, synonyms that
//! resolve to SRD text, rules queries.

mod common;

use borealis_search::analyzer::plain_words;
use borealis_search::{
    Corpus, Edition, EditionScope, SearchConfig, SearchEngine, SearchRequest, Synonyms,
};
use common::{srd_engine, srd_inputs, srd_store};
use std::sync::OnceLock;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    engine: SearchEngine,
}

fn fixture() -> &'static Fixture {
    static F: OnceLock<Fixture> = OnceLock::new();
    F.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let engine = srd_engine(dir.path());
        Fixture { _dir: dir, engine }
    })
}

fn search(corpus: Corpus, query: &str) -> Vec<borealis_search::Hit> {
    search_req(corpus, &SearchRequest::new(query))
}

fn search_req(corpus: Corpus, req: &SearchRequest) -> Vec<borealis_search::Hit> {
    fixture()
        .engine
        .search(corpus, req, &SearchConfig::default())
        .unwrap()
}

#[test]
fn fireball_exact_is_rank_one_in_lore() {
    let hits = search(Corpus::Lore, "Fireball");
    assert_eq!(hits[0].title, "Fireball");
    assert_eq!(hits[0].ty, "Spell");
    for q in ["fireball", "FIREBALL", "  Fireball "] {
        assert_eq!(search(Corpus::Lore, q)[0].title, "Fireball", "{q}");
    }
    for edition in [Edition::Y2014, Edition::Y2024] {
        let mut req = SearchRequest::new("Fireball");
        req.edition = EditionScope::Only(edition);
        let hits = search_req(Corpus::Lore, &req);
        assert_eq!(hits[0].title, "Fireball");
        assert_eq!(
            hits[0].ruleset,
            if edition == Edition::Y2014 {
                "2014"
            } else {
                "2024"
            }
        );
    }
}

#[test]
fn exact_names_rank_first_for_a_spread_of_srd_elements() {
    for name in [
        "Eldritch Blast",
        "Magic Missile",
        "Cure Wounds",
        "Fire Bolt",
        "Aboleth",
        "Longsword",
        "Plate Armor",
        "Barbarian",
        "Rage",
    ] {
        let hits = search(Corpus::Lore, name);
        assert_eq!(
            hits[0].title.to_lowercase(),
            name.to_lowercase(),
            "{name}: {:?}",
            &hits[..3]
        );
    }
}

#[test]
fn grappled_exact_is_the_condition_element_in_rules() {
    let hits = search(Corpus::Rules, "Grappled");
    assert_eq!(hits[0].title, "Grappled");
    assert_eq!(hits[0].ty, "Condition");
    assert!(
        hits[0].doc_key.starts_with("ID_"),
        "element ids are the cite keys"
    );
}

#[test]
fn typos_prefixes_and_synonyms_find_srd_text() {
    assert_eq!(search(Corpus::Lore, "firball")[0].title, "Fireball");
    assert_eq!(
        search(Corpus::Lore, "eldrich blast")[0].title,
        "Eldritch Blast"
    );
    assert_eq!(search(Corpus::Lore, "polymorf")[0].title, "Polymorph");
    assert!(search(Corpus::Lore, "polym")
        .iter()
        .any(|h| h.title == "Polymorph"));
    assert!(search(Corpus::Lore, "cure w")
        .iter()
        .any(|h| h.title == "Cure Wounds"));
}

#[test]
fn rules_corpus_answers_rules_questions_with_cited_sections() {
    let hits = search(Corpus::Rules, "opportunity attack");
    assert!(
        hits.iter()
            .take(5)
            .any(|h| h.title.to_lowercase().contains("opportunity attack")),
        "{:?}",
        hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
    let hits = search(Corpus::Rules, "how does grappling work");
    assert!(hits
        .iter()
        .take(5)
        .any(|h| h.path.to_lowercase().contains("grappl")));
    let hits = search(Corpus::Rules, "falling damage");
    assert!(hits
        .iter()
        .take(5)
        .any(|h| h.snippet.to_lowercase().contains("fall")));
    assert!(hits.iter().all(|h| !h.doc_key.is_empty()));
}

#[test]
fn rules_text_sections_have_citation_keys_and_paths() {
    let hits = search(Corpus::Rules, "dash action");
    let section = hits
        .iter()
        .find(|h| h.doc_key.starts_with("rules:srd-5.2.1-rules-text/"))
        .expect("a rules-text hit");
    assert!(section.path.contains(" > ") || !section.path.is_empty());
    assert_eq!(section.ty, "Rules");
    assert_eq!(section.ruleset, "2024");
}

#[test]
fn conditions_are_not_indexed_twice_in_the_rules_corpus() {
    let hits = search(Corpus::Rules, "Prone");
    let prone_2024: Vec<_> = hits
        .iter()
        .filter(|h| h.title == "Prone" && h.ruleset == "2024")
        .collect();
    assert_eq!(prone_2024.len(), 1, "{prone_2024:?}");
}

#[test]
fn edition_filter_on_a_rules_question() {
    let mut req = SearchRequest::new("rest");
    req.edition = EditionScope::Only(Edition::Y2014);
    assert!(search_req(Corpus::Rules, &req)
        .iter()
        .all(|h| h.ruleset != "2024"));
    req.edition = EditionScope::Only(Edition::Y2024);
    assert!(search_req(Corpus::Rules, &req)
        .iter()
        .all(|h| h.ruleset != "2014"));
}

/// Plain-word text of every chunk of both SRD corpora, joined with a separator no phrase can span.
fn srd_text() -> Vec<String> {
    let store = srd_store();
    let (rules, lore) = srd_inputs(&store);
    rules
        .iter()
        .chain(lore.iter())
        .flat_map(|p| p.chunks.iter())
        .map(|c| format!("{} {}", c.name, c.body))
        .map(|t| plain_words(&t).join(" "))
        .collect()
}

#[test]
fn expansions_resolve_to_srd_text() {
    // Abbreviations (AoE, adv, ...) are not SRD words; the full form of every group and every rename target is.
    let text = srd_text();
    let present = |phrase: &str| {
        let needle = plain_words(phrase).join(" ");
        text.iter().any(|t| {
            t == &needle
                || t.starts_with(&format!("{needle} "))
                || t.ends_with(&format!(" {needle}"))
                || t.contains(&format!(" {needle} "))
        })
    };
    let synonyms = Synonyms::bundled();
    let mut unresolved: Vec<String> = Vec::new();
    for group in synonyms.groups() {
        if !group.iter().any(|t| present(t)) {
            unresolved.push(group.join(" / "));
        }
    }
    for (from, to) in synonyms.renames() {
        unresolved.extend(
            to.iter()
                .filter(|t| !present(t))
                .map(|t| format!("{from} -> {t}")),
        );
    }
    assert!(unresolved.is_empty(), "no SRD text for: {unresolved:?}");
}

#[test]
fn synonym_queries_return_srd_text_that_contains_the_expansion() {
    for (query, expansion, corpus) in [
        ("AC", "armor class", Corpus::Rules),
        ("OA", "opportunity attack", Corpus::Rules),
        ("THP", "temporary hit points", Corpus::Rules),
        ("DC", "difficulty class", Corpus::Rules),
        ("PB", "proficiency bonus", Corpus::Rules),
        ("elf race", "species", Corpus::Rules),
        ("ki", "focus points", Corpus::Lore),
    ] {
        let hits = search_req(
            corpus,
            &SearchRequest {
                limit: 50,
                ..SearchRequest::new(query)
            },
        );
        assert!(!hits.is_empty(), "{query}");
        let found = hits.iter().take(50).any(|h| {
            let hay = format!("{} {} {}", h.title, h.path, h.snippet).to_lowercase();
            hay.contains(expansion)
        });
        assert!(
            found,
            "{query}: no top-50 hit mentions {expansion}: {:?}",
            hits.iter().map(|h| &h.title).collect::<Vec<_>>()
        );
    }
}
