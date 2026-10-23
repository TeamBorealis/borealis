//! B-44: a 2024 character may take a 2014 subclass of its class unless a 2024 version of that
//! subclass is loaded. Older features listed below level 3 arrive at 3; the 2024 Cleric's
//! Blessed Strikes replaces an older domain's Divine Strike / Potent Spellcasting. The mapping
//! is by the parent class (2014 `Roguish Archetype` / `Divine Domain` choice, 2024 `Rogue
//! Subclass` / `Cleric Subclass` choice), for bundled SRD content and imported Aurora XML alike.
//! Synthetic subclasses over the bundled core + SRD 5.1 + SRD 5.2.1 packs.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Rule, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use borealis_migrate::{convert_xml, Document};
use serde_json::{json, Value};
use std::path::Path;

const ROGUE_24: &str = "ID_2024_CLASS_ROGUE";
const ROGUE_14: &str = "ID_WOTC_PHB_CLASS_ROGUE";
const ROGUE_CHOICE_24: &str = "ID_2024_CLASS_FEATURE_ROGUE_ROGUE_SUBCLASS";
const ROGUE_CHOICE_14: &str = "ID_WOTC_PHB_CLASS_FEATURE_ROGUE_ROGUISHARCHETYPE";
const ROGUE_CHOICE_NAME: &str = "Roguish Archetype";
const THIEF_14: &str = "ID_WOTC_PHB_ARCHETYPE_THIEF";
const THIEF_24: &str = "ID_2024_SUBCLASS_ROGUE_THIEF";
const CLERIC_24: &str = "ID_2024_CLASS_CLERIC";
const CLERIC_14: &str = "ID_WOTC_PHB_CLASS_CLERIC";
const CLERIC_CHOICE_24: &str = "ID_2024_CLASS_FEATURE_CLERIC_CLERIC_SUBCLASS";
const CLERIC_CHOICE_14: &str = "ID_WOTC_PHB_CLASS_FEATURE_CLERIC_DIVINEDOMAIN";
const DOMAIN_CHOICE_NAME: &str = "Divine Domain";
const BLESSED_STRIKES: &str = "ID_2024_CLASS_FEATURE_CLERIC_BLESSED_STRIKES";

/// A 2014 rogue subclass no 2024 book reprints.
const LANTERN: &str = "ID_T_B44_LANTERN_WAY";
const LANTERN_F3: &str = "ID_T_B44_LANTERN_WAY_F3";
/// A 2014 rogue subclass that shares its name with the bundled 2024 Thief.
const OTHER_THIEF: &str = "ID_T_B44_OTHER_THIEF";
/// A 2014 cleric domain with a level 1, a level 6 and a level 8 Divine Strike feature.
const DOMAIN: &str = "ID_T_B44_TIDE_DOMAIN";
const DOMAIN_F1: &str = "ID_T_B44_TIDE_WARD";
const DOMAIN_F6: &str = "ID_T_B44_TIDE_SURGE";
const DOMAIN_STRIKE: &str = "ID_T_B44_TIDE_STRIKE";
/// 5etools shapes: a 2014 subclass and its `_copy` for the 2024 class (`classSource XPHB`,
/// already fitting the 2024 choice), and a 2014 subclass reprinted under a new name
/// (`reprintedAs` short name `Gloam`) as the 2024 `Way of the Gloam`.
const EMBER: &str = "ID_T_B44_EMBER_WAY";
const EMBER_COPY: &str = "ID_T_B44_EMBER_WAY_COPY";
const GLOAMING: &str = "ID_T_B44_GLOAMING_ART";
const GLOAM_24: &str = "ID_T_B44_WAY_OF_THE_GLOAM";
/// Imported from Aurora XML.
const XML_ARCH: &str = "ID_T_B44_XML_MOTH_PATH";
const XML_FEATURE: &str = "ID_T_B44_XML_MOTH_WINGS";

fn el(id: &str, name: &str, ty: &str, extra: Value) -> Value {
    let mut v = json!({"id": id, "name": name, "type": ty, "source": "Test"});
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    v
}

fn grant(id: &str, level: u32) -> Value {
    json!({"kind": "grant", "id": id, "type": "Archetype Feature", "level": level})
}

fn synthetic_2014() -> ContentFile {
    let rogue = json!({"supports": ["Roguish Archetype"]});
    let lantern = json!({"supports": ["Roguish Archetype"], "rules": [grant(LANTERN_F3, 3)]});
    let domain = json!({"supports": ["Divine Domain"],
        "rules": [grant(DOMAIN_F1, 1), grant(DOMAIN_F6, 6), grant(DOMAIN_STRIKE, 8)]});
    let none = json!({});
    let elements = vec![
        el(LANTERN, "Lantern Way", "Archetype", lantern),
        el(
            LANTERN_F3,
            "Lantern Sight",
            "Archetype Feature",
            none.clone(),
        ),
        el(OTHER_THIEF, "Thief", "Archetype", rogue),
        el(DOMAIN, "Tide Domain", "Archetype", domain),
        el(DOMAIN_F1, "Tide Ward", "Archetype Feature", none.clone()),
        el(DOMAIN_F6, "Tide Surge", "Archetype Feature", none.clone()),
        el(DOMAIN_STRIKE, "Divine Strike", "Archetype Feature", none),
        el(
            EMBER,
            "Ember Way",
            "Archetype",
            rogue_choice("Roguish Archetype"),
        ),
        el(
            EMBER_COPY,
            "Ember Way",
            "Archetype",
            rogue_choice("Rogue Subclass"),
        ),
        el(
            GLOAMING,
            "Gloaming Art",
            "Archetype",
            json!({"supports": ["Roguish Archetype"],
                "data": {"reprinted_as": ["Gloam|Rogue|YYY|YYY"]}}),
        ),
    ];
    let file = json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements});
    serde_json::from_value(file).expect("synthetic content")
}

