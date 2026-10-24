//! One test group per ranking tweak (spec section 5, tweaks 1-7): each flag changes the outcome in the way the
//! spec describes, and with the flag off the behaviour is absent. Synthetic fixtures only.

mod common;

use borealis_search::{Edition, EditionScope, EditionTag, SearchRequest, Tweaks};
use common::{chunk, lore_engine, only, titles, titles_req};

const NONE: EditionTag = EditionTag::None;

#[test]
fn tweak1_field_weights_rank_a_name_hit_above_a_dense_body_hit() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Zephyr", "Test", "calm morning breeze", NONE),
            chunk(
                "b",
                "Gale",
                "Test",
                "zephyr zephyr zephyr zephyr zephyr zephyr",
                NONE,
            ),
        ],
    );
    let weighted = titles(&engine, "zephyr", only(|t| t.field_weights = true));
    let unweighted = titles(&engine, "zephyr", Tweaks::none());
    assert_eq!(weighted[0], "Zephyr");
    assert_eq!(unweighted[0], "Gale");
}

#[test]
fn tweak2_exact_name_boost_puts_the_named_element_first() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Magic Missile", "Spell", "darts of force", NONE),
            chunk(
                "b",
                "Magic Missile Handbook",
                "Book",
                "magic missile magic missile magic missile magic missile",
                NONE,
            ),
        ],
    );
    assert_eq!(
        titles(&engine, "Magic Missile", only(|t| t.exact_name = true))[0],
        "Magic Missile"
    );
    assert_eq!(
        titles(&engine, "magic missile", Tweaks::none())[0],
        "Magic Missile Handbook"
    );
}

#[test]
fn tweak2_exact_name_ignores_case_quotes_and_punctuation() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Tasha\u{2019}s Brew", "Spell", "bubbling", NONE),
            chunk(
                "b",
                "Brew Notes",
                "Book",
                "tashas tashas tashas brew brew",
                NONE,
            ),
        ],
    );
    let t = only(|t| t.exact_name = true);
    assert_eq!(
        titles(&engine, "TASHA'S brew!", t)[0],
        "Tasha\u{2019}s Brew"
    );
}

#[test]
fn tweak3_prefix_finds_names_by_their_start_only() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Fireball", "Spell", "a bright streak", NONE),
            chunk("b", "Cure Wounds", "Spell", "a creature you touch", NONE),
        ],
    );
    let on = only(|t| t.prefix = true);
    assert_eq!(titles(&engine, "fireb", on), ["Fireball"]);
    assert_eq!(titles(&engine, "cure w", on), ["Cure Wounds"]);
    assert!(titles(&engine, "fireb", Tweaks::none()).is_empty());
    // A trailing space means the last word is finished: no prefix matching.
    assert!(titles(&engine, "fireb ", on).is_empty());
    // One letter is too short for a prefix clause.
    assert!(titles(&engine, "f", on).is_empty());
}

#[test]
fn tweak3_prefix_skips_sentences_and_known_abbreviations() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Acid Arrow", "Spell", "a shimmering green arrow", NONE),
            chunk("b", "Fireball", "Spell", "a bright streak", NONE),
        ],
    );
    let on = only(|t| t.prefix = true);
    // Up to three words are a name being typed; a longer sentence is not.
    assert_eq!(titles(&engine, "cast fireb", on), ["Fireball"]);
    assert!(titles(&engine, "how do i cast fireb", on).is_empty());
    // `AC` is a known abbreviation (synonym table), so it is not the start of "Acid Arrow".
    assert!(titles(&engine, "ac", on).is_empty());
}

#[test]
fn default_tweaks_are_the_adopted_configuration() {
    let t = Tweaks::default();
    assert!(t.prefix && t.typo && t.synonyms && t.edition);
    assert!(!t.field_weights && !t.exact_name && !t.phrase);
}

#[test]
fn tweak3_prefix_does_not_match_body_text() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![chunk(
            "a",
            "Other",
            "Spell",
            "fireball is mentioned only here",
            NONE,
        )],
    );
    assert!(titles(&engine, "fireb", only(|t| t.prefix = true)).is_empty());
}

