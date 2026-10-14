//! Tests of the section locator (synthetic sources).

use super::*;

fn section<'a>(src: &'a str, name: &str, quote: &str) -> Vec<&'a str> {
    let ex = locate_section(src, quote, name).expect("located");
    src.lines()
        .skip(ex.start_line - 1)
        .take(ex.end_line - ex.start_line + 1)
        .collect()
}

const BOOK: &str = "# Wanderers of the Mist\n\nEmberkin – Tidewalker – Glasswing\n\n## Contents\n\nEmberkin ........ 4\nTidewalker ........ 6\nGlasswing ........ 9\nIndex ........ 12\n\n## Emberkin\n\nEmberkin carry a coal in their chest.\n\n### Emberkin Traits\n\nDarkvision. You see in dim light within 60 feet.\nEmber Heart. You resist fire damage.\n\nThe coal never cools.\n\n## Tidewalker\n\nTidewalkers breathe water.\n\nDarkvision. Tidewalkers see 30 feet in murk.\n\n## Index\n\nEmberkin 4\nGlasswing 9\nTidewalker 6\n";

#[test]
fn skips_cover_and_contents_and_keeps_subsections() {
    let s = section(BOOK, "Emberkin", "");
    assert_eq!(s[0], "## Emberkin");
    assert!(s.contains(&"### Emberkin Traits"));
    assert!(s.contains(&"The coal never cools."));
    assert!(!s.iter().any(|l| l.contains("Tidewalker")), "{s:?}");
    // Glasswing has no section of its own: only the cover, the contents and the index name it.
    assert!(locate_section(BOOK, "", "Glasswing").is_none());
}

#[test]
fn run_in_traits_stop_at_the_next_trait_and_quotes_pick_the_right_one() {
    let s = section(BOOK, "Ember Heart", "");
    assert_eq!(
        s.first().copied(),
        Some("Ember Heart. You resist fire damage.")
    );
    assert!(s.contains(&"The coal never cools."));
    let s = section(BOOK, "Darkvision", "see 30 feet in murk");
    assert_eq!(s, vec!["Darkvision. Tidewalkers see 30 feet in murk."]);
    let s = section(BOOK, "Darkvision", "");
    assert!(s[0].contains("dim light"), "{s:?}");
}

#[test]
fn unmarked_sections_keep_their_own_parts() {
    let src = "Glimmerkin\n\nSmall folk who glow at dusk and hum.\n\nGlimmerkin Traits\n\n\
Ability Score Increase. Your Charisma rises by 2.\nSize. You are Small.\n\nDuskling\n\nA shy folk.\n";
    let s = section(src, "Glimmerkin", "");
    assert!(s.contains(&"Size. You are Small."), "{s:?}");
    assert!(!s.contains(&"Duskling"), "{s:?}");
    let s = section(src, "Ability Score Increase", "");
    assert_eq!(s, vec!["Ability Score Increase. Your Charisma rises by 2."]);
}

#[test]
fn a_run_in_line_without_its_stop_does_not_end_the_section() {
    // Unmarked text (headings guessed): a run-in base line lacking its final full stop
    // looks like a title line; it is a paragraph of the race under its traits heading.
    let src = "Prologue\n\nGLIMMERKIN\n\nSmall folk who glow at dusk.\n\nGlimmerkin Traits\n\n\
Your glimmerkin character has these traits.\n\nAbility Score Increase. Your Charisma score increases by 2\n\n\
Size. You are Small.\n\nLanguages. You can speak Common and Mossic.\n\nDUSKLING\n\nA shy folk.\n";
    let s = section(src, "Glimmerkin", "");
    assert_eq!(s[0], "GLIMMERKIN", "{s:?}");
    assert!(s.contains(&"Size. You are Small."), "{s:?}");
    assert!(s.iter().any(|l| l.starts_with("Languages.")), "{s:?}");
    assert!(!s.contains(&"DUSKLING"), "{s:?}");
    let s = section(src, "Ability Score Increase", "");
    assert_eq!(
        s,
        vec!["Ability Score Increase. Your Charisma score increases by 2"]
    );
    // A title with a colon stays a title.
    assert!(!run_in_paragraph("Tier 1: The Long Road"));
    assert!(run_in_paragraph("Size. You are Small"));
}

