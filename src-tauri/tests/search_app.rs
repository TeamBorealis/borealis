//! S1.5 (local-search-spec.md sections 9 and 11): the app's keyword search service. The `search` command over the
//! Rules and Lore indexes, typo tolerance offline (`firball` -> Fireball first), `search_compendium` on the Lore
//! index with unchanged ranking rules, the rules passage with its attribution, and the index lifecycle: background
//! build, incremental sync on pack enable/disable, status events, clear caches, fallback before the first build.

use borealis_lib::content_packs::NullSink;
use borealis_lib::dto_ext::CompendiumQuery;
use borealis_lib::search_api::{SearchQuery, SearchScope};
use borealis_lib::search_service::{IndexState, IndexStatus, StatusSink};
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FIREBALL_2014: &str = "ID_PHB_SPELL_FIREBALL";
const WAIT: Duration = Duration::from_secs(120);

fn state() -> (tempfile::TempDir, AppState) {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    (data, AppState::new(paths).unwrap())
}

fn built() -> (tempfile::TempDir, AppState) {
    let (dir, app) = state();
    app.build_search_index_now().expect("index builds");
    (dir, app)
}

fn q(scope: SearchScope, text: &str) -> SearchQuery {
    SearchQuery {
        scope,
        query: text.into(),
        limit: 20,
        ..SearchQuery::default()
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<IndexStatus>>);
impl StatusSink for Recorder {
    fn index_status(&self, s: &IndexStatus) {
        self.0.lock().unwrap().push(s.clone());
    }
}

#[test]
fn a_typo_finds_fireball_first_in_every_element_scope() {
    // Arrange
    let (_dir, app) = built();

    // Act
    let all = app.search(&q(SearchScope::All, "firball")).unwrap();
    let lore = app.search(&q(SearchScope::Lore, "firball")).unwrap();

    // Assert
    for page in [&all, &lore] {
        let first = page
            .items
            .first()
            .unwrap_or_else(|| panic!("no hit: all={} lore={}", all.total, lore.total));
        assert_eq!(
            first.title,
            "Fireball",
            "{:?}",
            page.items.iter().map(|h| &h.title).collect::<Vec<_>>()
        );
        assert_eq!(first.ty, "spell");
        assert!(first.element_id.is_some());
    }
}

#[test]
fn compendium_search_uses_the_lore_index_for_typos_and_keeps_name_tiers_first() {
    // Arrange
    let (_dir, app) = built();
    let query = |text: &str| CompendiumQuery {
        query: text.into(),
        types: None,
        sources: None,
        ruleset: None,
        limit: 500,
        offset: 0,
    };

    // Act
    let typo = app.search_compendium(&query("firball")).unwrap();
    let fire = app.search_compendium(&query("fire")).unwrap();

    // Assert: the typo reaches Fireball through the index (no name contains "firball").
    assert_eq!(
        typo.items.first().map(|i| i.name.as_str()),
        Some("Fireball")
    );
    // Name tiers stay as before: exact/prefix/word/substring, then description (index) matches.
    let tier = |n: &str| {
        let n = n.to_lowercase();
        if n == "fire" {
            0
        } else if n.starts_with("fire") {
            1
        } else if n
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.starts_with("fire"))
        {
            2
        } else if n.contains("fire") {
            3
        } else {
            4
        }
    };
    let tiers: Vec<u8> = fire.items.iter().map(|i| tier(&i.name)).collect();
    assert!(tiers.windows(2).all(|w| w[0] <= w[1]), "{tiers:?}");
    assert!(
        tiers.contains(&4),
        "description matches come from the index"
    );
    let mut ids: Vec<&str> = fire.items.iter().map(|i| i.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), fire.items.len(), "no element twice");
}

#[test]
fn rules_scope_finds_sections_and_conditions_and_opens_the_passage_with_attribution() {
    // Arrange
    let (_dir, app) = built();

    // Act
    let grappled = app.search(&q(SearchScope::Rules, "grappled")).unwrap();
    let cover = app
        .search(&SearchQuery {
            edition: Some("dnd2024".into()),
            ..q(SearchScope::Rules, "half cover")
        })
        .unwrap();

    // Assert
    let first = grappled.items.first().expect("a hit");
    assert_eq!(first.title, "Grappled");
    assert!(first
        .element_id
        .as_deref()
        .is_some_and(|id| id.contains("CONDITION_GRAPPLED")));
    let section = cover
        .items
        .iter()
        .find(|h| h.ty == "rules")
        .expect("a rules section");
    assert!(
        section.element_id.is_none() && section.doc_key.starts_with("rules:srd-5.2.1-rules-text/")
    );
    assert!(!section.path.is_empty());
    assert!(cover
        .items
        .iter()
        .all(|h| h.ruleset != Some(borealis_content::Ruleset::Dnd2014)));
    let passage = app.get_search_passage(&section.doc_key).unwrap();
    assert!(!passage.html.is_empty() && !passage.html.contains("<script"));
    assert_eq!(passage.license, "CC-BY-4.0");
    assert!(passage
        .attribution
        .contains("System Reference Document 5.2.1"));
    assert!(app.get_search_passage("rules:nope/x").is_err());
}

