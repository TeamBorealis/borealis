//! The engine load check and the balance report through the crate's public API, on the 2024 SRD (the import
//! pipeline's 2014 cases stay in `borealis-assist/tests/balance.rs`).

use std::path::Path;

use borealis_authoring::check::{
    balance_report, load_into_engine, Baselines, CharacterCheck, Severity,
};
use borealis_content::file::ContentFile;
use borealis_content::{ContentStoreBuilder, Ruleset};
use serde_json::{json, Value};

fn base() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    b
}

fn doc(element: Value) -> Value {
    json!({"format": "borealis-content/1", "elements": [element]})
}

#[test]
fn a_2024_species_loads_and_a_test_character_takes_it() {
    let species = doc(
        json!({"id": "ID_HB_SPECIES_MARSHFOLK", "name": "Marshfolk", "type": "Species",
        "source": "Homebrew", "rules": [{"kind": "grant", "id": "ID_SIZE_MEDIUM", "type": "Size"}]}),
    );
    let c = load_into_engine(&base(), Ruleset::Dnd2024, &species);
    assert!(c.passes(), "{c:?}");
    assert_eq!(c.characters[0].0, "ID_HB_SPECIES_MARSHFOLK");
    assert_eq!(c.characters[0].1, CharacterCheck::Taken);
}

#[test]
fn text_that_is_not_content_fails_the_check() {
    let c = load_into_engine(&base(), Ruleset::Dnd2024, &json!({"elements": "nope"}));
    assert!(!c.loaded);
    assert!(!c.passes());
    assert!(c.store_errors[0].contains("does not parse"), "{c:?}");
}

#[test]
fn a_2024_spell_far_above_the_srd_is_flagged() {
    let spell = doc(
        json!({"id": "ID_HB_SPELL_DOOM", "name": "Doom", "type": "Spell", "source": "Homebrew",
        "supports": ["Wizard"],
        "description": "<p>Each creature in a 20-foot-radius sphere takes 12d10 fire damage on a failed save.</p>",
        "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
            {"name": "time", "value": "Action"}, {"name": "duration", "value": "Instantaneous"},
            {"name": "range", "value": "150 feet"}]}),
    );
    let element = ContentFile::from_json_str(&spell.to_string())
        .expect("parses")
        .elements
        .remove(0);
    let baselines = Baselines::from_store(&base().build(), Ruleset::Dnd2024);
    let r = balance_report(&baselines, &element);
    assert_eq!(r.element_type, "Spell");
    assert!(r.max_severity() >= Severity::Medium, "{:?}", r.flags);
}
