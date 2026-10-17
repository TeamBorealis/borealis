//! Tests of the wave-13 loss checks (synthetic sources only).

use serde_json::json;

use super::super::content_tests::{checked, failed_of, located, set};
use super::super::*;
use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};

fn species(id: &str, name: &str, start: usize, end: usize) -> PlannedElement {
    located(
        planned(id, name, "Race", Role::Standalone, None),
        start,
        end,
    )
}

const NOTES: &str = "# Tallowkin\n\nLore: small, bright folk of the deep caves.\n- Ability Score Increase: DEX +2, WIS +1.\n- Size: Small; walking speed 25 ft.\n- Darkvision: 60 ft.\n- Glow: shed dim light 5 ft.\n- Languages: Common, one other.\n";

const PRINT: &str = "# Tallowkin\n\nSmall, bright folk of the deep caves.\n- Ability Score Increase: Your Dexterity score increases by 2.\n- Size: You are Small. Your walking speed is 25 feet.\n- Darkvision: You can see in dim light within 60 feet of you.\n- Glow: You shed dim light in a 5-foot radius.\n- Languages: You can speak Common and one other language.\n";

#[test]
fn a_passage_of_terse_label_notes_fails_condensed_notes_and_so_do_its_traits() {
    // Arrange: a species passage read as notes, with one trait planned inside it.
    let plan = vec![
        species("ID_T_TALLOW", "Tallowkin", 1, 8),
        located(
            planned("ID_T_GLOW", "Glow", "Racial Trait", Role::Part, None),
            7,
            7,
        ),
    ];
    let d = draft(
        NOTES,
        plan,
        &["<p>Small, bright folk.</p>", "<p>Shed dim light 5 ft.</p>"],
    );
    // Act
    let d = checked(d);
    // Assert
    let f = failed_of(&d, "ID_T_TALLOW", CONDENSED_NOTES);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("short notes"), "{}", f[0]);
    assert_eq!(failed_of(&d, "ID_T_GLOW", CONDENSED_NOTES).len(), 1);
}

#[test]
fn printed_label_lists_that_address_the_reader_pass() {
    // Arrange
    let plan = vec![species("ID_T_TALLOW", "Tallowkin", 1, 8)];
    let d = draft(
        PRINT,
        plan,
        &["<p>Small, bright folk of the deep caves.</p>"],
    );
    // Act
    let d = checked(d);
    // Assert
    assert!(failed_of(&d, "ID_T_TALLOW", CONDENSED_NOTES).is_empty());
}

#[test]
fn a_printed_creature_type_the_species_lacks_fails_creature_type_missing() {
    // Arrange
    let src =
        "# Cogborn\n\nCreature Type. You are a Construct.\nSpeed. Your walking speed is 30 feet.\n";
    let mk = || {
        draft(
            src,
            vec![species("ID_T_COG", "Cogborn", 1, 4)],
            &["<p>Built folk.</p>"],
        )
    };
    // Act
    let missing = checked(mk());
    let mut with = mk();
    set(
        &mut with,
        "ID_T_COG",
        "data",
        json!({"creature_type": "Construct"}),
    );
    let with = checked(with);
    // Assert
    let f = failed_of(&missing, "ID_T_COG", CREATURE_TYPE_MISSING);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("construct"), "{}", f[0]);
    assert!(failed_of(&with, "ID_T_COG", CREATURE_TYPE_MISSING).is_empty());
}

#[test]
fn a_humanoid_species_needs_no_creature_type() {
    let src = "# Plainfolk\n\nCreature Type. You are a Humanoid.\n";
    let d = checked(draft(
        src,
        vec![species("ID_T_PLAIN", "Plainfolk", 1, 3)],
        &["<p>Folk.</p>"],
    ));
    assert!(failed_of(&d, "ID_T_PLAIN", CREATURE_TYPE_MISSING).is_empty());
}