#[test]
fn tweak4_typo_tolerance_fixes_one_and_two_edit_misspellings() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Fireball", "Spell", "a bright streak", NONE),
            chunk("b", "Polymorph", "Spell", "transforms a creature", NONE),
            chunk("c", "Eldritch Blast", "Cantrip", "a beam", NONE),
        ],
    );
    let on = only(|t| t.typo = true);
    assert_eq!(titles(&engine, "firball", on)[0], "Fireball");
    assert_eq!(titles(&engine, "eldrich blast", on)[0], "Eldritch Blast");
    // Distance 2 only from 8 characters on: "polymorf" is 8 and two edits away.
    assert_eq!(titles(&engine, "polymorf", on)[0], "Polymorph");
    assert!(titles(&engine, "firball", Tweaks::none()).is_empty());
}

#[test]
fn tweak4_short_words_are_never_fuzzed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Qbc", "Test", "short", NONE),
            chunk("b", "Abcf", "Test", "four letters", NONE),
        ],
    );
    let on = only(|t| t.typo = true);
    assert!(
        titles(&engine, "abc", on).is_empty(),
        "3 letters: no fuzzing"
    );
    assert_eq!(
        titles(&engine, "abcd", on),
        ["Abcf"],
        "4 letters: distance 1"
    );
}

#[test]
fn tweak4_five_letter_words_get_distance_one_only() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![chunk("a", "Goblin", "Monster", "small green", NONE)],
    );
    let on = only(|t| t.typo = true);
    assert_eq!(titles(&engine, "goblen", on), ["Goblin"]);
    assert!(
        titles(&engine, "gxblen", on).is_empty(),
        "two edits at length 6"
    );
}

#[test]
fn tweak5_phrase_proximity_prefers_adjacent_words() {
    let dir = tempfile::tempdir().unwrap();
    // Identical lengths and term frequencies: only proximity separates them; "a-far" wins ties by key.
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk(
                "a-far",
                "Doc One",
                "Test",
                "alpha one two three four beta",
                NONE,
            ),
            chunk(
                "z-near",
                "Doc Two",
                "Test",
                "alpha beta one two three four",
                NONE,
            ),
        ],
    );
    let on = only(|t| t.phrase = true);
    assert_eq!(titles(&engine, "alpha beta", on)[0], "Doc Two");
    assert_eq!(titles(&engine, "alpha beta", Tweaks::none())[0], "Doc One");
}

#[test]
fn tweak6_synonyms_expand_abbreviations_and_renames() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk(
                "a",
                "Opportunity Attack",
                "Rule",
                "a reaction when a creature leaves reach",
                NONE,
            ),
            chunk(
                "b",
                "Species",
                "Rule",
                "your character's biological kind",
                NONE,
            ),
        ],
    );
    let on = only(|t| t.synonyms = true);
    assert_eq!(titles(&engine, "OA", on), ["Opportunity Attack"]);
    assert_eq!(titles(&engine, "race", on), ["Species"]);
    assert!(titles(&engine, "OA", Tweaks::none()).is_empty());
    // Renames are one-way.
    assert!(titles(&engine, "species", only(|t| t.synonyms = true))
        .iter()
        .all(|t| t == "Species"));
}

#[test]
fn tweak6_the_users_own_words_outrank_their_expansions() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Armor Class", "Rule", "how hard you are to hit", NONE),
            chunk("b", "AC", "Rule", "the abbreviation as written", NONE),
        ],
    );
    assert_eq!(titles(&engine, "ac", Tweaks::ALL).len(), 2);
    assert_eq!(titles(&engine, "ac", Tweaks::ALL)[0], "AC");
}

fn edition_engine(dir: &std::path::Path) -> borealis_search::SearchEngine {
    lore_engine(
        dir,
        vec![
            chunk(
                "a-2014",
                "Dash",
                "Action",
                "move extra distance",
                EditionTag::Y2014,
            ),
            chunk(
                "b-2024",
                "Dash",
                "Action",
                "move extra distance",
                EditionTag::Y2024,
            ),
            chunk("c-none", "Dash", "Action", "move extra distance", NONE),
        ],
    )
}

