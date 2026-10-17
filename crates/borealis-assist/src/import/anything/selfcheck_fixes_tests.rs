//! Tests of the wave-13b self-check repairs and checks: choice options resolved or flagged as
//! empty, an item's attunement from its printed header, a spell kept as a use pool, a spell
//! the text casts that nothing grants, an option cut to its table row, note-form bullets with
//! emphasised labels and a legal footer that is page furniture (synthetic sources only).

use borealis_content::file::{ContentFile, ContentFormat, PackOrigin};
use serde_json::json;

use super::content_tests::{failed_of, located, set};
use super::*;
use crate::import::anything::fidelity::tests::{draft, planned};

/// A small rules library: two sizes, two languages and one spell.
fn library() -> ContentStoreBuilder {
    let doc = json!({"format": ContentFormat::ID, "ruleset": "dnd2024", "elements": [
        {"id": "ID_SIZE_MEDIUM", "name": "Medium", "type": "Size", "source": "T"},
        {"id": "ID_SIZE_SMALL", "name": "Small", "type": "Size", "source": "T"},
        {"id": "ID_T_LANGUAGE_ELVISH", "name": "Elvish", "type": "Language", "source": "T"},
        {"id": "ID_T_LANGUAGE_DWARVISH", "name": "Dwarvish", "type": "Language", "source": "T"},
        {"id": "ID_T_SPELL_GLIMMER_BOLT", "name": "Glimmer Bolt", "type": "Spell", "source": "T",
            "setters": [{"name": "level", "value": "0"}]}
    ]});
    let file = ContentFile::from_json_str(&doc.to_string()).expect("library loads");
    ContentStoreBuilder::new().add_file("lib", PackOrigin::Custom, file)
}

fn checked_with(mut d: ImportDraft) -> ImportDraft {
    apply(&library(), &mut d);
    d
}

fn species(rules: serde_json::Value) -> ImportDraft {
    let src = "## Reedfolk\n\nYou are small and quick. You speak the river tongues.\n";
    let p = located(
        planned(
            "ID_T_RACE_REEDFOLK",
            "Reedfolk",
            "Race",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(
        src,
        vec![p],
        &["<p>You are small and quick. You speak the river tongues.</p>"],
    );
    set(&mut d, "ID_T_RACE_REEDFOLK", "rules", rules);
    set(&mut d, "ID_T_RACE_REEDFOLK", "source", json!("T"));
    d
}

fn rules_of<'a>(d: &'a ImportDraft, id: &str) -> &'a Vec<Value> {
    d.element(id).expect("element").json["rules"]
        .as_array()
        .expect("rules")
}

// ---------------------------------------------------------------------------------------------
// choice options

#[test]
fn choice_options_written_as_names_point_at_their_elements() {
    // Arrange
    let d = species(
        json!([{"kind": "select", "type": "Language", "name": "Language",
        "supports": "Elvish|Dwarvish"}]),
    );
    // Act
    let d = checked_with(d);
    // Assert
    let id = "ID_T_RACE_REEDFOLK";
    assert_eq!(
        rules_of(&d, id)[0]["supports"],
        "ID_T_LANGUAGE_ELVISH|ID_T_LANGUAGE_DWARVISH"
    );
    let e = d.element(id).expect("element");
    assert!(e
        .normalised
        .iter()
        .any(|n| n.contains("Elvish -> ID_T_LANGUAGE_ELVISH")));
    assert!(failed_of(&d, id, choices::CHOICE_EMPTY).is_empty());
    // Idempotent: the checks run again after a repair round.
    let again = checked_with(d.clone());
    assert_eq!(again.element(id).expect("element").normalised, e.normalised);
}

#[test]
fn a_choice_whose_options_match_nothing_keeps_them_and_fails_choice_empty() {
    // Arrange
    let d = species(json!([
        {"kind": "select", "type": "Language", "name": "Language", "supports": "Elvish|Glimtongue"},
        {"kind": "select_size", "name": "Size", "options": ["Smallish"]}
    ]));
    // Act
    let d = checked_with(d);
    // Assert: not every name resolves, so nothing is guessed; both choices are flagged.
    let id = "ID_T_RACE_REEDFOLK";
    assert_eq!(rules_of(&d, id)[0]["supports"], "Elvish|Glimtongue");
    let f = failed_of(&d, id, choices::CHOICE_EMPTY);
    assert_eq!(f.len(), 2, "{f:?}");
    assert!(
        f[0].contains("\"Language\"") && f[0].contains("\"Glimtongue\""),
        "{}",
        f[0]
    );
    assert!(f[1].contains("\"Smallish\""), "{}", f[1]);
}