#[test]
fn numbered_sub_headings_and_tables_stay_in_their_section() {
    let src = "# Workshop\n\n## Glass Lanterns\n\nLanterns are blown in stages.\n\n## Tier 1\n\n\
Cost | Time\n5 gp | 1 day\n\n## Lantern Table\n\nd6 | Colour\n1 | Amber\n\n## Rope Bridges\n\nBridges sway.\n\n\
## Price List\n\nd4 | Item | Effect\n1 | Moth Jar | Glows faintly for an hour.\n2 | Dew Bottle | Never runs dry.\n";
    let s = section(src, "Glass Lanterns", "");
    assert!(s.contains(&"5 gp | 1 day"), "{s:?}");
    assert!(s.contains(&"1 | Amber"), "{s:?}");
    assert!(!s.iter().any(|l| l.contains("Bridges")), "{s:?}");
    let s = section(src, "Moth Jar", "");
    assert_eq!(s, vec!["1 | Moth Jar | Glows faintly for an hour."]);
}

#[test]
fn an_entry_body_beats_a_name_in_a_list_and_an_exact_heading_beats_both() {
    let src = "# Spell Lists\n\n## Bard\n\nGlimmer Ward\nMoth Swarm\nDew Lance\nReed Song\n\n\
## Spells\n\n### ~ Margin Note Moth Swarm\n\n2nd-level conjuration\n\nCasting Time: 1 action\n\
Range: 60 feet\n\nMoths fill a cube.\n\n### Dew Lance\n\nEvocation cantrip\n\nA lance of dew.\n";
    let s = section(src, "Moth Swarm", "");
    assert_eq!(s[0], "### ~ Margin Note Moth Swarm", "{s:?}");
    assert!(s.contains(&"Moths fill a cube."), "{s:?}");
    let src = "# Lamplighter\n\nKeepers of lamps.\n\n## Wick Paths\n\n#### Moth Lamplighter\n\n\
Prerequisite: 5th level\n\nYou befriend moths.\n";
    assert_eq!(section(src, "Lamplighter", "")[0], "# Lamplighter");
}

#[test]
fn a_heading_with_glued_junk_beats_a_list_line_split_by_a_page_number() {
    // A sidebar signature recurs on its own line and is glued to one spell heading; the
    // class list puts the spell's name alone above a page number.
    let src = "# Spells\n\n### ~ Quillmark\n\nA margin note.\n\n### ~ Quillmark Moth Swarm\n\n\
2nd-level conjuration\n\nCasting Time: 1 action\n\nMoths fill a cube.\n\n### Dew Lance\n\n\
Evocation cantrip\n\nA lance of dew.\n\n~ Quillmark\n\n# Lists\n\n### 2nd Level\n\nGlimmer Ward\n\
Reed Song\nMoth Swarm\n\n55\n\nTide Call\n\n### 3rd Level\n\nDew Lance\n";
    let s = section(src, "Moth Swarm", "");
    assert_eq!(s[0], "### ~ Quillmark Moth Swarm", "{s:?}");
    assert!(s.contains(&"Moths fill a cube."), "{s:?}");
    // Without the margin mark the recurring prefix is still junk.
    let src2 = src.replace("### ~ Quillmark Moth Swarm", "### Quillmark Moth Swarm");
    assert_eq!(
        section(&src2, "Moth Swarm", "")[0],
        "### Quillmark Moth Swarm"
    );
    // A heading that only ends with the name of something else stays a mention-heading.
    let doc_src = "# A\n\n### Stone and Hide\n\nHides tan in the sun.\n\n\
#### Medium Armor\n\nName | Cost | AC\n\nHide | 10 gp | 12\n\nScale | 50 gp | 14\n";
    assert_eq!(section(doc_src, "Hide", ""), vec!["Hide | 10 gp | 12"]);
}

