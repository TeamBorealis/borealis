use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};
use crate::import::anything::types::ElementStatus;

const TRAITS: &str = "## Lantern Kin\n\
\n\
Lantern kin walk the marsh roads.\n\
**Ember Sight.** You can see through smoke and fog out to 30\n\
feet as if it were clear air.\n\
**Warm Hands.** Once per long rest you can warm a creature you\n\
touch, ending the frozen condition on it.\n\
**Languages.** Common and Marsh Cant.\n";

fn kin() -> Vec<PlannedElement> {
    vec![
        planned("t0", "Lantern Kin", "Species", Role::Standalone, Some(1)),
        planned("t1", "Ember Sight", "Racial Trait", Role::Part, Some(4)),
        planned("t2", "Warm Hands", "Racial Trait", Role::Part, Some(6)),
    ]
}

#[test]
fn a_run_in_trait_passage_is_its_paragraph_without_the_label() {
    let d = draft(TRAITS, kin(), &[]);
    let p = own_passage(TRAITS, &d.plan.elements[2], &d.plan).unwrap();
    assert_eq!(
        p,
        [
            "Once per long rest you can warm a creature you",
            "touch, ending the frozen condition on it."
        ]
    );
    assert_eq!(
        html_of(&p),
        "<p>Once per long rest you can warm a creature you touch, ending the frozen condition on it.</p>"
    );
}

#[test]
fn a_headed_passage_stops_at_the_next_heading_of_its_level() {
    let src = "#### Steady Aim 1st\n\nYou ignore half cover. Your aim is sure-\nfooted.\n\n#### Grit 2nd\n\nYou have grit points.\n";
    let mut p = planned(
        "t1",
        "Steady Aim",
        "Class Feature",
        Role::ClassFeature,
        Some(1),
    );
    // The locator's passage runs on into the next feature.
    p.excerpt.as_mut().unwrap().end_line = 8;
    let d = draft(src, vec![p], &[]);
    let lines = own_passage(src, &d.plan.elements[0], &d.plan).unwrap();
    assert_eq!(
        html_of(&lines),
        "<p>You ignore half cover. Your aim is surefooted.</p>"
    );
}

#[test]
fn shared_and_table_row_passages_are_not_an_elements_own() {
    let src = "#### Lamps\n\nA lamp of three kinds.\n\nOil Lamp | 5 gp | 1 lb.\n";
    let d = draft(
        src,
        vec![
            planned("t1", "Brass Lamp", "Item", Role::Standalone, Some(1)),
            planned("t2", "Glass Lamp", "Item", Role::Standalone, Some(1)),
            planned("t3", "Oil Lamp", "Item", Role::Standalone, Some(5)),
        ],
        &[],
    );
    assert!(own_passage(src, &d.plan.elements[0], &d.plan).is_none());
    assert!(own_passage(src, &d.plan.elements[2], &d.plan).is_none());
}

#[test]
fn a_spell_passage_starts_after_its_stat_block() {
    let src = "### Cinder Lash\n\n2nd-level evocation\n\nCasting Time: 1 action\nRange: 30 feet\n\nA whip of embers strikes one creature you can see.\n";
    let d = draft(
        src,
        vec![planned(
            "t1",
            "Cinder Lash",
            "Spell",
            Role::Standalone,
            Some(1),
        )],
        &[],
    );
    let lines = own_passage(src, &d.plan.elements[0], &d.plan).unwrap();
    assert_eq!(
        lines,
        ["A whip of embers strikes one creature you can see."]
    );
}

#[test]
fn a_reworded_description_becomes_the_printed_passage() {
    let mut d = draft(
        TRAITS,
        kin(),
        &[
            // Verbatim already: untouched.
            "<p>Lantern kin walk the marsh roads.</p>",
            "<p>You can see through smoke and fog out to 30 feet as if it were clear air.</p>",
            "<p>Once per long rest, you can warm a creature you touch, which ends the frozen condition on it.</p><p><strong>Cost:</strong> 5 gp</p>",
        ],
    );
    let changed = restore(&mut d);
    assert_eq!(changed, vec!["t2".to_string()]);
    assert_eq!(
        d.elements[2].json["description"],
        "<p>Once per long rest you can warm a creature you touch, ending the frozen condition on it.</p><p><strong>Cost:</strong> 5 gp</p>"
    );
    assert_eq!(d.elements[2].normalised.len(), 1);
    assert_eq!(d.elements[2].status, ElementStatus::Ok);
}