#[test]
fn highlights_are_valid_ranges_of_the_snippet() {
    // Arrange
    let (_dir, app) = built();

    // Act
    let page = app
        .search(&q(SearchScope::All, "opportunity attack"))
        .unwrap();

    // Assert
    assert!(!page.items.is_empty());
    let mut any = false;
    for hit in &page.items {
        let units: Vec<u16> = hit.snippet.encode_utf16().collect();
        for [s, e] in &hit.highlights {
            assert!(s < e && *e <= units.len(), "{hit:?}");
            any = true;
        }
    }
    assert!(any, "some snippet is highlighted");
}

#[test]
fn edition_and_filters_narrow_the_hits_and_sessions_stay_empty() {
    // Arrange
    let (_dir, app) = built();

    // Act
    let only_2014 = app
        .search(&SearchQuery {
            edition: Some("dnd2014".into()),
            ..q(SearchScope::Lore, "fireball")
        })
        .unwrap();
    let spells = app
        .search(&SearchQuery {
            types: Some(vec!["spell".into()]),
            ..q(SearchScope::All, "fire")
        })
        .unwrap();
    let sessions = app.search(&q(SearchScope::Sessions, "fireball")).unwrap();

    // Assert
    assert_eq!(
        only_2014
            .items
            .first()
            .and_then(|h| h.element_id.as_deref()),
        Some(FIREBALL_2014)
    );
    assert!(only_2014
        .items
        .iter()
        .all(|h| h.ruleset != Some(borealis_content::Ruleset::Dnd2024)));
    assert!(!spells.items.is_empty() && spells.items.iter().all(|h| h.ty == "spell"));
    assert_eq!(sessions.total, 0);
    assert!(app
        .search(&SearchQuery {
            edition: Some("5e".into()),
            ..q(SearchScope::All, "x")
        })
        .is_err());
}

#[test]
fn before_the_first_build_search_falls_back_and_starts_the_build() {
    // Arrange
    let (_dir, app) = state();
    let rec = Arc::new(Recorder::default());
    app.search.set_sink(rec.clone());
    assert_eq!(app.search_index_status().state, IndexState::Idle);

    // Act
    let early = app.search(&q(SearchScope::Lore, "fireball")).unwrap();
    let rules_early = app.search(&q(SearchScope::Rules, "grappled")).unwrap();
    assert!(app.search.wait_idle(WAIT));

    // Assert
    assert!(
        early.items.iter().any(|h| h.title == "Fireball"),
        "compendium scan fallback"
    );
    assert!(rules_early.items.is_empty() || app.search_index_status().state == IndexState::Ready);
    assert_eq!(app.search_index_status().state, IndexState::Ready);
    let states: Vec<IndexState> = rec.0.lock().unwrap().iter().map(|s| s.state).collect();
    assert_eq!(states.first(), Some(&IndexState::Building));
    assert_eq!(states.last(), Some(&IndexState::Ready));
    let typo = app.search(&q(SearchScope::Lore, "firball")).unwrap();
    assert_eq!(
        typo.items.first().map(|h| h.title.as_str()),
        Some("Fireball")
    );
}

#[test]
fn disabling_a_pack_resyncs_incrementally_in_the_background() {
    // Arrange
    let (dir, app) = state();
    app.start_search_index().unwrap();
    assert!(app.search.wait_idle(WAIT));
    let manifest = || std::fs::read_to_string(dir.path().join("search/v1/manifest.json")).unwrap();
    assert!(manifest().contains("\"srd-5.1\""));

    // Act
    app.set_pack_enabled("srd-5.1", false, &NullSink).unwrap();
    assert!(app.search.wait_idle(WAIT));
    let after = app
        .search(&SearchQuery {
            limit: 200,
            ..q(SearchScope::Lore, "fireball")
        })
        .unwrap();

    // Assert
    assert!(
        !manifest().contains("\"srd-5.1\""),
        "the disabled pack left the lore index"
    );
    assert!(manifest().contains("\"srd-5.2.1\""));
    assert!(after
        .items
        .iter()
        .all(|h| h.element_id.as_deref() != Some(FIREBALL_2014)));
    assert!(
        after.items.iter().any(|h| h.title == "Fireball"),
        "the 2024 Fireball stays"
    );
}

#[test]
fn clear_caches_removes_the_keyword_indexes_and_the_next_search_rebuilds() {
    // Arrange
    let (dir, app) = built();
    assert!(dir.path().join("search/v1/lore").is_dir());

    // Act
    app.clear_caches(|| 0).unwrap();

    // Assert
    assert!(!dir.path().join("search").exists());
    assert_eq!(app.search_index_status().state, IndexState::Idle);
    app.search(&q(SearchScope::All, "fireball")).unwrap();
    assert!(app.search.wait_idle(WAIT));
    assert_eq!(app.search_index_status().state, IndexState::Ready);
    assert!(dir.path().join("search/v1/manifest.json").is_file());
}
