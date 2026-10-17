//! Official spells nobody has (synthetic names next to bundled SRD spells).

use std::sync::OnceLock;

use borealis_content::{ContentStore, ContentStoreBuilder, IssueCode, Severity};
use serde_json::{json, Value};

use super::super::super::derived::{content_doc, Derived};
use super::super::super::types::{Edition, ImportPlan, PlannedElement, Role, SpellGrant};
use super::*;

fn library() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(pack)).expect("bundled pack");
        }
        b.build()
    })
}

fn grant(level: u32, name: &str, id: Option<&str>) -> SpellGrant {
    SpellGrant {
        level,
        name: name.into(),
        id: id.map(str::to_string),
        class: "Hedgekeeper".into(),
    }
}

fn planned(id: &str, spells: Vec<SpellGrant>) -> PlannedElement {
    PlannedElement {
        id: id.into(),
        ty: "Archetype Feature".into(),
        name: "Mire Circle Spells".into(),
        role: Role::SubclassFeature,
        level: Some(1),
        owner: None,
        brief: String::new(),
        excerpt: None,
        parent: None,
        picks: None,
        prerequisite_level: None,
        spells,
    }
}

fn plan(edition: Edition, elements: Vec<PlannedElement>) -> ImportPlan {
    ImportPlan {
        edition,
        groups: Vec::new(),
        elements,
        notes: Vec::new(),
    }
}

fn derive(plan: &ImportPlan, docs: Vec<Value>) -> Derived {
    let mut d = Derived {
        docs,
        ..Derived::default()
    };
    official_spells(plan, library(), &mut d);
    d
}

/// Errors and dangling references of the derived pack, loaded next to the library.
fn pack_problems(d: &Derived) -> Vec<String> {
    let mut doc = content_doc(d);
    for el in doc["elements"].as_array_mut().into_iter().flatten() {
        if el.get("source").is_none() {
            el["source"] = json!("Test");
        }
    }
    let file = borealis_content::file::ContentFile::from_json_str(&doc.to_string())
        .expect("the pack loads");
    let ids: Vec<String> = file.elements.iter().map(|e| e.id.clone()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("bundled pack");
    }
    let store = b
        .add_file("t", borealis_content::file::PackOrigin::Custom, file)
        .build();
    store
        .validate()
        .into_iter()
        .filter(|i| i.element.as_ref().is_some_and(|e| ids.contains(e)))
        .filter(|i| i.severity == Severity::Error || i.code == IssueCode::DanglingReference)
        .map(|i| i.message)
        .collect()
}

fn setter<'a>(el: &'a Value, name: &str) -> &'a str {
    el["setters"]
        .as_array()
        .and_then(|s| s.iter().find(|x| x["name"] == name))
        .and_then(|x| x["value"].as_str())
        .unwrap_or("")
}