#[test]
fn equipment_prefers_its_stat_row_over_a_same_named_prose_heading() {
    let src = "# Gear\n\n#### Light Armor\n\nName | Cost | AC\n\nQuilted | 5 gp | 11\n\n\
Wicker | 10 gp | 11\n\n# Tailoring\n\n#### Quilted\n\nSlots: 2\n\nYou resist cold.\n\n\
#### Lined\n\nSlots: 1\n\nYou stay dry.\n";
    let at = |ty: &str| {
        let ex = locate_section_as(src, "", "Quilted", ty).expect("located");
        src.lines().nth(ex.start_line - 1).unwrap()
    };
    assert_eq!(at("Armor"), "Quilted | 5 gp | 11");
    // Untyped, or another type, keeps the same-named heading.
    assert_eq!(at(""), "#### Quilted");
    assert_eq!(at("Rules Glossary Entry"), "#### Quilted");
    // An equipment heading that opens an entry (type and rarity line) keeps it.
    let src =
        "# Gear\n\n## Armor\n\nName | Cost | AC\nGlass Mail | 90 gp | 15\nWicker | 10 gp | 11\n\n\
## Rare Armor\n\n#### Glass Mail\n\nArmor (medium), rare\n\nIt shimmers.\n";
    let ex = locate_section_as(src, "", "Glass Mail", "Armor").unwrap();
    assert_eq!(src.lines().nth(ex.start_line - 1), Some("#### Glass Mail"));
}

#[test]
fn a_captioned_table_stays_with_its_entry_and_kinds_take_the_entry() {
    let src = "# Items\n\n#### Common Things\n\nMoth lamp (tin)\nMoth lamp (glass)\nReed flute\nRope ladder\nTin cup\n\n#### Moth Lamp\n\n\
Wondrous item, rarity varies\n\nA lamp that draws moths.\n\n#### Kinds of moth lamps\n\n\
d6 | Kind | Rarity\n1-4 | Tin | Common\n5-6 | Glass | Rare\n\nThe moths obey the lamp.\n\n\
#### Reed Flute\n\nWondrous item, common\n\nIt hums.\n";
    let s = section(src, "Moth Lamp", "");
    assert!(s.contains(&"5-6 | Glass | Rare"), "{s:?}");
    assert!(s.contains(&"The moths obey the lamp."), "{s:?}");
    assert!(!s.iter().any(|l| l.contains("Reed Flute")), "{s:?}");
    // The kind is only in the index list above: it takes the lamp's section.
    let s = section(src, "Moth Lamp, Glass", "");
    assert_eq!(s[0], "#### Moth Lamp", "{s:?}");
    // A kind the entry does not name keeps its own match.
    assert!(locate_section(src, "", "Moth Lamp, Brass").is_none());
}

#[test]
fn a_table_section_does_not_run_into_a_sibling_table_sharing_a_word() {
    let src = "# Costs\n\n#### Light Lanterns\n\nName | Cost\nTin lantern | 2 gp\nReed lantern | 1 gp\n\n\
#### Heavy Lanterns\n\nName | Cost\nIron lantern | 9 gp\n\n#### Wicks\n\nWicks burn.\n";
    let s = section(src, "Light Lanterns", "");
    assert!(s.contains(&"Reed lantern | 1 gp"), "{s:?}");
    assert!(!s.iter().any(|l| l.contains("Iron lantern")), "{s:?}");
}