#[test]
fn a_description_with_other_content_or_a_user_revision_is_kept() {
    let added = "<p>Once per long rest you can warm a creature you touch, ending the frozen condition. \
        Kin who overuse this gift grow cold and pale for days, and the marsh spirits notice them.</p>";
    let mut d = draft(TRAITS, kin(), &["", "", added]);
    assert!(restore(&mut d).is_empty());
    let mut d = draft(TRAITS, kin(), &["", "", "<p>Once per long rest, you can warm a creature you touch, which ends the frozen condition on it.</p>"]);
    d.elements[2].revisions = 1;
    assert!(restore(&mut d).is_empty());
    // A root with parts: its description carries their text past its own passage.
    let mut parts = kin();
    parts[1].parent = Some("t0".into());
    let mut d = draft(
        TRAITS,
        parts,
        &["<p>Lantern kin travel the marsh roads.</p>"],
    );
    assert!(restore(&mut d).is_empty());
}

const CLASS: &str = "## Lamplighter\n\
\n\
Lamplighters keep the roads lit.\n\
\n\
### Quick Build\n\
\n\
Put your highest score in Wisdom.\n\
\n\
### Hit Points\n\
\n\
Hit Dice: 1d8 per lamplighter level\n\
\n\
### Kindle 1st\n\
\n\
You light a lamp.\n\
\n\
#### Kindle Options\n\
\n\
Pick one flame.\n\
\n\
## Other Things\n\
\n\
### Multiclassing\n\
\n\
Prerequisite: Wisdom 13. You gain light armor proficiency.\n";

fn lamplighter() -> Vec<PlannedElement> {
    let mut root = planned("c0", "Lamplighter", "Class", Role::Class, Some(1));
    root.excerpt.as_mut().unwrap().end_line = 20;
    vec![
        root,
        planned(
            "c1",
            "Kindle",
            "Class Feature",
            Role::ClassFeature,
            Some(13),
        ),
    ]
}

#[test]
fn unplanned_prose_sub_sections_reach_the_class_description() {
    let d = draft(CLASS, lamplighter(), &[]);
    let subs = unplanned_sections(CLASS, &d.plan.elements[0], &d.plan);
    let titles: Vec<&str> = subs.iter().map(|s| s.title.as_str()).collect();
    // Hit points are the class's rules; Kindle and its own sub-heading are planned.
    assert_eq!(titles, ["Quick Build"]);
    let mut d = draft(
        CLASS,
        lamplighter(),
        &[
            "<p>Lamplighters keep the roads lit.</p>",
            "<p>You light a lamp.</p>",
        ],
    );
    assert_eq!(add_sub_sections(&mut d), vec!["c0".to_string()]);
    assert_eq!(
        d.elements[0].json["description"],
        "<p>Lamplighters keep the roads lit.</p><p><strong>Quick Build</strong></p><p>Put your highest score in Wisdom.</p>"
    );
    // Run again: the content is there now.
    assert!(add_sub_sections(&mut d).is_empty());
}

#[test]
fn flat_headings_before_the_first_feature_are_the_roots_sub_sections() {
    let src = "#### Lamplighter\n\nLamplighters keep the roads lit.\n\n#### Creating a Lamplighter\n\nThink of the road you walked.\n\n#### Kindle 1st\n\nYou light a lamp.\n\n#### Afterword\n\nThanks for reading this book.\n";
    let mut kindle = planned("c1", "Kindle", "Class Feature", Role::ClassFeature, Some(9));
    kindle.owner = Some("c0".into());
    let d = draft(
        src,
        vec![
            planned("c0", "Lamplighter", "Class", Role::Class, Some(1)),
            kindle,
        ],
        &[],
    );
    let titles: Vec<String> = unplanned_sections(src, &d.plan.elements[0], &d.plan)
        .into_iter()
        .map(|s| s.title)
        .collect();
    assert_eq!(titles, ["Creating a Lamplighter"]);
}

#[test]
fn a_class_root_gets_the_multiclassing_section_outside_its_passage() {
    let d = draft(CLASS, lamplighter(), &[]);
    let m = multiclass_section(CLASS, &d.plan.elements[0]).unwrap();
    assert!(m.starts_with("### Multiclassing"));
    assert!(m.contains("light armor proficiency"));
    assert!(multiclass_section(CLASS, &d.plan.elements[1]).is_none());
}

