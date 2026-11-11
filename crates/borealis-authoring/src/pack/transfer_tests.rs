//! `transfer`: leave the copied text out (HC8) and import a pack as a copy (spec 14.2).

use super::*;
use crate::ids::IdGenerator;
use borealis_content::{Append, ContentFile, Element, PackManifest};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

fn manifest() -> PackManifest {
    PackManifest::from_json_str(
        &json!({"format": "borealis-pack/1", "id": "homebrew-gh", "name": "Gloom Hollow", "version": "1.0.3",
                "origin": "custom", "rulesets": ["dnd2024"], "files": [{"path": "content.json", "kind": "content"}]})
        .to_string(),
    )
    .unwrap()
}

fn element(v: Value) -> Element {
    serde_json::from_value(v).unwrap()
}

/// A pack: its book, a class granting its feature and an SRD feature, the feature, a spell copied from an imported
/// book, a creature copied from it too, a language moved in from another pack (`ID_OW_...`), and an append that adds
/// the spell to a bundled list and one that targets its own class.
fn pack() -> (ContentFile, CreatorFile) {
    let mut content = ContentFile::new(vec![
        element(
            json!({"id": "ID_GH_SOURCE_GLOOM_HOLLOW", "name": "Gloom Hollow", "type": "Source", "source": "Gloom Hollow",
            "setters": [{"name": "abbreviation", "value": "GH"}, {"name": "url", "value": ""}]}),
        ),
        element(
            json!({"id": "ID_GH_CLASS_WARDEN", "name": "Warden", "type": "Class", "source": "Gloom Hollow",
            "description": "<p>My own words.</p>",
            "requirements": "!ID_GH_CLASS_WARDEN_2,ID_OW_LANGUAGE_GLOOM",
            "rules": [{"kind": "grant", "type": "Class Feature", "id": "ID_GH_CLASS_FEATURE_WARDEN_WATCH", "level": 1},
                      {"kind": "grant", "type": "Class Feature", "id": "ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING", "level": 1}]}),
        ),
        element(
            json!({"id": "ID_GH_CLASS_FEATURE_WARDEN_WATCH", "name": "Watch", "type": "Class Feature", "source": "Gloom Hollow",
            "description": "<p>Mine.</p>"}),
        ),
        element(
            json!({"id": "ID_GH_SPELL_SHADOW_BOLT_COPY", "name": "Shadow Bolt (copy)", "type": "Spell", "source": "Gloom Hollow",
            "description": "<p>A bolt of shadow.</p>",
            "sheet": {"descriptions": [{"text": "Shadow damage."}]}}),
        ),
        element(
            json!({"id": "ID_GH_CREATURE_GLOOM_HOUND", "name": "Gloom Hound", "type": "Monster", "source": "Gloom Hollow",
            "data": {"description": "Flavour.", "traits": [{"name": "Keen Smell", "text": "It smells."}],
                     "legendary": {"per_round": 3, "preamble": "It can take 3.", "actions": [{"name": "Bite", "text": "It bites."}]},
                     "lair_actions": {"preamble": "In its lair:", "options": [{"text": "Fog."}]}}}),
        ),
        element(
            json!({"id": "ID_OW_LANGUAGE_GLOOM", "name": "Gloom", "type": "Language", "source": "Gloom Hollow"}),
        ),
    ]);
    content.appends = vec![
        Append {
            id: "ID_2024_SPELL_LIST_WIZARD".into(),
            ..Append::default()
        },
        Append {
            id: "ID_GH_CLASS_WARDEN".into(),
            ..Append::default()
        },
    ];
    let mut creator = CreatorFile::adopt("GH", "1.5.0", &content);
    for id in ["ID_GH_SPELL_SHADOW_BOLT_COPY", "ID_GH_CREATURE_GLOOM_HOUND"] {
        let e = creator.elements.get_mut(id).unwrap();
        e.non_srd_text = true;
        e.copied_from = Some(CopiedFrom {
            id: "ID_GT_X".into(),
            pack_id: "imported-book".into(),
            srd: false,
        });
    }
    creator.retired_ids = vec!["ID_GH_SPELL_OLD".into()];
    (content, creator)
}

fn by_id<'a>(c: &'a ContentFile, id: &str) -> &'a Element {
    c.elements.iter().find(|e| e.id == id).unwrap()
}

