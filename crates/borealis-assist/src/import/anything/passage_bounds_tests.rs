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