#[test]
fn a_table_row_keeps_only_printed_text() {
    let src = "#### Lamps\n\nOil Lamp | 5 gp | 1 lb. | Burns for six hours on a pint of oil.\n";
    let mut d = draft(
        src,
        vec![planned("t1", "Oil Lamp", "Item", Role::Standalone, Some(3))],
        &["<p>A simple clay lamp that lights a small room with a steady flame.</p><p>Burns for six hours on a pint of oil.</p><p><strong>Cost:</strong> 5 gp</p>"],
    );
    assert_eq!(drop_unprinted_row_text(&mut d), vec!["t1".to_string()]);
    assert_eq!(
        d.elements[0].json["description"],
        "<p>Burns for six hours on a pint of oil.</p><p><strong>Cost:</strong> 5 gp</p>"
    );
}

#[test]
fn prose_no_element_carries_is_a_report_entry() {
    let src = "# Lantern Book\n\nThis book collects the marsh folk and their old roads, \
with notes for running them at your table.\n\n## Lantern Kin\n\nLantern kin walk the marsh \
roads and keep the lamps burning through every night.\n";
    let d = draft(
        src,
        vec![planned("t0", "Lantern Kin", "Species", Role::Standalone, Some(5))],
        &["<p>Lantern kin walk the marsh roads and keep the lamps burning through every night.</p>"],
    );
    let entries = uncovered_prose(&d);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "Lantern Book");
    assert_eq!(entries[0].outcome, "skipped");
    assert!(entries[0]
        .note
        .contains("\"This book collects the marsh folk"));
}

