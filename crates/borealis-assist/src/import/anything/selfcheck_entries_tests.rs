//! Tests of the wave-12 review checks: shared descriptions, legal text, transcription
//! summaries, unresolved spell grants, pools without a recharge, attunement, printed stat block
//! numbers, list members outside the rules, flattened tables, split owner tables, and the item
//! rule wording (synthetic sources only).

use serde_json::json;

use super::content_tests::{checked, failed_of, located, passed_of, set};
use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};

fn text(id: &str, name: &str, start: usize, end: usize) -> PlannedElement {
    located(
        planned(id, name, "Rules Glossary Entry", Role::Text, None),
        start,
        end,
    )
}

fn item(id: &str, name: &str, start: usize, end: usize) -> PlannedElement {
    located(
        planned(id, name, "Magic Item", Role::Standalone, None),
        start,
        end,
    )
}

// ---------------------------------------------------------------------------------------------
// description_shared

const OPTIONS: &str = "## Stance Options\n\nStance | Effect\nIron Root | You can't be pushed while you stand still this turn.\nWillow Sway | Attacks of opportunity against you have disadvantage.\nRiver Step | Your speed rises by 10 feet until the end of the turn.\n";

#[test]
fn entries_each_holding_the_whole_table_that_names_them_fail_description_shared() {
    // Arrange: every row's entry got the whole table as its text.
    let whole = "<p>Stance Effect Iron Root You can't be pushed while you stand still this turn. Willow Sway Attacks of opportunity against you have disadvantage. River Step Your speed rises by 10 feet until the end of the turn.</p>";
    let plan = vec![
        text("ID_T_IRON", "Iron Root", 4, 4),
        text("ID_T_WILLOW", "Willow Sway", 5, 5),
        text("ID_T_RIVER", "River Step", 6, 6),
    ];
    let d = draft(OPTIONS, plan, &[whole, whole, whole]);
    // Act
    let d = checked(d);
    // Assert
    for id in ["ID_T_IRON", "ID_T_WILLOW", "ID_T_RIVER"] {
        let f = failed_of(&d, id, DESCRIPTION_SHARED);
        assert_eq!(f.len(), 1, "{id}");
        assert!(
            f[0].starts_with("Its text is the same as 2 other"),
            "{}",
            f[0]
        );
    }
}

#[test]
fn rows_cut_to_their_own_text_and_a_parent_holding_its_rows_are_not_shared() {
    // Arrange: each row's entry has its own row; the section holds the whole table.
    let plan = vec![
        text("ID_T_OPTIONS", "Stance Options", 1, 6),
        text("ID_T_IRON", "Iron Root", 4, 4),
        text("ID_T_WILLOW", "Willow Sway", 5, 5),
    ];
    let whole = "<p>Stance Effect Iron Root You can't be pushed while you stand still this turn. Willow Sway Attacks of opportunity against you have disadvantage. River Step Your speed rises by 10 feet until the end of the turn.</p>";
    let d = draft(
        OPTIONS,
        plan,
        &[
            whole,
            "<p>Iron Root: You can't be pushed while you stand still this turn.</p>",
            "<p>Willow Sway: Attacks of opportunity against you have disadvantage.</p>",
        ],
    );
    // Act
    let d = checked(d);
    // Assert
    for id in ["ID_T_OPTIONS", "ID_T_IRON", "ID_T_WILLOW"] {
        assert!(failed_of(&d, id, DESCRIPTION_SHARED).is_empty(), "{id}");
    }
}

// ---------------------------------------------------------------------------------------------
// end_matter_text and transcription_paraphrase

