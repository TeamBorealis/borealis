//! Tests of the content, list, stat block and split-table checks and of the text fragments of
//! the heading check (synthetic sources only).

use serde_json::{json, Value};

use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};
use crate::import::anything::types::{Edition, OtherItem, SourceExcerpt};

fn located(mut p: PlannedElement, start: usize, end: usize) -> PlannedElement {
    p.excerpt = Some(SourceExcerpt {
        start_line: start,
        end_line: end,
        snippet: String::new(),
    });
    p
}

fn empty() -> ContentStoreBuilder {
    ContentStoreBuilder::new()
}

fn checked(mut d: ImportDraft) -> ImportDraft {
    apply(&empty(), &mut d);
    d
}

fn failed_of<'a>(d: &'a ImportDraft, id: &str, code: &str) -> Vec<&'a str> {
    d.element(id)
        .expect("element")
        .checks
        .iter()
        .filter(|c| c.failed && c.code == code)
        .map(|c| c.message.as_str())
        .collect()
}

fn passed_of(d: &ImportDraft, id: &str, code: &str) -> bool {
    d.element(id)
        .expect("element")
        .checks
        .iter()
        .any(|c| !c.failed && c.code == code)
}

fn set(d: &mut ImportDraft, id: &str, key: &str, v: Value) {
    let e = d.elements.iter_mut().find(|e| e.id == id).expect("element");
    e.json[key] = v;
}

// ---------------------------------------------------------------------------------------------
// stat_block_absent

const BEAST: &str = "## Reed Stalker\n\nA tall wading hunter of the marsh.\n\n\
Armor Class 13\nHit Points 22 (4d8 + 4)\nSpeed 30 ft.\nSTR DEX CON INT WIS CHA\n14 (+2) 16 (+3) 12 (+1) 3 (-4) 12 (+1) 6 (-2)\n";

fn beast(ac: u64, hp: u64, str_: u64) -> ImportDraft {
    let p = located(
        planned(
            "ID_T_MONSTER_REED",
            "Reed Stalker",
            "Monster",
            Role::Standalone,
            None,
        ),
        1,
        9,
    );
    let mut d = draft(
        BEAST,
        vec![p],
        &["<p>A tall wading hunter of the marsh.</p>"],
    );
    set(
        &mut d,
        "ID_T_MONSTER_REED",
        "data",
        json!({"ac": [{"value": ac}], "hp": {"average": hp, "formula": "4d8 + 4"},
            "abilities": {"str": str_, "dex": 16, "con": 12, "int": 3, "wis": 12, "cha": 6}}),
    );
    d
}

#[test]
fn a_printed_stat_block_passes_and_one_invented_number_is_named() {
    // Arrange
    let exact = beast(13, 22, 14);
    let one_off = beast(13, 22, 18);
    // Act
    let exact = checked(exact);
    let one_off = checked(one_off);
    // Assert
    assert!(passed_of(&exact, "ID_T_MONSTER_REED", STAT_BLOCK_ABSENT));
    assert!(failed_of(&exact, "ID_T_MONSTER_REED", NUMBER_NOT_IN_SOURCE).is_empty());
    assert!(passed_of(&one_off, "ID_T_MONSTER_REED", STAT_BLOCK_ABSENT));
    assert_eq!(
        failed_of(&one_off, "ID_T_MONSTER_REED", NUMBER_NOT_IN_SOURCE),
        vec!["The STR 18 is not in the source text"]
    );
}

