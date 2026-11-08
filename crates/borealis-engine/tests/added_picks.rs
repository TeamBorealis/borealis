//! B3 (GitHub #35): the builder's Add actions for the extra feats, spells, languages and
//! proficiencies a DM grants. An added pick is an equipped, hidden Aurora proxy line marked
//! `added_pick`; with prerequisites enforced only qualifying options are offered, and the HR3
//! house rule "Ignore feat prerequisites" (GitHub #24) widens the list to every installed option.
//! The picks change the snapshot like any grant and round-trip through the character file.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::added_picks::{AddedKind, AddedPickSnapshot};
use borealis_engine::commands::{self, CommandError};
use borealis_engine::state::{CharacterState, Choice, ClassSlot, EquipSlot};
use borealis_engine::Engine;
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;

const BOOK: &str = "Test Book";
const PLAIN: &str = "ID_TB_FEAT_PLAIN";
const PLAIN_PROXY: &str = "ID_TB_INTERNAL_ITEM_FEAT_PROXY_PLAIN";
const STRONG: &str = "ID_TB_FEAT_STRONG";
const TONGUE: &str = "ID_TB_LANGUAGE_OLD_TONGUE";
const STEALTH: &str = "ID_PROFICIENCY_SKILL_STEALTH";
const WIZARD: &str = "ID_2024_CLASS_WIZARD";
const MAGIC_MISSILE: &str = "ID_2024_SPELL_MAGIC_MISSILE";
const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";
const CURE_WOUNDS: &str = "ID_2024_SPELL_CURE_WOUNDS";

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let elements = vec![
            json!({"id": "ID_TB_SOURCE", "name": BOOK, "type": "Source", "source": "Core",
                   "setters": [{"name": "abbreviation", "value": "TB"}]}),
            json!({"id": PLAIN, "name": "Plain Feat", "type": "Feat", "source": BOOK}),
            json!({"id": STRONG, "name": "Strong Feat", "type": "Feat", "source": BOOK,
                   "prerequisite": "Strength 13 or higher", "requirements": "[str:13]"}),
            json!({"id": TONGUE, "name": "Old Tongue", "type": "Language", "source": BOOK}),
        ];
        let file: ContentFile =
            serde_json::from_value(json!({"format": "borealis-content/1", "elements": elements}))
                .expect("synthetic content");
        let store = ["core", "srd-5.1", "srd-5.2.1"]
            .iter()
            .fold(ContentStoreBuilder::new(), |b, p| {
                b.add_pack_dir(&root.join(p)).expect("bundled pack")
            })
            .add_file("extras.json", PackOrigin::Custom, file)
            .build();
        Engine::new(&store)
    })
}

/// A 2024 wizard of `level` with Strength `strength`.
fn wizard(level: u32, strength: i32) -> CharacterState {
    let e = engine();
    let mut s = commands::new_character(e, Ruleset::Dnd2024);
    s.abilities.base[0] = strength;
    let key = e
        .derive(&s)
        .slots
        .iter()
        .find(|x| x.ty == "Class")
        .expect("class slot")
        .key
        .clone();
    let mut s = commands::select(e, &s, &key, Choice::Element(WIZARD.into()))
        .expect("wizard")
        .state;
    s.levels = vec![ClassSlot::Main; level as usize];
    s
}

fn offered(s: &CharacterState, kind: AddedKind, cast: Option<&str>) -> Vec<(String, bool)> {
    engine()
        .added_pick_options(s, kind, cast)
        .into_iter()
        .map(|o| (o.id, o.qualifies))
        .collect()
}

fn offers(s: &CharacterState, kind: AddedKind, cast: Option<&str>, id: &str) -> Option<bool> {
    offered(s, kind, cast)
        .into_iter()
        .find(|(o, _)| o == id)
        .map(|(_, q)| q)
}

fn add(s: &CharacterState, kind: AddedKind, id: &str, cast: Option<&str>) -> CharacterState {
    commands::add_pick(engine(), s, kind, id, cast)
        .unwrap_or_else(|e| panic!("add {id}: {e}"))
        .state
}

fn pick<'a>(snap: &'a [AddedPickSnapshot], id: &str) -> &'a AddedPickSnapshot {
    snap.iter()
        .find(|p| p.element.id == id)
        .unwrap_or_else(|| panic!("no added pick {id}"))
}