fn rogue_choice(tag: &str) -> Value {
    json!({"supports": [tag]})
}

fn synthetic_2024() -> ContentFile {
    let elements = vec![el(
        GLOAM_24,
        "Way of the Gloam",
        "Archetype",
        rogue_choice("Rogue Subclass"),
    )];
    let file = json!({"format": "borealis-content/1", "ruleset": "dnd2024", "elements": elements});
    serde_json::from_value(file).expect("synthetic content")
}

/// An Aurora XML homebrew rogue subclass, converted as the app's import does (the migrator
/// stamps imported Aurora content `dnd2014`).
fn aurora_xml() -> ContentFile {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Moth Path" type="Archetype" source="Test" id="{XML_ARCH}">
    <supports>Roguish Archetype</supports>
    <description><p>Synthetic.</p></description>
    <rules>
      <grant type="Archetype Feature" id="{XML_FEATURE}" level="3" />
    </rules>
  </element>
  <element name="Moth Wings" type="Archetype Feature" source="Test" id="{XML_FEATURE}">
    <description><p>Synthetic.</p></description>
  </element>
</elements>"#
    );
    let conv = convert_xml(&xml, "moth.xml");
    let Some(Document::Content(mut file)) = conv.document else {
        panic!("converted: {:?}", conv.issues)
    };
    file.ruleset = Some(Ruleset::Dnd2014);
    file
}

fn engine() -> Engine {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
        .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
        .expect("bundled packs")
        .add_file("b44.json", PackOrigin::Custom, synthetic_2014())
        .add_file("b44-2024.json", PackOrigin::Custom, synthetic_2024())
        .add_file("moth.json", PackOrigin::Custom, aurora_xml())
        .build();
    Engine::new(&store)
}

fn pick(e: &Engine, s: &CharacterState, owner: &str, name: &str, id: &str) -> CharacterState {
    let key =
        slot_key(&e.derive(s), owner, name, 1).unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{id}: {err}"))
        .state
}

fn at_level(e: &Engine, ruleset: Ruleset, class: &str, level: u32) -> CharacterState {
    let mut s = commands::new_character(e, ruleset);
    s = pick(e, &s, "ID_LEVEL_1", "Class", class);
    for _ in 1..level {
        s = commands::level_up_main(e, &s, None).expect("level").state;
    }
    s
}

fn offered(snap: &CharacterSnapshot, owner: &str, name: &str) -> Vec<String> {
    let slot = snap
        .slots
        .iter()
        .find(|x| x.owner.id == owner && x.name == name)
        .unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    let mut ids: Vec<String> = slot.options.iter().map(|o| o.id.clone()).collect();
    ids.sort();
    ids
}

#[test]
fn a_2024_rogue_is_offered_older_subclasses_without_a_2024_version() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2024, ROGUE_24, 3);
    let snap = e.derive(&s);
    let ids = offered(&snap, ROGUE_CHOICE_24, ROGUE_CHOICE_NAME);
    assert!(ids.contains(&THIEF_24.to_string()), "{ids:?}");
    assert!(ids.contains(&LANTERN.to_string()), "{ids:?}");
    // the SRD 5.1 Thief and a homebrew 2014 "Thief" both have the bundled 2024 Thief
    assert!(!ids.contains(&THIEF_14.to_string()), "{ids:?}");
    assert!(!ids.contains(&OTHER_THIEF.to_string()), "{ids:?}");
    // one entry per subclass, the 2014 one tagged with its edition
    let mut dedup = ids.clone();
    dedup.dedup();
    assert_eq!(dedup, ids);
    let slot = snap
        .slots
        .iter()
        .find(|x| x.owner.id == ROGUE_CHOICE_24)
        .expect("slot");
    let lantern = slot
        .options
        .iter()
        .find(|o| o.id == LANTERN)
        .expect("lantern");
    assert_eq!(
        lantern.edition.as_ref().map(|b| b.from),
        Some(Ruleset::Dnd2014)
    );
    let thief = slot
        .options
        .iter()
        .find(|o| o.id == THIEF_24)
        .expect("thief");
    assert!(thief.edition.is_none());
    // the older subclass works: its level 3 feature arrives
    let s = pick(&e, &s, ROGUE_CHOICE_24, ROGUE_CHOICE_NAME, LANTERN);
    assert!(e.derive(&s).has_element(LANTERN_F3));
}