fn keys(engine: &borealis_search::SearchEngine, req: &SearchRequest, t: Tweaks) -> Vec<String> {
    let cfg = borealis_search::SearchConfig {
        tweaks: t,
        ..Default::default()
    };
    engine
        .search(borealis_search::Corpus::Lore, req, &cfg)
        .unwrap()
        .into_iter()
        .map(|h| h.doc_key)
        .collect()
}

#[test]
fn tweak7_edition_filter_keeps_the_chosen_edition_and_untagged_content() {
    let dir = tempfile::tempdir().unwrap();
    let engine = edition_engine(dir.path());
    let mut req = SearchRequest::new("dash");
    req.edition = EditionScope::Only(Edition::Y2014);
    let on = only(|t| t.edition = true);
    let mut got = keys(&engine, &req, on);
    got.sort();
    assert_eq!(got, ["a-2014", "c-none"]);
    req.edition = EditionScope::Only(Edition::Y2024);
    let mut got = keys(&engine, &req, on);
    got.sort();
    assert_eq!(got, ["b-2024", "c-none"]);
    // Flag off: the scope is ignored.
    assert_eq!(keys(&engine, &req, Tweaks::none()).len(), 3);
}

#[test]
fn tweak7_both_editions_boosts_the_open_characters_edition() {
    let dir = tempfile::tempdir().unwrap();
    let engine = edition_engine(dir.path());
    let mut req = SearchRequest::new("dash");
    req.edition = EditionScope::Both {
        preferred: Some(Edition::Y2024),
    };
    let on = only(|t| t.edition = true);
    let got = keys(&engine, &req, on);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], "b-2024");
    // Without the boost the tie falls back to key order.
    assert_eq!(keys(&engine, &req, Tweaks::none())[0], "a-2014");
}

#[test]
fn filters_by_type_pack_and_excluded_source() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk("a", "Dash", "Action", "move", NONE),
            chunk("b", "Dash Two", "Action", "move", NONE),
        ],
    );
    let mut req = SearchRequest::new("dash");
    req.types = vec!["Nothing".into()];
    assert!(titles_req(&engine, &req, Tweaks::ALL).is_empty());
    req.types = vec!["Test".into()];
    assert_eq!(titles_req(&engine, &req, Tweaks::ALL).len(), 2);
    req.packs = Some(vec!["other".into()]);
    assert!(titles_req(&engine, &req, Tweaks::ALL).is_empty());
    req.packs = Some(vec!["fixture".into()]);
    req.exclude_sources = vec!["Fixture".into()];
    assert!(titles_req(&engine, &req, Tweaks::ALL).is_empty());
}

#[test]
fn snippets_are_plain_text_with_highlight_offsets_and_pages_work() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(
        dir.path(),
        vec![
            chunk(
                "a",
                "Alpha",
                "Test",
                "the quick brown fox jumps over the lazy dog",
                NONE,
            ),
            chunk("b", "Beta", "Test", "another fox appears", NONE),
        ],
    );
    let cfg = borealis_search::SearchConfig::default();
    let mut req = SearchRequest::new("fox");
    let hits = engine
        .search(borealis_search::Corpus::Lore, &req, &cfg)
        .unwrap();
    assert_eq!(hits.len(), 2);
    for h in &hits {
        assert!(!h.highlights.is_empty());
        let (s, e) = h.highlights[0];
        assert_eq!(&h.snippet[s..e], "fox");
        assert!(!h.snippet.contains('<'));
    }
    req.limit = 1;
    req.offset = 1;
    let page = engine
        .search(borealis_search::Corpus::Lore, &req, &cfg)
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].doc_key, hits[1].doc_key);
}

#[test]
fn empty_and_stopword_only_queries_return_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = lore_engine(dir.path(), vec![chunk("a", "Alpha", "Test", "text", NONE)]);
    assert!(titles(&engine, "", Tweaks::ALL).is_empty());
    assert!(titles(&engine, "  ", Tweaks::ALL).is_empty());
    assert!(titles(&engine, "the of ", Tweaks::none()).is_empty());
}