#[test]
fn size_options_written_as_ids_are_offered_as_finalize_writes_them() {
    // Arrange: the engine prefixes size options; finalize turns id-form options into names.
    let ids = species(json!([{"kind": "select_size", "name": "Size",
        "options": ["ID_SIZE_MEDIUM", "ID_SIZE_SMALL"]}]));
    let names = species(json!([{"kind": "select_size", "name": "Size",
        "options": ["Medium", "Small"]}]));
    // Act
    let ids = checked_with(ids);
    let names = checked_with(names);
    // Assert
    assert!(failed_of(&ids, "ID_T_RACE_REEDFOLK", choices::CHOICE_EMPTY).is_empty());
    assert!(failed_of(&names, "ID_T_RACE_REEDFOLK", choices::CHOICE_EMPTY).is_empty());
}

// ---------------------------------------------------------------------------------------------
// attunement

fn amulet(src: &str, setters: serde_json::Value) -> ImportDraft {
    let p = located(
        planned(
            "ID_T_AMULET",
            "Tide Amulet",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        4,
    );
    let mut d = draft(src, vec![p], &["<p>The amulet is cold to the touch.</p>"]);
    set(&mut d, "ID_T_AMULET", "setters", setters);
    d
}

#[test]
fn an_item_whose_header_requires_attunement_gets_it_from_the_header() {
    // Arrange
    let src = "## Tide Amulet\nWondrous item, rare (requires attunement by a\ndruid)\nThe amulet is cold to the touch.\n";
    let missing = amulet(src, json!([{"name": "rarity", "value": "Rare"}]));
    let restriction = amulet(src, json!([{"name": "attunement", "value": "by a druid"}]));
    let plain = amulet(
        "## Tide Amulet\nWondrous item, rare\nThe amulet is cold to the touch.\n",
        json!([{"name": "rarity", "value": "Rare"}]),
    );
    // Act
    let missing = checked_with(missing);
    let restriction = checked_with(restriction);
    let plain = checked_with(plain);
    // Assert
    for d in [&missing, &restriction] {
        let el = &d.element("ID_T_AMULET").expect("element").json;
        let att = el["setters"]
            .as_array()
            .and_then(|s| s.iter().find(|x| x["name"] == "attunement"))
            .expect("attunement setter");
        assert_eq!(att["value"], "true");
        assert_eq!(att["attributes"]["addition"], "by a druid");
        assert!(failed_of(d, "ID_T_AMULET", ATTUNEMENT_MISMATCH).is_empty());
        assert!(!d
            .element("ID_T_AMULET")
            .expect("element")
            .normalised
            .is_empty());
    }
    let el = &plain.element("ID_T_AMULET").expect("element").json;
    assert!(
        !el.to_string().contains("attunement"),
        "nothing printed: nothing added"
    );
}

// ---------------------------------------------------------------------------------------------
// spells cast from the text

fn headband(sentence: &str, rules: serde_json::Value) -> ImportDraft {
    let src = format!("## Headband of Sparks\nWondrous item, uncommon\n{sentence}\n");
    let p = located(
        planned(
            "ID_T_HEADBAND",
            "Headband of Sparks",
            "Magic Item",
            Role::Standalone,
            None,
        ),
        1,
        3,
    );
    let mut d = draft(&src, vec![p], &[&format!("<p>{sentence}</p>")]);
    set(&mut d, "ID_T_HEADBAND", "rules", rules);
    d
}

fn pool() -> serde_json::Value {
    json!([{"kind": "resource", "id": "headband of sparks", "name": "Headband of Sparks",
        "max": {"type": "fixed", "value": 1}, "recharge": ["short_rest", "long_rest"]}])
}

#[test]
fn a_spell_kept_as_a_use_pool_becomes_free_casts_of_that_spell() {
    // Arrange
    let d = headband(
        "While wearing this headband you can cast glimmer bolt. You can't use this ability again until you finish a short or long rest.",
        pool(),
    );
    // Act
    let d = checked_with(d);
    // Assert
    let rules = rules_of(&d, "ID_T_HEADBAND");
    assert_eq!(rules.len(), 1);
    assert_eq!(
        rules[0],
        json!({"kind": "grant_spell", "spell": "ID_T_SPELL_GLIMMER_BOLT", "mode": "always_prepared",
            "free_cast": {"uses": 1, "recharge": "short_rest"}})
    );
    assert!(failed_of(&d, "ID_T_HEADBAND", losses::SPELL_NOT_GRANTED).is_empty());
}

#[test]
fn a_spell_the_text_casts_that_nothing_grants_fails_spell_not_granted() {
    // Arrange: the spell is in neither the library nor the import, so nothing can be granted.
    let unknown = headband(
        "While wearing this headband you can cast mistveil. You can't use this ability again until you finish a long rest.",
        pool(),
    );
    let generic = headband(
        "While wearing this headband you can cast spells as rituals, and you can cast it again.",
        pool(),
    );
    // Act
    let unknown = checked_with(unknown);
    let generic = checked_with(generic);
    // Assert
    let f = failed_of(&unknown, "ID_T_HEADBAND", losses::SPELL_NOT_GRANTED);
    assert_eq!(f.len(), 1);
    assert!(f[0].contains("\"mistveil\""), "{}", f[0]);
    assert_eq!(rules_of(&unknown, "ID_T_HEADBAND")[0]["kind"], "resource");
    assert!(failed_of(&generic, "ID_T_HEADBAND", losses::SPELL_NOT_GRANTED).is_empty());
}

// ---------------------------------------------------------------------------------------------
// description_cut

const MATERIALS: &str = "## Rare Materials\n\n### Ironbark\n\nIronbark grows only in the oldest forests of the north, where its trees take centuries to reach a useful size. Armor made from it weighs half as much as steel armor and never rusts, and a shield of ironbark can turn aside even the blow of a giant's club.\n\n## Material Costs\n\nMaterial | Cost | Property\nIronbark | 500 gp | light and rustproof\n";

fn ironbark(description: &str) -> ImportDraft {
    let root = located(
        planned(
            "ID_T_MATERIALS",
            "Rare Materials",
            "Rules Glossary Entry",
            Role::Text,
            None,
        ),
        1,
        2,
    );
    let mut opt = located(
        planned(
            "ID_T_IRONBARK",
            "Ironbark",
            "Rules Glossary Entry",
            Role::Option,
            None,
        ),
        3,
        6,
    );
    opt.parent = Some("ID_T_MATERIALS".into());
    draft(
        MATERIALS,
        vec![root, opt],
        &["<p>Rare Materials</p>", description],
    )
}

#[test]
fn an_option_cut_to_its_table_row_while_its_section_prints_prose_fails_description_cut() {
    // Arrange
    let cut = ironbark("<p>Ironbark | 500 gp | light and rustproof</p>");
    let whole = ironbark("<p>Ironbark grows only in the oldest forests of the north, where its trees take centuries to reach a useful size. Armor made from it weighs half as much as steel armor and never rusts, and a shield of ironbark can turn aside even the blow of a giant's club.</p>");
    // Act
    let cut = checked_with(cut);
    let whole = checked_with(whole);
    // Assert
    let f = failed_of(&cut, "ID_T_IRONBARK", losses::DESCRIPTION_CUT);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].contains("only its row of a table"), "{}", f[0]);
    assert!(failed_of(&whole, "ID_T_IRONBARK", losses::DESCRIPTION_CUT).is_empty());
}