/// `BOREALIS_FIDELITY_REPLAY=<draft.json> [BOREALIS_FIDELITY_OUT=<draft out>] cargo test -p
/// borealis-assist --lib -- --ignored replay_fidelity_on_a_stored_draft --nocapture`: applies the
/// deterministic description steps to a stored draft and prints the fidelity numbers before and
/// after, the elements the repair would get and the uncovered-prose entries.
#[test]
#[ignore = "needs BOREALIS_FIDELITY_REPLAY (a stored draft.json)"]
fn replay_fidelity_on_a_stored_draft() {
    use crate::import::anything::fidelity::{name_mask, sentences, sweep, untraced, Index};
    let Ok(path) = std::env::var("BOREALIS_FIDELITY_REPLAY") else {
        return;
    };
    let text = std::fs::read_to_string(&path).expect("draft reads");
    let mut d: ImportDraft = serde_json::from_str(&text).expect("draft parses");
    let stats = |d: &ImportDraft| {
        let (index, mask) = (Index::new(&d.source_text), name_mask(d));
        let checked = d
            .elements
            .iter()
            .filter(|e| !e.json.is_null() && !matches!(e.role, Role::Text | Role::SubclassGate));
        let total: usize = checked
            .clone()
            .map(|e| {
                sentences(
                    e.json
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .iter()
                .filter(|s| words(s).len() >= 6)
                .count()
            })
            .sum();
        let flagged: usize = checked
            .map(|e| untraced(&e.json, &index, &mask).len())
            .sum();
        (total, flagged)
    };
    let before = stats(&d);
    let plan = d.plan.clone();
    let mut typed = 0;
    for e in d
        .elements
        .iter_mut()
        .filter(|e| e.ty == "Magic Item" && !e.json.is_null())
    {
        let had = e.json.clone();
        crate::import::anything::items::magic_item_type(&mut e.json, &e.name, &plan);
        typed += usize::from(had != e.json);
    }
    println!("magic items given a type setter: {typed}");
    let restored = restore(&mut d);
    let added = add_sub_sections(&mut d);
    println!(
        "table rows with unprinted text removed: {}",
        drop_unprinted_row_text(&mut d).len()
    );
    let after = stats(&d);
    let mut by_role: std::collections::BTreeMap<String, usize> = Default::default();
    for id in &restored {
        let role = d
            .element(id)
            .map(|e| format!("{:?}", e.role))
            .unwrap_or_default();
        *by_role.entry(role).or_default() += 1;
    }
    println!("sentences >=6 words / untraced: before {before:?}, after {after:?}");
    println!(
        "restored to the printed passage: {} {by_role:?}",
        restored.len()
    );
    println!("sub-sections added to roots: {}", added.len());
    println!("fidelity repairs (capped): {}", sweep(&d).len());
    let mut noted = d.clone();
    crate::import::anything::fidelity::note_leftovers(&mut noted);
    let flagged = noted
        .elements
        .iter()
        .filter(|e| {
            e.issues
                .iter()
                .any(|i| i.starts_with("description not traced"))
        })
        .count();
    println!("elements with an untraced sentence (review note if the repair leaves it): {flagged}");
    let entries = uncovered_prose(&d);
    println!("uncovered-prose entries: {}", entries.len());
    for e in entries.iter().take(8) {
        println!(
            "  {}: {}",
            e.source,
            e.note.chars().take(140).collect::<String>()
        );
    }
    if let Ok(out) = std::env::var("BOREALIS_FIDELITY_OUT") {
        std::fs::write(&out, serde_json::to_string(&d).expect("draft serialises")).expect("writes");
        println!("written to {out}");
    }
}

#[test]
fn a_rules_section_printed_flat_reaches_over_its_unplanned_headings() {
    let src = "#### Lamp FAQ\n\nAnswers.\n\n#### Can a lamp go out?\n\nOnly in rain.\n\n\
#### Is oil required?\n\nYes, always.\n\n#### Appendix: Numbers\n\nTables of numbers.\n\n\
#### Wick Rules\n\nTrim it.\n";
    let mut faq = planned("t1", "Lamp FAQ", "Rules", Role::Text, Some(1));
    faq.ty = "Rules Glossary Entry".into();
    let d = draft(src, vec![faq.clone()], &[]);
    let lines = super::super::parts::section_lines(src, &d.plan.elements[0], &d.plan);
    assert!(
        lines.iter().any(|l| l.contains("Yes, always.")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Tables")),
        "an appendix is not part of it"
    );
    // A planned heading ends it.
    let d = draft(
        src,
        vec![
            faq,
            planned("t2", "Is oil required", "Rules", Role::Text, Some(9)),
        ],
        &[],
    );
    let lines = super::super::parts::section_lines(src, &d.plan.elements[0], &d.plan);
    assert!(lines.iter().any(|l| l.contains("Only in rain.")));
    assert!(!lines.iter().any(|l| l.contains("Yes, always.")));
    // A generated feature's passage does not reach past its own heading's section.
    let feat = planned(
        "f1",
        "Lamp FAQ",
        "Class Feature",
        Role::ClassFeature,
        Some(1),
    );
    let d = draft(src, vec![feat], &[]);
    let lines = super::super::parts::section_lines(src, &d.plan.elements[0], &d.plan);
    assert!(!lines.iter().any(|l| l.contains("Only in rain.")));
}

#[test]
fn a_planned_lore_section_in_a_flat_class_intro_is_its_own_and_the_intro_goes_on() {
    let src = "#### Lamplighter\n\nLamplighters keep the roads lit.\n\n#### The Lamplighter\n\nA life of lamps and long nights on the road.\n\n#### Quick Build\n\nPut your highest score in Wisdom.\n\n#### Kindle 1st\n\nYou light a lamp.\n";
    let mut kindle = planned(
        "c1",
        "Kindle",
        "Class Feature",
        Role::ClassFeature,
        Some(13),
    );
    kindle.owner = Some("c0".into());
    let d = draft(
        src,
        vec![
            planned("c0", "Lamplighter", "Class", Role::Class, Some(1)),
            planned(
                "x1",
                "The Lamplighter",
                "Rules Glossary Entry",
                Role::Text,
                Some(5),
            ),
            kindle,
        ],
        &[],
    );
    let titles: Vec<String> = unplanned_sections(src, &d.plan.elements[0], &d.plan)
        .into_iter()
        .map(|s| s.title)
        .collect();
    // The lore entry carries its own text (printed once); the intro goes on past it.
    assert_eq!(titles, ["Quick Build"]);
    // The lore entry's text is its own section, the class's intro is not in it.
    let lore = super::super::parts::section_lines(src, &d.plan.elements[1], &d.plan);
    assert!(lore.iter().any(|l| l.contains("long nights")), "{lore:?}");
    assert!(!lore.iter().any(|l| l.contains("Wisdom")), "{lore:?}");
}

#[test]
fn a_table_row_gets_the_run_in_rule_printed_after_its_table() {
    let src = "#### Lamp Oils\n\nOil | Cost\nGlow Oils (5) | 2 gp\nSmoke Oil (5) | 1 gp\n\n\
#### Special Oils\n\nGlow Oil. A lamp filled with it sheds bright light\nfor twice as far.\n\n\
Other text. Not a row.\n";
    let mut d = draft(
        src,
        vec![
            planned("r1", "Glow Oils (5)", "Item", Role::Standalone, Some(4)),
            planned("r2", "Smoke Oil (5)", "Item", Role::Standalone, Some(5)),
        ],
        &[
            "<p><strong>Cost:</strong> 2 gp</p>",
            "<p><strong>Cost:</strong> 1 gp</p>",
        ],
    );
    assert_eq!(add_sub_sections(&mut d), vec!["r1".to_string()]);
    assert_eq!(
        d.elements[0].json["description"],
        "<p>A lamp filled with it sheds bright light for twice as far.</p><p><strong>Cost:</strong> 2 gp</p>"
    );
    assert!(add_sub_sections(&mut d).is_empty(), "already there");
}

#[test]
fn uncovered_prose_counts_a_generated_element_by_its_located_passage() {
    let src = "#### Kindle\n\nYou light a lamp.\n\nThe lamplighters of the old roads kept every lamp lit through the longest winter nights.\n";
    let mut kindle = planned("c1", "Kindle", "Class Feature", Role::ClassFeature, Some(1));
    kindle.excerpt.as_mut().unwrap().end_line = 3;
    let d = draft(src, vec![kindle], &["<p>You light a lamp.</p>"]);
    let entries = uncovered_prose(&d);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].source, "Kindle");
}

