//! The builder description's Grants list (tester video 2026-11-02): what choosing an element
//! gives, named the way the builder asks for it. Bundled SRD content only.

use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const ACOLYTE: &str = "ID_2024_BACKGROUND_ACOLYTE";

fn app() -> (tempfile::TempDir, AppState) {
    borealis_engine::invariants::enable_for_thread();
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    };
    (dir, AppState::new(paths).expect("state"))
}

/// Text of an HTML fragment the way the description pane compares it: tags become spaces and
/// runs of space collapse.
fn pane_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_cut_short_description_is_the_start_of_the_body_text() {
    // B9: the pane leaves out a short description that only repeats the body; it can tell once
    // the host's trailing "..." is dropped.
    let (_d, app) = app();
    let acolyte = app.get_element(ACOLYTE).expect("Acolyte");
    let short = acolyte.summary.short_description.trim();
    let cut = short
        .strip_suffix("...")
        .expect("the Acolyte's summary is cut");
    assert!(
        pane_text(&acolyte.description_html).starts_with(cut.trim_end()),
        "{cut:?} starts {:?}",
        pane_text(&acolyte.description_html)
    );
}

#[test]
fn grants_name_custom_feat_and_package_rules() {
    // B9: the Acolyte's Grants named only its three proficiencies; the feat, the ability score
    // choice and the starting equipment are rules of other kinds.
    let (_d, app) = app();
    let acolyte = app.get_element(ACOLYTE).expect("Acolyte");
    for wanted in [
        "Magic Initiate (Cleric)",
        "Choose: Ability Scores (Acolyte)",
        "Choose: Starting Equipment (Acolyte)",
        "Insight",
        "Religion",
    ] {
        assert!(
            acolyte.grants.iter().any(|g| g == wanted),
            "{wanted} in {:?}",
            acolyte.grants
        );
    }
}