#[test]
fn only_elements_with_copied_text_are_counted_and_left_out() {
    let (content, creator) = pack();
    assert_eq!(
        copied_text_ids(&content, &creator),
        ["ID_GH_SPELL_SHADOW_BOLT_COPY", "ID_GH_CREATURE_GLOOM_HOUND"]
    );
    let (out, changed) = without_copied_text(&content, &creator);
    assert_eq!(
        changed,
        ["ID_GH_CREATURE_GLOOM_HOUND", "ID_GH_SPELL_SHADOW_BOLT_COPY"]
    );
    let spell = by_id(&out, "ID_GH_SPELL_SHADOW_BOLT_COPY");
    assert_eq!(spell.description, "");
    assert!(spell.sheet.as_ref().unwrap().descriptions.is_empty());
    let hound = serde_json::to_string(by_id(&out, "ID_GH_CREATURE_GLOOM_HOUND")).unwrap();
    for gone in [
        "Flavour.",
        "It smells.",
        "It can take 3.",
        "It bites.",
        "In its lair:",
        "Fog.",
    ] {
        assert!(!hound.contains(gone), "{gone} left in {hound}");
    }
    assert!(
        hound.contains("Keen Smell") && hound.contains("\"per_round\":3"),
        "names and numbers stay: {hound}"
    );
    assert_eq!(hound.matches(LEFT_OUT_TEXT).count(), 5);
    // the user's own text stays; the marks stay too, so the pack is still not redistributable
    assert_eq!(
        by_id(&out, "ID_GH_CLASS_WARDEN").description,
        "<p>My own words.</p>"
    );
    // nothing is left to leave out a second time
    assert!(copied_text_ids(&out, &creator).is_empty());
    assert_eq!(without_copied_text(&out, &creator).0, out);
}

/// A copied background and feat holding the other texts a copy keeps: the short description, the display
/// prerequisite, the sheet's short name and usage, the "At Higher Levels" text, and a characteristic list's entries.
fn copied_other_texts() -> (ContentFile, CreatorFile) {
    let content = ContentFile::new(vec![
        element(
            json!({"id": "ID_GH_BACKGROUND_ACOLYTE_COPY", "name": "Acolyte (copy)", "type": "Background",
            "source": "Gloom Hollow", "prerequisite": "Must have served a temple.",
            "requirements": "ID_GH_LANGUAGE_GLOOM",
            "setters": [{"name": "short", "value": "You served a temple."}, {"name": "category", "value": "Origin"}],
            "sheet": {"alt": "Temple Server", "usage": "Once per dawn", "action": "Action"},
            "rules": [
                {"kind": "select", "type": "List", "name": "Personality Trait", "number": 2,
                 "items": [{"id": 1, "text": "I quote sacred texts."}, {"id": 2, "text": "I am patient."}]},
                {"kind": "grant", "type": "Language", "id": "ID_GH_LANGUAGE_GLOOM"}
            ]}),
        ),
        element(
            json!({"id": "ID_GH_SPELL_EMBER_COPY", "name": "Ember (copy)", "type": "Spell", "source": "Gloom Hollow",
            "setters": [{"name": "level", "value": "1"}, {"name": "short", "value": "A small flame."}],
            "data": {"higher_level": "Using a Higher-Level Spell Slot. More flame.", "level_1_text": "1st-level evocation"}}),
        ),
    ]);
    let mut creator = CreatorFile::adopt("GH", "1.5.0", &content);
    for id in ["ID_GH_BACKGROUND_ACOLYTE_COPY", "ID_GH_SPELL_EMBER_COPY"] {
        let e = creator.elements.get_mut(id).unwrap();
        e.non_srd_text = true;
        e.copied_from = Some(CopiedFrom {
            id: "ID_GT_X".into(),
            pack_id: "imported-book".into(),
            srd: false,
        });
    }
    (content, creator)
}

#[test]
fn every_other_copied_text_is_left_out_and_the_rules_stay() {
    let (content, creator) = copied_other_texts();
    assert_eq!(
        copied_text_ids(&content, &creator),
        ["ID_GH_BACKGROUND_ACOLYTE_COPY", "ID_GH_SPELL_EMBER_COPY"],
        "elements holding only those texts are counted too"
    );
    let (out, changed) = without_copied_text(&content, &creator);
    assert_eq!(changed.len(), 2);
    let text = serde_json::to_string(&out).unwrap();
    for gone in [
        "Must have served a temple.",
        "You served a temple.",
        "Temple Server",
        "Once per dawn",
        "I quote sacred texts.",
        "I am patient.",
        "A small flame.",
        "More flame.",
        "1st-level evocation",
    ] {
        assert!(!text.contains(gone), "{gone} left in {text}");
    }
    let bg = by_id(&out, "ID_GH_BACKGROUND_ACOLYTE_COPY");
    assert_eq!(
        bg.requirements, "ID_GH_LANGUAGE_GLOOM",
        "the requirement is a rule"
    );
    assert_eq!(bg.setter_value("category"), Some("Origin"));
    assert_eq!(
        bg.sheet.as_ref().and_then(|s| s.action.as_deref()),
        Some("Action")
    );
    let list = serde_json::to_value(&bg.rules).unwrap();
    assert_eq!(list[0]["name"], "Personality Trait");
    assert_eq!(list[0]["number"], 2);
    assert_eq!(
        list[0]["items"],
        json!([{"id": 1, "text": LEFT_OUT_TEXT}, {"id": 2, "text": LEFT_OUT_TEXT}]),
        "the list keeps its entries to pick from"
    );
    assert_eq!(list[1]["id"], "ID_GH_LANGUAGE_GLOOM");
    let spell = by_id(&out, "ID_GH_SPELL_EMBER_COPY");
    assert_eq!(spell.setter_value("level"), Some("1"));
    assert!(copied_text_ids(&out, &creator).is_empty());
    assert_eq!(without_copied_text(&out, &creator).0, out);
}

