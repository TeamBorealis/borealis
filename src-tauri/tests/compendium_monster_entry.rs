//! Backlog B-05 / audit-8 FX-62: bundled SRD monsters are their own compendium type ("Monster",
//! with a count and a type filter) and read as a stat block in the description pane.

use borealis_lib::dto_ext::CompendiumQuery;
use borealis_lib::state::{AppPaths, AppState};
use std::path::PathBuf;

const GOBLIN: &str = "ID_SRD51_MONSTER_GOBLIN";
const UNICORN: &str = "ID_SRD51_MONSTER_UNICORN";

fn state() -> (tempfile::TempDir, AppState) {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    let state = AppState::new(paths).unwrap();
    (data, state)
}

#[test]
fn monsters_have_their_own_type_with_a_count_and_filter() {
    // Arrange
    let (_dir, app) = state();

    // Act
    let facets = app.compendium_facets().unwrap();
    let page = app
        .search_compendium(&CompendiumQuery {
            query: "goblin".into(),
            types: Some(vec!["monster".into()]),
            sources: None,
            ruleset: None,
            limit: 50,
            offset: 0,
        })
        .unwrap();

    // Assert
    let monster = facets
        .types
        .iter()
        .find(|t| t.id == "monster")
        .expect("a monster type");
    assert_eq!(monster.label, "Monster");
    assert!(monster.count >= 400, "{}", monster.count);
    assert!(
        page.items
            .iter()
            .any(|i| i.id == GOBLIN && i.ty == "monster"),
        "{:?}",
        page.items.iter().map(|i| &i.id).collect::<Vec<_>>()
    );
    assert!(page.items.iter().all(|i| i.ty == "monster"));
}

#[test]
fn the_entry_reads_type_then_the_whole_stat_block_then_its_sections() {
    // Arrange
    let (_dir, app) = state();

    // Act
    let goblin = app.get_element(GOBLIN).unwrap();
    let unicorn = app.get_element(UNICORN).unwrap();

    // Assert
    let facts: Vec<(&str, &str)> = goblin
        .facts
        .iter()
        .map(|f| (f.label.as_str(), f.value.as_str()))
        .collect();
    assert_eq!(facts.first(), Some(&("Type", "Monster")), "{facts:?}");
    assert!(
        facts.iter().all(|(l, _)| *l != "Armor Class"),
        "the stat block is the entry's text, not facts: {facts:?}"
    );
    // the whole stat block, in its order, before the sections
    let goblin_html = &goblin.description_html;
    let order = [
        "<p><i>Small humanoid (goblinoid), neutral evil</i></p>",
        "<p><i>Armor Class.</i> 15 (leather armor, shield)</p>",
        "<p><i>Hit Points.</i> 7 (2d6)</p>",
        "<p><i>Speed.</i> 30 ft.</p>",
        "<th>STR</th><th>DEX</th><th>CON</th><th>INT</th><th>WIS</th><th>CHA</th>",
        "<td>8 (-1)</td><td>14 (+2)</td><td>10 (+0)</td><td>10 (+0)</td><td>8 (-1)</td><td>8 (-1)</td>",
        "<p><i>Skills.</i> Stealth +6</p>",
        "<p><i>Senses.</i> Darkvision 60 ft., passive Perception 9</p>",
        "<p><i>Languages.</i> Common, Goblin</p>",
        "<p><i>Challenge.</i> 1/4 (50 XP)</p>",
        "<h4>Traits</h4>",
    ];
    let at: Vec<usize> = order
        .iter()
        .map(|o| {
            goblin_html
                .find(o)
                .unwrap_or_else(|| panic!("{o} missing in {goblin_html}"))
        })
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{goblin_html}");
    let unicorn_html = &unicorn.description_html;
    for line in [
        "<p><i>Speed.</i> 50 ft.</p>",
        "<p><i>Damage Immunities.</i> Poison</p>",
        "<p><i>Condition Immunities.</i> Charmed, Paralyzed, Poisoned</p>",
        "<p><i>Languages.</i> Celestial, Elvish, Sylvan, telepathy 60 ft.</p>",
    ] {
        assert!(
            unicorn_html.contains(line),
            "{line} missing in {unicorn_html}"
        );
    }
    assert!(
        goblin_html.contains("<h4>Traits</h4><p><b><i>Nimble Escape.</i></b>"),
        "{goblin_html}"
    );
    assert!(goblin_html.contains("<h4>Actions</h4>"), "{goblin_html}");
    let html = &unicorn.description_html;
    let actions = html.find("<h4>Actions</h4>").expect("actions");
    let legendary = html.find("<h4>Legendary actions</h4>").expect("legendary");
    assert!(actions < legendary, "{html}");
    assert!(
        html.contains("<b><i>Healing Touch (3/Day).</i></b>"),
        "{html}"
    );
    assert!(
        html.contains("<b><i>Heal Self (Costs 3 Actions).</i></b>"),
        "{html}"
    );
}