#[test]
fn a_text_section_leaves_its_planned_children_out_and_keeps_the_rest() {
    let src = "## Lamp Lore\n\nLamps are old.\n\n### Tin Lamps\n\nTin lamps rust in the rain.\n\n\
### Oil\n\nOil burns slow and clean.\n\n### Glass Lamps\n\nGlass lamps shine far.\n\n\
Lamp | Cost\nTin lamp | 2 gp\n\n## Roads\n\nRoads are long.\n";
    let mut lore = planned(
        "t0",
        "Lamp Lore",
        "Rules Glossary Entry",
        Role::Text,
        Some(1),
    );
    // The locator's passage is the whole section, children included.
    lore.excerpt.as_mut().unwrap().end_line = 19;
    // A generated child (a text-only one would reach over the flat "Oil" heading itself).
    let tin = planned("t1", "Tin Lamps", "Item", Role::Standalone, Some(5));
    let glass = planned(
        "t2",
        "Glass Lamps",
        "Rules Glossary Entry",
        Role::Text,
        Some(13),
    );
    let row = planned("i1", "Tin lamp", "Item", Role::Standalone, Some(18));
    let d = draft(src, vec![lore, tin, glass, row], &[]);
    let lines = super::super::parts::section_lines(src, &d.plan.elements[0], &d.plan);
    let text = lines.join("\n");
    assert!(text.contains("Lamps are old."), "{text}");
    assert!(
        text.contains("Oil burns slow"),
        "an unplanned sub-section stays: {text}"
    );
    assert!(
        !text.contains("rust in the rain"),
        "a planned child is its own: {text}"
    );
    assert!(!text.contains("shine far"), "{text}");
    assert!(!text.contains("Roads"), "{text}");
    // The child keeps its table, a row element's row included.
    let lines = super::super::parts::section_lines(src, &d.plan.elements[2], &d.plan);
    assert!(lines.contains(&"Tin lamp | 2 gp"), "{lines:?}");
}

#[test]
fn uncovered_prose_inside_a_generated_parent_after_its_child_is_reported() {
    let src =
        "## Lamplighter\n\nLamplighters keep the roads lit through the long and dark winter.\n\n\
### Kindle\n\nYou light a lamp.\n\n## Other Notes\n\n### Printing\n\n\
This little book was printed on reed paper by the lamplighters of the old marsh road.\n";
    let mut root = planned("c0", "Lamplighter", "Class", Role::Class, Some(1));
    root.excerpt.as_mut().unwrap().end_line = 15;
    let kindle = planned("c1", "Kindle", "Class Feature", Role::ClassFeature, Some(5));
    let d = draft(
        src,
        vec![root, kindle],
        &[
            "<p>Lamplighters keep the roads lit through the long and dark winter.</p>",
            "<p>You light a lamp.</p>",
        ],
    );
    let entries = uncovered_prose(&d);
    assert_eq!(entries.len(), 1, "{entries:?}");
    // Reported under the widest section no element is planned in.
    assert_eq!(entries[0].source, "Other Notes");
}

