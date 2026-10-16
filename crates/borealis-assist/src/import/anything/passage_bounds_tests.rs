//! Passage boundaries: end matter after the last element, sub-sections a description already
//! partly prints, lore a generated child element left out (synthetic sources).

use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};

const MIREFOLK: &str = "# Mirefolk\n\
Mirefolk live in bogs and sing to frogs at dusk every night of the year.\n\
**Swim.** You swim at 30 feet.\n\
\n\
### Deep Mirefolk\n\
[Summary: Deep mirefolk dwell under the black lakes and rarely come up to see the moon.]\n\
**Ability Score Increase.** Your Constitution score increases by 1.\n\
**Hold Breath.** You hold your breath for an hour under the black water.\n\
\n\
## More to Follow\n\
The author hopes to add the sky mirefolk and the salt mirefolk to a later edition of this book.\n\
\n\
## Legal Information\n\
All names are trademarks of their owners and are used here with their kind permission today.\n\
\n\
## Special Thanks To\n\
Thanks to the frog choir for their patience with every single draft of this little book.\n";

fn mirefolk() -> Vec<PlannedElement> {
    let mut root = planned("r0", "Mirefolk", "Race", Role::Standalone, Some(1));
    root.excerpt.as_mut().unwrap().end_line = 17;
    let mut swim = planned("r1", "Swim", "Racial Trait", Role::Part, Some(3));
    swim.owner = Some("r0".into());
    let mut deep = planned("s1", "Deep Mirefolk", "Sub Race", Role::Standalone, Some(5));
    deep.excerpt.as_mut().unwrap().end_line = 17;
    // A trait of the subrace planned on its own: the subrace's passage up to it is lore and a
    // short line only.
    let mut hold = planned("s2", "Hold Breath", "Racial Trait", Role::Part, Some(8));
    hold.parent = Some("s1".into());
    vec![root, swim, deep, hold]
}

const ROOT_DESC: &str = "<p>Mirefolk live in bogs and sing to frogs at dusk every night of the year.</p><p>You swim at 30 feet.</p>";
const DEEP_DESC: &str = "<p><strong>Ability Score Increase.</strong> Your Constitution score increases by 1.</p><p><strong>Hold Breath.</strong> You hold your breath for an hour under the black water.</p>";

#[test]
fn the_end_matter_after_the_last_element_is_no_sub_section_of_it_and_is_reported() {
    let mut d = draft(
        MIREFOLK,
        mirefolk(),
        &[
            ROOT_DESC,
            "<p>You swim at 30 feet.</p>",
            DEEP_DESC,
            "<p>You hold your breath for an hour under the black water.</p>",
        ],
    );
    let changed = add_sub_sections(&mut d);
    assert!(!changed.contains(&"r0".to_string()), "{changed:?}");
    let root = d.elements[0].json["description"].as_str().unwrap();
    assert!(
        !root.contains("Legal") && !root.contains("frog choir") && !root.contains("More to"),
        "{root}"
    );
    // Not silently lost: each closing section is a report entry.
    let sources: Vec<String> = uncovered_prose(&d).into_iter().map(|e| e.source).collect();
    for s in ["More to Follow", "Legal Information", "Special Thanks To"] {
        assert!(sources.iter().any(|x| x == s), "{s}: {sources:?}");
    }
}

#[test]
fn a_childs_bracketed_opening_lore_it_left_out_is_put_back_without_the_brackets() {
    let mut d = draft(
        MIREFOLK,
        mirefolk(),
        &[
            ROOT_DESC,
            "<p>You swim at 30 feet.</p>",
            DEEP_DESC,
            "<p>You hold your breath for an hour under the black water.</p>",
        ],
    );
    let changed = add_sub_sections(&mut d);
    assert!(changed.contains(&"s1".to_string()), "{changed:?}");
    let deep = d.elements[2].json["description"].as_str().unwrap();
    assert!(
        deep.starts_with(
            "<p>Deep mirefolk dwell under the black lakes and rarely come up to see the moon.</p>"
        ),
        "{deep}"
    );
    assert!(!deep.contains("[Summary"), "{deep}");
}

#[test]
fn a_sub_section_the_description_partly_prints_gets_only_its_missing_table() {
    let src = "## Lamplighter\n\
Lamplighters keep the roads lit through every long and dark winter night.\n\
\n\
### Feature: Wick Lore\n\
You know every wick maker in the city and they know you by name.\n\
\n\
### Suggested Characteristics\n\
Lamplighters are patient and quiet, and they keep odd hours all year.\n\
\n\
d4 | Personality Trait\n\
1 | I hum to the flame.\n\
2 | **Calm.** I never hurry.\n";
    let root = planned("r0", "Lamplighter", "Background", Role::Standalone, Some(1));
    let mut lore = planned("r1", "Wick Lore", "Background Feature", Role::Part, Some(4));
    lore.parent = Some("r0".into());
    let mut d = draft(
        src,
        vec![root, lore],
        &[
            "<p>Lamplighters keep the roads lit through every long and dark winter night.</p><h5>Suggested Characteristics</h5><p>Lamplighters are patient and quiet, and they keep odd hours all year.</p>",
            "<p>You know every wick maker in the city and they know you by name.</p>",
        ],
    );
    assert_eq!(add_sub_sections(&mut d), vec!["r0".to_string()]);
    let desc = d.elements[0].json["description"].as_str().unwrap();
    assert_eq!(
        desc.matches("Suggested Characteristics").count(),
        1,
        "{desc}"
    );
    assert_eq!(desc.matches("patient and quiet").count(), 1, "{desc}");
    assert!(
        desc.ends_with("<table><thead><tr><th>d4</th><th>Personality Trait</th></tr></thead><tbody><tr><td>1</td><td>I hum to the flame.</td></tr><tr><td>2</td><td><strong>Calm.</strong> I never hurry.</td></tr></tbody></table>"),
        "{desc}"
    );
}