#[test]
fn table_spells_nobody_has_become_placeholders_at_the_level_their_row_fixes() {
    let feature = "ID_TST_ARCHETYPE_FEATURE_MIRE_CIRCLE_SPELLS";
    let p = plan(
        Edition::Y2014,
        vec![planned(
            feature,
            vec![
                grant(1, "Sleep", Some("ID_PHB_SPELL_SLEEP")),
                grant(1, "frost needle", None),
                grant(3, "Suggestion", Some("ID_PHB_SPELL_SUGGESTION")),
                grant(3, "gloom veil", None),
                grant(5, "rime lance", None),
            ],
        )],
    );
    let doc = json!({"id": feature, "name": "Mire Circle Spells", "type": "Archetype Feature",
        "description": "<p>You always have these spells prepared.</p>",
        "rules": [{"kind": "grant", "type": "Spell", "id": "ID_PHB_SPELL_SLEEP", "level": 1,
            "spellcasting": "Hedgekeeper", "prepared": true}]});
    let d = derive(&p, vec![doc]);
    let levels: Vec<(&str, &str, &str)> = d
        .extra
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap(),
                setter(e, "level"),
                e["id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        levels,
        [
            ("Frost Needle", "1", "ID_TST_SPELL_FROST_NEEDLE"),
            ("Gloom Veil", "2", "ID_TST_SPELL_GLOOM_VEIL"),
            ("Rime Lance", "3", "ID_TST_SPELL_RIME_LANCE"),
        ]
    );
    for e in &d.extra {
        assert_eq!(e["source"], PLACEHOLDER_SOURCE);
        assert!(e["description"]
            .as_str()
            .unwrap()
            .contains("Mire Circle Spells"));
        assert!(e.get("rules").is_none(), "no rules of its own");
    }
    let rules = d.docs[0]["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 4, "{rules:?}");
    assert_eq!(rules[3]["id"], "ID_TST_SPELL_RIME_LANCE");
    assert_eq!(rules[3]["level"], 5);
    assert_eq!(rules[3]["prepared"], true);
    assert_eq!(d.notes.len(), 3);
    assert!(d
        .notes
        .iter()
        .all(|(id, m)| id == feature && m.starts_with(UNRESOLVED_OFFICIAL)));
    assert!(pack_problems(&d).is_empty(), "{:?}", pack_problems(&d));
    // Idempotent: the placeholders resolve the second time.
    let again = derive(&p, [d.docs.clone(), d.extra.clone()].concat());
    assert!(
        again.extra.is_empty() && again.notes.is_empty(),
        "{:?}",
        again.notes
    );
}

#[test]
fn a_row_level_is_not_guessed_when_no_known_spell_confirms_the_ranks() {
    let feature = "ID_TST_ARCHETYPE_FEATURE_MIRE_CIRCLE_SPELLS";
    let p = plan(
        Edition::Y2024,
        vec![planned(feature, vec![grant(3, "gloom veil", None)])],
    );
    let doc = json!({"id": feature, "name": "Mire Circle Spells", "type": "Archetype Feature",
        "description": "<p>You learn gloom veil at 3rd level.</p>"});
    let d = derive(&p, vec![doc]);
    assert!(d.extra.is_empty());
    assert!(d.docs[0]["rules"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(d.notes.len(), 1);
    assert!(d.notes[0].1.contains("not granted"), "{:?}", d.notes);
    // A cantrip the text names keeps its grant.
    let p = plan(
        Edition::Y2024,
        vec![planned(feature, vec![grant(1, "ember whistle", None)])],
    );
    let doc = json!({"id": feature, "name": "Mire Circle Spells", "type": "Archetype Feature",
        "description": "<p>You learn the ember whistle and fire bolt cantrips.</p>"});
    let d = derive(&p, vec![doc]);
    assert_eq!(setter(&d.extra[0], "level"), "0");
    assert_eq!(d.docs[0]["rules"][0]["kind"], "grant_spell");
    assert_eq!(d.docs[0]["rules"][0]["mode"], "known_cantrip");
    assert_eq!(d.docs[0]["rules"][0]["spell"], d.extra[0]["id"]);
}

#[test]
fn guessed_official_ids_resolve_by_name_or_become_placeholders_never_dangling() {
    let item = json!({"id": "ID_TST_MAGIC_ITEM_LANTERN_OF_REEDS", "name": "Lantern of Reeds",
    "type": "Magic Item", "description": "<p>While holding it you know two cantrips.</p>",
    "rules": [
        {"kind": "grant_spell", "spell": "ID_GUESS_SPELL_SACRED_FLAME", "mode": "known_cantrip"},
        {"kind": "grant_spell", "spell": "ID_XGE_SPELL_EMBER_WHISTLE", "mode": "known_cantrip"},
        {"kind": "grant_spell", "spell": "ID_XGE_SPELL_FOG_LANTERN", "mode": "at_will_no_slot"}
    ]});
    let d = derive(&plan(Edition::Y2024, Vec::new()), vec![item]);
    // A target written as a name: a library spell, or a cantrip the text names.
    let named = json!({"id": "ID_TST_RACIAL_TRAIT_PEBBLE_LORE", "name": "Pebble Lore",
        "type": "Racial Trait", "description": "<p>You know the pebble skip cantrip.</p>",
        "rules": [{"kind": "grant_spell", "spell": "Pebble Skip"},
            {"kind": "grant_spell", "spell": "Sacred Flame"},
            {"kind": "grant_spell", "spell": "Sacred Flames"}]});
    let n = derive(&plan(Edition::Y2024, Vec::new()), vec![named]);
    assert_eq!(n.docs[0]["rules"][0]["spell"], "ID_TST_SPELL_PEBBLE_SKIP");
    assert_eq!(n.docs[0]["rules"][1]["spell"], "ID_2024_SPELL_SACRED_FLAME");
    assert_eq!(n.docs[0]["rules"][2]["spell"], "ID_2024_SPELL_SACRED_FLAME");
    assert!(pack_problems(&n).is_empty(), "{:?}", pack_problems(&n));
    let rules = d.docs[0]["rules"].as_array().unwrap();
    assert_eq!(
        rules.len(),
        2,
        "the unlevelled grant is reported, not left dangling"
    );
    assert_eq!(
        rules[0]["spell"], "ID_2024_SPELL_SACRED_FLAME",
        "same name, this edition"
    );
    assert_eq!(rules[1]["spell"], "ID_TST_SPELL_EMBER_WHISTLE");
    assert_eq!(d.extra.len(), 1);
    assert_eq!(setter(&d.extra[0], "level"), "0");
    let unresolved: Vec<&str> = d
        .notes
        .iter()
        .filter(|(_, m)| m.starts_with(UNRESOLVED_OFFICIAL))
        .map(|(_, m)| m.as_str())
        .collect();
    assert_eq!(unresolved.len(), 2, "{:?}", d.notes);
    assert!(unresolved[1].contains("\"Fog Lantern\""));
    assert!(pack_problems(&d).is_empty(), "{:?}", pack_problems(&d));
    // The review reads the same signal.
    let flagged = unresolved_official(
        &plan(Edition::Y2024, Vec::new()),
        &[json!({"id": "ID_TST_FEAT_X", "name": "X", "type": "Feat",
            "rules": [{"kind": "grant", "type": "Spell", "id": "ID_XGE_SPELL_FOG_LANTERN"}]})],
        library(),
    );
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].0, "ID_TST_FEAT_X");
}

#[test]
fn an_items_placeholder_cantrip_is_on_the_sheet_in_both_editions() {
    for (edition, ruleset, tag) in [
        (
            Edition::Y2014,
            borealis_content::Ruleset::Dnd2014,
            "dnd2014",
        ),
        (
            Edition::Y2024,
            borealis_content::Ruleset::Dnd2024,
            "dnd2024",
        ),
    ] {
        let item = json!({"id": "ID_TST_MAGIC_ITEM_LANTERN_OF_REEDS", "name": "Lantern of Reeds",
            "type": "Magic Item", "source": "Test", "ruleset": tag,
            "setters": [{"name": "category", "value": "Wondrous Items"},
                {"name": "type", "value": "Wondrous item"}, {"name": "rarity", "value": "Rare"}],
            "rules": [{"kind": "grant_spell", "spell": "ID_XGE_SPELL_EMBER_WHISTLE",
                "mode": "known_cantrip"}]});
        let d = derive(&plan(edition, Vec::new()), vec![item]);
        let mut extra = d.docs.clone();
        extra.extend(d.extra.iter().map(|e| {
            let mut e = e.clone();
            e["ruleset"] = json!(tag);
            e
        }));
        let snap = super::super::super::items::tests::fighter_sheet(
            ruleset,
            &extra,
            &["ID_TST_MAGIC_ITEM_LANTERN_OF_REEDS"],
            [16, 10, 14, 10, 14, 8],
        );
        let listed: Vec<&str> = snap
            .spellcasting
            .iter()
            .flat_map(|b| b.spells.iter().map(|s| s.name.as_str()))
            .collect();
        assert!(listed.contains(&"Ember Whistle"), "{tag}: {listed:?}");
    }
}

fn rules_of(d: &Derived, id: &str) -> Vec<Value> {
    d.docs
        .iter()
        .find(|e| e["id"] == id)
        .and_then(|e| e["rules"].as_array().cloned())
        .unwrap_or_default()
}

/// A race whose text repeats its trait's, the trait naming cantrips it gives with no grant.
fn reed_folk(trait_text: &str) -> Vec<Value> {
    let race = json!({"id": "ID_TST_RACE_REED_FOLK", "name": "Reed Folk", "type": "Race",
        "description": format!("<p>Reed folk live on rivers.</p>{trait_text}"),
        "rules": [{"kind": "grant", "type": "Racial Trait", "id": "ID_TST_RACIAL_TRAIT_REED_MAGIC"}]});
    let tr = json!({"id": "ID_TST_RACIAL_TRAIT_REED_MAGIC", "name": "Reed Magic",
        "type": "Racial Trait", "description": trait_text});
    vec![race, tr]
}

#[test]
fn a_cantrip_the_text_gives_with_no_grant_is_granted_once_in_its_family() {
    let text = "<p><strong>Reed Magic.</strong> You know the <em>Ember Whistle</em> and light \
        cantrips. You also learn the chosen cantrip.</p>";
    let p = plan(Edition::Y2014, Vec::new());
    let d = derive(&p, reed_folk(text));
    assert!(
        rules_of(&d, "ID_TST_RACE_REED_FOLK").len() == 1,
        "the race grants only its trait"
    );
    let tr = rules_of(&d, "ID_TST_RACIAL_TRAIT_REED_MAGIC");
    let ids: Vec<&str> = tr.iter().filter_map(|r| r["spell"].as_str()).collect();
    assert_eq!(
        ids,
        ["ID_TST_SPELL_EMBER_WHISTLE", "ID_PHB_SPELL_LIGHT"],
        "{tr:?}"
    );
    // Known from the trait on.
    assert_eq!(
        tr[0],
        json!({"kind": "grant_spell", "spell": "ID_TST_SPELL_EMBER_WHISTLE", "mode": "known_cantrip"})
    );
    assert_eq!(d.extra.len(), 1);
    assert_eq!(setter(&d.extra[0], "level"), "0");
    assert_eq!(d.extra[0]["source"], PLACEHOLDER_SOURCE);
    assert!(d
        .notes
        .iter()
        .any(|(_, m)| m.starts_with(UNRESOLVED_OFFICIAL)));
    assert!(
        !d.notes.iter().any(|(_, m)| m.contains(NOT_GRANTED)),
        "{:?}",
        d.notes
    );
    assert!(pack_problems(&d).is_empty(), "{:?}", pack_problems(&d));
    let again = derive(&p, [d.docs.clone(), d.extra.clone()].concat());
    assert!(
        again.extra.is_empty() && again.notes.is_empty(),
        "{:?}",
        again.notes
    );
}

#[test]
fn a_listed_cantrip_next_to_a_known_one_is_certain_an_unprinted_lone_name_is_only_noted() {
    let p = plan(Edition::Y2024, Vec::new());
    let listed = "<p>You gain the frost gleam, light, and reed songSh cantrips.</p>";
    let d = derive(&p, reed_folk(listed));
    let tr = rules_of(&d, "ID_TST_RACIAL_TRAIT_REED_MAGIC");
    let spells: Vec<&str> = tr.iter().filter_map(|r| r["spell"].as_str()).collect();
    assert_eq!(
        spells,
        [
            "ID_TST_SPELL_FROST_GLEAM",
            "ID_2024_SPELL_LIGHT",
            "ID_TST_SPELL_REED_SONG"
        ],
        "{tr:?}"
    );
    assert!(tr.iter().all(|r| r["mode"] == "known_cantrip"));
    let names: Vec<&str> = d.extra.iter().filter_map(|e| e["name"].as_str()).collect();
    assert_eq!(names, ["Frost Gleam", "Reed Song"]);

    let lone = "<p>You know the reed song cantrip.</p>";
    let d = derive(&p, reed_folk(lone));
    assert!(rules_of(&d, "ID_TST_RACIAL_TRAIT_REED_MAGIC").is_empty());
    assert!(d.extra.is_empty());
    let flagged: Vec<&(String, String)> = d
        .notes
        .iter()
        .filter(|(_, m)| m.starts_with(UNRESOLVED_OFFICIAL) && m.contains(NOT_GRANTED))
        .collect();
    assert_eq!(flagged.len(), 1, "{:?}", d.notes);
    assert_eq!(flagged[0].0, "ID_TST_RACIAL_TRAIT_REED_MAGIC");
    assert!(flagged[0].1.contains("\"Reed Song\""));
}

#[test]
fn a_grant_already_in_the_family_or_an_unknown_id_shape_is_kept_not_doubled() {
    let p = plan(Edition::Y2014, Vec::new());
    let text = "<p>You know the <em>Ember Whistle</em> cantrip.</p>";
    let mut docs = reed_folk(text);
    // The model granted it on the race, by an id of no known shape.
    docs[0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind": "grant", "type": "Spell", "id": "ID_XYZ_EMBER_WHISTLE"}));
    let d = derive(&p, docs);
    let race = rules_of(&d, "ID_TST_RACE_REED_FOLK");
    assert_eq!(race[1]["id"], "ID_TST_SPELL_EMBER_WHISTLE", "{race:?}");
    assert!(rules_of(&d, "ID_TST_RACIAL_TRAIT_REED_MAGIC").is_empty());
    assert_eq!(d.extra.len(), 1);
    assert!(pack_problems(&d).is_empty(), "{:?}", pack_problems(&d));
}

#[test]
fn a_cantrip_an_items_text_gives_is_on_the_sheet_in_2014() {
    let item = json!({"id": "ID_TST_MAGIC_ITEM_REED_FLUTE", "name": "Reed Flute",
        "type": "Magic Item", "source": "Test", "ruleset": "dnd2014",
        "description": "<p>While holding this flute, you know the <em>Ember Whistle</em> cantrip.</p>",
        "setters": [{"name": "category", "value": "Wondrous Items"},
            {"name": "type", "value": "Wondrous item"}, {"name": "rarity", "value": "Rare"}]});
    let d = derive(&plan(Edition::Y2014, Vec::new()), vec![item]);
    let mut extra = d.docs.clone();
    extra.extend(d.extra.iter().map(|e| {
        let mut e = e.clone();
        e["ruleset"] = json!("dnd2014");
        e
    }));
    let snap = super::super::super::items::tests::fighter_sheet(
        borealis_content::Ruleset::Dnd2014,
        &extra,
        &["ID_TST_MAGIC_ITEM_REED_FLUTE"],
        [16, 10, 14, 10, 14, 8],
    );
    let listed: Vec<&str> = snap
        .spellcasting
        .iter()
        .flat_map(|b| b.spells.iter().map(|s| s.name.as_str()))
        .collect();
    assert!(listed.contains(&"Ember Whistle"), "{listed:?}");
}
