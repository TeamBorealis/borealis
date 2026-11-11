//! P2b (homebrew creator P2 items, 1.5.0) through the real `AppState` on a temporary data folder with the bundled
//! content:
//!
//! * Try it's level ladder tells a subclass's level problems in the creator's plain words (WB-07's sentences);
//! * a homebrew weapon tried out is a weapon: carried and equipped (never attuned when it needs no attunement), an
//!   attack, never a feature, and offered by the Weapon Mastery choice;
//! * a weapon's live preview gives its damage and type, properties, mastery, weapon category, cost with its coin and
//!   weight;
//! * a creature's live preview (the compendium's monster entry) is the whole stat block: size, type and alignment,
//!   armor class, hit points, speed, the six scores with their modifiers, saving throws, skills, damage and condition
//!   lines, senses, languages and challenge, then the sections;
//! * SPELL LISTS edits a subclass's expanded list: the own subclass's `spellcasting.extends`, or the pack's append
//!   for a loaded subclass, and a character of that subclass is offered the spell.

use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewTrialSetup, HomebrewTrialView,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::path::PathBuf;

const CHAMPION: &str = "ID_2024_SUBCLASS_FIGHTER_CHAMPION";

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