#[test]
fn index_lines_are_recognised() {
    let lines = [
        "Alpha 3",
        "Beta 7",
        "Gamma 12",
        "",
        "Level 3",
        "Deal 2d6 damage",
        "Body.",
    ];
    let m = index_mask(&lines);
    assert_eq!(m, vec![true, true, true, false, false, false, false]);
    assert!(is_list_line("Alpha – Beta – Gamma"));
    assert!(!is_list_line("Alpha, the first of them, wakes."));
    // Table rows ending in a number (a DC, a weight) are data, not page references.
    let rows = [
        "Moth Club | 1 sp | 10",
        "Reed Pike | 5 gp | 25",
        "Dew Axe | 2 gp | 25",
    ];
    assert_eq!(index_mask(&rows), vec![false; 3]);
    let src = "Weapons\n\nName | Cost | DC\nMoth Club | 1 sp | 10\nReed Pike | 5 gp | 25\nDew Axe | 2 gp | 25";
    let ex = locate_section(src, "", "Reed Pike").unwrap();
    assert_eq!((ex.start_line, ex.end_line), (5, 5));
}

const GUIDE: &str = "# Lantern Guide\n\n## Building Lanterns\n\nBuilding takes a week.\n\n### Mishaps\n\nA lantern mishap cracks the glass and costs a day.\n\n## Charming Lanterns\n\nCharms take a month.\n\n### Mishaps\n\nA charm mishap dims the flame and drains a spark.\n\n## Charms\n\n### Glass Charms\n\n### Tier 1\n\n#### Burn Bright for an Hour\n\nCost: 10\n\n#### Shed Light in a Cone of Sixty\n\nfeet and Glow Like a Lamp\n\n#### Until Dawn\n\nCost: 20\n\nKeep the Wick Dry in the Rain\nand the Wind\nCost: 5\n\n### Tier 2\n\n#### Burn Bright for an Hour\n\nCost: 40\n\nCharms\nGlow Twice as Far\nCost: 60\nPrerequisites: Ability to light a lamp of the second tier or higher\nwithout help.\n\n\n### Steel Charms\n\nCharms\n\nSteel charms are rare.\n";

/// "<Name> (<Parent>)": the plan's qualifier picks the entry printed under that heading.
#[test]
fn a_parent_qualified_name_is_found_under_its_parent_heading() {
    let s = section(GUIDE, "Mishaps (Charming Lanterns)", "");
    assert_eq!(s[0], "### Mishaps");
    assert!(s.iter().any(|l| l.contains("dims the flame")), "{s:?}");
    let s = section(GUIDE, "Mishaps (Building Lanterns)", "");
    assert!(s.iter().any(|l| l.contains("cracks the glass")), "{s:?}");
    let s = section(GUIDE, "Burn Bright for an Hour (Tier 2)", "");
    assert!(s.contains(&"Cost: 40"), "{s:?}");
    assert!(qualified("Plain Name").is_none());
}

/// A name wrapped over two or three lines (headings or plain lines) is found.
#[test]
fn a_name_wrapped_over_lines_is_found() {
    let s = section(
        GUIDE,
        "Shed Light in a Cone of Sixty Feet and Glow Like a Lamp Until Dawn",
        "",
    );
    assert_eq!(s[0], "#### Shed Light in a Cone of Sixty");
    assert!(s.contains(&"Cost: 20"), "{s:?}");
    assert!(!s.contains(&"Cost: 10"), "{s:?}");
    let s = section(GUIDE, "Keep the Wick Dry in the Rain and the Wind", "");
    assert_eq!(s[0], "Keep the Wick Dry in the Rain");
    assert!(s.contains(&"Cost: 5"), "{s:?}");
}

/// A bare name line printed again elsewhere (a running page header) is no definition: the
/// exact heading wins even when the header line is followed by text.
#[test]
fn a_recurring_bare_line_loses_to_the_exact_heading() {
    let s = section(GUIDE, "Charms", "");
    assert_eq!(s[0], "## Charms");
}

/// An FAQ question heading naming the element is no definition of it.
#[test]
fn a_question_heading_is_no_definition() {
    let src = "# Book\n\n## Questions\n\n### Does Glimmer stack with itself?\n\nNo, it never stacks with another use of itself.\n\n## Features\n\nGlimmer\nYou shine dimly for one minute and light the room.\n";
    let s = section(src, "Glimmer", "");
    assert_eq!(s[0], "Glimmer");
}