/// JSON round trip of the build, as the character file stores it.
fn reload(state: &CharacterState) -> CharacterState {
    serde_json::from_str(&serde_json::to_string(state).expect("save")).expect("load")
}

fn ids(s: &CharacterState) -> Vec<String> {
    let mut v: Vec<String> = engine()
        .derive(s)
        .elements
        .into_iter()
        .map(|r| r.element.id)
        .collect();
    v.sort();
    v
}

#[test]
fn adding_each_kind_registers_it_and_lists_it_as_added() {
    let hero = wizard(1, 10);
    let before = engine().derive(&hero);
    for id in [PLAIN, TONGUE, STEALTH, MAGIC_MISSILE] {
        assert!(!before.has_element(id), "{id}");
    }
    let s = add(&hero, AddedKind::Feat, PLAIN, None);
    let s = add(&s, AddedKind::Language, TONGUE, None);
    let s = add(&s, AddedKind::Proficiency, STEALTH, None);
    let s = add(&s, AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"));
    let snap = engine().derive(&s);
    for id in [PLAIN, TONGUE, STEALTH, MAGIC_MISSILE] {
        assert!(snap.has_element(id), "{id} registered");
    }
    let kinds: Vec<(AddedKind, &str, Option<&str>, bool)> = snap
        .added_picks
        .iter()
        .map(|p| {
            (
                p.kind,
                p.element.id.as_str(),
                p.spellcasting.as_deref(),
                p.applied,
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (AddedKind::Feat, PLAIN, None, true),
            (AddedKind::Language, TONGUE, None, true),
            (AddedKind::Proficiency, STEALTH, None, true),
            (AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"), true),
        ]
    );
    assert!(snap.added_picks.iter().all(|p| !p.prerequisites_unmet));
    // the feat is a feature like any other, tagged as added
    let feat = snap.features.iter().find(|f| f.id == PLAIN).expect("feat");
    assert!(feat.added && !feat.prerequisites_unmet);
    assert!(snap.features.iter().filter(|f| f.added).count() == 1);
    // the skill makes the wizard proficient in Stealth
    let stealth = |s: &CharacterState| {
        engine()
            .derive(s)
            .skill("stealth")
            .map(|k| k.proficient)
            .unwrap_or(false)
    };
    assert!(!stealth(&hero) && stealth(&s));
    // the spell joins the wizard's block
    let block = snap.caster("Wizard").expect("wizard block");
    assert!(block.spells.iter().any(|x| x.id == MAGIC_MISSILE));
}

#[test]
fn an_added_pick_is_an_equipped_hidden_aurora_proxy_line() {
    let s = add(&wizard(1, 10), AddedKind::Feat, PLAIN, None);
    let line = s.inventory.items.last().expect("line");
    assert_eq!(line.item, PLAIN_PROXY, "Aurora's Additional Feat item");
    assert!(line.added_pick && line.hidden && !line.card && !line.sidebar);
    assert_eq!(line.equipped, Some(EquipSlot::Worn));
    // an Additional Feat item added and equipped in the equipment screen (an Aurora import) is
    // the same grant, but not an added pick
    let e = engine();
    let hero = wizard(1, 10);
    let added = commands::add_item(e, &hero, PLAIN_PROXY, 1)
        .expect("add")
        .state;
    let uid = added.inventory.items.last().expect("line").uid.clone();
    let equipped = commands::equip(e, &added, &uid, None).expect("equip").state;
    let snap = e.derive(&equipped);
    assert!(snap.has_element(PLAIN));
    assert!(snap.added_picks.is_empty());
    assert!(!snap.features.iter().any(|f| f.added));
    assert!(
        commands::remove_pick(e, &equipped, &uid).is_err(),
        "remove_pick leaves equipment lines alone"
    );
}

#[test]
fn removing_an_added_pick_takes_its_element_away() {
    let s = add(&wizard(1, 10), AddedKind::Language, TONGUE, None);
    let uid = s.inventory.items.last().expect("line").uid.clone();
    let out = commands::remove_pick(engine(), &s, &uid)
        .expect("remove")
        .state;
    let snap = engine().derive(&out);
    assert!(!snap.has_element(TONGUE));
    assert!(snap.added_picks.is_empty());
    assert!(out.inventory.items.iter().all(|i| !i.added_pick));
    assert_eq!(ids(&out), ids(&wizard(1, 10)));
}

#[test]
fn prerequisites_filter_the_offered_feats() {
    let weak = wizard(1, 8);
    assert_eq!(offers(&weak, AddedKind::Feat, None, PLAIN), Some(true));
    assert_eq!(
        offers(&weak, AddedKind::Feat, None, STRONG),
        None,
        "Strength 13 is not met"
    );
    assert!(
        offered(&weak, AddedKind::Feat, None)
            .iter()
            .all(|(_, q)| *q),
        "only qualifying options without the house rule"
    );
    assert!(matches!(
        commands::add_pick(engine(), &weak, AddedKind::Feat, STRONG, None),
        Err(CommandError::NotAllowed(_))
    ));
    let strong = wizard(1, 13);
    assert_eq!(offers(&strong, AddedKind::Feat, None, STRONG), Some(true));
    let s = add(&strong, AddedKind::Feat, STRONG, None);
    assert!(engine().derive(&s).has_element(STRONG));
}

#[test]
fn ignoring_feat_prerequisites_offers_every_feat_marks_it_and_applies_it() {
    let mut weak = wizard(1, 8);
    weak.house_rules.ignore_feat_prerequisites = true;
    assert_eq!(
        offers(&weak, AddedKind::Feat, None, STRONG),
        Some(false),
        "offered, not qualifying"
    );
    assert_eq!(offers(&weak, AddedKind::Feat, None, PLAIN), Some(true));
    let s = add(&weak, AddedKind::Feat, STRONG, None);
    let snap = engine().derive(&s);
    assert!(snap.has_element(STRONG));
    let p = pick(&snap.added_picks, STRONG);
    assert!(p.applied && p.prerequisites_unmet);
    let feature = snap.features.iter().find(|f| f.id == STRONG).expect("feat");
    assert!(feature.added && feature.prerequisites_unmet);

    // the rule off: kept, not applied; on again: applied
    let mut off = s.clone();
    off.house_rules.ignore_feat_prerequisites = false;
    let snap = engine().derive(&off);
    assert!(!snap.has_element(STRONG));
    let p = pick(&snap.added_picks, STRONG);
    assert!(!p.applied && !p.prerequisites_unmet);
    let mut on = off.clone();
    on.house_rules.ignore_feat_prerequisites = true;
    assert!(engine().derive(&on).has_element(STRONG));
}

#[test]
fn spells_must_be_on_the_class_list_and_castable_unless_prerequisites_are_ignored() {
    let mut hero = wizard(1, 10);
    let wiz = Some("Wizard");
    assert_eq!(
        offers(&hero, AddedKind::Spell, wiz, MAGIC_MISSILE),
        Some(true)
    );
    assert_eq!(
        offers(&hero, AddedKind::Spell, wiz, CURE_WOUNDS),
        None,
        "not a wizard spell"
    );
    assert_eq!(
        offers(&hero, AddedKind::Spell, wiz, FIREBALL),
        None,
        "too high for a level 1 wizard"
    );
    assert!(engine()
        .added_pick_options(&hero, AddedKind::Spell, None)
        .is_empty());
    assert!(engine()
        .added_pick_options(&hero, AddedKind::Spell, Some("Cleric"))
        .is_empty());
    assert!(matches!(
        commands::add_pick(engine(), &hero, AddedKind::Spell, MAGIC_MISSILE, None),
        Err(CommandError::Spell(_))
    ));
    hero.levels = vec![ClassSlot::Main; 5];
    assert_eq!(offers(&hero, AddedKind::Spell, wiz, FIREBALL), Some(true));

    let mut open = wizard(1, 10);
    open.house_rules.ignore_feat_prerequisites = true;
    assert_eq!(
        offers(&open, AddedKind::Spell, wiz, CURE_WOUNDS),
        Some(false)
    );
    assert_eq!(offers(&open, AddedKind::Spell, wiz, FIREBALL), Some(false));
    let s = add(&open, AddedKind::Spell, CURE_WOUNDS, wiz);
    let snap = engine().derive(&s);
    assert!(snap
        .caster("Wizard")
        .expect("block")
        .spells
        .iter()
        .any(|x| x.id == CURE_WOUNDS));
    assert!(pick(&snap.added_picks, CURE_WOUNDS).prerequisites_unmet);
}

#[test]
fn what_the_character_has_is_not_offered_again() {
    let s = add(&wizard(1, 10), AddedKind::Language, TONGUE, None);
    assert_eq!(offers(&s, AddedKind::Language, None, TONGUE), None);
    let s = add(&s, AddedKind::Proficiency, STEALTH, None);
    assert_eq!(offers(&s, AddedKind::Proficiency, None, STEALTH), None);
    let s = add(&s, AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"));
    assert_eq!(
        offers(&s, AddedKind::Spell, Some("Wizard"), MAGIC_MISSILE),
        None
    );
    assert!(matches!(
        commands::add_pick(engine(), &s, AddedKind::Language, TONGUE, None),
        Err(CommandError::NotAllowed(_))
    ));
}

#[test]
fn the_offered_proficiencies_name_their_group() {
    let options = engine().added_pick_options(&wizard(1, 10), AddedKind::Proficiency, None);
    let group = |id: &str| {
        options
            .iter()
            .find(|o| o.id == id)
            .and_then(|o| o.group.clone())
            .unwrap_or_else(|| panic!("{id} offered"))
    };
    assert_eq!(group(STEALTH), "Skill");
    assert_eq!(group("ID_PROFICIENCY_SAVINGTHROW_STRENGTH"), "Saving Throw");
    assert_eq!(
        group("ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR"),
        "Armor"
    );
    assert_eq!(
        group("ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS"),
        "Weapon"
    );
    assert_eq!(
        group("ID_PROFICIENCY_TOOL_PROFICIENCY_ARTISANS_TOOLS"),
        "Tool"
    );
    assert!(
        options
            .iter()
            .all(|o| !o.id.starts_with("ID_INTERNAL_PROFICIENCY_SPELLFOCUS")),
        "Aurora lists no proxy for the spell focus groups"
    );
}

#[test]
fn added_picks_round_trip_through_the_character_file() {
    let mut s = add(&wizard(1, 8), AddedKind::Feat, PLAIN, None);
    s.house_rules.ignore_feat_prerequisites = true;
    let s = add(&s, AddedKind::Feat, STRONG, None);
    let s = add(&s, AddedKind::Language, TONGUE, None);
    let s = add(&s, AddedKind::Proficiency, STEALTH, None);
    let s = add(&s, AddedKind::Spell, MAGIC_MISSILE, Some("Wizard"));
    let back = reload(&s);
    assert_eq!(back, s, "the state survives save and load");
    assert_eq!(ids(&back), ids(&s));
    assert_eq!(
        engine().derive(&back).added_picks,
        engine().derive(&s).added_picks
    );
    // the flag is written only on added lines; a file without it (older builds) reads false
    let json = serde_json::to_value(&s).expect("json");
    let flags: Vec<bool> = json["inventory"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i["added_pick"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(flags, [true; 5]);
    let mut old = json;
    for item in old["inventory"]["items"].as_array_mut().expect("items") {
        item.as_object_mut().expect("item").remove("added_pick");
    }
    let old: CharacterState = serde_json::from_value(old).expect("old file");
    assert!(old.inventory.items.iter().all(|i| !i.added_pick));
    assert!(engine().derive(&old).added_picks.is_empty());
    assert!(
        engine().derive(&old).has_element(PLAIN),
        "an older build keeps the feat as an equipped Additional Feat item"
    );
}

#[test]
fn a_proxy_id_both_srd_books_share_names_each_editions_own_spell() {
    let uni = |r| engine().universe(r);
    let id = uni(Ruleset::Dnd2024)
        .proxies()
        .in_category("Additional Wizard Spell")
        .find(|p| p.target.el.id == MAGIC_MISSILE)
        .expect("the 2024 Magic Missile has a proxy")
        .id;
    let target = |r| match uni(r).get(&id).map(|p| p.el.rules[0].clone()) {
        Some(borealis_content::Rule::Grant(g)) => g.id,
        other => panic!("{other:?}"),
    };
    assert_eq!(target(Ruleset::Dnd2024), MAGIC_MISSILE);
    assert_eq!(
        target(Ruleset::Dnd2014),
        "ID_PHB_SPELL_MAGIC_MISSILE",
        "the same Aurora id names the 2014 spell for a 2014 character"
    );
}