#[test]
fn a_2014_rogue_is_unchanged() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2014, ROGUE_14, 3);
    let ids = offered(&e.derive(&s), ROGUE_CHOICE_14, ROGUE_CHOICE_NAME);
    let mut want = vec![
        EMBER.to_string(),
        GLOAMING.to_string(),
        LANTERN.to_string(),
        OTHER_THIEF.to_string(),
        THIEF_14.to_string(),
        XML_ARCH.to_string(),
    ];
    want.sort();
    assert_eq!(ids, want);
}

#[test]
fn a_5etools_copy_is_offered_once_and_a_renamed_reprint_hides_the_original() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2024, ROGUE_24, 3);
    let ids = offered(&e.derive(&s), ROGUE_CHOICE_24, ROGUE_CHOICE_NAME);
    // the copy made for the 2024 class stands for the subclass; the original is not added
    assert!(ids.contains(&EMBER_COPY.to_string()), "{ids:?}");
    assert!(!ids.contains(&EMBER.to_string()), "{ids:?}");
    // `reprintedAs` names the 2024 version by its short name
    assert!(ids.contains(&GLOAM_24.to_string()), "{ids:?}");
    assert!(!ids.contains(&GLOAMING.to_string()), "{ids:?}");
}

#[test]
fn an_aurora_xml_subclass_for_the_2014_choice_shows_on_the_2024_class() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2024, ROGUE_24, 3);
    let ids = offered(&e.derive(&s), ROGUE_CHOICE_24, ROGUE_CHOICE_NAME);
    assert!(ids.contains(&XML_ARCH.to_string()), "{ids:?}");
    let s = pick(&e, &s, ROGUE_CHOICE_24, ROGUE_CHOICE_NAME, XML_ARCH);
    assert!(e.derive(&s).has_element(XML_FEATURE));
}

#[test]
fn older_features_below_level_3_arrive_at_3_and_later_ones_keep_their_level() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2024, CLERIC_24, 3);
    let s = pick(&e, &s, CLERIC_CHOICE_24, DOMAIN_CHOICE_NAME, DOMAIN);
    let snap = e.derive(&s);
    assert!(snap.has_element(DOMAIN_F1), "level 1 feature at 3");
    assert!(!snap.has_element(DOMAIN_F6));
    let mut s = s;
    for _ in 3..5 {
        s = commands::level_up_main(&e, &s, None).expect("level").state;
    }
    assert!(!e.derive(&s).has_element(DOMAIN_F6), "not at 5");
    s = commands::level_up_main(&e, &s, None).expect("level").state;
    assert!(e.derive(&s).has_element(DOMAIN_F6), "level 6 feature at 6");
    // the 2024 universe lists the level 1 grant at 3; the 2014 one keeps it at 1
    let level_of = |rs: Ruleset, feature: &str| {
        e.universe(rs)
            .get(DOMAIN)
            .and_then(|u| {
                u.el.rules.iter().find_map(|r| match r {
                    Rule::Grant(g) if g.id == feature => g.level,
                    _ => None,
                })
            })
            .expect("grant")
    };
    assert_eq!(level_of(Ruleset::Dnd2024, DOMAIN_F1), 3);
    assert_eq!(level_of(Ruleset::Dnd2024, DOMAIN_F6), 6);
    assert_eq!(level_of(Ruleset::Dnd2014, DOMAIN_F1), 1);
    let entry = snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == DOMAIN)
        .expect("compatibility entry");
    assert!(entry.converted);
    assert!(
        entry.notes.iter().any(|n| n.contains("level 3")),
        "{:?}",
        entry.notes
    );
}

#[test]
fn blessed_strikes_replaces_an_older_domains_divine_strike() {
    let e = engine();
    let s = at_level(&e, Ruleset::Dnd2024, CLERIC_24, 3);
    let mut s = pick(&e, &s, CLERIC_CHOICE_24, DOMAIN_CHOICE_NAME, DOMAIN);
    for _ in 3..8 {
        s = commands::level_up_main(&e, &s, None).expect("level").state;
    }
    let snap = e.derive(&s);
    assert!(snap.has_element(BLESSED_STRIKES));
    assert!(snap.has_element(DOMAIN_F6));
    assert!(
        !snap.has_element(DOMAIN_STRIKE),
        "no Divine Strike on top of Blessed Strikes"
    );
    let entry = snap
        .compatibility
        .entries
        .iter()
        .find(|c| c.element.id == DOMAIN)
        .expect("compatibility entry");
    assert!(
        entry.notes.iter().any(|n| n.contains("Blessed Strikes")),
        "{:?}",
        entry.notes
    );
    // a 2014 cleric of the same domain keeps its Divine Strike
    let s = at_level(&e, Ruleset::Dnd2014, CLERIC_14, 1);
    let mut s = pick(&e, &s, CLERIC_CHOICE_14, DOMAIN_CHOICE_NAME, DOMAIN);
    for _ in 1..8 {
        s = commands::level_up_main(&e, &s, None).expect("level").state;
    }
    let snap = e.derive(&s);
    assert!(snap.has_element(DOMAIN_F1));
    assert!(snap.has_element(DOMAIN_STRIKE));
}
