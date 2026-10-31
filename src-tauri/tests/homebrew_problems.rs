//! GitHub #11 (C1) through the real command layer, with an invented book shaped like the
//! reporter's: a fresh character carries no notice about content it never touches; a held class
//! with a select whose filter can't be read shows the problem on that choice, not as a notice.

mod contract_support;
mod homebrew;
use borealis_lib::dto::Ruleset;
use homebrew::{book, App, OLD};
use serde_json::Value;

#[test]
fn a_fresh_character_has_no_notice_about_content_it_never_touches_and_a_broken_choice_says_so() {
    let app = App::new();
    app.import("old.xml", &book(OLD, "OTH", "Oldcaller", "20230101", false));
    let fresh = app.hero(Ruleset::Dnd2014);
    assert!(
        fresh.notices.iter().all(|n| !n.contains("unparsable")),
        "{:?}",
        fresh.notices
    );
    let held = app.pick_class(&fresh, "ID_OTH_CLASS");
    let v = app.checked("apply_selection", &held);
    let broken = v["slots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["title"] == "Broken Gift")
        .cloned()
        .expect("broken slot");
    assert_eq!(
        broken["problem"],
        "This choice's filter, \"Broken (Gift\", can't be read, so it offers nothing."
    );
    // shown on the choice, so not repeated in the character's notices; the feature the class
    // grants holds the unreadable requirements and is reported once
    assert!(
        held.notices
            .iter()
            .all(|n| !n.starts_with("ID_OTH_CLASS has unparsable")),
        "{:?}",
        held.notices
    );
    assert!(
        held.notices
            .iter()
            .any(|n| n.starts_with("ID_OTH_CF_ULTIMATE has unparsable")),
        "{:?}",
        held.notices
    );
    // a choice with options has no problem
    let ok = v["slots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["title"] == "Spider Gift")
        .cloned()
        .unwrap();
    assert_eq!(ok["problem"], Value::Null);
}

/// Review L1: the class's notice is left out only when every unreadable part of it is a select
/// shown on a choice. Here the class also has a stat whose requirement can't be read, which no
/// choice shows, so the notice stays.
#[test]
fn a_notice_stays_while_part_of_it_is_shown_nowhere() {
    let app = App::new();
    let xml = book(OLD, "OTH", "Oldcaller", "20230101", false).replace(
        r#"<grant type="Class Feature" id="ID_OTH_CF_ULTIMATE" level="1" />"#,
        r#"<grant type="Class Feature" id="ID_OTH_CF_ULTIMATE" level="1" />
      <stat name="speed" value="5" requirements="Some Prose Name" />"#,
    );
    app.import("old.xml", &xml);
    let held = app.pick_class(&app.hero(Ruleset::Dnd2014), "ID_OTH_CLASS");
    assert!(
        held.notices
            .iter()
            .any(|n| n.starts_with("ID_OTH_CLASS has unparsable")),
        "{:?}",
        held.notices
    );
}