#[test]
fn legal_text_appended_to_an_entry_fails_but_the_legal_entry_itself_does_not() {
    // Arrange
    let src = "## Closing Words\n\nMore adventures will follow.\n\n## Legal Information\n\nAll rights reserved.\n";
    let legal =
        "<p>This work is published under the community content agreement. All rights reserved.</p>";
    let plan = vec![
        text("ID_T_CLOSING", "Closing Words", 1, 7),
        text("ID_T_LEGAL", "Legal Information", 5, 7),
        text("ID_T_CURSE", "Copyright Curse", 1, 1),
    ];
    let appended = format!("<p>More adventures will follow.</p>{legal}");
    let d = draft(
        src,
        plan,
        &[
            &appended,
            legal,
            "<p>A copyright on your soul: you owe a favour.</p>",
        ],
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_CLOSING", END_MATTER_TEXT);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("legal or credits text"), "{}", f[0]);
    assert!(failed_of(&d, "ID_T_LEGAL", END_MATTER_TEXT).is_empty());
    assert!(failed_of(&d, "ID_T_CURSE", END_MATTER_TEXT).is_empty());
}

#[test]
fn text_a_transcription_marked_as_a_summary_fails_transcription_paraphrase() {
    // Arrange: one race's lore is a marked summary; the other's is printed text.
    let src = "## Mossfolk\n(Summary) Mossfolk are patient bog dwellers who trade in herbs.\n\n## Reedkin\nReedkin weave homes from river reeds and sing at dusk.\n";
    let plan = vec![
        located(
            planned("ID_T_MOSS", "Mossfolk", "Race", Role::Standalone, None),
            1,
            3,
        ),
        located(
            planned("ID_T_REED", "Reedkin", "Race", Role::Standalone, None),
            4,
            5,
        ),
    ];
    let d = draft(
        src,
        plan,
        &[
            "<p>Mossfolk are patient bog dwellers who trade in herbs.</p>",
            "<p>Reedkin weave homes from river reeds and sing at dusk.</p>",
        ],
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_MOSS", TRANSCRIPTION_PARAPHRASE);
    assert_eq!(f.len(), 1);
    assert!(
        f[0].contains("mossfolk are patient bog dwellers"),
        "{}",
        f[0]
    );
    assert!(failed_of(&d, "ID_T_REED", TRANSCRIPTION_PARAPHRASE).is_empty());
}

#[test]
fn a_passage_a_reading_note_calls_paraphrased_fails_transcription_paraphrase() {
    // Arrange: the transcription step names the reworded passage in a reading note.
    let src = "## Reedkin\nReedkin weave homes from river reeds and sing at dusk.\n";
    let plan = vec![located(
        planned("ID_T_REED", "Reedkin", "Race", Role::Standalone, None),
        1,
        2,
    )];
    let mut d = draft(
        src,
        plan,
        &["<p>Reedkin weave homes from river reeds and sing at dusk.</p>"],
    );
    d.detection
        .notes
        .extend(crate::import::anything::ingest::transcript::span_notes(
            "page 2",
            &["Reedkin weave homes from river reeds and sing at dusk.".to_string()],
        ));
    // Act
    let d = checked(d);
    // Assert
    assert_eq!(
        failed_of(&d, "ID_T_REED", TRANSCRIPTION_PARAPHRASE).len(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// grant_unresolved and recharge_missing

fn charm(rules: serde_json::Value, extra: Vec<PlannedElement>) -> ImportDraft {
    let src = "## Ember Charm\n\nYou know the Spark Wisp cantrip. You have 3 ember points. You regain all expended ember points when you finish a long rest.\n";
    let mut plan = vec![located(
        planned(
            "ID_T_CHARM",
            "Ember Charm",
            "Racial Trait",
            Role::Standalone,
            None,
        ),
        1,
        3,
    )];
    plan.extend(extra);
    let descs = vec!["<p>You know the Spark Wisp cantrip.</p>"; plan.len()];
    let mut d = draft(src, plan, &descs);
    set(&mut d, "ID_T_CHARM", "rules", rules);
    d
}

#[test]
fn a_cantrip_grant_no_content_has_fails_grant_unresolved() {
    // Arrange: the spell id was guessed; neither the import nor the rules have it.
    let d = charm(
        json!([{"kind": "grant_spell", "mode": "known_cantrip", "spell": "ID_X_SPELL_SPARK_WISP"}]),
        Vec::new(),
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_CHARM", GRANT_UNRESOLVED);
    assert_eq!(f.len(), 1);
    assert!(f[0].starts_with("It grants \"Spark Wisp\""), "{}", f[0]);
}

#[test]
fn a_grant_of_an_imported_spell_passes_and_a_noted_target_fails() {
    // Arrange
    let spell = planned(
        "ID_T_SPELL_SPARK_WISP",
        "Spark Wisp",
        "Spell",
        Role::Standalone,
        None,
    );
    let ok = charm(
        json!([{"kind": "grant_spell", "mode": "known_cantrip", "spell": "ID_T_SPELL_SPARK_WISP"}]),
        vec![spell],
    );
    let mut noted = charm(json!([]), Vec::new());
    noted.elements[0].issues.push(format!(
        "{}\"Spark Wisp\" is an official spell in neither this import nor the installed content",
        grants::GRANT_NOTE_PREFIX
    ));
    // Act
    let ok = checked(ok);
    let noted = checked(noted);
    // Assert
    assert!(passed_of(&ok, "ID_T_CHARM", GRANT_UNRESOLVED));
    let f = failed_of(&noted, "ID_T_CHARM", GRANT_UNRESOLVED);
    assert_eq!(f.len(), 1);
    assert!(f[0].starts_with("It grants \"Spark Wisp\""), "{}", f[0]);
}

#[test]
fn a_pool_its_text_refills_on_a_rest_without_a_recharge_fails_recharge_missing() {
    // Arrange
    let none = charm(
        json!([{"kind": "resource", "id": "ember points", "name": "Ember Points", "max": {"type": "fixed", "value": 3}}]),
        Vec::new(),
    );
    let rested = charm(
        json!([{"kind": "resource", "id": "ember points", "name": "Ember Points", "max": {"type": "fixed", "value": 3}, "recharge": ["long_rest"]}]),
        Vec::new(),
    );
    // Act
    let none = checked(none);
    let rested = checked(rested);
    // Assert
    let f = failed_of(&none, "ID_T_CHARM", RECHARGE_MISSING);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("\"ember points\""), "{}", f[0]);
    assert!(failed_of(&rested, "ID_T_CHARM", RECHARGE_MISSING).is_empty());
}

// ---------------------------------------------------------------------------------------------
// list members outside the rules

#[test]
fn a_listed_cantrip_no_content_has_and_left_out_of_the_choice_fails() {
    // Arrange: the choice offers the two known cantrips and drops the third.
    let src = "## Ember Gift\n\n**Ember Gift.** Learn one of these cantrips: *Cinder Lash*, *Frost Needle* or *Hollow Chime*. Charisma is your spellcasting ability for it.\n";
    let plan = vec![
        located(
            planned(
                "ID_T_GIFT",
                "Ember Gift",
                "Racial Trait",
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
    let descs = [
        "<p>Learn one of these cantrips.</p>",
        "<p>x</p>",
        "<p>y</p>",
    ];
    let mut dropped = draft(src, plan.clone(), &descs);
    set(
        &mut dropped,
        "ID_T_GIFT",
        "rules",
        json!([{"kind": "select", "type": "Spell", "supports": "ID_T_SPELL_CINDER_LASH|ID_T_SPELL_FROST_NEEDLE"}]),
    );
    let mut offered = draft(src, plan, &descs);
    set(
        &mut offered,
        "ID_T_GIFT",
        "rules",
        json!([{"kind": "select", "type": "Spell", "supports": "ID_T_SPELL_CINDER_LASH|ID_T_SPELL_FROST_NEEDLE|ID_X_SPELL_HOLLOW_CHIME"}]),
    );
    // Act
    let dropped = checked(dropped);
    let offered = checked(offered);
    // Assert
    let f = failed_of(&dropped, "ID_T_GIFT", LIST_MEMBER_MISSING);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].starts_with("Its source lists \"Hollow Chime\""),
        "{}",
        f[0]
    );
    assert!(failed_of(&offered, "ID_T_GIFT", LIST_MEMBER_MISSING).is_empty());
    // Offered but in no content: the grant check says so instead.
    assert_eq!(failed_of(&offered, "ID_T_GIFT", GRANT_UNRESOLVED).len(), 1);
}

// ---------------------------------------------------------------------------------------------
// attunement_mismatch

fn armor(setters: serde_json::Value) -> ImportDraft {
    let src = "## Mirror Mail\nArmor (chain mail), rare (requires\nattunement by a paladin)\nThis armor shines.\n";
    let mut d = draft(
        src,
        vec![item("ID_T_MAIL", "Mirror Mail", 1, 4)],
        &["<p>This armor shines.</p>"],
    );
    set(&mut d, "ID_T_MAIL", "setters", setters);
    d
}

#[test]
fn attunement_stored_as_its_restriction_or_missing_fails_attunement_mismatch() {
    // Arrange
    let restriction = armor(json!([{"name": "attunement", "value": "by a paladin"}]));
    let missing = armor(json!([{"name": "rarity", "value": "Rare"}]));
    let right = armor(
        json!([{"name": "attunement", "value": "true", "attributes": {"addition": "by a paladin"}}]),
    );
    // Act
    let restriction = checked(restriction);
    let missing = checked(missing);
    let right = checked(right);
    // Assert
    let f = failed_of(&restriction, "ID_T_MAIL", ATTUNEMENT_MISMATCH);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("\"by a paladin\""), "{}", f[0]);
    let f = failed_of(&missing, "ID_T_MAIL", ATTUNEMENT_MISMATCH);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("requires attunement"), "{}", f[0]);
    assert!(failed_of(&right, "ID_T_MAIL", ATTUNEMENT_MISMATCH).is_empty());
}

// ---------------------------------------------------------------------------------------------
// rule_missing wording

fn gear(sentence: &str, rules: serde_json::Value) -> ImportDraft {
    let src = format!("## Gleaming Buckler\n\n{sentence}\n");
    let mut d = draft(
        &src,
        vec![item("ID_T_SHIELD", "Gleaming Buckler", 1, 3)],
        &[&format!("<p>{sentence}</p>")],
    );
    set(&mut d, "ID_T_SHIELD", "rules", rules);
    d
}

#[test]
fn an_ac_bonus_for_the_wielder_needs_an_armor_class_rule_not_the_letters_ac() {
    // Arrange: the rules only hold a reaction pool (its words contain "ac" as letters).
    let pool = json!([{"kind": "resource", "id": "glint", "name": "Glint", "action": "reaction"}]);
    let missing = gear("This buckler gives its wielder a +2 bonus to AC.", pool);
    let ruled = gear(
        "This buckler gives its wielder a +2 bonus to AC.",
        json!([{"kind": "stat", "name": "ac:armored:enhancement", "value": 2}]),
    );
    let wielded = gear(
        "You have a +1 bonus to AC while you hold this buckler.",
        json!([]),
    );
    // Act
    let missing = checked(missing);
    let ruled = checked(ruled);
    let wielded = checked(wielded);
    // Assert
    let f = failed_of(&missing, "ID_T_SHIELD", RULE_MISSING);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("an Armor Class bonus"), "{}", f[0]);
    assert!(passed_of(&ruled, "ID_T_SHIELD", RULE_MISSING));
    assert_eq!(failed_of(&wielded, "ID_T_SHIELD", RULE_MISSING).len(), 1);
}

#[test]
fn advantage_limited_to_a_situation_is_kept_as_text_without_a_flag() {
    // Arrange: advantage only while dancing, or on checks made to swim.
    let dancing = gear(
        "While dancing in moonlight you have advantage on Charisma (Performance) checks.",
        json!([]),
    );
    let swim = gear(
        "While wearing it you gain advantage on Strength (Athletics) checks made to swim against a current.",
        json!([]),
    );
    // Act
    let dancing = checked(dancing);
    let swim = checked(swim);
    // Assert
    assert!(passed_of(&dancing, "ID_T_SHIELD", RULE_MISSING));
    assert!(passed_of(&swim, "ID_T_SHIELD", RULE_MISSING));
}

// ---------------------------------------------------------------------------------------------
// printed_number_differs

const STALKER: &str = "## Reed Stalker\n\nA tall wading hunter that strides 15 feet a step.\n\n\
Armor Class 13\nHit Points 22 (4d8 + 4)\nSpeed 30 ft.\nSTR DEX CON INT WIS CHA\n14 (+2) 16 (+3) 12 (+1) 3 (-4) 12 (+1) 6 (-2)\nChallenge 1 (150 XP)\n";

fn stalker(ac: u64, xp: u64) -> ImportDraft {
    let p = located(
        planned(
            "ID_T_STALKER",
            "Reed Stalker",
            "Monster",
            Role::Standalone,
            None,
        ),
        1,
        10,
    );
    let mut d = draft(STALKER, vec![p], &["<p>A tall wading hunter.</p>"]);
    set(
        &mut d,
        "ID_T_STALKER",
        "data",
        json!({"ac": [{"value": ac}], "hp": {"average": 22, "formula": "4d8 + 4"}, "xp": xp,
            "abilities": {"str": 14, "dex": 16, "con": 12, "int": 3, "wis": 12, "cha": 6}}),
    );
    d
}

#[test]
fn a_stat_block_number_other_than_the_printed_one_fails_printed_number_differs() {
    // Arrange: 15 is printed (feet) but not as the armor class; XP set to the standard 200.
    let swapped = stalker(15, 200);
    let exact = stalker(13, 150);
    // Act
    let swapped = checked(swapped);
    let exact = checked(exact);
    // Assert
    let f = failed_of(&swapped, "ID_T_STALKER", PRINTED_NUMBER_DIFFERS);
    assert_eq!(
        f,
        vec![
            "Its armor class is 15 but the source prints 13: the sheet shows a number the source does not have",
            "Its XP is 200 but the source prints 150: the sheet shows a number the source does not have",
        ]
    );
    assert!(failed_of(&exact, "ID_T_STALKER", PRINTED_NUMBER_DIFFERS).is_empty());
}

// ---------------------------------------------------------------------------------------------
// table_row_missing: flattened tables and split owner tables

#[test]
fn a_table_flattened_to_its_first_cells_says_so() {
    // Arrange: the entry kept the names and lost the type column.
    let src = "## Rare Gear\n\nItem | Type\nLantern of Dusk +1 | Wondrous item\nThorn Whip-Staff +2 | Weapon (quarterstaff)\nGlass Buckler +1 | Armor (shield)\n";
    let d = draft(
        src,
        vec![text("ID_T_RARE", "Rare Gear", 1, 6)],
        &["<p>Item Lantern of Dusk +1 Thorn Whip-Staff +2 Glass Buckler +1</p>"],
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_RARE", TABLE_ROW_MISSING);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].starts_with("Its table was flattened into text"),
        "{}",
        f[0]
    );
}

#[test]
fn an_owners_table_past_its_located_passage_split_into_entries_is_missing_from_it() {
    // Arrange: the item's passage was located up to its own text; its effect table, under a
    // deeper heading, became one text entry per row.
    let src = "### Weather Wand\n\nRoll on the table each dawn.\n\n#### Wand Weather\n\nd4 | Weather\n1 | Fog rolls in over 30 feet\n2 | Sleet falls for 10 minutes\n3 | Thunder sounds for 2 rounds\n4 | Calm air for 1 hour\n\n### Next Item\n\nText.\n";
    let mut plan = vec![item("ID_T_WAND", "Weather Wand", 1, 3)];
    for (k, line) in (8..=11).enumerate() {
        plan.push(text(
            &format!("ID_T_ROW_{k}"),
            &format!("Row {k}"),
            line,
            line,
        ));
    }
    plan.push(item("ID_T_NEXT", "Next Item", 13, 15));
    let mut descs = vec!["<p>Roll on the table each dawn.</p>"];
    descs.extend([
        "<p>1 Fog rolls in over 30 feet</p>",
        "<p>2 Sleet falls for 10 minutes</p>",
        "<p>3 Thunder sounds for 2 rounds</p>",
        "<p>4 Calm air for 1 hour</p>",
        "<p>Text.</p>",
    ]);
    let d = draft(src, plan, &descs);
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_WAND", TABLE_ROW_MISSING);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(
        f[0].starts_with("4 of its own table's rows are separate entries"),
        "{}",
        f[0]
    );
    assert!(failed_of(&d, "ID_T_NEXT", TABLE_ROW_MISSING).is_empty());
}