#[test]
fn a_run_in_printed_under_a_paragraph_is_reported_under_its_label() {
    let src = "## Gear\n\nThe gear of a lamplighter is listed here for the table.\n\
Starting Coin. A lamplighter starts with four gold pieces and a tin of lamp oil.\n";
    let d = draft(
        src,
        vec![planned(
            "g0",
            "Gear",
            "Rules Glossary Entry",
            Role::Standalone,
            Some(1),
        )],
        &["<p>The gear of a lamplighter is listed here for the table.</p>"],
    );
    let entries = uncovered_prose(&d);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].source, "Starting Coin");
}

#[test]
fn a_feature_gets_the_printed_text_of_its_parts() {
    let src = "#### Lamp Magic\n\nYou cast spells by lamplight.\n\n#### Wicks\n\n\
You have wick points equal to your level.\n\n#### Casting Focus\n\nA lit lamp is your focus.\n";
    let feat = planned(
        "f0",
        "Lamp Magic",
        "Class Feature",
        Role::ClassFeature,
        Some(1),
    );
    let mut wicks = planned("p1", "Wicks", "Class Feature", Role::Part, Some(5));
    wicks.parent = Some("f0".into());
    let mut focus = planned("p2", "Casting Focus", "Class Feature", Role::Part, Some(9));
    focus.parent = Some("f0".into());
    let mut d = draft(
        src,
        vec![feat, wicks, focus],
        &[
            "<p>You cast spells by lamplight.</p>",
            "<p>You have wick points equal to your level.</p>",
            "<p>A lit lamp is your focus.</p>",
        ],
    );
    assert_eq!(add_sub_sections(&mut d), vec!["f0".to_string()]);
    assert_eq!(
        d.elements[0].json["description"],
        "<p>You cast spells by lamplight.</p><p><strong>Wicks</strong></p><p>You have wick points equal to your level.</p><p><strong>Casting Focus</strong></p><p>A lit lamp is your focus.</p>"
    );
    assert!(add_sub_sections(&mut d).is_empty(), "already there");
    // A species keeps its traits' text by its brief; nothing is appended.
    let mut d = draft(TRAITS, kin(), &["<p>Lantern kin walk the marsh roads.</p>"]);
    d.plan.elements[1].parent = Some("t0".into());
    assert!(add_sub_sections(&mut d).is_empty());
}

#[test]
fn a_plural_or_merged_run_in_reaches_every_row_it_names() {
    let src = "#### Lamps\n\nLamp | Cost\nLantern, hooded | 5 gp\nLantern, bullseye | 9 gp\n\
Tin Cup | 1 sp\nReed Mat | 2 sp\n\n#### Rules\n\nLamps are described here.\n\
Lanterns. A lantern can be shuttered as a bonus action.\n\n\
Tin Cups and Reed Mats. These never break when dropped.\n";
    let mut d = draft(
        src,
        vec![
            planned("r1", "Lantern, hooded", "Item", Role::Standalone, Some(4)),
            planned("r2", "Lantern, bullseye", "Item", Role::Standalone, Some(5)),
            planned("r3", "Tin Cup", "Item", Role::Standalone, Some(6)),
            planned("r4", "Reed Mat", "Item", Role::Standalone, Some(7)),
        ],
        &["<p>x</p>", "<p>y</p>", "<p>z</p>", "<p>w</p>"],
    );
    assert_eq!(add_sub_sections(&mut d), ["r1", "r2", "r3", "r4"]);
    assert!(d.elements[1].json["description"]
        .as_str()
        .unwrap()
        .starts_with("<p>A lantern can be shuttered"));
    assert!(d.elements[3].json["description"]
        .as_str()
        .unwrap()
        .starts_with("<p>These never break"));
}