// ---------------------------------------------------------------------------------------------
// condensed_notes with emphasised labels

#[test]
fn bullets_with_emphasised_labels_and_no_reader_are_notes() {
    // Arrange
    let notes = "## Mirefolk\n\n- **Size.** Stocky, about 5 feet tall; Medium.\n- **Speed.** 30 feet.\n- **Darkvision.** 60 feet.\n- **Mire Stride.** Ignores difficult terrain from mud.\n- **Languages.** Common and Aquan.\n";
    let printed = "## Mirefolk\n\n- **Size.** Your size is Medium.\n- **Speed.** Your walking speed is 30 feet.\n- **Darkvision.** You can see in dim light within 60 feet of you.\n- **Mire Stride.** You ignore difficult terrain made of mud.\n- **Languages.** You speak Common and Aquan.\n";
    let make = |src: &str| {
        let p = located(
            planned("ID_T_MIREFOLK", "Mirefolk", "Race", Role::Standalone, None),
            1,
            8,
        );
        draft(src, vec![p], &["<p>Mirefolk</p>"])
    };
    // Act
    let notes = checked_with(make(notes));
    let printed = checked_with(make(printed));
    // Assert
    assert_eq!(
        failed_of(&notes, "ID_T_MIREFOLK", losses::CONDENSED_NOTES).len(),
        1
    );
    assert!(failed_of(&printed, "ID_T_MIREFOLK", losses::CONDENSED_NOTES).is_empty());
}

// ---------------------------------------------------------------------------------------------
// end_matter_text and page furniture

#[test]
fn a_legal_footer_repeated_on_every_page_is_furniture_not_end_matter() {
    // Arrange: the notice is the page footer, which the pack leaves out of every text.
    let footer = "Not for resale. Permission granted to print or photocopy this document for personal use only.";
    let mut src = String::from("## Ember Gift\n\nYou kindle a small flame in your palm.\n\n");
    for page in 2..10 {
        for line in 0..12 {
            src.push_str(&format!("Line {line} of page {page} goes on here.\n"));
        }
        src.push_str(&format!("\n{footer}\n\nEMBER BOOK | {page}!\n\n"));
    }
    let p = located(
        planned(
            "ID_T_EMBER",
            "Ember Gift",
            "Class Feature",
            Role::ClassFeature,
            None,
        ),
        1,
        3,
    );
    let d = draft(
        &src,
        vec![p],
        &[&format!(
            "<p>You kindle a small flame in your palm.</p><p>{footer}</p>"
        )],
    );
    // Act
    let d = checked_with(d);
    // Assert
    assert!(failed_of(&d, "ID_T_EMBER", END_MATTER_TEXT).is_empty());
}
