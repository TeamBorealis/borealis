//! P2b (homebrew creator P2 items, 1.5.0) through the real `AppState` on a temporary data folder with the bundled
//! content:
//!
//! * Try it's level ladder tells a subclass's level problems in the creator's plain words (WB-07's sentences);
//! * a homebrew weapon tried out is a weapon: carried and equipped (never attuned when it needs no attunement), an
//!   attack, never a feature, and offered by the Weapon Mastery choice; an armor is worn and sets the Armor Class, and
//!   a magic weapon or armor on one base item adds its +1 to the attack line or the AC (HmG6I);
//! * a weapon's live preview gives its damage and type, properties, mastery, weapon category, cost with its coin and
//!   weight;
//! * a creature's live preview (the compendium's monster entry) is the whole stat block: size, type and alignment,
//!   armor class, hit points, speed, the six scores with their modifiers, saving throws, skills, damage and condition
//!   lines, senses, languages and challenge, then the sections;
//! * SPELL LISTS edits a subclass's expanded list: the own subclass's `spellcasting.extends`, or the pack's append
//!   for a loaded subclass, and a character of that subclass is offered the spell.

use borealis_authoring::form::FormState;
use borealis_content::Ruleset as ContentRuleset;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewPageRequest, HomebrewPickerFilters,
    HomebrewPickerTarget, HomebrewSpellListView, HomebrewTrialSetup, HomebrewTrialView,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const CHAMPION: &str = "ID_2024_SUBCLASS_FIGHTER_CHAMPION";
const WARLOCK: &str = "ID_WOTC_PHB_CLASS_WARLOCK";
const FIEND: &str = "ID_WOTC_PHB_ARCHETYPE_OTHERWORLDLY_PATRON_FIEND";
const BURNING_HANDS: &str = "ID_PHB_SPELL_BURNING_HANDS";
const CURE_WOUNDS: &str = "ID_PHB_SPELL_CURE_WOUNDS";

struct Quiet;
impl EventSink for Quiet {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    _data: tempfile::TempDir,
    state: AppState,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        Self { _data: data, state }
    }

    fn pack(&self, edition: Ruleset) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Gloom Hollow".into(),
                    code: "GH".into(),
                    edition,
                    author: None,
                    description: None,
                    license: None,
                },
                &Quiet,
            )
            .unwrap()
            .id
    }

    /// A deep copy of a loaded element (its features made inside it).
    fn deep_copy(&self, pack: &str, id: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, "other", Some(id), None, Some(true))
            .unwrap()
    }

    fn update(&self, v: &HomebrewDraftView, form: &FormState) -> HomebrewDraftView {
        self.state.homebrew_update_draft(&v.draft_id, form).unwrap()
    }

    fn try_it(&self, v: &HomebrewDraftView, level: u32) -> HomebrewTrialView {
        self.state
            .homebrew_trial_start(
                &v.draft_id,
                &HomebrewTrialSetup {
                    level: Some(level),
                    ..Default::default()
                },
            )
            .unwrap()
    }
}

/// The child form named `name`.
fn child_named<'f>(form: &'f mut FormState, name: &str) -> &'f mut FormState {
    form.children
        .iter_mut()
        .find(|c| c.fields.get("name") == Some(&json!(name)))
        .unwrap_or_else(|| panic!("a child named {name}"))
}

#[test]
fn the_ladder_tells_a_subclass_s_level_problems_in_plain_words() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let copy = app.deep_copy(&pack, CHAMPION);
    let mut form = copy.form.clone();
    // the copied level-3 feature gets a choice of something no proficiency is: it offers nothing
    let athlete = child_named(&mut form, "Remarkable Athlete");
    athlete.blocks.push(
        serde_json::from_value(json!({"key": "b-new", "kind": "select",
            "fields": {"type": "Proficiency", "name": "Moonlit Lore", "level": 3, "number": 1,
                "supports": {"value": {"shape": "kept", "text": "ID_GH_NO_SUCH_SUPPORT", "reason": "test"}}}}))
        .unwrap(),
    );
    let draft = app.update(&copy, &form);
    // Try it loads the unsaved copy as a save writes it: the copy grants its own (edited) feature, not the SRD's
    let view = app.try_it(&draft, 3);
    let features: Vec<String> = serde_json::to_value(&view.snapshot.play.features)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["id"].as_str().map(str::to_string))
        .collect();
    assert!(
        features
            .iter()
            .any(|id| id.starts_with("ID_GH_") && id.ends_with("_REMARKABLE_ATHLETE")),
        "the copied feature is on the test character: {features:?}"
    );
    let at3: Vec<&str> = view.ladder[2]
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    assert!(
        at3.contains(&"At level 3 the choice \"Moonlit Lore\" has nothing to offer."),
        "the creator's sentence: {at3:?}"
    );
    let every: Vec<&str> = view
        .ladder
        .iter()
        .flat_map(|r| r.problems.iter().map(|p| p.message.as_str()))
        .collect();
    for raw in [
        "offers no options",
        "level-up to",
        "is missing at level",
        "select \"",
    ] {
        assert!(
            every.iter().all(|m| !m.contains(raw)),
            "no engine text ({raw}): {every:?}"
        );
    }
}