/// An item whose per-version effect table is printed under a caption of the item's own heading
/// level, one blank line after each row, with two versions planned (1-based lines: item 1,
/// caption 6, rows 8..14, next item 16).
const CLOAK: &str = "#### Cloak of Seasons\n\
Wondrous item, rare\n\
This cloak of woven leaves turns with the year.\n\
Roll on the table to find its season.\n\
\n\
#### Seasons of the cloak\n\
\n\
d3 | Season | Effect\n\
\n\
1 | Spring | You resist poison damage.\n\
\n\
2 | Summer | You resist fire damage.\n\
\n\
3 | Winter | You resist cold damage.\n\
\n\
#### Lamp of Dusk\n\
Wondrous item, common\n\
This lamp glows at dusk.\n";

fn cloak() -> Vec<PlannedElement> {
    let mut item = planned(
        "c",
        "Cloak of Seasons",
        "Magic Item",
        Role::Standalone,
        Some(1),
    );
    item.excerpt.as_mut().unwrap().end_line = 14;
    let spring = planned(
        "c1",
        "Cloak of Seasons, Spring",
        "Magic Item",
        Role::Standalone,
        Some(1),
    );
    let winter = planned(
        "c3",
        "Cloak of Seasons, Winter",
        "Magic Item",
        Role::Standalone,
        Some(1),
    );
    let caption = planned(
        "t",
        "Seasons of the cloak",
        "Rules Glossary Entry",
        Role::Text,
        Some(6),
    );
    let lamp = planned(
        "l",
        "Lamp of Dusk",
        "Magic Item",
        Role::Standalone,
        Some(16),
    );
    vec![item, spring, winter, caption, lamp]
}

#[test]
fn a_per_version_table_under_a_caption_of_the_items_level_is_the_items() {
    // Arrange: the item's text left the table out; a text entry was planned on its caption.
    let item_desc = "<p>This cloak of woven leaves turns with the year. Roll on the table to find its season.</p>";
    let mut d = draft(
        CLOAK,
        cloak(),
        &[
            item_desc,
            item_desc,
            item_desc,
            "<p>d3 Season Effect</p>",
            "<p>This lamp glows at dusk.</p>",
        ],
    );
    // Act
    let folded = tables::owner_table_entries(&d.plan, CLOAK);
    let changed = tables::add_owner_tables(&mut d);
    // Assert: the caption entry is a piece of the item, the item gets the whole table (one
    // table, under its caption); the versions keep their text (their row comes from the item).
    assert_eq!(folded.len(), 1, "{folded:?}");
    assert_eq!(folded[0].0, "t");
    assert!(
        folded[0].1.contains("row of Cloak of Seasons, kept"),
        "{folded:?}"
    );
    assert_eq!(changed, ["c"]);
    let desc = d.elements[0].json["description"].as_str().unwrap();
    assert_eq!(desc.matches("<table>").count(), 1, "{desc}");
    assert!(
        desc.contains("<p><strong>Seasons of the cloak</strong></p><table>"),
        "{desc}"
    );
    assert!(
        desc.contains("<td>Summer</td>") && desc.contains("<td>Winter</td>"),
        "{desc}"
    );
    assert_eq!(d.elements[1].json["description"], item_desc);
}

/// A text document (no heading marks): a closing section, then the legal notice and thanks as
/// plain title lines (1-based: 1, 5, 9).
const PLAIN_END: &str = "More to Follow\n\nThe author hopes to add the sky mirefolk and the salt \
mirefolk to a later edition of this book.\n\nLegal Information\n\nAll names are trademarks of \
their owners and are used here with their kind permission today.\n\nSpecial Thanks To\n\n\
Thanks to the frog choir for their patience with every single draft of this little book.\n";

#[test]
fn the_end_matter_of_a_document_without_heading_marks_is_no_part_of_the_last_entry() {
    // Arrange
    let mut more = planned(
        "m",
        "More to Follow",
        "Rules Glossary Entry",
        Role::Text,
        Some(1),
    );
    more.excerpt.as_mut().unwrap().end_line = 3;
    let mut d = draft(PLAIN_END, vec![more], &[]);
    let el = crate::import::anything::parts::text_element(&d.plan.elements[0], &d.plan, PLAIN_END);
    // Assert: the entry stops at the legal notice; the end matter is reported under its titles.
    let desc = el["description"].as_str().unwrap();
    assert!(
        desc.contains("salt mirefolk") && !desc.contains("trademarks"),
        "{desc}"
    );
    assert!(!desc.contains("frog choir"), "{desc}");
    d.elements[0].json = el;
    let sources: Vec<String> = uncovered_prose(&d).into_iter().map(|e| e.source).collect();
    assert_eq!(sources, ["Legal Information", "Special Thanks To"]);
}