fn charm(desc: &str, rules: serde_json::Value) -> ImportDraft {
    let src = "# Ember Charm\n\nWhile you wear it, you learn the Ember Spark cantrip.\n";
    let plan = vec![located(
        planned(
            "ID_T_CHARM",
            "Ember Charm",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        3,
    )];
    let mut d = draft(src, plan, &[desc]);
    set(&mut d, "ID_T_CHARM", "rules", rules);
    d
}

fn spell_flags(desc: &str, targets: Vec<String>) -> Vec<ElementCheck> {
    let d = charm(desc, json!([]));
    let e = d.element("ID_T_CHARM").expect("element");
    spell_grant_check(&d, e, &[], &|_| targets.clone(), &[])
}

#[test]
fn a_cantrip_the_text_teaches_with_no_grant_fails_spell_not_granted() {
    // Arrange
    let desc = "<p>While you wear it, you learn the <em>Ember Spark</em> cantrip.</p>";
    // Act
    let none = spell_flags(desc, Vec::new());
    let granted = spell_flags(desc, vec!["ID_T_SPELL_EMBER_SPARK".into()]);
    // Assert
    assert_eq!(none.len(), 1);
    assert!(
        none[0].message.contains("ember spark"),
        "{}",
        none[0].message
    );
    assert!(granted.is_empty());
}

#[test]
fn a_spell_the_text_lets_you_cast_is_read_when_known_or_when_nothing_grants_a_spell() {
    let desc = "<p>Once per day you can cast frost lance with it.</p>";
    let known = [(
        "frost lance".to_string(),
        "ID_T_SPELL_FROST_LANCE".to_string(),
    )];
    let d = charm(desc, json!([]));
    let e = d.element("ID_T_CHARM").expect("element");
    assert_eq!(
        spell_grant_check(&d, e, &known, &|_| Vec::new(), &[]).len(),
        1
    );
    // Not a known spell: read only while nothing of the family grants a spell.
    let unknown = spell_grant_check(&d, e, &[], &|_| Vec::new(), &[]);
    assert_eq!(unknown.len(), 1);
    assert!(unknown[0].message.contains("\"frost lance\""));
    let grants = |_: &str| vec!["ID_T_SPELL_RIME_DART".to_string()];
    assert!(spell_grant_check(&d, e, &[], &grants, &[]).is_empty());
}

#[test]
fn a_generic_cantrip_phrase_names_no_spell() {
    let desc = "<p>You learn the following cantrip of your choice.</p>";
    assert!(spell_flags(desc, Vec::new()).is_empty());
}

const ROLLS: &str = "# Bag of Pelts\n\nRoll to see what it holds.\n\nd4 | Pelt\n1 | Fox pelt\n2 | Wolf pelt\n3 | Bear pelt\n4 | Seal pelt\n";
const PICKS: &str = "# Stances\n\nChoose one stance.\n\nStance | Effect\nFox | You move 5 feet more.\nWolf | You gain +1 to hit.\nBear | You gain 5 temporary hit points.\n";

fn options(src: &str, rows: [(&str, &str, usize); 3]) -> ImportDraft {
    let mut plan = vec![located(
        planned(
            "ID_T_ROOT",
            "Root",
            "Rules Glossary Entry",
            Role::Text,
            None,
        ),
        1,
        10,
    )];
    for (id, name, line) in rows {
        let mut p = located(
            planned(id, name, "Class Feature", Role::Option, None),
            line,
            line,
        );
        p.parent = Some("ID_T_ROOT".into());
        plan.push(p);
    }
    draft(
        src,
        plan,
        &["<p>Root.</p>", "<p>A.</p>", "<p>B.</p>", "<p>C.</p>"],
    )
}

#[test]
fn rows_of_a_roll_table_imported_as_options_fail_roll_table_option() {
    // Act
    let rolled = checked(options(
        ROLLS,
        [
            ("ID_T_A", "Fox pelt", 6),
            ("ID_T_B", "Wolf pelt", 7),
            ("ID_T_C", "Bear pelt", 8),
        ],
    ));
    let picked = checked(options(
        PICKS,
        [
            ("ID_T_A", "Fox", 6),
            ("ID_T_B", "Wolf", 7),
            ("ID_T_C", "Bear", 8),
        ],
    ));
    // Assert
    assert_eq!(failed_of(&rolled, "ID_T_A", ROLL_TABLE_OPTION).len(), 1);
    assert!(failed_of(&picked, "ID_T_A", ROLL_TABLE_OPTION).is_empty());
}