#[test]
fn a_stat_block_made_up_for_lore_fails_stat_block_absent() {
    // Arrange: the passage is lore only; every core number is invented.
    let src = "## Reed Stalker\n\nA tall wading hunter of the marsh, feared by fishers.\n";
    let p = located(
        planned(
            "ID_T_MONSTER_REED",
            "Reed Stalker",
            "Monster",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(src, vec![p], &["<p>A tall wading hunter of the marsh.</p>"]);
    set(
        &mut d,
        "ID_T_MONSTER_REED",
        "data",
        json!({"ac": [{"value": 15}], "hp": {"average": 45},
            "abilities": {"str": 17, "dex": 14, "con": 15, "int": 3, "wis": 11, "cha": 5}}),
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_MONSTER_REED", STAT_BLOCK_ABSENT);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].starts_with("8 of its 8 stat block numbers"),
        "{}",
        f[0]
    );
}

#[test]
fn a_creature_kept_as_text_by_generation_fails_stat_block_absent() {
    // Arrange: generation's guard turned the creature into a text entry and said why.
    let src = "## Reed Stalker\n\nA tall wading hunter of the marsh.\n";
    let p = located(
        planned(
            "ID_T_REED",
            "Reed Stalker",
            "Rules Glossary Entry",
            Role::Text,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(src, vec![p], &["<p>A tall wading hunter of the marsh.</p>"]);
    d.elements[0].issues = vec![
        "stat block not found in the source text: kept as a text entry with its printed passage"
            .into(),
    ];
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_REED", STAT_BLOCK_ABSENT);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("No printed stat block"), "{}", f[0]);
}

// ---------------------------------------------------------------------------------------------
// list_member_missing

const CALLING: &str = "## Tide Calling\n\nYou always have these spells prepared.\n\n\
Level | Spells\n1st | cinder lash, frost needle\n3rd | gloam veil, tide surge\n\n\
You also learn the spark wisp cantrip.\n";

fn calling(granted: &[&str]) -> ImportDraft {
    let spells = [
        ("ID_T_SPELL_CINDER_LASH", "Cinder Lash"),
        ("ID_T_SPELL_FROST_NEEDLE", "Frost Needle"),
        ("ID_T_SPELL_GLOAM_VEIL", "Gloam Veil"),
        ("ID_T_SPELL_TIDE_SURGE", "Tide Surge"),
        ("ID_T_SPELL_SPARK_WISP", "Spark Wisp"),
    ];
    let mut plan = vec![located(
        planned(
            "ID_T_FEATURE_CALLING",
            "Tide Calling",
            "Archetype Feature",
            Role::Standalone,
            None,
        ),
        1,
        9,
    )];
    plan.extend(
        spells
            .iter()
            .map(|(id, n)| planned(id, n, "Spell", Role::Standalone, None)),
    );
    let descs = vec!["<p>You always have these spells prepared.</p>"; plan.len()];
    let mut d = draft(CALLING, plan, &descs);
    let rules: Vec<Value> = granted
        .iter()
        .map(|id| json!({"kind": "grant", "type": "Spell", "id": id}))
        .collect();
    set(&mut d, "ID_T_FEATURE_CALLING", "rules", json!(rules));
    d
}

#[test]
fn every_listed_spell_granted_passes() {
    // Arrange
    let d = calling(&[
        "ID_T_SPELL_CINDER_LASH",
        "ID_T_SPELL_FROST_NEEDLE",
        "ID_T_SPELL_GLOAM_VEIL",
        "ID_T_SPELL_TIDE_SURGE",
        "ID_T_SPELL_SPARK_WISP",
    ]);
    // Act
    let d = checked(d);
    // Assert
    assert!(passed_of(&d, "ID_T_FEATURE_CALLING", LIST_MEMBER_MISSING));
}

#[test]
fn a_spell_dropped_from_a_listed_row_and_an_ungranted_cantrip_fail() {
    // Arrange: one member of the 3rd-level row and the taught cantrip are missing.
    let d = calling(&[
        "ID_T_SPELL_CINDER_LASH",
        "ID_T_SPELL_FROST_NEEDLE",
        "ID_T_SPELL_GLOAM_VEIL",
    ]);
    // Act
    let d = checked(d);
    // Assert
    assert_eq!(
        failed_of(&d, "ID_T_FEATURE_CALLING", LIST_MEMBER_MISSING),
        vec!["Its source lists \"tide surge\", \"spark wisp\" but it does not grant them"]
    );
}

#[test]
fn a_choice_joining_ids_with_bars_grants_each_of_them() {
    // Arrange: a select whose `supports` lists the ids.
    let mut d = calling(&[]);
    set(
        &mut d,
        "ID_T_FEATURE_CALLING",
        "rules",
        json!([{"kind": "select", "type": "Spell", "supports": "ID_T_SPELL_CINDER_LASH|ID_T_SPELL_FROST_NEEDLE|ID_T_SPELL_GLOAM_VEIL|ID_T_SPELL_TIDE_SURGE|ID_T_SPELL_SPARK_WISP"}]),
    );
    // Act
    let d = checked(d);
    // Assert
    assert!(failed_of(&d, "ID_T_FEATURE_CALLING", LIST_MEMBER_MISSING).is_empty());
}

#[test]
fn spells_named_in_prose_by_an_element_granting_none_are_not_a_list() {
    // Arrange: the passage only mentions spells; nothing is granted.
    let src = "## Quiet Step\n\nYou move like cinder lash or frost needle, but you cast nothing.\n";
    let plan = vec![
        located(
            planned(
                "ID_T_FEAT_QUIET",
                "Quiet Step",
                "Feat",
                Role::Standalone,
                None,
            ),
            1,
            3,
        ),
        planned(
            "ID_T_SPELL_CINDER_LASH",
            "Cinder Lash",
            "Spell",
            Role::Standalone,
            None,
        ),
        planned(
            "ID_T_SPELL_FROST_NEEDLE",
            "Frost Needle",
            "Spell",
            Role::Standalone,
            None,
        ),
    ];
    let d = draft(
        src,
        plan,
        &[
            "<p>You move like cinder lash or frost needle, but you cast nothing.</p>",
            "<p>x</p>",
            "<p>y</p>",
        ],
    );
    // Act
    let d = checked(d);
    // Assert
    assert!(d
        .element("ID_T_FEAT_QUIET")
        .expect("feat")
        .checks
        .iter()
        .all(|c| c.code != LIST_MEMBER_MISSING));
}

// ---------------------------------------------------------------------------------------------
// rule_missing

fn trinket(sentence: &str, rules: Value) -> ImportDraft {
    let src = format!("## Fogglass Ring\n\n{sentence}\n");
    let p = located(
        planned(
            "ID_T_ITEM_FOGGLASS",
            "Fogglass Ring",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(&src, vec![p], &[&format!("<p>{sentence}</p>")]);
    set(&mut d, "ID_T_ITEM_FOGGLASS", "rules", rules);
    d
}

#[test]
fn an_items_lasting_advantage_without_a_rule_fails_rule_missing() {
    // Arrange
    let d = trinket(
        "While wearing this ring, you have advantage on Dexterity (Stealth) checks.",
        json!([]),
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_ITEM_FOGGLASS", RULE_MISSING);
    assert_eq!(f.len(), 1);
    assert!(f[0].starts_with("Its text gives advantage"), "{}", f[0]);
}

#[test]
fn an_items_mechanic_carried_by_a_rule_or_spent_as_a_charge_passes() {
    // Arrange
    let ruled = trinket(
        "While wearing this ring, you have resistance to cold damage.",
        json!([{"kind": "grant", "id": "ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_COLD"}]),
    );
    let charged = trinket(
        "As an action, you can expend 1 charge and you are immune to fire damage for 1 minute.",
        json!([]),
    );
    // Act
    let ruled = checked(ruled);
    let charged = checked(charged);
    // Assert
    assert!(passed_of(&ruled, "ID_T_ITEM_FOGGLASS", RULE_MISSING));
    assert!(passed_of(&charged, "ID_T_ITEM_FOGGLASS", RULE_MISSING));
}

// ---------------------------------------------------------------------------------------------
// description_empty

#[test]
fn a_description_that_is_only_the_name_fails_unless_it_heads_sub_entries() {
    // Arrange: a list of names read away from their text, under a heading with deeper entries.
    let src = "### Stock Fittings\n\n#### Reed Sight\n\nReed Sight\n\nShort Stock\n\nThe stock is cut down for close quarters.\n";
    let plan = vec![
        located(
            planned(
                "ID_T_FITTINGS",
                "Stock Fittings",
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            1,
            9,
        ),
        located(
            planned(
                "ID_T_REED_SIGHT",
                "Reed Sight",
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            3,
            5,
        ),
        located(
            planned(
                "ID_T_SHORT",
                "Short Stock",
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            7,
            9,
        ),
    ];
    let d = draft(
        src,
        plan,
        &[
            "<p><strong>Stock Fittings</strong></p>",
            "<p>Reed Sight</p>",
            "<p>The stock is cut down for close quarters.</p>",
        ],
    );
    // Act
    let d = checked(d);
    // Assert
    assert!(failed_of(&d, "ID_T_FITTINGS", DESCRIPTION_EMPTY).is_empty());
    assert_eq!(failed_of(&d, "ID_T_REED_SIGHT", DESCRIPTION_EMPTY).len(), 1);
    assert!(failed_of(&d, "ID_T_SHORT", DESCRIPTION_EMPTY).is_empty());
}

// ---------------------------------------------------------------------------------------------
// narrative_text

#[test]
fn an_adventures_keyed_area_kept_as_rules_fails_narrative_text() {
    // Arrange
    let text =
        "When the characters enter the mill, read aloud the following. The party hears grinding. \
If the characters search the loft, they find a ladder. The miller greets the adventurers warmly.";
    let src = format!("## The Old Mill\n\n{text}\n");
    let p = located(
        planned(
            "ID_T_MILL",
            "The Old Mill",
            "Rules Glossary Entry",
            Role::Text,
            None,
        ),
        1,
        3,
    );
    let d = draft(&src, vec![p], &[&format!("<p>{text}</p>")]);
    // Act
    let d = checked(d);
    // Assert
    assert_eq!(failed_of(&d, "ID_T_MILL", NARRATIVE_TEXT).len(), 1);
}

#[test]
fn a_rule_about_characters_and_an_entry_detected_as_narrative() {
    // Arrange: a travel rule mentions characters once; a short entry the detection called a
    // keyed location.
    let rule =
        "Each day of travel, characters cover 24 miles. A forced march adds 3 miles per hour. \
Each extra hour calls for a Constitution saving throw.";
    let src = format!("## Travel Pace\n\n{rule}\n\n## Gate House\n\nA squat tower.\n");
    let plan = vec![
        located(
            planned(
                "ID_T_PACE",
                "Travel Pace",
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            1,
            3,
        ),
        located(
            planned(
                "ID_T_GATE",
                "Gate House",
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            5,
            7,
        ),
    ];
    let mut d = draft(
        &src,
        plan,
        &[&format!("<p>{rule}</p>"), "<p>A squat tower.</p>"],
    );
    d.detection.other = vec![OtherItem {
        name: "Gate House".into(),
        what: "keyed location".into(),
        note: String::new(),
    }];
    // Act
    let d = checked(d);
    // Assert
    assert!(failed_of(&d, "ID_T_PACE", NARRATIVE_TEXT).is_empty());
    assert_eq!(failed_of(&d, "ID_T_GATE", NARRATIVE_TEXT).len(), 1);
}

// ---------------------------------------------------------------------------------------------
// library_duplicate

#[test]
fn an_official_item_the_source_only_mentions_fails_library_duplicate() {
    // Arrange: the library has the item; the source mentions it in prose only.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let base = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("srd-5.1"))
        .expect("srd loads");
    let store = base.clone().build();
    let official = store
        .by_type("Magic Item")
        .next()
        .expect("a library magic item")
        .element
        .clone();
    let src = format!(
        "## The Vault\n\nThe chest holds 40 gold pieces and a {} wrapped in cloth.\n",
        official.name.to_lowercase()
    );
    let p = located(
        planned(
            "ID_T_COPY",
            &official.name,
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(&src, vec![p], &["<p>A copy.</p>"]);
    d.edition = Edition::Y2014;
    // Act
    apply(&base, &mut d);
    // Assert
    let f = failed_of(&d, "ID_T_COPY", LIBRARY_DUPLICATE);
    assert_eq!(f.len(), 1, "{:?}", d.elements[0].checks);
    assert!(f[0].contains(&official.id), "{}", f[0]);
}

// ---------------------------------------------------------------------------------------------
// Split tables and tables of contents

#[test]
fn rows_of_an_items_table_planned_as_entries_are_missing_from_the_item() {
    // Arrange: the item's own table was split into one text entry per row.
    let src = "### Weather Wand\n\nRoll on the table each dawn.\n\nd4 | Weather\n1 | Fog rolls in over 30 feet\n2 | Sleet falls for 10 minutes\n3 | Thunder sounds for 2 rounds\n4 | Calm air for 1 hour\n\n### Next Item\n\nText.\n";
    let mut plan = vec![located(
        planned(
            "ID_T_WAND",
            "Weather Wand",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        10,
    )];
    for (k, line) in (6..=9).enumerate() {
        plan.push(located(
            planned(
                &format!("ID_T_ROW_{k}"),
                &format!("Row {k}"),
                "Rules Glossary Entry",
                Role::Text,
                None,
            ),
            line,
            line,
        ));
    }
    plan.push(located(
        planned(
            "ID_T_NEXT",
            "Next Item",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        11,
        13,
    ));
    let mut descs = vec!["<p>Roll on the table each dawn.</p>"];
    let rows = [
        "<p>1 Fog rolls in over 30 feet</p>",
        "<p>2 Sleet falls for 10 minutes</p>",
        "<p>3 Thunder sounds for 2 rounds</p>",
        "<p>4 Calm air for 1 hour</p>",
    ];
    descs.extend(rows);
    descs.push("<p>Text.</p>");
    let d = draft(src, plan, &descs);
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_WAND", TABLE_ROW_MISSING);
    assert_eq!(f.len(), 1, "{:?}", d.elements[0].checks);
    assert!(
        f[0].starts_with("4 of its own table's rows are separate entries"),
        "{}",
        f[0]
    );
    assert!(failed_of(&d, "ID_T_NEXT", TABLE_ROW_MISSING).is_empty());
}

#[test]
fn a_table_of_contents_is_no_table_to_carry() {
    // Arrange
    let src = "## Contents\n\nOld Ways | 3\nNew Roads | 7\nFar Shores | 12\n";
    let p = located(
        planned("ID_T_CONTENTS", "Contents", "Feat", Role::Standalone, None),
        1,
        5,
    );
    let d = draft(src, vec![p], &["<p>Contents</p>"]);
    // Act
    let d = checked(d);
    // Assert
    assert!(failed_of(&d, "ID_T_CONTENTS", TABLE_ROW_MISSING).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Text fragments of the heading check

#[test]
fn a_margin_quote_read_as_a_heading_is_text_not_a_section() {
    // Arrange: two uncovered "headings": a sentence cut at the line end and a real title.
    let body = "This body is long enough to be content of its own and nobody imported any of it at all, sadly.";
    let src = format!(
        "## Anchor\n\nAnchor text.\n\n### I always wondered who\n\n{body}\n\n### Tide Lore\n\n{body}\n"
    );
    let p = located(
        planned("ID_T_ANCHOR", "Anchor", "Feat", Role::Standalone, None),
        1,
        3,
    );
    let d = draft(&src, vec![p], &["<p>Anchor text.</p>"]);
    // Act
    let d = checked(d);
    // Assert
    let codes: Vec<(&str, &str)> = d
        .source_checks
        .iter()
        .filter(|c| c.failed)
        .map(|c| (c.code.as_str(), c.message.as_str()))
        .collect();
    assert_eq!(
        codes,
        vec![
            (
                TEXT_UNCOVERED,
                "The text starting \"I always wondered who\" was not imported"
            ),
            (
                HEADING_UNCOVERED,
                "The section \"Tide Lore\" was not imported"
            ),
        ]
    );
}

#[test]
fn fragments_and_titles() {
    assert!(fragment("Oh, the poor souls who have laid at my"));
    assert!(fragment("My old friend... It was almost a pet"));
    assert!(fragment("I've always liked the dead"));
    assert!(fragment("\"Never again,\" she said"));
    assert!(!fragment("Tide Lore"));
    assert!(!fragment("Ostra's Quiet Lantern Variant"));
    assert!(!fragment("Part I"));
}