#[test]
fn a_creature_s_live_preview_is_the_whole_stat_block() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2014);
    let copy = app.deep_copy(&pack, "ID_SRD51_MONSTER_UNICORN");
    let html = &copy.preview.as_ref().expect("a preview").description_html;
    let order = [
        "<p><i>Large celestial, lawful good</i></p>",
        "<p><i>Armor Class.</i> 12</p>",
        "<p><i>Hit Points.</i> 67 (9d10+18)</p>",
        "<p><i>Speed.</i> 50 ft.</p>",
        "<td>18 (+4)</td><td>14 (+2)</td><td>15 (+2)</td><td>11 (+0)</td><td>17 (+3)</td><td>16 (+3)</td>",
        "<p><i>Damage Immunities.</i> Poison</p>",
        "<p><i>Condition Immunities.</i> Charmed, Paralyzed, Poisoned</p>",
        "<p><i>Senses.</i> Darkvision 60 ft., passive Perception 13</p>",
        "<p><i>Languages.</i> Celestial, Elvish, Sylvan, telepathy 60 ft.</p>",
        "<p><i>Challenge.</i> 5 (1800 XP)</p>",
        "<h4>Traits</h4>",
        "<h4>Actions</h4>",
        "<h4>Legendary actions</h4>",
    ];
    let at: Vec<usize> = order
        .iter()
        .map(|o| {
            html.find(o)
                .unwrap_or_else(|| panic!("{o} missing in {html}"))
        })
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{html}");
}

/// The form of a new weapon as a user fills it: Twinblade, a martial melee weapon of 2d4 slashing, two-handed and
/// finesse, with the Vex mastery, 50 gp and 6 lb.
fn twinblade(app: &App, pack: &str) -> HomebrewDraftView {
    let v = app
        .state
        .homebrew_new_draft(pack, "weapon", None, None, None)
        .unwrap();
    let mut form = v.form.clone();
    let fields = json!({
        "name": "Twinblade",
        "weaponCategory": {"id": "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE", "name": "Martial Melee", "type": "Weapon Category"},
        "damage": "2d4",
        "damageType": {"id": "ID_INTERNAL_DAMAGE_TYPE_SLASHING", "name": "Slashing", "type": "Damage Type"},
        "properties": [
            {"id": "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED", "name": "Two-Handed", "type": "Weapon Property"},
            {"id": "ID_INTERNAL_WEAPON_PROPERTY_FINESSE", "name": "Finesse", "type": "Weapon Property"}
        ],
        "mastery": [{"id": "ID_2024_MASTERY_VEX", "name": "Vex", "type": "Mastery Property"}],
        "cost": "50", "weight": "6"
    });
    for (k, val) in fields.as_object().unwrap() {
        form.fields.insert(k.clone(), val.clone());
    }
    app.update(&v, &form)
}

#[test]
fn a_weapon_s_live_preview_gives_every_weapon_fact() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = twinblade(&app, &pack);
    let preview = draft.preview.expect("a preview");
    let facts: Vec<(String, String)> = preview
        .facts
        .iter()
        .map(|f| (f.label.clone(), f.value.clone()))
        .collect();
    for want in [
        ("Weapon category", "Martial Melee"),
        ("Damage", "2d4 slashing"),
        ("Properties", "Two-Handed, Finesse"),
        ("Mastery", "Vex"),
        ("Cost", "50 gp"),
        ("Weight", "6 lb."),
    ] {
        let want = (want.0.to_string(), want.1.to_string());
        assert!(facts.contains(&want), "{want:?} not in {facts:?}");
    }
    assert!(
        preview.description_html.contains("<h4>Mastery: Vex</h4>"),
        "the mastery's rule: {}",
        preview.description_html
    );
}

