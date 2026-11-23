//! The builder description's linked elements: an option shows the cards of the elements it grants,
//! offers, creates or names. Bundled SRD content only.

use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

/// 2014 Warlock Pact Boon option: grants the Find Familiar spell.
const PACT_OF_THE_CHAIN: &str = "ID_WOTC_PHB_CLASS_FEATURE_WARLOCK_PACT_BOON_PACT_OF_THE_CHAIN";
/// 2024 Druid feature whose rule casts Find Familiar with a Wild Shape use.
const WILD_COMPANION: &str = "ID_2024_CLASS_FEATURE_DRUID_WILD_COMPANION";
const BARBARIAN: &str = "ID_2024_CLASS_BARBARIAN";

#[test]
fn a_feature_that_grants_a_spell_links_the_spell_with_its_text() {
    let (_d, app) = app();
    let pact = app
        .get_element(PACT_OF_THE_CHAIN)
        .expect("Pact of the Chain");
    let spell = pact
        .links
        .iter()
        .find(|l| l.summary.id == "ID_PHB_SPELL_FIND_FAMILIAR")
        .unwrap_or_else(|| panic!("Find Familiar in {:?}", pact.links));
    assert_eq!(
        (spell.via.as_str(), spell.content_type.as_str()),
        ("grant", "Spell")
    );
    assert!(
        spell.description_html.contains("familiar"),
        "{}",
        spell.description_html
    );
}

#[test]
fn a_rule_that_names_a_spell_links_it() {
    let (_d, app) = app();
    let wild = app.get_element(WILD_COMPANION).expect("Wild Companion");
    assert!(
        wild.links
            .iter()
            .any(|l| l.summary.id == "ID_2024_SPELL_FIND_FAMILIAR" && l.via == "rule"),
        "{:?}",
        wild.links
    );
}

#[test]
fn class_features_and_proficiencies_the_grants_list_shows_are_not_linked_again() {
    let (_d, app) = app();
    let barbarian = app.get_element(BARBARIAN).expect("Barbarian");
    assert!(
        barbarian.links.is_empty(),
        "{:?}",
        barbarian
            .links
            .iter()
            .map(|l| &l.summary.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn with_the_setting_off_no_links_are_built() {
    let (_d, app) = app();
    set_linked_elements(&app, false);
    let pact = app
        .get_element(PACT_OF_THE_CHAIN)
        .expect("Pact of the Chain");
    assert!(pact.links.is_empty(), "{:?}", pact.links);
    set_linked_elements(&app, true);
    let pact = app
        .get_element(PACT_OF_THE_CHAIN)
        .expect("Pact of the Chain");
    assert!(!pact.links.is_empty());
}

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    let app = AppState::new(paths).expect("state");
    set_linked_elements(&app, true);
    (dir, app)
}

fn set_linked_elements(app: &AppState, on: bool) {
    let mut settings = app.settings().expect("settings");
    settings.linked_elements = on;
    app.set_settings(settings).expect("set settings");
}