/// Offline: `BOREALIS_PRINTED_REPLAY=<draft.json> BOREALIS_PRINTED_OUT=<dir>` re-applies the
/// plan's structure (`parts::apply`) and the printed-passage fixes (`keep_printed`) to a stored
/// draft, as generation would, then finalizes it into `<dir>`.
#[test]
#[ignore = "needs a local draft.json"]
fn reprint_a_stored_draft() {
    let Ok(path) = std::env::var("BOREALIS_PRINTED_REPLAY") else {
        return;
    };
    let mut d: ImportDraft =
        serde_json::from_str(&std::fs::read_to_string(path).expect("reads")).expect("parses");
    // BOREALIS_PRINTED_FOLD: drop the text-only entries a rebuilt plan folds into a table or
    // leaves to an outlined feature.
    if std::env::var("BOREALIS_PRINTED_FOLD").is_ok() {
        let folded =
            super::super::plan::sections::fold_table_headings(&d.plan.elements, &d.source_text);
        let ids: Vec<&String> = folded.iter().map(|(id, _)| id).collect();
        for (_, n) in &folded {
            println!("folded: {n}");
        }
        d.plan.elements.retain(|e| !ids.contains(&&e.id));
        d.plan
            .groups
            .retain(|g| !g.root_id.as_ref().is_some_and(|r| ids.contains(&r)));
        d.elements.retain(|e| !ids.contains(&&e.id));
        // And the text-only groups an outline already plans as features.
        let mut groups = d.plan.groups.clone();
        for n in super::super::plan::sections::drop_outlined_texts(&mut groups) {
            println!("dropped: {n}");
        }
        let gone: Vec<String> = d
            .plan
            .groups
            .iter()
            .filter(|g| !groups.iter().any(|k| k.name == g.name))
            .filter_map(|g| g.root_id.clone())
            .collect();
        d.plan.groups = groups;
        d.plan.elements.retain(|e| !gone.contains(&e.id));
        d.elements.retain(|e| !gone.contains(&e.id));
    }
    // BOREALIS_PRINTED_TEXT: text-only descriptions rebuilt from their sections.
    if std::env::var("BOREALIS_PRINTED_TEXT").is_ok() {
        let plan = d.plan.clone();
        for e in d.elements.iter_mut().filter(|e| e.role == Role::Text) {
            if let Some(p) = plan
                .elements
                .iter()
                .find(|p| p.id == e.id && p.excerpt.is_some())
            {
                let el = super::super::parts::text_element(p, &plan, &d.source_text);
                e.json["description"] = el["description"].clone();
            }
        }
    }
    let before: Vec<Value> = d.elements.iter().map(|e| e.json.clone()).collect();
    let plan = d.plan.clone();
    for e in d.elements.iter_mut().filter(|e| !e.json.is_null()) {
        if let Some(p) = plan.elements.iter().find(|p| p.id == e.id) {
            super::super::parts::apply(&mut e.json, p, &plan);
        }
    }
    super::super::generate::keep_printed(&mut d);
    for (e, b) in d.elements.iter().zip(&before) {
        if &e.json != b {
            println!("changed: {}", e.id);
        }
    }
    for r in uncovered_prose(&d) {
        let note: String = r.note.chars().take(160).collect();
        println!("uncovered: {}: {note}", r.source);
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = borealis_content::ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let done = super::super::finalize::finalize_draft(&b, &d).expect("finalizes");
    if let Ok(out) = std::env::var("BOREALIS_PRINTED_OUT") {
        done.pack
            .write_to_dir(std::path::Path::new(&out))
            .expect("pack writes");
    }
}

#[test]
fn a_short_sentence_alone_under_its_heading_is_reported_and_page_furniture_is_not() {
    let mut src = String::from(
        "## Lamp Lists\n\nThese lists hold every lamp spell.\n\n## Wicks\n\nTrim.\n\n",
    );
    for page in 1..=6 {
        src.push_str(&format!(
            "## Page {page}\n\nNot to be sold. Printed for your own table only, all rights kept by the maker.\n\nLAMP BOOK | {page}!\n\n"
        ));
        src.push_str(&"Filler line of the page body.\n".repeat(10));
        src.push('\n');
    }
    let d = draft(&src, vec![], &[]);
    let entries = uncovered_prose(&d);
    let sources: Vec<&str> = entries.iter().map(|e| e.source.as_str()).collect();
    assert!(sources.contains(&"Lamp Lists"), "{sources:?}");
    assert!(
        !sources.contains(&"Wicks"),
        "one word is no prose: {sources:?}"
    );
    assert!(
        !entries.iter().any(|e| e.note.contains("Not to be sold")),
        "a footer is no prose: {entries:?}"
    );
}