#[test]
fn a_weapon_tried_out_is_carried_equipped_and_an_attack_never_a_feature() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = twinblade(&app, &pack);
    let view = app.try_it(&draft, 1);
    let snap = serde_json::to_value(&view.snapshot).unwrap();
    let carried: Vec<&serde_json::Value> = snap["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["name"] == "Twinblade")
        .collect();
    assert_eq!(carried.len(), 1, "{:?}", snap["inventory"]["items"]);
    assert_eq!(carried[0]["isWeapon"], true);
    assert_eq!(carried[0]["equipped"], "primary");
    assert_eq!(
        carried[0]["attuned"], false,
        "a weapon that needs no attunement isn't attuned"
    );
    assert!(
        snap["attacks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "Twinblade"
                && a["damage"].as_str().is_some_and(|d| d.contains("slashing"))),
        "an attack: {}",
        snap["attacks"]
    );
    assert!(
        snap["play"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["name"] != "Twinblade"),
        "never a feature"
    );
    // the Fighter's Weapon Mastery choice offers it
    let mastery = view
        .snapshot
        .slots
        .iter()
        .find(|s| s.title == "Weapon Mastery" && s.chosen.is_none())
        .expect("a Weapon Mastery choice");
    let page = app
        .state
        .homebrew_trial_options(&view.trial_id, &mastery.id, Some("Twin"), Some(10), None)
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|i| i.base.summary.name.as_str())
            .collect::<Vec<_>>(),
        ["Twinblade"]
    );
}

/// The spells a level-1 2014 Warlock of `patron` may learn (the choice that offers Burning Hands, the Fiend's).
fn warlock_spells(e: &Engine, patron: &str) -> Vec<String> {
    let mut s: CharacterState = commands::new_character(e, ContentRuleset::Dnd2014);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("a class choice");
    s = commands::select(e, &s, &key, Choice::Element(WARLOCK.into()))
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let key = snap
        .slots
        .iter()
        .find(|x| x.options.iter().any(|o| o.id == patron))
        .map(|x| x.key.clone())
        .expect("the patron choice offers it");
    s = commands::select(e, &s, &key, Choice::Element(patron.into()))
        .unwrap()
        .state;
    let snap = e.derive(&s);
    let mut out: Vec<String> = snap
        .slots
        .iter()
        .filter(|x| x.ty == "Spell")
        .flat_map(|x| x.options.iter().map(|o| o.id.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn spell_lists_edit_a_loaded_subclass_s_expanded_list_through_an_append() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2014);
    // the Fiend is a list choice of SPELL LISTS
    let lists = app
        .state
        .homebrew_reference_options(
            &HomebrewPickerTarget {
                types: vec!["Expanded Spell List".into()],
                tags: None,
                multiple: None,
            },
            "fiend",
            &HomebrewPickerFilters::default(),
            HomebrewPageRequest {
                offset: 0,
                limit: 50,
            },
            None,
        )
        .unwrap();
    assert!(
        lists.items.iter().any(|r| r.id == FIEND),
        "{:?}",
        lists.items.iter().map(|r| &r.id).collect::<Vec<_>>()
    );
    let row = |v: &HomebrewSpellListView, id: &str| {
        v.rows
            .iter()
            .find(|r| r.row.id == id)
            .map(|r| (r.on_list, r.removable))
            .unwrap_or_else(|| panic!("{id} is a row"))
    };
    let view = app.state.homebrew_expanded_list(&pack, FIEND).unwrap();
    assert_eq!(view.expanded_of.as_deref(), Some(FIEND));
    assert!(!view.own);
    assert_eq!(view.count, 10, "the Fiend's ten spells");
    assert_eq!(
        row(&view, BURNING_HANDS),
        (true, false),
        "its own pack's: locked"
    );
    assert_eq!(row(&view, CURE_WOUNDS), (false, false));
    let before = warlock_spells(&app.state.hub().unwrap().engine, FIEND);
    assert!(before.contains(&BURNING_HANDS.to_string()));
    assert!(!before.contains(&CURE_WOUNDS.to_string()));

    // checking Cure Wounds writes the pack's append; a Fiend warlock may learn it
    let view = app
        .state
        .homebrew_set_expanded_list(&pack, FIEND, &[CURE_WOUNDS.into()], &[], &Quiet)
        .unwrap();
    assert_eq!(row(&view, CURE_WOUNDS), (true, true));
    assert_eq!(view.count, 11);
    let content: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            app._data
                .path()
                .join("library")
                .join(&pack)
                .join("content.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        content["appends"],
        json!([{"id": FIEND, "spellcasting": {"name": "Warlock", "extend": true, "extends": [{"supports": CURE_WOUNDS}]}}])
    );
    let after = warlock_spells(&app.state.hub().unwrap().engine, FIEND);
    assert!(after.contains(&CURE_WOUNDS.to_string()), "{after:?}");
    // only the pack's own addition comes off
    assert!(app
        .state
        .homebrew_set_expanded_list(&pack, FIEND, &[], &[BURNING_HANDS.into()], &Quiet)
        .is_err());
    let view = app
        .state
        .homebrew_set_expanded_list(&pack, FIEND, &[], &[CURE_WOUNDS.into()], &Quiet)
        .unwrap();
    assert_eq!(view.count, 10);
}

#[test]
fn spell_lists_edit_an_own_subclass_s_expanded_list_in_its_extends() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2014);
    let copy = app.deep_copy(&pack, FIEND);
    let saved = app
        .state
        .homebrew_save_draft(&copy.draft_id, &Quiet)
        .unwrap()
        .element_ids;
    let sub = saved[0].clone();
    let view = app.state.homebrew_expanded_list(&pack, &sub).unwrap();
    assert!(view.own);
    assert_eq!(view.count, 10, "the copy's feature holds the copied list");
    let burning = view
        .rows
        .iter()
        .find(|r| r.row.id == BURNING_HANDS)
        .unwrap();
    assert!(burning.on_list && burning.removable, "the pack's own list");
    let view = app
        .state
        .homebrew_set_expanded_list(
            &pack,
            &sub,
            &[CURE_WOUNDS.into()],
            &[BURNING_HANDS.into()],
            &Quiet,
        )
        .unwrap();
    assert_eq!(view.count, 10);
    let content: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            app._data
                .path()
                .join("library")
                .join(&pack)
                .join("content.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        content
            .get("appends")
            .is_none_or(|a| a.as_array().is_none_or(Vec::is_empty)),
        "no append for an own subclass"
    );
    let holder = content["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["spellcasting"]["extend"] == true)
        .expect("the copied feature holds the list");
    let extends = holder["spellcasting"]["extends"].to_string();
    assert!(
        extends.contains(CURE_WOUNDS) && !extends.contains(BURNING_HANDS),
        "{extends}"
    );
    let spells = warlock_spells(&app.state.hub().unwrap().engine, &sub);
    assert!(spells.contains(&CURE_WOUNDS.to_string()), "{spells:?}");
    assert!(!spells.contains(&BURNING_HANDS.to_string()), "{spells:?}");
}