#[test]
fn a_copy_code_takes_the_next_free_number() {
    let taken = |codes: &[&str]| {
        codes
            .iter()
            .map(|c| (*c).to_string())
            .collect::<HashSet<_>>()
    };
    assert_eq!(copy_code("GH", &taken(&["GH"])), "GH2");
    assert_eq!(copy_code("GH", &taken(&["GH", "GH2"])), "GH3");
    assert_eq!(copy_code("GH2", &taken(&["GH", "GH2"])), "GH3");
    assert_eq!(copy_code("ABCDEFGH", &taken(&["ABCDEFGH"])), "ABCDEFG2");
    assert_eq!(copy_code("12", &taken(&["12"])), "122");
    assert_eq!(copy_name("Gloom Hollow"), "Gloom Hollow (copy)");
    assert_eq!(copy_name(&"x".repeat(200)).chars().count(), PACK_NAME_MAX);
}

#[test]
fn a_copy_gives_every_id_of_the_pack_again_and_references_follow() {
    let (content, creator) = pack();
    let loaded = ["ID_GH2_CLASS_WARDEN"]; // another pack already has this id
    let mut ids = IdGenerator::new("GH2", loaded, Vec::<String>::new()).unwrap();
    let copy = pack_copy(
        &manifest(),
        &content,
        &creator,
        "Gloom Hollow (copy)",
        &mut ids,
    )
    .unwrap();
    let map: HashMap<&str, &str> = copy
        .ids
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    assert_eq!(
        map["ID_GH_SOURCE_GLOOM_HOLLOW"],
        "ID_GH2_SOURCE_GLOOM_HOLLOW"
    );
    assert_eq!(
        map["ID_GH_CLASS_WARDEN"], "ID_GH2_CLASS_WARDEN_2",
        "a taken id gets a number"
    );
    assert_eq!(
        map["ID_OW_LANGUAGE_GLOOM"], "ID_GH2_LANGUAGE_GLOOM",
        "an id of another code gets a creator id"
    );
    assert_eq!(copy.manifest.id, "homebrew-gh2");
    assert_eq!(copy.manifest.name, "Gloom Hollow (copy)");
    assert_eq!(copy.manifest.version, "1.0.3");
    let class = by_id(&copy.content, "ID_GH2_CLASS_WARDEN_2");
    assert_eq!(class.source, "Gloom Hollow (copy)");
    assert_eq!(
        class.requirements, "!ID_GH2_CLASS_WARDEN_2_2,ID_GH2_LANGUAGE_GLOOM",
        "another id of the pack's code follows too, made unique"
    );
    let text = serde_json::to_string(&copy.content).unwrap();
    assert!(text.contains("ID_GH2_CLASS_FEATURE_WARDEN_WATCH"));
    assert!(
        text.contains("ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING"),
        "other packs' ids stay"
    );
    assert!(
        !text.contains("ID_GH_") && !text.contains("ID_OW_"),
        "{text}"
    );
    let book = by_id(&copy.content, "ID_GH2_SOURCE_GLOOM_HOLLOW");
    assert_eq!(book.name, "Gloom Hollow (copy)");
    assert_eq!(book.setter_value("abbreviation"), Some("GH2"));
    assert_eq!(
        copy.content
            .appends
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["ID_2024_SPELL_LIST_WIZARD", "ID_GH2_CLASS_WARDEN_2"],
        "an append to a bundled list keeps its target; one to the pack's own element follows it"
    );
    // creator.json: the new code, entries under the new ids with their provenance, no retired ids
    assert_eq!(copy.creator.code, "GH2");
    assert!(copy.creator.retired_ids.is_empty());
    let hound = &copy.creator.elements["ID_GH2_CREATURE_GLOOM_HOUND"];
    assert!(hound.non_srd_text);
    assert_eq!(hound.copied_from.as_ref().unwrap().pack_id, "imported-book");
    assert_eq!(copy.creator.elements.len(), creator.elements.len());

    // equal apart from the ids and the book's name and code
    let back: HashMap<&str, &str> = copy
        .ids
        .iter()
        .map(|(o, n)| (n.as_str(), o.as_str()))
        .collect();
    let mut v = serde_json::to_value(&copy.content).unwrap();
    rewrite_ids(&mut v, &back);
    let mut restored: ContentFile = serde_json::from_value(v).unwrap();
    for e in &mut restored.elements {
        e.source = "Gloom Hollow".into();
        if e.ty == "Source" {
            e.name = "Gloom Hollow".into();
            e.setters[0].value = "GH".into();
        }
    }
    assert_eq!(restored, content);
}

#[test]
fn rewriting_ids_follows_object_keys_and_leaves_longer_ids_alone() {
    let map: HashMap<&str, &str> = [("ID_A", "ID_B")].into_iter().collect();
    let mut v = json!({"ID_A": ["ID_A|ID_AB", "x ID_A y", 3], "k": "ID_ABC"});
    rewrite_ids(&mut v, &map);
    assert_eq!(
        v,
        json!({"ID_B": ["ID_B|ID_AB", "x ID_B y", 3], "k": "ID_ABC"})
    );
}