/// A new item draft of `kind` with these field values.
fn item_draft(app: &App, pack: &str, kind: &str, fields: serde_json::Value) -> HomebrewDraftView {
    let v = app
        .state
        .homebrew_new_draft(pack, kind, None, None, None)
        .unwrap();
    let mut form = v.form.clone();
    for (k, val) in fields.as_object().unwrap() {
        form.fields.insert(k.clone(), val.clone());
    }
    app.update(&v, &form)
}

/// "Applies to" naming one element (the magic item form's base item builder, Specific elements).
fn applies_to(id: &str) -> serde_json::Value {
    json!({"value": {"shape": "model", "model": {"specific": true,
        "root": {"node": "row", "row": {"row": "element", "id": id}}}}})
}

fn attack<'s>(snap: &'s serde_json::Value, name: &str) -> &'s serde_json::Value {
    snap["attacks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("an attack {name}: {}", snap["attacks"]))
}

fn carried<'s>(snap: &'s serde_json::Value, name: &str) -> &'s serde_json::Value {
    snap["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == name)
        .unwrap_or_else(|| panic!("{name} carried: {}", snap["inventory"]["items"]))
}

/// HmG6I: item drafts in Try it, past the plain weapon above: an armor is worn and sets the Armor Class; a magic
/// weapon on one base item is held and its attack line has its +1 to hit and damage; a magic armor on one base adds
/// its +1 to that armor's AC; one for a group of bases is tried on the first of them. The test character is the edition's SRD fighter (Strength +2, proficiency +2).
#[test]
fn item_drafts_tried_out_are_equipped_with_their_attack_line_and_armor_class() {
    const LONGSWORD: &str = "ID_2024_WEAPON_LONGSWORD";
    const CHAIN_MAIL: &str = "ID_2024_ARMOR_CHAIN_MAIL";
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);

    let plate = item_draft(
        &app,
        &pack,
        "armor",
        json!({"name": "Ember Plate", "armorType": "Heavy", "baseAc": 17, "stealth": true, "strength": 15}),
    );
    let view = app.try_it(&plate, 1);
    let snap = serde_json::to_value(&view.snapshot).unwrap();
    assert_eq!(carried(&snap, "Ember Plate")["equipped"], "worn");
    assert_eq!(
        snap["stats"]["armorClass"], 17,
        "heavy armor adds no Dexterity"
    );
    assert!(
        view.changed.iter().any(|c| c == "stats.armorClass"),
        "{:?}",
        view.changed
    );
    // Iqzlk: the inventory's AC column, written from Armor Class with "Armor Class as shown" left empty
    let armor = &carried(&snap, "Ember Plate")["armor"];
    assert_eq!(
        (&armor["acText"], &armor["acBase"]),
        (&json!("17"), &json!(17)),
        "{armor}"
    );
    app.state.homebrew_trial_end(&view.trial_id).unwrap();

    let blade = item_draft(
        &app,
        &pack,
        "magicItem",
        json!({"name": "Emberblade", "magicType": "Weapon", "rarity": "Uncommon",
            "appliesToWeapon": applies_to(LONGSWORD), "enhancement": 1}),
    );
    let view = app.try_it(&blade, 1);
    let snap = serde_json::to_value(&view.snapshot).unwrap();
    assert_eq!(carried(&snap, "Emberblade")["equipped"], "primary");
    let unarmed = attack(&snap, "Unarmed Strike")["toHit"].as_i64().unwrap();
    let line = attack(&snap, "Emberblade");
    assert_eq!(
        line["toHit"].as_i64(),
        Some(unarmed + 1),
        "+1 to hit: {line}"
    );
    assert_eq!(
        line["damage"], "1d8+3 slashing",
        "the Longsword's die, Strength +2 and +1"
    );
    assert!(
        view.changed.iter().any(|c| c == "attacks:Emberblade"),
        "{:?}",
        view.changed
    );
    app.state.homebrew_trial_end(&view.trial_id).unwrap();

    // IqzUc: a magic weapon for a group of bases ("any martial melee weapon") is tried on the first base it can be made
    // from, by name, as a character adds it on a base it chooses: held, with its attack line and its +1
    let martial = json!({"value": {"shape": "model", "model": {"specific": true,
        "root": {"node": "row", "row": {"row": "element", "id": "ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE"}}}}});
    let brand = item_draft(
        &app,
        &pack,
        "magicItem",
        json!({"name": "Emberbrand", "magicType": "Weapon", "rarity": "Uncommon",
            "appliesToWeapon": martial, "enhancement": 1}),
    );
    let view = app.try_it(&brand, 1);
    let snap = serde_json::to_value(&view.snapshot).unwrap();
    let line = snap["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["displayName"] == "Battleaxe (Emberbrand)")
        .unwrap_or_else(|| panic!("Emberbrand carried: {}", snap["inventory"]["items"]));
    assert_eq!(line["equipped"], "primary", "{line}");
    let uid = line["id"].clone();
    // the attack line is the base weapon's, as on a character's sheet
    let line = snap["attacks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == uid)
        .unwrap_or_else(|| panic!("the Emberbrand line's attack: {}", snap["attacks"]));
    assert_eq!(line["name"], "Battleaxe");
    assert_eq!(
        line["toHit"].as_i64(),
        Some(unarmed + 1),
        "+1 to hit: {line}"
    );
    assert_eq!(
        line["damage"], "1d8+3 slashing",
        "the Battleaxe's die, Strength +2 and +1"
    );
    let notes = serde_json::to_value(&view.ladder[0].problems).unwrap();
    assert!(
        notes.as_array().unwrap().iter().any(|n| n["message"]
            .as_str()
            .is_some_and(|m| m.contains("Battleaxe"))),
        "a note names the base: {notes}"
    );
    app.state.homebrew_trial_end(&view.trial_id).unwrap();

    let mail = item_draft(
        &app,
        &pack,
        "magicItem",
        json!({"name": "Ember Mail", "magicType": "Armor", "rarity": "Rare",
            "appliesToArmor": applies_to(CHAIN_MAIL), "enhancement": 1}),
    );
    let view = app.try_it(&mail, 1);
    let snap = serde_json::to_value(&view.snapshot).unwrap();
    assert_eq!(carried(&snap, "Ember Mail")["equipped"], "worn");
    assert_eq!(snap["stats"]["armorClass"], 17, "Chain Mail 16, +1");
    app.state.homebrew_trial_end(&view.trial_id).unwrap();
}
